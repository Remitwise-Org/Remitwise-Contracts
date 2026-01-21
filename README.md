# Remitwise Smart Contracts

Comprehensive smart contract suite for financial management of remittance recipients on the Stellar blockchain using Soroban.

## 📋 Overview

Remitwise provides a modular suite of Soroban smart contracts enabling remittance recipients to automatically:

- **Split incoming remittances** across multiple financial categories
- **Manage bills** including recurring payments
- **Track insurance policies** and premium payments
- **Implement family spending controls** with role-based access
- **Monitor savings goals** toward long-term objectives

## 🎯 Problem Statement

Remittance recipients often struggle with:

- Managing multiple financial obligations simultaneously
- Lack of enforcement for savings discipline
- Limited spending controls for family members
- No automated budget allocation mechanism
- Difficulty tracking progress toward financial goals

## ✅ Solution Features

### 1. **Automated Remittance Splitting**

- Configurable percentage allocations
- Support for 4 spending categories
- Deterministic calculations
- Remainder handling

### 2. **Bill Payment Management**

- Create one-time and recurring bills
- Automatic renewal for recurring bills
- Track payment status
- Query unpaid obligations

### 3. **Insurance Policy Management**

- Create policies with coverage details
- Monthly premium payment tracking
- Policy activation/deactivation
- Total obligation calculations

### 4. **Family Wallet Controls**

- Role-based access (admin, sender, recipient)
- Individual spending limits
- Real-time limit validation
- Dynamic limit adjustment

### 5. **Savings Goals Tracking**

- Create goals with targets and dates
- Incremental contribution tracking
- Progress monitoring
- Completion detection

## 📦 Contract Modules

### Bill Payments (`bill_payments/`)

```
Manages bill payments including recurring bills
├─ create_bill()
├─ pay_bill()
├─ get_unpaid_bills()
├─ get_total_unpaid()
└─ get_bill()
```

### Family Wallet (`family_wallet/`)

```
Implements family spending controls and access management
├─ add_member()
├─ get_member()
├─ get_all_members()
├─ update_spending_limit()
└─ check_spending_limit()
```

### Insurance (`insurance/`)

```
Tracks insurance policies and premium payments
├─ create_policy()
├─ pay_premium()
├─ get_active_policies()
├─ get_total_monthly_premium()
├─ deactivate_policy()
└─ get_policy()
```

### Remittance Split (`remittance_split/`)

```
Orchestrates remittance distribution across categories
├─ initialize_split()
├─ get_split()
└─ calculate_split()
```

### Savings Goals (`savings_goals/`)

```
Manages personal savings objectives
├─ create_goal()
├─ add_to_goal()
├─ get_goal()
├─ get_all_goals()
└─ is_goal_completed()
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.70.0+
- Soroban CLI v20.0.0+
- Cargo
- Docker (optional)

### Installation

```bash
# Clone repository
git clone https://github.com/Remitwise-Org/Remitwise-Contracts.git
cd Remitwise-Contracts

# Install Soroban CLI
cargo install --locked soroban-cli

# Verify installation
soroban version
```

### Build Contracts

```bash
# Build all contracts
for dir in bill_payments family_wallet insurance remittance_split savings_goals; do
  cd $dir
  cargo build --release --target wasm32-unknown-unknown
  cd ..
done
```

### Deploy to Testnet

```bash
# Configure network
soroban config network add \
  --name testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase 'Test SDF Network ; September 2015'

# Create deployer account
soroban config identity create deployer
soroban config identity fund deployer --network testnet

# Deploy contract
soroban contract deploy \
  --wasm bill_payments/target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --network testnet \
  --identity deployer
```

## 📚 Documentation

### Getting Started

- [Quick Start Guide](docs/DEPLOYMENT_GUIDE.md)
- [Architecture Overview](docs/ARCHITECTURE.md)

### API Documentation

- [Complete API Reference](docs/API_REFERENCE.md)
- [Contract-Specific README Files](./README.md)
  - [Bill Payments](bill_payments/README.md)
  - [Family Wallet](family_wallet/README.md)
  - [Insurance](insurance/README.md)
  - [Remittance Split](remittance_split/README.md)
  - [Savings Goals](savings_goals/README.md)

### Integration & Usage

- [Usage Examples](docs/USAGE_EXAMPLES.md)
- [Integration Patterns](docs/ARCHITECTURE.md#integration-patterns)
- [Complete Flow Example](docs/USAGE_EXAMPLES.md#complete-integration-example)

### Deployment & Operations

- [Deployment Guide](docs/DEPLOYMENT_GUIDE.md)
- [Network Configuration](docs/DEPLOYMENT_GUIDE.md#network-configuration)
- [Troubleshooting](docs/DEPLOYMENT_GUIDE.md#troubleshooting)

## 📚 Core Contracts Reference

### Bill Payments

Tracks and manages bill payments with recurring support.

**Key Functions:**

- `create_bill`: Create a new bill (electricity, school fees, etc.)
- `pay_bill`: Mark a bill as paid and create next recurring bill if applicable
- `get_unpaid_bills`: Get all unpaid bills
- `get_total_unpaid`: Get total amount of unpaid bills

**Full Documentation**: [bill_payments/README.md](bill_payments/README.md)

### Insurance

Manages micro-insurance policies and premium payments.

**Key Functions:**

- `create_policy`: Create a new insurance policy
- `pay_premium`: Pay monthly premium
- `get_active_policies`: Get all active policies
- `get_total_monthly_premium`: Calculate total monthly premium cost

**Full Documentation**: [insurance/README.md](insurance/README.md)

### Family Wallet

Manages family members, roles, and spending limits.

**Key Functions:**

- `add_member`: Add a family member with role and spending limit
- `get_member`: Get member details
- `update_spending_limit`: Update spending limit for a member
- `check_spending_limit`: Verify if spending is within limit

**Full Documentation**: [family_wallet/README.md](family_wallet/README.md)

### Remittance Split

Automatically allocates remittances across categories.

**Key Functions:**

- `initialize_split`: Set percentage allocation
- `get_split`: Get current split configuration
- `calculate_split`: Calculate actual amounts from total

**Full Documentation**: [remittance_split/README.md](remittance_split/README.md)

### Savings Goals

Manages goal-based savings with target dates.

**Key Functions:**

- `create_goal`: Create a new savings goal
- `add_to_goal`: Add funds to a goal
- `get_goal`: Get goal details
- `is_goal_completed`: Check if goal target is reached

**Full Documentation**: [savings_goals/README.md](savings_goals/README.md)

## Testing

Run tests for all contracts:

```bash
cargo test
```

Run tests for a specific contract:

```bash
cd remittance_split
cargo test
```

## Deployment

Deploy to testnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source <your-key> \
  --network testnet
```

## Development

This is a basic MVP implementation. Future enhancements:

- Integration with Stellar Asset Contract (USDC)
- Cross-contract calls for automated allocation
- Event emissions for transaction tracking
- Multi-signature support for family wallets
- Emergency mode with priority processing

## License

MIT
