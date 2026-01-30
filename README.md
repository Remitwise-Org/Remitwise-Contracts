# RemitWise Smart Contracts

Stellar Soroban smart contracts for the RemitWise remittance platform.

## Overview

This workspace contains the core smart contracts that power RemitWise's post-remittance financial planning features:

- **remittance_split**: Automatically splits remittances into spending, savings, bills, and insurance
- **savings_goals**: Goal-based savings with target dates and locked funds
- **bill_payments**: Automated bill payment tracking and scheduling
- **insurance**: Micro-insurance policy management and premium payments
- **family_wallet**: Family member management with spending limits and permissions

## Prerequisites

- Rust (latest stable version)
- Stellar CLI (soroban-cli)
- Cargo

## Installation

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

## Contracts

### Remittance Split

Handles automatic allocation of remittance funds into different categories.

**Key Functions:**
- `initialize_split`: Set percentage allocation (spending, savings, bills, insurance)
- `get_split`: Get current split configuration
- `calculate_split`: Calculate actual amounts from total remittance

### Savings Goals

Manages goal-based savings with target dates.

**Key Functions:**
- `create_goal`: Create a new savings goal (education, medical, etc.)
- `add_to_goal`: Add funds to a goal
- `get_goal`: Get goal details
- `is_goal_completed`: Check if goal target is reached

### Bill Payments

Tracks and manages bill payments with recurring support.

**Key Functions:**
- `create_bill`: Create a new bill (electricity, school fees, etc.)
- `pay_bill`: Mark a bill as paid and create next recurring bill if applicable
- `get_unpaid_bills`: Get all unpaid bills
- `get_total_unpaid`: Get total amount of unpaid bills

### Insurance

Manages micro-insurance policies and premium payments.

**Key Functions:**
- `create_policy`: Create a new insurance policy
- `pay_premium`: Pay monthly premium
- `get_active_policies`: Get all active policies
- `get_total_monthly_premium`: Calculate total monthly premium cost

### Family Wallet

Manages family members, roles, and spending limits.

**Key Functions:**
- `add_member`: Add a family member with role and spending limit
- `get_member`: Get member details
- `update_spending_limit`: Update spending limit for a member
- `check_spending_limit`: Verify if spending is within limit

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

### USDC remittance split checks (local & CI)

- `cargo test -p remittance_split` exercises the USDC distribution logic with a mocked Stellar Asset Contract (`env.register_stellar_asset_contract_v2`) and built-in auth mocking.
- The suite covers minting the payer account, splitting across spending/savings/bills/insurance, and asserting balances along with the new allocation metadata helper.
- The same command is intended for CI so it runs without manual setup; re-run locally whenever split logic changes or new USDC paths are added.

## Gas Benchmarks

See `docs/gas-optimization.md` for methodology, before/after results, and assumptions.

Run the deterministic gas benchmarks:

```bash
RUST_TEST_THREADS=1 cargo test -p bill_payments --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p savings_goals --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture
```

## Deployment

See the [Deployment Guide](DEPLOYMENT.md) for comprehensive deployment instructions.

Quick deploy to testnet:

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

