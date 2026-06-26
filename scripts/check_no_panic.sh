#!/usr/bin/env bash
set -euo pipefail

# Ensure cargo-expand is installed and available on PATH.
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo-expand >/dev/null 2>&1; then
  echo "Installing cargo-expand..."
  cargo install --locked cargo-expand
fi

packages=(
  remittance_split
  insurance
  orchestrator
  emergency_killswitch
)

for package in "${packages[@]}"; do
  echo "Inspecting expanded production lib for package: $package"
  if cargo expand -p "$package" --lib 2>/dev/null | grep -n "panic!"; then
    echo "\nERROR: Found production panic! in expanded lib of package: $package"
    echo "This check ensures panic! is only present in test-only code paths."
    exit 1
  fi
done

echo "✅ No production panic! statements found in expanded contract libs."
