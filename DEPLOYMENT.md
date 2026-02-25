# Deployment Guide

This guide covers the deployment of the Remitwise Contracts suite to the Stellar network using Soroban.

## Prerequisites

- Soroban CLI installed (version 21.0.0 or compatible)
- Stellar account with sufficient XLM for deployment
- Rust toolchain for contract compilation
- Network access (Testnet or Mainnet)

> **Note**: For detailed version compatibility information, see the [Compatibility section in README.md](README.md#compatibility) and the [Upgrade Guide](UPGRADE_GUIDE.md).

## Contracts Overview

The Remitwise Contracts suite consists of five main contracts:

1. **Remittance Split** - Manages fund allocation percentages
2. **Bill Payments** - Handles bill creation and payment tracking
3. **Insurance** - Manages insurance policies and premiums
4. **Savings Goals** - Tracks savings goals and fund management
5. **Reporting** - Cross-contract aggregation and financial reporting

## Deployment Steps

### 1. Environment Setup

> **Version Check**: Ensure you're using compatible versions. See [README Compatibility section](README.md#compatibility) for tested versions.

```bash
# Install Soroban CLI (if not already installed)
# Use version 21.0.0 or compatible
cargo install --locked --version 21.0.0 soroban-cli

# Verify installation
soroban version

# Configure network
soroban config network add testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"

soroban config network add mainnet \
  --rpc-url https://soroban-rpc.mainnet.stellar.org:443 \
  --network-passphrase "Public Global Stellar Network ; September 2015"
```

### 2. Build Contracts

```bash
# Build all contracts
cd bill_payments
soroban contract build

cd ../insurance
soroban contract build

cd ../remittance_split
soroban contract build

cd ../savings_goals
soroban contract build

cd ../reporting
soroban contract build
```

### 3. Deploy to Testnet

#### Create Deployer Identity

```bash
# Create or import deployer identity
soroban keys generate deployer
# Or import existing: soroban keys import deployer <secret_key>
```

#### Fund Deployer Account

```bash
# Get deployer address
soroban keys address deployer

# Fund the account using Stellar Laboratory or friendbot
# For testnet: https://laboratory.stellar.org/#account-creator?network=testnet
```

#### Deploy Contracts

```bash
# Set network
soroban config network testnet

# Deploy Remittance Split contract
cd remittance_split
REMittance_SPLIT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source deployer \
  --network testnet)

echo "Remittance Split deployed: $REMittance_SPLIT_ID"

# Deploy Bill Payments contract
cd ../bill_payments
BILL_PAYMENTS_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --source deployer \
  --network testnet)

echo "Bill Payments deployed: $BILL_PAYMENTS_ID"

# Deploy Insurance contract
cd ../insurance
INSURANCE_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/insurance.wasm \
  --source deployer \
  --network testnet)

echo "Insurance deployed: $INSURANCE_ID"

# Deploy Savings Goals contract
cd ../savings_goals
SAVINGS_GOALS_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/savings_goals.wasm \
  --source deployer \
  --network testnet)

echo "Savings Goals deployed: $SAVINGS_GOALS_ID"

# Deploy Reporting contract (must be deployed last)
cd ../reporting
REPORTING_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/reporting.wasm \
  --source deployer \
  --network testnet)

echo "Reporting deployed: $REPORTING_ID"
```

### 4. Initialize Contracts

#### Initialize Savings Goals

```bash
# Initialize storage
soroban contract invoke \
  --id $SAVINGS_GOALS_ID \
  --source deployer \
  --network testnet \
  -- \
  init
```

#### Initialize Reporting Contract

```bash
# Initialize with admin address
ADMIN_ADDRESS="GA..."  # Your admin address

soroban contract invoke \
  --id $REPORTING_ID \
  --source deployer \
  --network testnet \
  -- \
  init \
  --admin $ADMIN_ADDRESS

# Configure contract addresses
soroban contract invoke \
  --id $REPORTING_ID \
  --source deployer \
  --network testnet \
  -- \
  configure_addresses \
  --caller $ADMIN_ADDRESS \
  --remittance_split $REMittance_SPLIT_ID \
  --savings_goals $SAVINGS_GOALS_ID \
  --bill_payments $BILL_PAYMENTS_ID \
  --insurance $INSURANCE_ID \
  --family_wallet "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"  # Family wallet address
```

### 5. Configuration

#### Set Up Remittance Split (Example)

```bash
# Initialize split configuration for a user
# First, get user address
USER_ADDRESS="GA..."

# Initialize with 50% spending, 30% savings, 15% bills, 5% insurance
soroban contract invoke \
  --id $REMittance_SPLIT_ID \
  --source deployer \
  --network testnet \
  -- \
  initialize_split \
  --owner $USER_ADDRESS \
  --spending_percent 50 \
  --savings_percent 30 \
  --bills_percent 15 \
  --insurance_percent 5
```

## Network Configuration

### Testnet Configuration

- RPC URL: `https://soroban-testnet.stellar.org:443`
- Network Passphrase: `Test SDF Network ; September 2015`
- Friendbot: `https://friendbot.stellar.org`

### Mainnet Configuration

- RPC URL: `https://soroban-rpc.mainnet.stellar.org:443`
- Network Passphrase: `Public Global Stellar Network ; September 2015`

## Contract Addresses

After deployment, record the contract IDs:

```bash
# Save contract addresses to a file
cat > contract-addresses.txt << EOF
REMittance_SPLIT_ID=$REMittance_SPLIT_ID
BILL_PAYMENTS_ID=$BILL_PAYMENTS_ID
INSURANCE_ID=$INSURANCE_ID
SAVINGS_GOALS_ID=$SAVINGS_GOALS_ID
REPORTING_ID=$REPORTING_ID
EOF
```

## Testing Deployment

### Basic Functionality Test

```bash
# Test remittance split calculation
soroban contract invoke \
  --id $REMittance_SPLIT_ID \
  --source deployer \
  --network testnet \
  -- \
  calculate_split \
  --total_amount 1000000000  # 100 XLM in stroops
```

### Integration Test

Create a complete user workflow:

1. Set up remittance split
2. Create savings goals
3. Create insurance policies
4. Create bills
5. Simulate remittance processing
6. Generate financial health report

```bash
# Generate a comprehensive financial health report
USER_ADDRESS="GA..."

soroban contract invoke \
  --id $REPORTING_ID \
  --source deployer \
  --network testnet \
  -- \
  get_financial_health_report \
  --user $USER_ADDRESS \
  --total_remittance 10000000000 \
  --period_start 1704067200 \
  --period_end 1706745600
```

## Troubleshooting

### Common Issues

#### Insufficient Funds

```
Error: insufficient funds
```

**Solution:** Ensure deployer account has enough XLM (at least 10 XLM recommended)

#### Build Failures

```
Error: failed to build contract
```

**Solution:** Check Rust toolchain and dependencies

```bash
rustup update
cargo clean
cargo build
```

#### Network Connection

```
Error: network error
```

**Solution:** Verify network configuration and internet connection

### Contract Verification

Verify deployed contracts:

```bash
# Check contract exists
soroban contract info --id $CONTRACT_ID --network testnet

# Test basic functionality
soroban contract invoke --id $CONTRACT_ID --network testnet -- get_split
```

## Production Deployment

For mainnet deployment:

1. Use mainnet network configuration
2. Fund deployer account with real XLM
3. Test thoroughly on testnet first
4. Consider multi-sig for deployer account
5. Document all contract addresses
6. Set up monitoring and alerts

## Cost Estimation

Approximate deployment costs (Testnet):

- Contract deployment: ~10 XLM per contract
- Storage operations: ~0.1 XLM per operation
- Function calls: ~0.01 XLM per call

## Maintenance

### Upgrading Contracts

When upgrading to a new Soroban version:

1. Review the [Upgrade Guide](UPGRADE_GUIDE.md) for detailed instructions
2. Test on testnet with new SDK version
3. Deploy new contract version
4. Migrate data if needed (see [data_migration contract](data_migration/))
5. Update client applications
6. Test thoroughly before mainnet deployment
7. Decommission old contract

For breaking changes and version-specific migration steps, see [UPGRADE_GUIDE.md](UPGRADE_GUIDE.md#version-specific-migration-guides).

### Monitoring

- Monitor contract storage usage
- Track function call volumes
- Set up alerts for failures
- Regular backup of contract states

### Off-chain reconciliation

To keep off-chain systems (databases, reporting, Anchor webhooks) aligned with on-chain state, follow the [Off-Chain Reconciliation Process](docs/off-chain-reconciliation.md). It covers data sources (events, webhooks, off-chain DB), a reconciliation checklist, and example flows for event-driven ingestion and periodic reconciliation.
