# RemitWise Smart Contracts

Stellar Soroban smart contracts for the RemitWise remittance platform.

## Overview

This workspace contains the core smart contracts that power RemitWise's post-remittance financial planning features:

- **remittance_split**: Automatically splits remittances into spending, savings, bills, and insurance
- **savings_goals**: Goal-based savings with target dates and locked funds
- **bill_payments**: Automated bill payment tracking and scheduling
- **insurance**: Micro-insurance policy management and premium payments

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

**Events:**
- `SplitInitializedEvent`: Emitted when split configuration is initialized
  - `spending_percent`, `savings_percent`, `bills_percent`, `insurance_percent`, `timestamp`
- `SplitCalculatedEvent`: Emitted when split amounts are calculated
  - `total_amount`, `spending_amount`, `savings_amount`, `bills_amount`, `insurance_amount`, `timestamp`

### Savings Goals

Manages goal-based savings with target dates.

**Key Functions:**
- `create_goal`: Create a new savings goal (education, medical, etc.)
- `add_to_goal`: Add funds to a goal
- `get_goal`: Get goal details
- `is_goal_completed`: Check if goal target is reached

**Events:**
- `GoalCreatedEvent`: Emitted when a new savings goal is created
  - `goal_id`, `name`, `target_amount`, `target_date`, `timestamp`
- `FundsAddedEvent`: Emitted when funds are added to a goal
  - `goal_id`, `amount`, `new_total`, `timestamp`
- `GoalCompletedEvent`: Emitted when a goal reaches its target amount
  - `goal_id`, `name`, `final_amount`, `timestamp`

### Bill Payments

Tracks and manages bill payments with recurring support.

**Key Functions:**
- `create_bill`: Create a new bill (electricity, school fees, etc.)
- `pay_bill`: Mark a bill as paid and create next recurring bill if applicable
- `get_unpaid_bills`: Get all unpaid bills
- `get_total_unpaid`: Get total amount of unpaid bills

**Events:**
- `BillCreatedEvent`: Emitted when a new bill is created
  - `bill_id`, `name`, `amount`, `due_date`, `recurring`, `timestamp`
- `BillPaidEvent`: Emitted when a bill is marked as paid
  - `bill_id`, `name`, `amount`, `timestamp`
- `RecurringBillCreatedEvent`: Emitted when a recurring bill generates the next bill
  - `bill_id`, `parent_bill_id`, `name`, `amount`, `due_date`, `timestamp`

### Insurance

Manages micro-insurance policies and premium payments.

**Key Functions:**
- `create_policy`: Create a new insurance policy
- `pay_premium`: Pay monthly premium
- `get_active_policies`: Get all active policies
- `get_total_monthly_premium`: Calculate total monthly premium cost
- `deactivate_policy`: Deactivate an insurance policy

**Events:**
- `PolicyCreatedEvent`: Emitted when a new insurance policy is created
  - `policy_id`, `name`, `coverage_type`, `monthly_premium`, `coverage_amount`, `timestamp`
- `PremiumPaidEvent`: Emitted when a premium is paid
  - `policy_id`, `name`, `amount`, `next_payment_date`, `timestamp`
- `PolicyDeactivatedEvent`: Emitted when a policy is deactivated
  - `policy_id`, `name`, `timestamp`

## Events

All contracts emit events for important state changes, enabling real-time tracking and frontend integration. Events follow Soroban best practices and include:

- **Relevant IDs**: All events include the ID of the entity being acted upon
- **Amounts**: Financial events include transaction amounts
- **Timestamps**: All events include the ledger timestamp for accurate tracking
- **Context Data**: Additional contextual information (names, dates, etc.)

### Event Topics

Each contract uses short symbol topics for efficient event identification:
- **Remittance Split**: `init`, `calc`
- **Savings Goals**: `created`, `added`, `completed`
- **Bill Payments**: `created`, `paid`, `recurring`
- **Insurance**: `created`, `paid`, `deactive`

### Querying Events

Events can be queried from the Stellar network using the Soroban SDK or via the Horizon API for frontend integration. Each event structure is exported and can be decoded using the contract's schema.

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
- Multi-signature support for family wallets
- Emergency mode with priority processing

## License

MIT

