# Orchestrator Contract

A Soroban smart contract that coordinates cross-contract remittance flows across family wallet, remittance split, savings goals, bill payments, and insurance.

## Features

- Coordinated multi-contract remittance execution
- Reentrancy protection (execution lock)
- Compensation/rollback support for failed flows
- Signed and unsigned flow execution
- Audit logging
- Execution statistics tracking
- Per-address nonce-based replay protection

## Quickstart

```rust
use orchestrator::{OrchestratorClient, RemittanceFlowParams};

let params = RemittanceFlowParams {
    caller: owner.clone(),
    total_amount: 10_000_0000000, // 10 USDC
    family_wallet: family_wallet_addr,
    remittance_split: remittance_split_addr,
    savings: savings_addr,
    bills: bills_addr,
    insurance: insurance_addr,
    goal_id: 1,
    bill_id: 1,
    policy_id: 1,
};

client.execute_remittance_flow(params);
```

## API Reference

### Main Functions

#### `execute_remittance_flow(env, params)`

Executes the full remittance flow across all contracts.

#### `execute_remittance_flow_signed(env, params, nonce, deadline, request_hash)`

Executes the flow with signed request validation.

### Queries

#### `get_execution_stats(env)`

Returns execution statistics.

#### `get_audit_log(env, from_index, limit)`

Returns paginated audit log entries.

#### `get_nonce(env, address)`

Returns the current nonce for an address.

## Running Tests

```bash
cargo test -p orchestrator
```

