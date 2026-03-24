# Cross-Contract Orchestrator

The Cross-Contract Orchestrator is a Soroban smart contract that coordinates automated remittance allocation across multiple contracts in the Remitwise ecosystem. It provides atomic, multi-contract operations with family wallet permission enforcement.

## Overview

The orchestrator acts as a coordination layer that:
1. Validates permissions via the Family Wallet contract
2. Calculates remittance splits via the Remittance Split contract
3. Executes downstream operations:
   - Deposits to Savings Goals
   - Pays Bills
   - Pays Insurance Premiums

## Architecture

### Contract Interfaces

The orchestrator integrates with five contracts:

1. **Family Wallet** - Permission and spending limit enforcement
2. **Remittance Split** - Allocation percentage calculation
3. **Savings Goals** - Goal-based savings deposits
4. **Bill Payments** - Bill payment execution
5. **Insurance** - Premium payment processing

### Atomicity Guarantees

All operations execute atomically via Soroban's panic/revert mechanism:
- If any step fails, all prior state changes in the transaction are reverted
- No partial state changes can occur
- Events are also rolled back on failure

This ensures that remittance flows either complete entirely or fail entirely, preventing inconsistent state across contracts.

## Building

Build the contract using Cargo:

```bash
cargo build --package orchestrator --target wasm32-unknown-unknown --release
```

## Testing

Run the test suite:

```bash
cargo test --package orchestrator
```

The test suite includes:
- Unit tests for individual operations
- Integration tests with mock contracts
- End-to-end remittance flow tests
- Failure and rollback scenarios
- Permission and spending limit enforcement tests

## Deployment

### Prerequisites

- Soroban CLI installed
- Access to a Stellar network (testnet, futurenet, or mainnet)
- Deployed instances of all integrated contracts

### Deployment Steps

1. Build the optimized WASM:
```bash
cargo build --package orchestrator --target wasm32-unknown-unknown --release
```

2. Deploy to Stellar network:
```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/orchestrator.wasm \
  --source <YOUR_SECRET_KEY> \
  --network testnet
```

3. Note the deployed contract address for integration with other contracts.

## Usage

### Execute Complete Remittance Flow

```rust
use orchestrator::{Orchestrator, OrchestratorClient};

// Create client
let client = OrchestratorClient::new(&env, &orchestrator_address);

// Execute remittance flow
let result = client.execute_remittance_flow(
    &user_address,
    &1000_0000000, // 1000 tokens (7 decimals)
    &family_wallet_addr,
    &remittance_split_addr,
    &savings_addr,
    &bills_addr,
    &insurance_addr,
    &goal_id,
    &bill_id,
    &policy_id,
);

match result {
    Ok(flow_result) => {
        println!("Total amount: {}", flow_result.total_amount);
        println!("Savings: {}", flow_result.savings_amount);
        println!("Bills: {}", flow_result.bills_amount);
        println!("Insurance: {}", flow_result.insurance_amount);
    },
    Err(e) => println!("Flow failed: {:?}", e),
}
```

### Execute Individual Operations

```rust
// Deposit to savings goal
client.execute_savings_deposit(
    &user_address,
    &amount,
    &family_wallet_addr,
    &savings_addr,
    &goal_id,
)?;

// Pay bill
client.execute_bill_payment(
    &user_address,
    &amount,
    &family_wallet_addr,
    &bills_addr,
    &bill_id,
)?;

// Pay insurance premium
client.execute_insurance_payment(
    &user_address,
    &amount,
    &family_wallet_addr,
    &insurance_addr,
    &policy_id,
)?;
```

### Query Execution Statistics

```rust
// Get execution stats
let stats = client.get_execution_stats();
println!("Total flows executed: {}", stats.total_flows_executed);
println!("Total flows failed: {}", stats.total_flows_failed);
println!("Total amount processed: {}", stats.total_amount_processed);

// Get audit log
let log = client.get_audit_log(&0, &10);
for entry in log.iter() {
    println!("Operation: {:?}, Amount: {}, Success: {}", 
             entry.operation, entry.amount, entry.success);
}
```

## Gas Estimation

Typical gas costs for orchestrator operations:

| Operation | Estimated Gas |
|-----------|--------------|
| Permission check | ~2,000 |
| Remittance split calculation | ~3,000 |
| Savings deposit | ~4,000 |
| Bill payment | ~4,000 |
| Insurance payment | ~4,000 |
| **Complete remittance flow** | **~22,000** |

These are estimates and actual costs may vary based on network conditions and contract state.

## Error Handling

The orchestrator defines the following error types:

- `PermissionDenied` - Family wallet denied permission
- `SpendingLimitExceeded` - Operation exceeds spending limit
- `SavingsDepositFailed` - Failed to deposit to savings goal
- `BillPaymentFailed` - Failed to pay bill
- `InsurancePaymentFailed` - Failed to pay insurance premium
- `RemittanceSplitFailed` - Failed to calculate split
- `InvalidAmount` - Amount must be positive
- `InvalidContractAddress` - Invalid contract address provided
- `CrossContractCallFailed` - Generic cross-contract call failure

All errors are returned as `Result<T, OrchestratorError>` and include detailed error events for debugging.

## Events

The orchestrator emits the following events:

### Success Event
```rust
RemittanceFlowEvent {
    caller: Address,
    total_amount: i128,
    allocations: Vec<i128>,
    timestamp: u64,
}
```

### Error Event
```rust
RemittanceFlowErrorEvent {
    caller: Address,
    failed_step: Symbol,
    error_code: u32,
    timestamp: u64,
}
```

## Integration with Remitwise Contracts

### Required Contract Addresses

When calling orchestrator functions, you must provide addresses for:

1. **Family Wallet** - For permission and spending limit checks
2. **Remittance Split** - For allocation calculation
3. **Savings Goals** - For savings deposits
4. **Bill Payments** - For bill payments
5. **Insurance** - For premium payments

### Contract Interface Requirements

The orchestrator expects the following function signatures:

**Family Wallet:**
```rust
fn check_spending_limit(env: Env, caller: Address, amount: i128) -> bool;
```

**Remittance Split:**
```rust
fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>;
```

**Savings Goals:**
```rust
fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128;
```

**Bill Payments:**
```rust
fn pay_bill(env: Env, caller: Address, bill_id: u32);
```

**Insurance:**
```rust
fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool;
```

## Security Considerations

1. **Authorization** - All public functions require caller authorization via `require_auth()`
2. **Permission Checks** - Family wallet permission is checked before every operation
3. **Spending Limits** - Spending limits are enforced via family wallet integration
4. **Atomicity** - All operations are atomic; failures trigger complete rollback
5. **Contract Addresses** - All contract addresses are passed as parameters (never hardcoded)

## Known Limitations

1. The `check_spending_limit` function must be added to the Family Wallet contract for full functionality
2. The orchestrator does not handle token transfers directly; it coordinates operations across contracts
3. Gas costs can be high for complete remittance flows due to multiple cross-contract calls

## Contributing

When contributing to the orchestrator:

1. Ensure all tests pass: `cargo test --package orchestrator`
2. Add tests for new functionality
3. Update documentation for API changes
4. Follow Rust and Soroban best practices
5. Include gas estimation comments for new cross-contract calls

## License

This contract is part of the Remitwise project.
