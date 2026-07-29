#!/usr/bin/env bash
# Issue #1559 – surface the WASM size delta on every build.
#
# Compares each contract's current WASM byte size (via collect_wasm_sizes.sh)
# against the committed baseline and prints a per-contract delta table, so a
# size regression is visible in the build output the moment it happens rather
# than at deploy time.
#
# Usage:
#   ./scripts/wasm_size_delta.sh            # print the delta table
#   ./scripts/wasm_size_delta.sh --update   # rewrite the baseline from current sizes
#
# This is a DX surface, not a gate: it always exits 0 (except on missing
# tooling), mirroring how compare_gas_results.sh reports gas movement.
# Refresh the baseline deliberately — in the same PR that changes a
# contract's size — with --update, mirroring update_baseline.sh.
set -euo pipefail

BASELINE="benchmarks/wasm_size_baseline.json"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

current=$("$SCRIPT_DIR/collect_wasm_sizes.sh")

if [ "${1:-}" = "--update" ]; then
    printf '%s\n' "$current" > "$BASELINE"
    echo "✅ WASM size baseline updated at $BASELINE"
    exit 0
fi

if [ ! -f "$BASELINE" ]; then
    echo "⚠️  No baseline at $BASELINE — run './scripts/wasm_size_delta.sh --update' after a clean build"
    exit 0
fi

python3 - "$BASELINE" <<PYEOF
import json, sys
baseline = json.load(open(sys.argv[1]))
current = json.loads('''$current''')

print()
print("WASM size delta vs benchmarks/wasm_size_baseline.json")
print(f"{'contract':<22} {'baseline':>10} {'current':>10} {'delta':>9} {'pct':>8}")
print("-" * 64)
total_b = total_c = 0
for name in sorted(set(baseline) | set(current)):
    b = baseline.get(name, 0)
    c = current.get(name, 0)
    total_b += b; total_c += c
    if b == 0 and c == 0:
        print(f"{name:<22} {'-':>10} {'-':>10} {'-':>9} {'-':>8}")
        continue
    delta = c - b
    pct = f"{100*delta/b:+.2f}%" if b else "new"
    marker = "" if delta == 0 else ("  ▲" if delta > 0 else "  ▼")
    print(f"{name:<22} {b:>10} {c:>10} {delta:>+9} {pct:>8}{marker}")
print("-" * 64)
tdelta = total_c - total_b
tpct = f"{100*tdelta/total_b:+.2f}%" if total_b else "n/a"
print(f"{'TOTAL':<22} {total_b:>10} {total_c:>10} {tdelta:>+9} {tpct:>8}")
print()
PYEOF
