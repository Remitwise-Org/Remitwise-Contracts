#!/usr/bin/env bash
# Seed script for local/test networks with realistic example data.
# Creates multiple example goals, bills, policies, remittance split, and optionally family members.
# Uses deterministic values for stable IDs when run in the same order on a fresh deployment.
#
# Usage:
#   Export contract IDs and network, then run:
#     ./scripts/seed_local.sh
#   Or pass contract IDs as arguments (in order: remittance_split savings_goals bill_payments insurance [family_wallet]):
#     ./scripts/seed_local.sh $REMITTANCE_SPLIT_ID $SAVINGS_GOALS_ID $BILL_PAYMENTS_ID $INSURANCE_ID
#
# Required env (or args): contract IDs for remittance_split, savings_goals, bill_payments, insurance.
# Optional env: NETWORK (default: testnet), SOURCE (default: deployer), SEED_FAMILY (1 to seed family_wallet), FAMILY_WALLET_ID.

set -e

# CLI: prefer soroban, fall back to stellar
if command -v soroban &>/dev/null; then
  CLI=soroban
elif command -v stellar &>/dev/null; then
  CLI=stellar
else
  echo "Error: Neither soroban nor stellar CLI found. Install Soroban CLI or Stellar CLI."
  exit 1
fi

# Contract IDs: from args or env
REMITTANCE_SPLIT_ID="${1:-$REMITTANCE_SPLIT_ID}"
SAVINGS_GOALS_ID="${2:-$SAVINGS_GOALS_ID}"
BILL_PAYMENTS_ID="${3:-$BILL_PAYMENTS_ID}"
INSURANCE_ID="${4:-$INSURANCE_ID}"
FAMILY_WALLET_ID="${5:-$FAMILY_WALLET_ID}"

NETWORK="${NETWORK:-testnet}"
SOURCE="${SOURCE:-deployer}"
SEED_FAMILY="${SEED_FAMILY:-0}"

if [[ -z "$REMITTANCE_SPLIT_ID" || -z "$SAVINGS_GOALS_ID" || -z "$BILL_PAYMENTS_ID" || -z "$INSURANCE_ID" ]]; then
  echo "Usage: $0 [REMITTANCE_SPLIT_ID SAVINGS_GOALS_ID BILL_PAYMENTS_ID INSURANCE_ID [FAMILY_WALLET_ID]]"
  echo "  Or set env: REMITTANCE_SPLIT_ID, SAVINGS_GOALS_ID, BILL_PAYMENTS_ID, INSURANCE_ID"
  echo "  Optional: NETWORK (default: testnet), SOURCE (default: deployer), SEED_FAMILY (1 to seed family wallet), FAMILY_WALLET_ID"
  exit 1
fi

# Resolve source to address for contract args (required for owner parameter in contract calls)
get_address() {
  if [[ "$CLI" == "soroban" ]]; then
    soroban keys address "$1" 2>/dev/null || true
  else
    stellar keys address "$1" 2>/dev/null || true
  fi
}

OWNER_ADDRESS="${OWNER_ADDRESS:-$(get_address "$SOURCE")}"
if [[ -z "$OWNER_ADDRESS" ]]; then
  echo "Error: Could not resolve owner address. Set OWNER_ADDRESS env or create identity: $CLI keys generate $SOURCE"
  exit 1
fi

invoke() {
  local contract_id="$1"
  shift
  $CLI contract invoke --id "$contract_id" --source "$SOURCE" --network "$NETWORK" -- "$@"
}

echo "Seeding contracts on network: $NETWORK (source: $SOURCE)"

# ---- Savings Goals: init + deterministic goals ----
echo "Initializing savings_goals..."
invoke "$SAVINGS_GOALS_ID" init

echo "Creating savings goals (deterministic)..."
invoke "$SAVINGS_GOALS_ID" create_goal --owner "$OWNER_ADDRESS" --name "Education Fund"    --target_amount 5000000000 --target_date 1767225600
invoke "$SAVINGS_GOALS_ID" create_goal --owner "$OWNER_ADDRESS" --name "Medical Emergency"  --target_amount 2000000000 --target_date 1735689600
invoke "$SAVINGS_GOALS_ID" create_goal --owner "$OWNER_ADDRESS" --name "Home Renovation"   --target_amount 10000000000 --target_date 1798761600

# ---- Bill Payments: deterministic bills ----
echo "Creating bills (deterministic)..."
# Due dates: unix timestamps (e.g. 1735689600 = 2025-01-01), amounts in stroops/smallest unit
invoke "$BILL_PAYMENTS_ID" create_bill --owner "$OWNER_ADDRESS" --name "Electricity"    --amount 150000000 --due_date 1735689600 --recurring true  --frequency_days 30
invoke "$BILL_PAYMENTS_ID" create_bill --owner "$OWNER_ADDRESS" --name "School Fees"    --amount 500000000 --due_date 1738368000 --recurring false --frequency_days 0
invoke "$BILL_PAYMENTS_ID" create_bill --owner "$OWNER_ADDRESS" --name "Internet"       --amount 75000000  --due_date 1735689600 --recurring true  --frequency_days 30

# ---- Insurance: deterministic policies ----
echo "Creating insurance policies (deterministic)..."
invoke "$INSURANCE_ID" create_policy --owner "$OWNER_ADDRESS" --name "Health Micro"   --coverage_type "health"   --monthly_premium 50000000 --coverage_amount 5000000000
invoke "$INSURANCE_ID" create_policy --owner "$OWNER_ADDRESS" --name "Crop Coverage"  --coverage_type "agriculture" --monthly_premium 25000000 --coverage_amount 2000000000

# ---- Remittance Split: get nonce then initialize ----
echo "Initializing remittance split (deterministic 50/30/15/5)..."
NONCE=$($CLI contract invoke --id "$REMITTANCE_SPLIT_ID" --source "$SOURCE" --network "$NETWORK" --send no -- get_nonce --address "$OWNER_ADDRESS" 2>/dev/null | tr -d '\n' || echo "0")
if [[ -z "$NONCE" || "$NONCE" == "null" ]]; then
  NONCE=0
fi
invoke "$REMITTANCE_SPLIT_ID" initialize_split --owner "$OWNER_ADDRESS" --nonce "$NONCE" --spending_percent 50 --savings_percent 30 --bills_percent 15 --insurance_percent 5

# ---- Optional: Family wallet + members ----
if [[ "$SEED_FAMILY" == "1" && -n "$FAMILY_WALLET_ID" ]]; then
  echo "Seeding family wallet (init with owner only; add members manually or extend script with extra addresses)..."
  # Init with owner only (empty initial_members). Requires Vec - CLI may accept empty.
  invoke "$FAMILY_WALLET_ID" init --owner "$OWNER_ADDRESS" --initial_members '[]'
  # add_member(admin, member_address, role, spending_limit): role 2=Admin, 3=Member
  # Skip if no extra member addresses; document in README how to add.
fi

echo "Seed completed successfully."
