#!/usr/bin/env bash
# Output a JSON object mapping each Soroban contract name to its WASM byte size.
# Contracts that have not been built (or failed to build) are reported as 0.
# Usage: ./scripts/collect_wasm_sizes.sh [output_file]
#   output_file defaults to stdout ("-")
set -euo pipefail

CONTRACTS=(
    remittance_split
    savings_goals
    bill_payments
    insurance
    family_wallet
    orchestrator
    data_migration
    emergency_killswitch
    reporting
)

OUTPUT="${1:--}"

json="{"
sep=""
for c in "${CONTRACTS[@]}"; do
    wasm_path="target/wasm32-unknown-unknown/release/${c}.wasm"
    if [ -f "$wasm_path" ]; then
        size=$(wc -c < "$wasm_path" | tr -d ' ')
    else
        size=0
    fi
    json="${json}${sep}\"${c}\":${size}"
    sep=","
done
json="${json}}"

if [ "$OUTPUT" = "-" ]; then
    printf '%s\n' "$json"
else
    printf '%s\n' "$json" > "$OUTPUT"
fi
