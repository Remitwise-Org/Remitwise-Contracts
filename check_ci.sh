#!/bin/bash
set -e

echo "Building WASM..."
cargo build --release --target wasm32-unknown-unknown

echo "Running tests..."
cargo test --all-features

echo "Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "Running clippy unwrap/expect ban (SC-054)..."
cargo clippy --workspace --lib -- -D clippy::unwrap_used -D clippy::expect_used

echo "Checking format..."
cargo fmt --all -- --check

echo "Running audit..."
cargo audit --deny warnings

echo "Running gas benchmarks..."
./scripts/run_gas_benchmarks.sh

echo "Running cross-contract invariant checks..."
python3 scripts/verify_cross_contract_invariants.py

echo "✅ All checks passed!"