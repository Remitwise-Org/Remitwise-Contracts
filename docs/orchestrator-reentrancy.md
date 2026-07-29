# Orchestrator Reentrancy Model

## Overview

The Orchestrator contract coordinates multiple cross-contract calls to execute a complete remittance flow. Because it interacts with multiple downstream dependencies, it is critical to prevent reentrancy attacks where a malicious or compromised downstream contract calls back into the Orchestrator to manipulate state or duplicate operations.

## Security Mechanism: EXEC_LOCK

The Orchestrator implements an execution state lock (`EXEC_LOCK`) to ensure that only one execution of `execute_remittance_flow` can be active at a time for a given contract instance.

### 1. Lock Acquisition
At the start of `execute_remittance_flow`, the contract attempts to acquire the lock:
- If `EXEC_LOCK` is already `true`, the call fails with `OrchestratorError::ReentrancyDetected`.
- If `EXEC_LOCK` is `false`, it is set to `true`.

### 2. Lock Release Guarantee
The lock is managed using a **RAII (Resource Acquisition Is Initialization)** pattern via the `LockGuard` struct. This ensures that the lock is released (set back to `false`) in the following scenarios:
- **Success**: When the flow completes successfully.
- **Handled Error**: When a downstream call returns an error result.
- **Early Return**: If any validation or intermediate step returns early using the `?` operator.

### 3. Panic & Rollback
In Soroban, if a contract panics (e.g., due to an `abort` or an unhandled error), the entire transaction state is rolled back. This includes the `EXEC_LOCK` being reset to its state prior to the transaction. This provides a secondary layer of safety against "stuck" locks.

## Implementation Details

### Reentrancy Guard
The `LockGuard` struct implements the `Drop` trait:
```rust
pub struct LockGuard {
    env: Env,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.env.storage().instance().set(&EXEC_LOCK, &false);
    }
}
```

### Audit Log
The result of every execution attempt is recorded in a rotating audit log via `append_audit`. The lock is guaranteed to be released *before* the audit entry is written, ensuring that audit operations themselves do not interfere with the lock state.

## Verification

The reentrancy protection is verified via tests in `orchestrator/src/test.rs`:
- `test_execute_flow_success`: Confirms lock is released after successful execution.
- `test_lock_released_on_invalid_amount`: Confirms lock is released after early return.
- `test_reentrancy_rejection`: Confirms that concurrent calls are rejected.
- `test_lock_recovery_after_failure`: Confirms that the lock is reset after a downstream panic.
