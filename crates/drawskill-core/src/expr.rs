//! A small expression language for `${ ... }` values in diagrams.
//!
//! Supports numbers, single/double-quoted strings, variable references, arithmetic
//! (`+ - * / %`, parentheses, unary minus), and a set of built-in functions — crucially the
//! text-measurement functions, so diagrams can size things without precomputing:
//!
//! ```text
//! ${ pad * 2 + text_width("Hello", 14) }
//! ${ para_height(body, 200, 12) }
//! ${ max(min_w, label_w + 20) }
//! ```
//!
//! Math helpers: `min`, `max`, `round`, `floor`, `ceil`, `abs`, `sqrt`.
//! Measurement: `text_width(text, size[, font])`, `text_height(text, size[, font])`,
//! `para_width(text, max_width, size[, font])`, `para_height(text, max_width, size[, font])`.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::measure::TextMeasurer;

/// A value produced by evaluating an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
}

impl Value {
    fn as_num(&self) -> Result<f64> {
        match self {
            Value::Num(n) => Ok(*n),
            Value::Str(s) => s
                .trim()
                .parse::<f64>()
                .map_err(|_| Error::Expr(format!("expected a number, found string {s:?}"))),
        }
    }

    fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            Value::Num(n) => format_num(*n),
        }
    }
}

/// Format a number compactly (integers without a decimal point).
pub fn format_num(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        let mut s = format!("{n:.4}");
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        s
    }
}

/// Evaluation context: variables in scope, the text measurer, and the font defaults used
/// when a measurement function omits the font/size.
pub struct EvalContext<'a> {
    pub vars: &'a HashMap<String, Value>,
    pub measurer: &'a dyn TextMeasurer,
    pub default_font: String,
    pub default_size: f64,
}

/// Evaluate an expression string to a [`Value`].
pub fn eval(input: &str, ctx: &EvalContext) -> Result<Value> {
    let tokens = tokenize(input)?;
    let mut parser = Parser { tokens, pos: 0, ctx };
    let v = parser.expr()?;
    if parser.pos != parser.tokens.len() {
        return Err(Error::Expr(format!("unexpected trailing input in {input:?}")));
    }
    Ok(v)
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Comma,
}

fn tokenize(input: &str) -> Result<Vec<Tok>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '+' => {
                tokens.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(Tok::Star);
                i += 1;
            }
            '/' => {
                tokens.push(Tok::Slash);
                i += 1;
            }
            '%' => {
                tokens.push(Tok::Percent);
                i += 1;
            }
            '(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Tok::Comma);
                i += 1;
            }
            '"' | '\'' => {
                let quote = c;
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != quote {
                    // Minimal escape support: \" \' \\ \n \t
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                        s.push(match chars[i] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    } else {
                        s.push(chars[i]);
                    }
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(Error::Expr("unterminated string literal".into()));
                }
                i += 1; // closing quote
                tokens.push(Tok::Str(s));
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let lit: String = chars[start..i].iter().collect();
                let n = lit
                    .parse::<f64>()
                    .map_err(|_| Error::Expr(format!("invalid number {lit:?}")))?;
                tokens.push(Tok::Num(n));
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                tokens.push(Tok::Ident(chars[start..i].iter().collect()));
            }
            other => return Err(Error::Expr(format!("unexpected character {other:?}"))),
        }
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Pratt-ish recursive descent parser/evaluator
// ---------------------------------------------------------------------------

struct Parser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    ctx: &'a EvalContext<'a>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expr(&mut self) -> Result<Value> {
        self.additive()
    }

    fn additive(&mut self) -> Result<Value> {
        let mut left = self.multiplicative()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Tok::Plus => {
                    self.pos += 1;
                    let right = self.multiplicative()?;
                    left = add(&left, &right)?;
                }
                Tok::Minus => {
                    self.pos += 1;
                    let right = self.multiplicative()?;
                    left = Value::Num(left.as_num()? - right.as_num()?);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Value> {
        let mut left = self.unary()?;
        while let Some(op) = self.peek().cloned() {
            match op {
                Tok::Star => {
                    self.pos += 1;
                    let right = self.unary()?;
                    left = Value::Num(left.as_num()? * right.as_num()?);
                }
                Tok::Slash => {
                    self.pos += 1;
                    let right = self.unary()?;
                    let d = right.as_num()?;
                    if d == 0.0 {
                        return Err(Error::Expr("division by zero".into()));
                    }
                    left = Value::Num(left.as_num()? / d);
                }
                Tok::Percent => {
                    self.pos += 1;
                    let right = self.unary()?;
                    let d = right.as_num()?;
                    if d == 0.0 {
                        return Err(Error::Expr("modulo by zero".into()));
                    }
                    left = Value::Num(left.as_num()? % d);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Value> {
        if let Some(Tok::Minus) = self.peek() {
            self.pos += 1;
            let v = self.unary()?;
            return Ok(Value::Num(-v.as_num()?));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Value> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Value::Num(n)),
            Some(Tok::Str(s)) => Ok(Value::Str(s)),
            Some(Tok::LParen) => {
                let v = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err(Error::Expr("expected ')'".into())),
                }
            }
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.pos += 1;
                    let args = self.args()?;
                    self.call(&name, args)
                } else {
                    self.ctx
                        .vars
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| Error::Expr(format!("unknown variable {name:?}")))
                }
            }
            other => Err(Error::Expr(format!("unexpected token {other:?}"))),
        }
    }

    fn args(&mut self) -> Result<Vec<Value>> {
        let mut args = Vec::new();
        if let Some(Tok::RParen) = self.peek() {
            self.pos += 1;
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                other => return Err(Error::Expr(format!("expected ',' or ')', found {other:?}"))),
            }
        }
        Ok(args)
    }

    fn call(&self, name: &str, args: Vec<Value>) -> Result<Value> {
        let font_of = |args: &[Value], idx: usize| -> String {
            args.get(idx).map(|v| v.as_str()).unwrap_or_else(|| self.ctx.default_font.clone())
        };
        let nums = |args: &[Value]| -> Result<Vec<f64>> { args.iter().map(|v| v.as_num()).collect() };

        match name {
            "text_width" | "text_height" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(Error::Expr(format!("{name}(text, size[, font]) takes 2-3 args")));
                }
                let text = args[0].as_str();
                let size = args[1].as_num()?;
                let font = font_of(&args, 2);
                let m = self.ctx.measurer.measure_line(&text, &font, size);
                Ok(Value::Num(if name == "text_width" { m.width } else { m.height() }))
            }
            "para_width" | "para_height" => {
                if args.len() < 3 || args.len() > 4 {
                    return Err(Error::Expr(format!(
                        "{name}(text, max_width, size[, font]) takes 3-4 args"
                    )));
                }
                let text = args[0].as_str();
                let max_width = args[1].as_num()?;
                let size = args[2].as_num()?;
                let font = font_of(&args, 3);
                let p = self.ctx.measurer.measure_paragraph(&text, max_width, &font, size);
                Ok(Value::Num(if name == "para_width" { p.width } else { p.height }))
            }
            "min" | "max" => {
                let ns = nums(&args)?;
                if ns.is_empty() {
                    return Err(Error::Expr(format!("{name}() needs at least one argument")));
                }
                let v = if name == "min" {
                    ns.into_iter().fold(f64::INFINITY, f64::min)
                } else {
                    ns.into_iter().fold(f64::NEG_INFINITY, f64::max)
                };
                Ok(Value::Num(v))
            }
            "round" | "floor" | "ceil" | "abs" | "sqrt" => {
                if args.len() != 1 {
                    return Err(Error::Expr(format!("{name}() takes exactly one argument")));
                }
                let x = args[0].as_num()?;
                let v = match name {
                    "round" => x.round(),
                    "floor" => x.floor(),
                    "ceil" => x.ceil(),
                    "abs" => x.abs(),
                    "sqrt" => x.sqrt(),
                    _ => unreachable!(),
                };
                Ok(Value::Num(v))
            }
            _ => Err(Error::Expr(format!("unknown function {name:?}"))),
        }
    }
}

fn add(left: &Value, right: &Value) -> Result<Value> {
    match (left, right) {
        (Value::Num(a), Value::Num(b)) => Ok(Value::Num(a + b)),
        // If either side is a string, concatenate (with numeric stringification).
        _ => Ok(Value::Str(format!("{}{}", left.as_str(), right.as_str()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::BasicMeasurer;

    fn ctx_with(vars: HashMap<String, Value>) -> (HashMap<String, Value>, BasicMeasurer) {
        (vars, BasicMeasurer::default())
    }

    fn eval_str(s: &str, vars: &HashMap<String, Value>) -> Result<Value> {
        let m = BasicMeasurer::default();
        let ctx = EvalContext {
            vars,
            measurer: &m,
            default_font: "sans-serif".into(),
            default_size: 14.0,
        };
        eval(s, &ctx)
    }

    #[test]
    fn arithmetic_precedence() {
        let vars = HashMap::new();
        assert_eq!(eval_str("2 + 3 * 4", &vars).unwrap(), Value::Num(14.0));
        assert_eq!(eval_str("(2 + 3) * 4", &vars).unwrap(), Value::Num(20.0));
        assert_eq!(eval_str("-5 + 2", &vars).unwrap(), Value::Num(-3.0));
        assert_eq!(eval_str("10 % 3", &vars).unwrap(), Value::Num(1.0));
    }

    #[test]
    fn variables_resolve() {
        let mut vars = HashMap::new();
        vars.insert("pad".to_string(), Value::Num(8.0));
        vars.insert("w".to_string(), Value::Num(100.0));
        assert_eq!(eval_str("w + pad * 2", &vars).unwrap(), Value::Num(116.0));
    }

    #[test]
    fn unknown_variable_errors() {
        let vars = HashMap::new();
        assert!(matches!(eval_str("nope", &vars), Err(Error::Expr(_))));
    }

    #[test]
    fn math_functions() {
        let vars = HashMap::new();
        assert_eq!(eval_str("max(1, 9, 3)", &vars).unwrap(), Value::Num(9.0));
        assert_eq!(eval_str("min(4, 2, 8)", &vars).unwrap(), Value::Num(2.0));
        assert_eq!(eval_str("ceil(2.1)", &vars).unwrap(), Value::Num(3.0));
        assert_eq!(eval_str("floor(2.9)", &vars).unwrap(), Value::Num(2.0));
        assert_eq!(eval_str("abs(-7)", &vars).unwrap(), Value::Num(7.0));
    }

    #[test]
    fn text_measurement_functions() {
        let vars = HashMap::new();
        // BasicMeasurer: width = chars * 0.6 * size. "abcd" at 10 -> 24.
        assert_eq!(eval_str("text_width(\"abcd\", 10)", &vars).unwrap(), Value::Num(24.0));
        // height = 1.0 * size for a single line (0.8 + 0.2).
        assert_eq!(eval_str("text_height(\"x\", 20)", &vars).unwrap(), Value::Num(20.0));
        // Composed with arithmetic.
        assert_eq!(
            eval_str("text_width(\"ab\", 10) + 4", &vars).unwrap(),
            Value::Num(16.0)
        );
    }

    #[test]
    fn paragraph_measurement_function() {
        let vars = HashMap::new();
        // Two hard lines -> height = 2 * 1.2 * 10 = 24.
        let v = eval_str("para_height(\"a\nb\", 1000, 10)", &vars).unwrap();
        assert_eq!(v, Value::Num(24.0));
    }

    #[test]
    fn string_concatenation() {
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), Value::Num(3.0));
        assert_eq!(
            eval_str("\"R\" + n", &vars).unwrap(),
            Value::Str("R3".to_string())
        );
    }

    #[test]
    fn division_by_zero_errors() {
        let vars = HashMap::new();
        assert!(matches!(eval_str("1 / 0", &vars), Err(Error::Expr(_))));
    }

    #[test]
    fn trailing_input_errors() {
        let vars = HashMap::new();
        assert!(matches!(eval_str("1 2", &vars), Err(Error::Expr(_))));
    }

    #[test]
    fn format_num_compact() {
        assert_eq!(format_num(5.0), "5");
        assert_eq!(format_num(5.5), "5.5");
        let _ = ctx_with(HashMap::new());
    }
}
