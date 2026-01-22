# Deployment Guide

This guide provides step-by-step instructions for building and deploying the RemitWise smart contracts to Stellar networks.

## Prerequisites

### System Requirements
- Rust (latest stable version)
- Cargo package manager
- Git

### Stellar Tools
- Soroban CLI (version 21.0.0 or later)
- Stellar account with XLM for deployment fees

### Network Access
- Access to Stellar testnet or mainnet
- Sufficient XLM in deployment account (minimum 10 XLM recommended)

## Installation

### 1. Install Soroban CLI

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Verify installation
soroban --version
```

### 2. Clone Repository

```bash
git clone https://github.com/your-org/remitwise-contracts.git
cd remitwise-contracts
```

### 3. Install Dependencies

```bash
# Install Rust target for WebAssembly
rustup target add wasm32-unknown-unknown

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

## Configuration

### Network Setup

Create a `.soroban` directory in your home folder and configure networks:

```bash
# Create Soroban config directory
mkdir -p ~/.soroban

# Configure testnet (create network.json)
cat > ~/.soroban/network.json << EOF
{
  "testnet": {
    "rpcUrl": "https://soroban-testnet.stellar.org",
    "networkPassphrase": "Test SDF Network ; September 2015"
  },
  "mainnet": {
    "rpcUrl": "https://soroban.stellar.org",
    "networkPassphrase": "Public Global Stellar Network ; September 2015"
  }
}
EOF
```

### Account Setup

```bash
# Create or import an account
soroban keys generate deployer

# Fund the account (for testnet)
# Visit https://laboratory.stellar.org/#account-creator
# and fund the address shown by: soroban keys address deployer
```

## Building Contracts

### Build All Contracts

```bash
# Build optimized WebAssembly binaries
cargo build --release --target wasm32-unknown-unknown

# Verify builds
ls -la target/wasm32-unknown-unknown/release/*.wasm
```

### Individual Contract Builds

```bash
# Build specific contracts
cd bill_payments && cargo build --release --target wasm32-unknown-unknown
cd ../insurance && cargo build --release --target wasm32-unknown-unknown
cd ../remittance_split && cargo build --release --target wasm32-unknown-unknown
cd ../savings_goals && cargo build --release --target wasm32-unknown-unknown
```

## Deployment

### Deploy to Testnet

```bash
# Set network to testnet
export SOROBAN_NETWORK=testnet

# Deploy contracts (run from project root)
./deploy.sh testnet

# Or deploy individually:
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --source deployer \
  --network testnet
```

### Deploy to Mainnet

```bash
# Set network to mainnet
export SOROBAN_NETWORK=mainnet

# Deploy contracts
./deploy.sh mainnet
```

### Automated Deployment Script

Create a deployment script `deploy.sh`:

```bash
#!/bin/bash

NETWORK=$1
SOURCE_KEY="deployer"

if [ -z "$NETWORK" ]; then
    echo "Usage: $0 <network>"
    echo "Networks: testnet, mainnet"
    exit 1
fi

echo "Deploying to $NETWORK network..."

# Deploy Remittance Split contract
echo "Deploying remittance_split..."
SPLIT_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source $SOURCE_KEY \
  --network $NETWORK)
echo "Remittance Split ID: $SPLIT_ID"

# Deploy Bill Payments contract
echo "Deploying bill_payments..."
BILL_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --source $SOURCE_KEY \
  --network $NETWORK)
echo "Bill Payments ID: $BILL_ID"

# Deploy Insurance contract
echo "Deploying insurance..."
INSURANCE_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/insurance.wasm \
  --source $SOURCE_KEY \
  --network $NETWORK)
echo "Insurance ID: $INSURANCE_ID"

# Deploy Savings Goals contract
echo "Deploying savings_goals..."
SAVINGS_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/savings_goals.wasm \
  --source $SOURCE_KEY \
  --network $NETWORK)
echo "Savings Goals ID: $SAVINGS_ID"

# Save contract IDs
cat > deployed_contracts_$NETWORK.json << EOF
{
  "remittance_split": "$SPLIT_ID",
  "bill_payments": "$BILL_ID",
  "insurance": "$INSURANCE_ID",
  "savings_goals": "$SAVINGS_ID",
  "network": "$NETWORK",
  "deployed_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}
EOF

echo "Deployment complete! Contract IDs saved to deployed_contracts_$NETWORK.json"
```

Make the script executable:
```bash
chmod +x deploy.sh
```

## Contract Initialization

After deployment, initialize contracts with default values:

### Initialize Remittance Split

```bash
# Set default split: 50% spending, 30% savings, 15% bills, 5% insurance
soroban contract invoke \
  --id $SPLIT_ID \
  --source deployer \
  --network testnet \
  -- \
  initialize_split \
  --spending_percent 50 \
  --savings_percent 30 \
  --bills_percent 15 \
  --insurance_percent 5
```

### Verify Deployment

Test basic functionality:

```bash
# Test remittance split calculation
soroban contract invoke \
  --id $SPLIT_ID \
  --source deployer \
  --network testnet \
  -- \
  calculate_split \
  --total_amount 1000
# Expected output: [500, 300, 150, 50]
```

## Environment Variables

Create a `.env` file for easy configuration:

```bash
# Network configuration
SOROBAN_NETWORK=testnet
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Account configuration
SOROBAN_SECRET_KEY=your_secret_key_here

# Contract IDs (after deployment)
REMITTANCE_SPLIT_ID=CA...
BILL_PAYMENTS_ID=CA...
INSURANCE_ID=CA...
SAVINGS_GOALS_ID=CA...
```

## Monitoring and Maintenance

### Check Contract Status

```bash
# Get contract info
soroban contract info --id $CONTRACT_ID --network testnet

# Check account balance
soroban keys balance deployer --network testnet
```

### Update Contracts

For contract updates:

```bash
# Build new version
cargo build --release --target wasm32-unknown-unknown

# Deploy new version (gets new contract ID)
NEW_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/contract.wasm \
  --source deployer \
  --network testnet)

# Migrate data if needed (contract-specific logic required)
```

## Troubleshooting

### Common Issues

1. **Insufficient funds**: Ensure deployment account has enough XLM
2. **Network timeout**: Check network connectivity and RPC URL
3. **Build failures**: Ensure Rust and wasm32 target are properly installed
4. **Contract deployment fails**: Verify WASM file exists and is valid

### Debug Commands

```bash
# Check Soroban version
soroban --version

# List available networks
soroban network ls

# Check account balance
soroban keys balance <account> --network testnet

# View transaction details
soroban tx get <tx_hash> --network testnet
```

### Logs and Debugging

Enable verbose logging:

```bash
export RUST_LOG=soroban=debug
soroban contract deploy --wasm contract.wasm --source deployer --network testnet
```

## Security Considerations

- **Private keys**: Never commit private keys to version control
- **Network selection**: Test thoroughly on testnet before mainnet deployment
- **Access control**: Implement proper authorization in frontend applications
- **Gas fees**: Monitor and optimize contract costs
- **Backup**: Keep backup of contract IDs and deployment information

## Support

For issues or questions:
- Check the [Soroban documentation](https://soroban.stellar.org/docs)
- Review [Stellar Laboratory](https://laboratory.stellar.org/) for testing
- Join the [Stellar Discord](https://discord.gg/stellar) for community support