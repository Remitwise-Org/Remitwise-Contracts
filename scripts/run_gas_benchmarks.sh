#!/bin/bash
set -e

# insurance gas_bench is disabled (insurance/tests/gas_bench.rs.disabled)
CONTRACTS=("bill_payments" "savings_goals" "family_wallet" "remittance_split")
OUTPUT_FILE="gas_results.json"
TEMP_FILE=$(mktemp)

echo "Running gas benchmarks..."

# Collect all JSON lines
for contract in "${CONTRACTS[@]}"; do
    echo "Benchmarking $contract..."
    RUST_TEST_THREADS=1 cargo test -p "$contract" --test gas_bench -- --nocapture 2>&1 | \
        grep -o '{"contract":[^}]*}' >> "$TEMP_FILE" || true
done

# Format as JSON array
if [ -s "$TEMP_FILE" ]; then
    echo "[" > "$OUTPUT_FILE"
    awk 'NR > 1 { print "," } { printf "  %s", $0 }' "$TEMP_FILE" >> "$OUTPUT_FILE"
    echo "" >> "$OUTPUT_FILE"
    echo "]" >> "$OUTPUT_FILE"
    
    echo ""
    echo "Gas benchmarks complete. Results:"
    cat "$OUTPUT_FILE"
else
    echo "[]" > "$OUTPUT_FILE"
    echo "No benchmark results found."
fi

rm -f "$TEMP_FILE"
