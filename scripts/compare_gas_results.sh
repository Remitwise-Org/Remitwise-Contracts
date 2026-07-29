#!/bin/bash
# Compare gas benchmark results against a baseline
# Usage: ./scripts/compare_gas_results.sh baseline.json current.json [threshold_percent]

set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <baseline.json> <current.json> [threshold_percent]"
    echo "Example: $0 baseline.json gas_results.json 10"
    echo ""
    echo "If threshold_percent is not provided, uses thresholds from benchmarks/thresholds.json"
    exit 1
fi

BASELINE=$1
CURRENT=$2
THRESHOLD=${3:-}  # Optional override threshold
THRESHOLD_CONFIG="benchmarks/thresholds.json"

if [ ! -f "$BASELINE" ]; then
    echo "Error: Baseline file not found: $BASELINE"
    exit 1
fi

if [ ! -f "$CURRENT" ]; then
    echo "Error: Current results file not found: $CURRENT"
    exit 1
fi

# Function to get threshold for a specific contract/method
get_threshold() {
    local contract=$1
    local method=$2
    local metric=$3  # cpu or mem
    local default_threshold=$4
    
    if [ -f "$THRESHOLD_CONFIG" ] && command -v jq &> /dev/null; then
        # Try method-specific first
        local method_threshold=$(jq -r ".method_specific.\"$method\".${metric}_percent // empty" "$THRESHOLD_CONFIG" 2>/dev/null)
        if [ -n "$method_threshold" ]; then
            echo "$method_threshold"
            return
        fi
        
        # Try contract-specific
        local contract_threshold=$(jq -r ".contract_specific.\"$contract\".${metric}_percent // empty" "$THRESHOLD_CONFIG" 2>/dev/null)
        if [ -n "$contract_threshold" ]; then
            echo "$contract_threshold"
            return
        fi
        
        # Use default from config
        local config_default=$(jq -r ".default.${metric}_percent // empty" "$THRESHOLD_CONFIG" 2>/dev/null)
        if [ -n "$config_default" ]; then
            echo "$config_default"
            return
        fi
    fi
    
    # Fallback to provided default
    echo "$default_threshold"
}

if [ -n "$THRESHOLD" ]; then
    echo "Comparing gas benchmarks (override threshold: ${THRESHOLD}% increase)"
else
    echo "Comparing gas benchmarks (using configured thresholds)"
fi
echo "Baseline: $BASELINE"
echo "Current:  $CURRENT"
echo ""

# Check if jq is available
if ! command -v jq &> /dev/null; then
    echo "⚠️  jq not installed. Install jq for detailed comparison."
    echo "Showing raw results:"
    echo ""
    echo "=== Baseline ==="
    cat "$BASELINE"
    echo ""
    echo "=== Current ==="
    cat "$CURRENT"
    exit 0
fi

REGRESSION=0
TEMP_RESULTS=$(mktemp)

# Compare each benchmark
jq -r '.[] | "\(.contract):\(.method):\(.scenario) \(.cpu) \(.mem)"' "$BASELINE" | while read -r line; do
    KEY=$(echo "$line" | cut -d' ' -f1)
    BASE_CPU=$(echo "$line" | cut -d' ' -f2)
    BASE_MEM=$(echo "$line" | cut -d' ' -f3)
    
    CONTRACT=$(echo "$KEY" | cut -d':' -f1)
    METHOD=$(echo "$KEY" | cut -d':' -f2)
    
    CURR_LINE=$(jq -r ".[] | select(.contract + \":\" + .method + \":\" + .scenario == \"$KEY\") | \"\(.cpu) \(.mem)\"" "$CURRENT")
    
    if [ -n "$CURR_LINE" ]; then
        CURR_CPU=$(echo "$CURR_LINE" | cut -d' ' -f1)
        CURR_MEM=$(echo "$CURR_LINE" | cut -d' ' -f2)
        
        # Skip if baseline is 0 (not yet measured)
        if [ "$BASE_CPU" -eq 0 ] && [ "$BASE_MEM" -eq 0 ]; then
            printf "%-60s BASELINE NOT SET (skipping)\n" "$KEY"
            continue
        fi
        
        # Calculate percentage changes
        if [ "$BASE_CPU" -gt 0 ]; then
            CPU_INCREASE=$(awk "BEGIN {print ($CURR_CPU - $BASE_CPU) / $BASE_CPU * 100}")
        else
            CPU_INCREASE=0
        fi
        
        if [ "$BASE_MEM" -gt 0 ]; then
            MEM_INCREASE=$(awk "BEGIN {print ($CURR_MEM - $BASE_MEM) / $BASE_MEM * 100}")
        else
            MEM_INCREASE=0
        fi
        
        # Get thresholds
        if [ -n "$THRESHOLD" ]; then
            CPU_THRESHOLD=$THRESHOLD
            MEM_THRESHOLD=$THRESHOLD
        else
            CPU_THRESHOLD=$(get_threshold "$CONTRACT" "$METHOD" "cpu" 10)
            MEM_THRESHOLD=$(get_threshold "$CONTRACT" "$METHOD" "mem" 10)
        fi
        
        # Format output
        printf "%-60s CPU: %+.1f%% (threshold: %s%%) MEM: %+.1f%% (threshold: %s%%)\n" \
            "$KEY" "$CPU_INCREASE" "$CPU_THRESHOLD" "$MEM_INCREASE" "$MEM_THRESHOLD"
        
        # Check for regression
        if (( $(echo "$CPU_INCREASE > $CPU_THRESHOLD" | bc -l) )); then
            echo "  ⚠️  CPU REGRESSION DETECTED"
            echo "1" >> "$TEMP_RESULTS"
        fi
        
        if (( $(echo "$MEM_INCREASE > $MEM_THRESHOLD" | bc -l) )); then
            echo "  ⚠️  MEMORY REGRESSION DETECTED"
            echo "1" >> "$TEMP_RESULTS"
        fi
        
        # Highlight improvements
        if (( $(echo "$CPU_INCREASE < -5" | bc -l) )) || (( $(echo "$MEM_INCREASE < -5" | bc -l) )); then
            echo "  ✨ Improvement detected"
        fi
    else
        printf "%-60s NOT FOUND in current results\n" "$KEY"
    fi
done

# Check for new benchmarks not in baseline
echo ""
echo "New benchmarks (not in baseline):"
jq -r '.[] | "\(.contract):\(.method):\(.scenario)"' "$CURRENT" | while read -r key; do
    if ! jq -e ".[] | select(.contract + \":\" + .method + \":\" + .scenario == \"$key\")" "$BASELINE" > /dev/null 2>&1; then
        echo "  + $key"
    fi
done

# Check if any regressions were detected
if [ -f "$TEMP_RESULTS" ] && [ -s "$TEMP_RESULTS" ]; then
    REGRESSION_COUNT=$(wc -l < "$TEMP_RESULTS")
    rm -f "$TEMP_RESULTS"
    echo ""
    echo "❌ Gas regression detected: $REGRESSION_COUNT issue(s) found"
    exit 1
else
    rm -f "$TEMP_RESULTS"
    echo ""
    echo "✅ No significant gas regressions"
fi
