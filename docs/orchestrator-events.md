# Orchestrator Contract Events Documentation

This document provides detailed information about all events emitted by the Orchestrator contract in the Remitwise system. These events enable off-chain consumers (indexers, frontends, analytics platforms) to track flow outcomes and contract state changes.

## Overview

The Orchestrator contract emits lifecycle events for remittance flow execution and contract state changes. All events follow the Soroban event publishing pattern with structured topics and payloads.

**Contract Name:** `orchestrator`  
**Primary Topic Prefix:** `"orch"` for direct events, `"Remitwise"` for categorized events

## Event Categories

### Transaction Events
Events related to remittance flow execution:
- `flow` - Flow execution started
- `flow_ok` - Flow completed successfully  
- `flow_fail` - Flow execution failed

### System Events
Events related to contract state changes:
- `init_ok` - Contract initialization completed
- `orch/upgraded` - Contract version upgraded

## Event Specifications

### Event: Flow Started

**Topic:** `("Remitwise", EventCategory::Transaction, EventPriority::High, "flow")`  
**Emitted by:** `execute_remittance_flow`, `execute_remittance_flow_signed`  
**Trigger:** Emitted when a remittance flow execution begins after passing validation checks

**Data Structure:**
```rust
pub struct FlowStartedEvent {
    pub executor: Address,     // Address executing the flow
    pub amount: i128,          // Total amount to be processed
}
```

**Example Event:**
```json
{
  "executor": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "amount": 10000
}
```

**Use Cases:**
- Track flow initiation for monitoring
- Detect potential stuck flows (flow without flow_ok/flow_fail)
- Analytics on flow attempt frequency

---

### Event: Flow Completed Successfully

**Topic:** `("Remitwise", EventCategory::Transaction, EventPriority::High, "flow_ok")`  
**Emitted by:** `execute_remittance_flow`, `execute_remittance_flow_signed`  
**Trigger:** Emitted when a remittance flow completes successfully

**Data Structure:**
```rust
pub struct FlowCompletedEvent {
    pub executor: Address,     // Address that executed the flow
    pub amount: i128,          // Total amount successfully processed
}
```

**Example Event:**
```json
{
  "executor": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "amount": 10000
}
```

**Use Cases:**
- Confirm successful flow completion
- Track total processed amounts
- Calculate success rates
- Update user balances off-chain

---

### Event: Flow Failed

**Topic:** `("Remitwise", EventCategory::Transaction, EventPriority::High, "flow_fail")`  
**Emitted by:** `execute_remittance_flow`, `execute_remittance_flow_signed`  
**Trigger:** Emitted when a remittance flow execution fails

**Data Structure:**
```rust
pub struct FlowFailedEvent {
    pub executor: Address,     // Address that attempted the flow
    pub error_code: u32,       // Error code from OrchestratorError enum
}
```

**Example Event:**
```json
{
  "executor": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "error_code": 2
}
```

**Error Codes:**
```rust
pub enum OrchestratorError {
    Unauthorized = 1,
    InvalidAmount = 2,
    Overflow = 3,
    CrossContractCallFailed = 4,
    NonceAlreadyUsed = 5,
    InvalidNonce = 6,
    DeadlineExpired = 7,
    ExecutionLocked = 8,
    InvalidDependency = 9,
    DuplicateDependency = 10,
}
```

**Security Note:** This event does NOT include the sensitive amount in the payload. Only the error code is included to prevent leaking financial information in failure cases.

**Use Cases:**
- Track failure rates
- Debug common failure modes
- Alert on systemic issues
- Calculate retry strategies

---

### Event: Contract Initialized

**Topic:** `("Remitwise", EventCategory::System, EventPriority::High, "init_ok")`  
**Emitted by:** `init`  
**Trigger:** Emitted when the orchestrator contract is successfully initialized

**Data Structure:**
```rust
pub struct InitCompletedEvent {
    pub caller: Address,       // Address that initialized the contract
}
```

**Example Event:**
```json
{
  "caller": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4"
}
```

**Use Cases:**
- Confirm successful deployment
- Track contract initialization
- Audit deployment history

---

### Event: Contract Upgraded

**Topic:** `("orch", "upgraded")`  
**Emitted by:** `set_version`  
**Trigger:** Emitted when the contract version is upgraded by the owner

**Data Structure:**
```rust
pub struct VersionUpgradeEvent {
    pub previous_version: u32, // Previous contract version
    pub new_version: u32,      // New contract version
}
```

**Example Event:**
```json
{
  "previous_version": 1,
  "new_version": 2
}
```

**Use Cases:**
- Track contract upgrades
- Ensure indexers are using correct version
- Audit version changes
- Coordinate frontend updates

## Event Lifecycle Flow

### Successful Flow Execution

```
1. Validation checks pass
2. emit("flow", executor, amount)           ← Flow started
3. Execute downstream operations
4. emit("flow_ok", executor, amount)         ← Flow completed successfully
```

### Failed Flow Execution

```
1. Validation checks pass
2. emit("flow", executor, amount)           ← Flow started
3. Error occurs during execution
4. emit("flow_fail", executor, error_code)  ← Flow failed (no amount leaked)
```

## Event Consumption Guidelines

### For Indexers

1. **Track Flow Completion:** Always look for matching `flow` → `flow_ok` or `flow` → `flow_fail` pairs
2. **Handle Missing Events:** If a `flow` event exists without a corresponding completion event within a timeout, flag for investigation
3. **Version Awareness:** Monitor `orch/upgraded` events to handle schema changes
4. **Error Aggregation:** Use `flow_fail` events to aggregate error types for monitoring

### For Frontends

1. **Real-time Updates:** Subscribe to `flow_ok` events to update user balances
2. **Error Display:** Map `flow_fail` error codes to user-friendly messages
3. **Progress Indication:** Use `flow` event to show flow initiation
4. **Version Checks:** Verify contract version before displaying features

### For Analytics

1. **Success Rates:** Calculate ratio of `flow_ok` to total `flow` events
2. **Failure Analysis:** Aggregate `flow_fail` events by error code
3. **Volume Tracking:** Sum amounts from `flow_ok` events for total processed volume
4. **Temporal Patterns:** Analyze event timestamps for usage patterns

## Security Considerations

### Failure Event Privacy
The `flow_fail` event intentionally excludes the amount parameter to prevent leaking sensitive financial information when operations fail. Only the executor address and error code are included.

### Event Ordering
Events are emitted in the following order for a single flow execution:
1. `flow` (started)
2. `flow_ok` OR `flow_fail` (result)

Consumers should not rely on events from different flows being ordered.

### Replay Protection
Flow execution uses nonce-based replay protection. The `flow` event indicates the start of a validated, nonce-checked operation.

## Testing

All events have comprehensive test coverage in `orchestrator/src/test.rs`:

- `test_flow_event_emitted_on_start` - Verifies `flow` event emission
- `test_flow_ok_event_emitted_on_success` - Verifies `flow_ok` event emission
- `test_flow_fail_event_emitted_on_failure` - Verifies `flow_fail` event emission
- `test_orch_upgraded_event_emitted` - Verifies `orch/upgraded` event emission
- `test_init_ok_event_emitted` - Verifies `init_ok` event emission
- `test_flow_lifecycle_events_order` - Verifies event ordering
- `test_flow_fail_does_not_leak_sensitive_amount` - Verifies security of failure events

## Future Enhancements

Potential future events that may be added:
- Flow pause/resume events for maintenance
- Dependency health check events
- Execution statistics snapshot events
- Cross-concall diagnostic events

## References

- Main EVENTS.md: [EVENTS.md](../EVENTS.md)
- Orchestrator contract: [orchestrator/src/lib.rs](../orchestrator/src/lib.rs)
- Test suite: [orchestrator/src/test.rs](../orchestrator/src/test.rs)
- Common event types: [remitwise-common/src/lib.rs](../remitwise-common/src/lib.rs)