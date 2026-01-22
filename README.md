# RemitWise Smart Contracts

[![Stellar](https://img.shields.io/badge/Stellar-Soroban-blue)](https://soroban.stellar.org)
[![Rust](https://img.shields.io/badge/Rust-1.70+-orange)](https://www.rust-lang.org)

Stellar Soroban smart contracts for the RemitWise remittance platform that enable automated financial planning and management for remittance recipients.

## 📋 Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Contracts](#contracts)
- [Documentation](#documentation)
- [Quick Start](#quick-start)
- [Testing](#testing)
- [Deployment](#deployment)
- [Contributing](#contributing)
- [License](#license)

## 🎯 Overview

RemitWise smart contracts provide a comprehensive financial management system that automatically allocates incoming remittances into spending, savings, bill payments, and insurance categories. This helps users build financial discipline and achieve long-term goals through automated, trustless financial planning.

## ✨ Features

- **Automated Fund Allocation**: Intelligent splitting of remittances based on user-defined percentages
- **Goal-Based Savings**: Create and track savings goals with target dates and locked funds
- **Bill Payment Automation**: Track recurring bills and automate payment scheduling
- **Micro-Insurance Management**: Policy lifecycle management with automated premium payments
- **Real-time Monitoring**: Comprehensive dashboards for financial health tracking
- **Cross-border Compatible**: Built on Stellar for global remittance support

## 📚 Contracts

This workspace contains four core smart contracts:

### 🔄 Remittance Split

Automatically splits remittances into spending, savings, bills, and insurance categories.

**Key Functions:**

- `initialize_split`: Configure percentage allocations
- `get_split`: Retrieve current split configuration
- `calculate_split`: Compute actual amounts from total remittance

### 🎯 Savings Goals

Manages goal-based savings with target dates and progress tracking.

**Key Functions:**

- `create_goal`: Create new savings goals (education, medical, emergency funds)
- `add_to_goal`: Allocate funds to specific goals
- `get_goal`: Retrieve goal details and progress
- `is_goal_completed`: Check completion status

### 💳 Bill Payments

Tracks and manages bill payments with recurring payment support.

**Key Functions:**

- `create_bill`: Create bills with optional recurring schedules
- `pay_bill`: Mark bills as paid and auto-create next recurring instance
- `get_unpaid_bills`: Retrieve all outstanding bills
- `get_total_unpaid`: Calculate total unpaid amount

### 🛡️ Insurance

Manages micro-insurance policies and premium payment automation.

**Key Functions:**

- `create_policy`: Create new insurance policies
- `pay_premium`: Process monthly premium payments
- `get_active_policies`: List all active policies
- `get_total_monthly_premium`: Calculate total monthly costs
- `deactivate_policy`: Cancel policy coverage

## 📖 Documentation

### API Reference
- [Bill Payments API](docs/api/bill_payments.md) - Complete function reference for bill management
- [Insurance API](docs/api/insurance.md) - Policy management and premium handling
- [Remittance Split API](docs/api/remittance_split.md) - Fund allocation configuration
- [Savings Goals API](docs/api/savings_goals.md) - Goal creation and tracking

### Guides
- [Usage Examples](docs/usage_examples.md) - Common integration patterns and code examples
- [Deployment Guide](docs/deployment_guide.md) - Step-by-step deployment instructions
- [Architecture](docs/architecture.md) - System design, data flow, and integration patterns

## 🚀 Quick Start

### Prerequisites

- Rust (latest stable version)
- Soroban CLI (version 21.0.0 or later)
- Cargo package manager

### Installation

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Verify installation
soroban --version

# Clone repository
git clone https://github.com/your-org/remitwise-contracts.git
cd remitwise-contracts

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

## 🧪 Testing

Run the complete test suite:

```bash
# Test all contracts
cargo test

# Test with verbose output
cargo test -- --nocapture

# Test specific contract
cd bill_payments && cargo test
```

## 🚀 Deployment

For detailed deployment instructions, see the [Deployment Guide](docs/deployment_guide.md).

### Quick Deploy to Testnet

```bash
# Set network
export SOROBAN_NETWORK=testnet

# Generate/deploy key
soroban keys generate deployer
soroban keys fund deployer

# Deploy contracts
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --source deployer \
  --network testnet
```

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Guidelines

- Follow Rust best practices and Soroban patterns
- Add comprehensive tests for new features
- Update documentation for API changes
- Ensure contracts compile to WASM successfully

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built on [Stellar Soroban](https://soroban.stellar.org) platform
- Inspired by financial inclusion initiatives worldwide
- Thanks to the Stellar developer community
