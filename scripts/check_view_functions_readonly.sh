#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# View Function Read-Only Check
# ==============================================================================
#
# THREAT MODEL:
# -------------
# View functions (get_*, is_*, read-only queries) are expected to be pure read
# operations with no side effects. If a view function can write to storage, an
# attacker gains the ability to:
#
# 1. HIDDEN STATE CHANGES: Modify contract state through seemingly innocuous
#    read operations, bypassing audit trails and event logs designed for
#    write operations.
#
# 2. AUDIT TRAIL BYPASS: Since view functions are often called without auth
#    checks (or with relaxed auth), they can be exploited to alter state
#    without triggering the proper authorization flow or emitting expected
#    events.
#
# 3. DENIAL OF SERVICE: An attacker could exhaust storage resources through
#    repeated "read" calls that actually write data, degrading contract
#    performance or causing unexpected failures.
#
# 4. QUERY MANIPULATION: Off-chain indexers and frontends expect view functions
#    to be side-effect free. Storage writes in view functions can cause
#    inconsistencies between on-chain state and indexed data.
#
# 5. REPLAY ATTACKS: If a view function writes based on its inputs, an attacker
#    can craft queries that manipulate state in ways the contract owner did not
#    authorize.
#
# DEFENSE-IN-DEPTH:
# -----------------
# This check enforces a "view functions are read-only" invariant at the source
# level, catching violations before they reach production. It complements:
# - Authorization checks (prevent unauthorized callers)
# - Event logging (ensure state changes are auditable)
# - Gas metering (prevent resource exhaustion)
#
# ==============================================================================

# Contract directories to check
contracts=(
  remittance_split
  savings_goals
  bill_payments
  insurance
  family_wallet
  orchestrator
  reporting
  emergency_killswitch
  data_migration
)

# Storage write patterns to detect
# These patterns specifically indicate storage MUTATIONS (writes/deletes/ttl changes).
# env.storage().*.get() calls are reads and intentionally excluded.
# env.storage().*.set() and .remove() are writes; .extend_ttl() is a TTL mutation.
storage_write_patterns=(
  'env\.storage\(\)\.[a-z]*\(\)\.set\('
  'env\.storage\(\)\.[a-z]*\(\)\.remove\('
  'env\.storage\(\)\.[a-z]*\(\)\.extend_ttl\('
)

# View function naming patterns
# Functions matching these patterns are expected to be read-only
view_function_prefixes=(
  "get_"
  "is_"
)

violations=()

echo "Checking view functions for storage writes..."
echo ""

for contract in "${contracts[@]}"; do
  contract_path="${contract}/src/lib.rs"
  
  if [ ! -f "$contract_path" ]; then
    echo "  ⚠️  Skipping $contract: file not found"
    continue
  fi
  
  echo "  Checking $contract..."
  
  # Use ripgrep if available for better performance, otherwise grep
  if command -v rg &> /dev/null; then
    grep_cmd="rg"
  else
    grep_cmd="grep"
  fi
  
  # Find all view function definitions
  for prefix in "${view_function_prefixes[@]}"; do
    # Find lines with "pub fn get_" or "pub fn is_"
    view_funcs=$(grep -n "pub fn ${prefix}" "$contract_path" 2>/dev/null || true)
    
    if [ -z "$view_funcs" ]; then
      continue
    fi
    
    # For each view function, check if it contains storage write operations
    while IFS= read -r line_info; do
      if [ -z "$line_info" ]; then
        continue
      fi
      
      line_num=$(echo "$line_info" | cut -d: -f1)
      func_line=$(echo "$line_info" | cut -d: -f2-)
      
      # Extract function name
      func_name=$(echo "$func_line" | sed 's/.*pub fn \([a-z_0-9]*\).*/\1/')
      
      # Extract the function body (from current line to next "pub fn" or end of impl)
      # Use awk to find the function's closing brace based on balanced braces
      # Simple approach: get the next 150 lines (most functions are shorter)
      end_line=$((line_num + 150))
      func_body=$(sed -n "${line_num},${end_line}p" "$contract_path")
      
      # Check for storage write patterns
      for pattern in "${storage_write_patterns[@]}"; do
        if echo "$func_body" | grep -qE "$pattern"; then
          # Get the specific lines that match
          matches=$(echo "$func_body" | grep -nE "$pattern" | head -3)
          
          while IFS= read -r match; do
            if [ -z "$match" ]; then
              continue
            fi
            
            rel_line=$(echo "$match" | cut -d: -f1)
            actual_line=$((line_num + rel_line - 1))
            match_text=$(echo "$match" | cut -d: -f2- | sed 's/^[[:space:]]*//')
            
            violations+=("$contract:$actual_line: Function '${func_name}' writes storage: ${match_text}")
          done <<< "$matches"
        fi
      done
      
    done <<< "$view_funcs"
  done
done

echo ""

if [ ${#violations[@]} -eq 0 ]; then
  echo "✅ All view functions are read-only (no storage writes detected)"
  exit 0
else
  echo "❌ ERROR: Found view functions that write to storage:"
  echo ""
  for violation in "${violations[@]}"; do
    echo "  $violation"
  done
  echo ""
  echo "View functions (get_*, is_*) must be read-only and should not modify storage."
  echo "Move storage writes to separate mutation functions with proper authorization."
  exit 1
fi
