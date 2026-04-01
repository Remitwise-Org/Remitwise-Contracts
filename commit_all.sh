#!/usr/bin/env bash
# commit_all.sh — staged commits for both repos from git status
set -euo pipefail

# ============================================================================
# Repo 1: /workspaces/Amana  (feature/trade-detail-panel)
# ============================================================================
if [[ -d /workspaces/Amana ]]; then
  (
    cd /workspaces/Amana

    git add frontend/package.json frontend/package-lock.json
    git commit -m "chore: update frontend dependencies"

    git add frontend/src/app/globals.css
    git commit -m "style: update global styles"

    git add frontend/src/app/layout.tsx
    git commit -m "feat: update app layout"

    git add frontend/src/app/assets/ frontend/src/components/ frontend/src/types/
    git commit -m "feat: add assets, components, and types"
  )
else
  echo "Skipping /workspaces/Amana (not found)."
fi

# ============================================================================
# Repo 2: /home/jeffersonyouashi/Documents/DRIPS/Remitwise-Contracts
#         (test/error-codes-contracts)
# ============================================================================
(
  cd /home/jeffersonyouashi/Documents/DRIPS/Remitwise-Contracts

  git add bill_payments/src/lib.rs
  git commit -m "fix: restore bill_payments currency handling"

  git add insurance/Cargo.toml
  git commit -m "chore: restore insurance dependencies"

  git add integration_tests/Cargo.toml integration_tests/tests/multi_contract_integration.rs
  git commit -m "test: update integration tests for error codes"

  git add integration_tests/test_snapshots/test_multi_contract_user_flow.1.json
  git commit -m "test: update integration test snapshots"

  git add remittance_split/src/lib.rs
  git commit -m "fix: restore remittance_split upgrade admin handling"

  git add remitwise-common/src/lib.rs
  git commit -m "chore: remove duplicate TTL constants"

  git add savings_goals/src/lib.rs
  git commit -m "fix: restore savings_goals upgrade admin handling"

  git add bill_payments/proptest-regressions/
  git commit -m "test: add bill_payments proptest regressions"
)

echo ""
echo "All commits applied successfully."
