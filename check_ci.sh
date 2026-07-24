#!/bin/bash
set -e

# Ensure Cargo.lock exists before validation (CI may not have it checked in)
if [ ! -f Cargo.lock ]; then
  echo "Cargo.lock not found — generating one..."
  cargo generate-lockfile
fi

# Pin ed25519-dalek to v2.x — soroban-env-host v21.2.1 specifies
# ">=2.0.0" which resolves to v3.0.0 on fresh lockfiles, breaking
# its testutils code that uses rand_chacha v0.3 / rand_core v0.6.
if cargo tree -i "ed25519-dalek@3.0.0" &>/dev/null; then
  echo "Pinning ed25519-dalek to v2.2.0 (v3.0.0 incompatible with soroban-env-host)..."
  cargo update -p "ed25519-dalek@3.0.0" --precise 2.2.0
fi

echo "Validating Cargo.lock soroban-sdk version..."
python3 scripts/validate_lockfile.py

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

echo "Running dependency check (GPL & Yanked Crates)..."
DENY_BIN=""
if [ -x "$HOME/.cargo/bin/cargo-deny" ]; then
    DENY_BIN="$HOME/.cargo/bin/cargo-deny"
elif command -v cargo-deny &> /dev/null; then
    DENY_BIN="cargo-deny"
else
    echo "❌ cargo-deny not found in ~/.cargo/bin or PATH. Please install cargo-deny."
    exit 1
fi
$DENY_BIN check

echo "Running gas benchmarks..."
./scripts/run_gas_benchmarks.sh

echo "Running workspace invariant checks..."
python3 scripts/check_workspace_invariants.py

echo "Running cross-contract invariant checks..."
python3 scripts/verify_cross_contract_invariants.py

echo "Checking feature flag consistency..."
if command -v python3 >/dev/null 2>&1; then
  python3 scripts/check_features.py
elif command -v python >/dev/null 2>&1; then
  python scripts/check_features.py
else
  echo "Error: Python is not installed (required by scripts/check_features.py)"
  exit 1
fi

echo "Checking for unsafe code outside soroban-sdk..."
if command -v python3 >/dev/null 2>&1; then
  python3 scripts/check_unsafe.py
else
  python scripts/check_unsafe.py
fi

echo "✅ All checks passed!"