# Remitwise Contracts - Deployment Guide

Complete instructions for deploying Remitwise smart contracts to Stellar networks.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Environment Setup](#environment-setup)
- [Building Contracts](#building-contracts)
- [Deployment Steps](#deployment-steps)
- [Network Configuration](#network-configuration)
- [Verification](#verification)
- [Post-Deployment](#post-deployment)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Tools

- **Rust** 1.70.0 or later
- **Cargo** (bundled with Rust)
- **Soroban CLI** v20.0.0 or later
- **Docker** (optional, for containerized builds)

### System Requirements

- Linux, macOS, or Windows (WSL2)
- 2GB RAM minimum
- 10GB disk space

### Network Requirements

- Internet connection
- Access to Stellar RPC endpoints
- Wallet with sufficient XLM for transaction fees

### Stellar Accounts

- **Deployer Account**: Account with funds for deployment transactions
- **Contract Admin Account**: Account to manage contracts post-deployment

---

## Environment Setup

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
rustup update
```

### 2. Install Soroban CLI

```bash
# Install latest Soroban CLI
cargo install --locked soroban-cli

# Verify installation
soroban version
```

### 3. Clone Repository

```bash
git clone https://github.com/Remitwise-Org/Remitwise-Contracts.git
cd Remitwise-Contracts
```

### 4. Configure Stellar Account

```bash
# Create or import account using Soroban CLI
soroban config identity create deployer

# Or import existing account
soroban config identity fund deployer --network testnet

# Verify account
soroban config identity show deployer
```

### 5. Set Network Configuration

```bash
# Add testnet RPC endpoint
soroban config network add \
  --name testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase 'Test SDF Network ; September 2015'

# Add mainnet RPC endpoint (production only)
soroban config network add \
  --name mainnet \
  --rpc-url https://soroban-mainnet.stellar.org:443 \
  --network-passphrase 'Public Global Stellar Network ; September 2015'

# Set default network
soroban config network use testnet
```

---

## Building Contracts

### 1. Build Single Contract

```bash
# Build Bill Payments contract
cd bill_payments
cargo build --release --target wasm32-unknown-unknown

# Output: target/wasm32-unknown-unknown/release/bill_payments.wasm
```

### 2. Build All Contracts

```bash
# From project root
cd ..

# Build all contracts
for dir in bill_payments family_wallet insurance remittance_split savings_goals; do
  echo "Building $dir..."
  cd $dir
  cargo build --release --target wasm32-unknown-unknown
  cd ..
done
```

### 3. Optimize WASM Binaries

```bash
# Install wasm-opt (optional but recommended)
npm install -g wasm-opt

# Optimize Bill Payments
wasm-opt -Oz \
  bill_payments/target/wasm32-unknown-unknown/release/bill_payments.wasm \
  -o bill_payments/target/wasm32-unknown-unknown/release/bill_payments_opt.wasm

# Use optimized version for deployment
```

### 4. Verify Build Artifacts

```bash
# Check WASM size
du -h bill_payments/target/wasm32-unknown-unknown/release/bill_payments.wasm

# Expected: < 100KB (non-optimized)
# Expected: < 50KB (optimized)
```

---

## Deployment Steps

### Step 1: Upload WASM to Stellar Network

```bash
# Deploy Bill Payments contract
soroban contract deploy \
  --wasm bill_payments/target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --network testnet \
  --identity deployer

# Output: Contract ID (save this!)
# Example: CAA3...4VSA
```

### Step 2: Deploy All Contracts

```bash
#!/bin/bash
# deploy_all.sh

NETWORK="testnet"
IDENTITY="deployer"

declare -A contracts
contracts[bill_payments]="bill_payments"
contracts[family_wallet]="family_wallet"
contracts[insurance]="insurance"
contracts[remittance_split]="remittance_split"
contracts[savings_goals]="savings_goals"

for contract in "${!contracts[@]}"; do
  echo "Deploying $contract..."
  CONTRACT_ID=$(soroban contract deploy \
    --wasm ${contract}/target/wasm32-unknown-unknown/release/${contracts[$contract]}.wasm \
    --network $NETWORK \
    --identity $IDENTITY)
  echo "$contract: $CONTRACT_ID" >> deployed_contracts.txt
done

echo "Deployment complete!"
cat deployed_contracts.txt
```

```bash
chmod +x deploy_all.sh
./deploy_all.sh
```

### Step 3: Record Contract Addresses

Create `contracts.env` file:

```bash
# contracts.env

# Testnet Contracts
BILL_PAYMENTS_ID="CAA3...4VSA"
FAMILY_WALLET_ID="CAB4...5WTB"
INSURANCE_ID="CAC5...6XUC"
REMITTANCE_SPLIT_ID="CAD6...7YVD"
SAVINGS_GOALS_ID="CAE7...8ZWE"

# Network Configuration
NETWORK="testnet"
RPC_URL="https://soroban-testnet.stellar.org:443"
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
```

---

## Network Configuration

### Testnet Configuration

**Use for**: Development and testing

```bash
# Network Details
Network: Test SDF Network
Passphrase: Test SDF Network ; September 2015
RPC: https://soroban-testnet.stellar.org:443
Horizon: https://horizon-testnet.stellar.org

# Get Testnet XLM
# Visit: https://laboratory.stellar.org/
# or use: soroban config identity fund deployer --network testnet
```

### Mainnet Configuration

**Use for**: Production deployment

```bash
# Network Details
Network: Public Global Stellar Network
Passphrase: Public Global Stellar Network ; September 2015
RPC: https://soroban-mainnet.stellar.org:443
Horizon: https://horizon.stellar.org

# WARNING: Requires real XLM for gas fees
# Deployment cost: ~1-2 XLM per contract
```

### Custom Network

```bash
# For private/custom networks
soroban config network add \
  --name custom \
  --rpc-url http://localhost:8000 \
  --network-passphrase "Private Test Network"

soroban config network use custom
```

---

## Verification

### 1. Verify Contract Deployment

```bash
# Check contract exists
soroban contract info CAA3...4VSA --network testnet

# Expected output includes:
# - Contract ID
# - Source Hash
# - Ledger Sequence
```

### 2. Query Contract State

```bash
# Check initial contract state
soroban contract invoke \
  --id CAA3...4VSA \
  --network testnet \
  --identity deployer \
  -- \
  get_unpaid_bills

# Expected: Empty result or error (no bills created yet)
```

### 3. Invoke Contract Function

```bash
# Create first bill to verify working
soroban contract invoke \
  --id CAA3...4VSA \
  --network testnet \
  --identity deployer \
  -- \
  create_bill \
  --name "Test Bill" \
  --amount 1000000 \
  --due_date 1704067200 \
  --recurring false \
  --frequency_days 0

# Expected: Bill ID returned (e.g., 1)
```

### 4. Verify All Contracts

```bash
#!/bin/bash
# verify_deployment.sh

source contracts.env

for contract_name in bill_payments family_wallet insurance remittance_split savings_goals; do
  contract_id_var="${contract_name^^}_ID"
  contract_id=${!contract_id_var}

  echo "Verifying $contract_name..."
  soroban contract info $contract_id --network testnet

  if [ $? -eq 0 ]; then
    echo "✓ $contract_name deployed successfully"
  else
    echo "✗ $contract_name deployment failed"
  fi
done
```

---

## Post-Deployment

### 1. Initialize Contracts

```bash
# Initialize remittance split with default percentages
soroban contract invoke \
  --id CAD6...7YVD \
  --network testnet \
  --identity deployer \
  -- \
  initialize_split \
  --spending_percent 50 \
  --savings_percent 30 \
  --bills_percent 15 \
  --insurance_percent 5

# Expected: true (success)
```

### 2. Setup Admin Accounts

```bash
# Add admin member to family wallet
soroban contract invoke \
  --id CAB4...5WTB \
  --network testnet \
  --identity deployer \
  -- \
  add_member \
  --address <ADMIN_ADDRESS> \
  --name "Admin" \
  --spending_limit 10000000000 \
  --role "admin"
```

### 3. Create Deployment Documentation

Create `DEPLOYMENT_LOG.md`:

```markdown
# Deployment Log

## Date: YYYY-MM-DD

## Network: testnet

## Deployer: deployer@account

### Contracts Deployed

| Contract         | Address     | Status | Verification |
| ---------------- | ----------- | ------ | ------------ |
| Bill Payments    | CAA3...4VSA | ✓      | Working      |
| Family Wallet    | CAB4...5WTB | ✓      | Working      |
| Insurance        | CAC5...6XUC | ✓      | Working      |
| Remittance Split | CAD6...7YVD | ✓      | Initialized  |
| Savings Goals    | CAE7...8ZWE | ✓      | Working      |

### Transaction Hashes

- Bill Payments: TX_HASH_1
- Family Wallet: TX_HASH_2
- Insurance: TX_HASH_3
- Remittance Split: TX_HASH_4
- Savings Goals: TX_HASH_5

### Notes

- All contracts deployed successfully
- Test transactions executed
- Ready for integration testing
```

### 4. Backup Contract Information

```bash
# Export contract metadata
soroban contract inspect CAA3...4VSA --network testnet > bill_payments_metadata.json

# Save all addresses
cat >> contracts_backup.txt << EOF
Bill Payments: CAA3...4VSA
Family Wallet: CAB4...5WTB
Insurance: CAC5...6XUC
Remittance Split: CAD6...7YVD
Savings Goals: CAE7...8ZWE
EOF

# Store securely (encrypted)
gpg --encrypt contracts_backup.txt
```

---

## Troubleshooting

### Issue: "insufficient balance for this transaction"

**Cause**: Deployer account lacks sufficient XLM

**Solution**:

```bash
# Fund account on testnet
soroban config identity fund deployer --network testnet

# Or transfer XLM manually to account
```

### Issue: "contract wasm exceeds max size"

**Cause**: Compiled WASM binary too large

**Solution**:

```bash
# Optimize WASM
wasm-opt -Oz input.wasm -o output.wasm

# Or remove unused features in Cargo.toml
# Remove default features and enable selectively
```

### Issue: "Invalid network passphrase"

**Cause**: Network configuration mismatch

**Solution**:

```bash
# Verify network config
soroban config network list

# Correct passphrase for testnet:
# "Test SDF Network ; September 2015"

# Reconfigure if needed
soroban config network add \
  --name testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase 'Test SDF Network ; September 2015'
```

### Issue: "contract invoke failed: not found"

**Cause**: Contract address incorrect or not deployed

**Solution**:

```bash
# Verify contract address
soroban contract info <CONTRACT_ID> --network testnet

# Redeploy if necessary
soroban contract deploy \
  --wasm path/to/contract.wasm \
  --network testnet \
  --identity deployer
```

### Issue: "Unable to parse response from RPC"

**Cause**: RPC endpoint connectivity issue

**Solution**:

```bash
# Test RPC connectivity
curl https://soroban-testnet.stellar.org:443/

# Try alternative RPC endpoint
soroban config network add \
  --name testnet-alt \
  --rpc-url https://soroban-testnet.stellar.org:443
```

### Issue: "Signature verification failed"

**Cause**: Signing with wrong identity

**Solution**:

```bash
# List available identities
soroban config identity list

# Use correct identity
soroban config identity use deployer

# Verify selected identity
soroban config identity show
```

---

## Security Checklist

- [ ] Private keys securely stored (not in git)
- [ ] Used testnet for initial deployment
- [ ] All contract functions tested
- [ ] Access controls verified
- [ ] Rate limiting configured
- [ ] Audit log enabled
- [ ] Backup of contract addresses created
- [ ] Deployment documented
- [ ] Admin accounts setup
- [ ] Ready for mainnet (if applicable)

---

## Mainnet Deployment Procedure

### Pre-deployment Checklist

1. **Audit**: Complete security audit
2. **Testing**: Full integration test suite passed
3. **Testnet**: Verified on testnet for 7+ days
4. **Documentation**: All docs updated
5. **Backup**: All addresses backed up securely
6. **Communication**: Team notified

### Mainnet Deployment

```bash
# Set mainnet as active network
soroban config network use mainnet

# Verify mainnet config
soroban config network list

# Deploy with explicit confirmation
soroban contract deploy \
  --wasm bill_payments/target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --network mainnet \
  --identity deployer

# Store transaction hash and contract IDs
```

### Post-Mainnet Deployment

1. Update all documentation with mainnet addresses
2. Notify stakeholders
3. Monitor contract interactions
4. Set up monitoring/alerts
5. Establish incident response procedures

---

## Upgrade Procedure

### Contract Code Upgrades (if applicable)

```bash
# 1. Develop and test new version on testnet
# 2. Deploy new version
soroban contract deploy \
  --wasm new_version.wasm \
  --network testnet \
  --identity deployer

# 3. Verify new version
# 4. Plan migration
# 5. Deploy to mainnet
# 6. Archive old contract ID for reference
```

---

## Additional Resources

- [Soroban Documentation](https://developers.stellar.org/learn/building-apps/example-application)
- [Stellar CLI Reference](https://github.com/stellar/stellar-cli)
- [Stellar Network Status](https://status.stellar.org/)
- [Remitwise Repository](https://github.com/Remitwise-Org/Remitwise-Contracts)
