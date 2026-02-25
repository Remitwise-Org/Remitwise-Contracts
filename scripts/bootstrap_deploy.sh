#!/usr/bin/env bash
# Bootstrap deployment script for Remitwise Contracts
# Builds, deploys, and initializes all contracts on localnet/testnet
#
# Usage:
#   ./scripts/bootstrap_deploy.sh [NETWORK] [SOURCE]
#
# Arguments:
#   NETWORK - Network to deploy to (default: testnet, options: testnet, mainnet, standalone)
#   SOURCE  - Source identity for deployment (default: deployer)
#
# Environment Variables:
#   SKIP_BUILD - Set to 1 to skip building contracts (default: 0)
#   OUTPUT_FILE - Path to output JSON file (default: ./deployed-contracts.json)
#
# Example:
#   ./scripts/bootstrap_deploy.sh testnet deployer
#   SKIP_BUILD=1 ./scripts/bootstrap_deploy.sh testnet deployer

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
NETWORK="${1:-testnet}"
SOURCE="${2:-deployer}"
SKIP_BUILD="${SKIP_BUILD:-0}"
OUTPUT_FILE="${OUTPUT_FILE:-./deployed-contracts.json}"

# CLI detection
if command -v soroban &>/dev/null; then
  CLI=soroban
elif command -v stellar &>/dev/null; then
  CLI=stellar
else
  echo -e "${RED}Error: Neither soroban nor stellar CLI found.${NC}"
  echo "Install Soroban CLI: cargo install --locked soroban-cli"
  exit 1
fi

echo -e "${BLUE}=== Remitwise Contracts Bootstrap Deployment ===${NC}"
echo -e "Network: ${GREEN}$NETWORK${NC}"
echo -e "Source: ${GREEN}$SOURCE${NC}"
echo -e "CLI: ${GREEN}$CLI${NC}"
echo ""

# Verify source identity exists
get_address() {
  if [[ "$CLI" == "soroban" ]]; then
    soroban keys address "$1" 2>/dev/null || true
  else
    stellar keys address "$1" 2>/dev/null || true
  fi
}

DEPLOYER_ADDRESS=$(get_address "$SOURCE")
if [[ -z "$DEPLOYER_ADDRESS" ]]; then
  echo -e "${RED}Error: Source identity '$SOURCE' not found.${NC}"
  echo "Create identity: $CLI keys generate $SOURCE"
  exit 1
fi

echo -e "${GREEN}✓${NC} Deployer address: $DEPLOYER_ADDRESS"
echo ""

# Build contracts
if [[ "$SKIP_BUILD" == "0" ]]; then
  echo -e "${BLUE}Step 1: Building contracts...${NC}"
  
  CONTRACTS=("remittance_split" "savings_goals" "bill_payments" "insurance" "family_wallet" "reporting" "orchestrator")
  
  for contract in "${CONTRACTS[@]}"; do
    echo -e "  Building ${YELLOW}$contract${NC}..."
    (cd "$contract" && $CLI contract build) || {
      echo -e "${RED}✗ Failed to build $contract${NC}"
      exit 1
    }
  done
  
  echo -e "${GREEN}✓${NC} All contracts built successfully"
  echo ""
else
  echo -e "${YELLOW}Skipping build (SKIP_BUILD=1)${NC}"
  echo ""
fi

# Deploy contracts
echo -e "${BLUE}Step 2: Deploying contracts...${NC}"

deploy_contract() {
  local name=$1
  local wasm_path=$2
  
  echo -e "  Deploying ${YELLOW}$name${NC}..."
  
  local contract_id
  contract_id=$($CLI contract deploy \
    --wasm "$wasm_path" \
    --source "$SOURCE" \
    --network "$NETWORK" 2>&1)
  
  if [[ -z "$contract_id" ]]; then
    echo -e "${RED}✗ Failed to deploy $name${NC}"
    exit 1
  fi
  
  echo -e "${GREEN}✓${NC} $name: $contract_id"
  echo "$contract_id"
}

# Deploy in dependency order
REMITTANCE_SPLIT_ID=$(deploy_contract "remittance_split" "target/wasm32-unknown-unknown/release/remittance_split.wasm")
SAVINGS_GOALS_ID=$(deploy_contract "savings_goals" "target/wasm32-unknown-unknown/release/savings_goals.wasm")
BILL_PAYMENTS_ID=$(deploy_contract "bill_payments" "target/wasm32-unknown-unknown/release/bill_payments.wasm")
INSURANCE_ID=$(deploy_contract "insurance" "target/wasm32-unknown-unknown/release/insurance.wasm")
FAMILY_WALLET_ID=$(deploy_contract "family_wallet" "target/wasm32-unknown-unknown/release/family_wallet.wasm")
REPORTING_ID=$(deploy_contract "reporting" "target/wasm32-unknown-unknown/release/reporting.wasm")
ORCHESTRATOR_ID=$(deploy_contract "orchestrator" "target/wasm32-unknown-unknown/release/orchestrator.wasm")

echo ""

# Initialize contracts
echo -e "${BLUE}Step 3: Initializing contracts...${NC}"

invoke() {
  local contract_id="$1"
  shift
  $CLI contract invoke --id "$contract_id" --source "$SOURCE" --network "$NETWORK" -- "$@"
}

# Initialize Savings Goals
echo -e "  Initializing ${YELLOW}savings_goals${NC}..."
invoke "$SAVINGS_GOALS_ID" init || {
  echo -e "${YELLOW}  Warning: savings_goals init may have already been called${NC}"
}

# Initialize Reporting with contract addresses
echo -e "  Initializing ${YELLOW}reporting${NC}..."
invoke "$REPORTING_ID" init --admin "$DEPLOYER_ADDRESS" || {
  echo -e "${YELLOW}  Warning: reporting init may have already been called${NC}"
}

echo -e "  Configuring ${YELLOW}reporting${NC} addresses..."
invoke "$REPORTING_ID" configure_addresses \
  --caller "$DEPLOYER_ADDRESS" \
  --remittance_split "$REMITTANCE_SPLIT_ID" \
  --savings_goals "$SAVINGS_GOALS_ID" \
  --bill_payments "$BILL_PAYMENTS_ID" \
  --insurance "$INSURANCE_ID" \
  --family_wallet "$FAMILY_WALLET_ID" || {
  echo -e "${YELLOW}  Warning: reporting addresses may have already been configured${NC}"
}

echo -e "${GREEN}✓${NC} Contracts initialized"
echo ""

# Create sensible defaults
echo -e "${BLUE}Step 4: Creating sensible defaults...${NC}"

# Get nonce for remittance split
NONCE=$($CLI contract invoke --id "$REMITTANCE_SPLIT_ID" --source "$SOURCE" --network "$NETWORK" --send no -- get_nonce --address "$DEPLOYER_ADDRESS" 2>/dev/null | tr -d '\n' || echo "0")
if [[ -z "$NONCE" || "$NONCE" == "null" ]]; then
  NONCE=0
fi

# Initialize remittance split with default allocation (50% spending, 30% savings, 15% bills, 5% insurance)
echo -e "  Setting default remittance split (50/30/15/5)..."
invoke "$REMITTANCE_SPLIT_ID" initialize_split \
  --owner "$DEPLOYER_ADDRESS" \
  --nonce "$NONCE" \
  --spending_percent 50 \
  --savings_percent 30 \
  --bills_percent 15 \
  --insurance_percent 5

# Create one example savings goal
echo -e "  Creating example savings goal..."
invoke "$SAVINGS_GOALS_ID" create_goal \
  --owner "$DEPLOYER_ADDRESS" \
  --name "Emergency Fund" \
  --target_amount 5000000000 \
  --target_date 1767225600

# Create one example bill
echo -e "  Creating example bill..."
invoke "$BILL_PAYMENTS_ID" create_bill \
  --owner "$DEPLOYER_ADDRESS" \
  --name "Monthly Utilities" \
  --amount 150000000 \
  --due_date 1735689600 \
  --recurring true \
  --frequency_days 30

# Create one example insurance policy
echo -e "  Creating example insurance policy..."
invoke "$INSURANCE_ID" create_policy \
  --owner "$DEPLOYER_ADDRESS" \
  --name "Health Insurance" \
  --coverage_type "health" \
  --monthly_premium 50000000 \
  --coverage_amount 5000000000

echo -e "${GREEN}✓${NC} Defaults created"
echo ""

# Output contract IDs to JSON file
echo -e "${BLUE}Step 5: Saving contract addresses...${NC}"

cat > "$OUTPUT_FILE" << EOF
{
  "network": "$NETWORK",
  "deployer": "$DEPLOYER_ADDRESS",
  "deployed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "contracts": {
    "remittance_split": "$REMITTANCE_SPLIT_ID",
    "savings_goals": "$SAVINGS_GOALS_ID",
    "bill_payments": "$BILL_PAYMENTS_ID",
    "insurance": "$INSURANCE_ID",
    "family_wallet": "$FAMILY_WALLET_ID",
    "reporting": "$REPORTING_ID",
    "orchestrator": "$ORCHESTRATOR_ID"
  }
}
EOF

echo -e "${GREEN}✓${NC} Contract addresses saved to: $OUTPUT_FILE"
echo ""

# Display summary
echo -e "${GREEN}=== Deployment Complete ===${NC}"
echo ""
echo -e "${BLUE}Contract Addresses:${NC}"
echo -e "  remittance_split: ${GREEN}$REMITTANCE_SPLIT_ID${NC}"
echo -e "  savings_goals:    ${GREEN}$SAVINGS_GOALS_ID${NC}"
echo -e "  bill_payments:    ${GREEN}$BILL_PAYMENTS_ID${NC}"
echo -e "  insurance:        ${GREEN}$INSURANCE_ID${NC}"
echo -e "  family_wallet:    ${GREEN}$FAMILY_WALLET_ID${NC}"
echo -e "  reporting:        ${GREEN}$REPORTING_ID${NC}"
echo -e "  orchestrator:     ${GREEN}$ORCHESTRATOR_ID${NC}"
echo ""
echo -e "${BLUE}Next Steps:${NC}"
echo -e "  1. Load contract addresses from: ${YELLOW}$OUTPUT_FILE${NC}"
echo -e "  2. Integrate with frontend/backend using the contract IDs"
echo -e "  3. Run seed script for more test data: ${YELLOW}./scripts/seed_local.sh${NC}"
echo -e "  4. Test contract interactions"
echo ""
echo -e "${GREEN}✓ Bootstrap deployment successful!${NC}"
