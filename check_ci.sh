#!/bin/bash
set -e

echo "Building WASM..."
cargo build --release --target wasm32-unknown-unknown

echo "Running tests..."
cargo test --all-features

echo "Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "Checking format..."
cargo fmt --all -- --check

echo "Running audit..."
cargo audit --deny warnings

echo "Running gas benchmarks..."
./scripts/run_gas_benchmarks.sh

echo "✅ All checks passed!"