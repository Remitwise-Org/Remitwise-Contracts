#!/bin/bash
# Update baseline with current gas measurements
# Usage: ./scripts/update_baseline.sh [--force]

set -e

BASELINE="benchmarks/baseline.json"
CURRENT="gas_results.json"
FORCE=false

# Parse arguments
if [ "$1" = "--force" ]; then
    FORCE=true
fi

# Check if current results exist
if [ ! -f "$CURRENT" ]; then
    echo "Error: Current results not found. Run ./scripts/run_gas_benchmarks.sh first"
    exit 1
fi

# Check if baseline exists and warn
if [ -f "$BASELINE" ] && [ "$FORCE" = false ]; then
    echo "⚠️  Baseline already exists at $BASELINE"
    echo ""
    echo "Current baseline:"
    cat "$BASELINE"
    echo ""
    echo "New measurements:"
    cat "$CURRENT"
    echo ""
    read -p "Do you want to update the baseline? (yes/no): " -r
    if [[ ! $REPLY =~ ^[Yy][Ee][Ss]$ ]]; then
        echo "Baseline update cancelled"
        exit 0
    fi
fi

# Backup existing baseline
if [ -f "$BASELINE" ]; then
    TIMESTAMP=$(date +%Y%m%d_%H%M%S)
    BACKUP="benchmarks/history/baseline_${TIMESTAMP}.json"
    mkdir -p benchmarks/history
    cp "$BASELINE" "$BACKUP"
    echo "✓ Backed up existing baseline to $BACKUP"
fi

# Update baseline
cp "$CURRENT" "$BASELINE"
echo "✓ Updated baseline at $BASELINE"
echo ""
echo "New baseline:"
cat "$BASELINE"
echo ""
echo "Don't forget to commit the updated baseline:"
echo "  git add $BASELINE"
echo "  git commit -m 'Update gas baseline after [describe changes]'"
