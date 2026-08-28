#!/usr/bin/env bash
# scripts/verify_toolchain.sh
#
# Verifies that the local Rust toolchain satisfies the requirements specified
# in rust-toolchain.toml. Exits with status 1 and a remediation hint if any
# check fails. Idempotent — safe to run multiple times.
#
# Usage:
#   bash scripts/verify_toolchain.sh
#
# Closes #1336

set -euo pipefail

PASS=0
FAIL=1
all_ok=true

pass() { echo "✅ $*"; }
fail() { echo "❌ $*"; all_ok=false; }

# ---------------------------------------------------------------------------
# 1. rustc available and on the stable channel
# ---------------------------------------------------------------------------
if ! command -v rustc &>/dev/null; then
  fail "rustc not found. Install rustup: https://rustup.rs"
  all_ok=false
else
  rustc_version=$(rustc --version 2>&1)
  if echo "$rustc_version" | grep -q "stable\|nightly\|beta"; then
    # The active toolchain is controlled by rust-toolchain.toml; we just
    # confirm rustc is callable and note the version.
    channel=$(rustup show active-toolchain 2>/dev/null | grep -oE 'stable|nightly|beta' | head -1 || echo "unknown")
    if [ "$channel" = "stable" ]; then
      pass "rustc stable found: $rustc_version"
    else
      fail "Active toolchain channel is '$channel', expected 'stable'."
      echo "     Hint: run 'rustup override set stable' or ensure rust-toolchain.toml is present."
    fi
  else
    pass "rustc found: $rustc_version (channel detection inconclusive, proceeding)"
  fi
fi

# ---------------------------------------------------------------------------
# 2. Required WASM targets
# ---------------------------------------------------------------------------
check_target() {
  local target="$1"
  if rustup target list --installed 2>/dev/null | grep -q "^${target}$"; then
    pass "target ${target} installed"
  else
    fail "target ${target} not installed."
    echo "     Hint: run 'rustup target add ${target}'"
    all_ok=false
  fi
}

check_target "wasm32-unknown-unknown"
check_target "wasm32v1-none"

# ---------------------------------------------------------------------------
# 3. Required components
# ---------------------------------------------------------------------------
check_component() {
  local component="$1"
  if rustup component list --installed 2>/dev/null | grep -q "^${component}"; then
    pass "component ${component} installed"
  else
    fail "component ${component} not installed."
    echo "     Hint: run 'rustup component add ${component}'"
    all_ok=false
  fi
}

check_component "rustfmt"
check_component "clippy"

# ---------------------------------------------------------------------------
# 4. Quick workspace check (no WASM compile — just host target)
# ---------------------------------------------------------------------------
# Scope to the root of the repo regardless of where the script is invoked.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ "$all_ok" = true ]; then
  echo ""
  echo "Running cargo check (host target)..."
  if cargo check --quiet --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1; then
    pass "cargo check passed"
  else
    fail "cargo check failed. Fix compilation errors before contributing."
    all_ok=false
  fi
else
  echo ""
  echo "⚠️  Skipping cargo check because earlier checks failed."
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
if [ "$all_ok" = true ]; then
  echo "✅ All toolchain checks passed."
  exit $PASS
else
  echo "❌ One or more toolchain checks failed. See hints above."
  exit $FAIL
fi
