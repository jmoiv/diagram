#!/usr/bin/env bash
# Run the same checks as CI locally: build, test, clippy, fmt.
set -euo pipefail

echo "==> cargo build"
cargo build --workspace

echo "==> cargo test"
cargo test --workspace

echo "==> cargo clippy"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> cargo fmt --check"
cargo fmt --all --check

echo "All checks passed."
