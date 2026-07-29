# Remittance Split: Idempotent Schedule Execution

**Document Version:** 1.0  
**Last Updated:** 2026-05-26  
**Related Issue:** #607  
**Contract:** `remittance_split`

---

## Overview

The `execute_due_remittance_schedules` function is a permissionless executor that processes remittance schedules whose `next_due` timestamp has been reached. It advances schedule state (`next_due`, `last_executed`, `missed_count`) in a manner that is **idempotent** and **auditable on-chain**, eliminating the need for off-chain systems to manually recompute schedule advancement.

### Key Innovation: Idempotent next_due Advancement

Unlike naive implementations that advance `next_due` immediately and risk double-execution on retry, this executor uses a two-phase approach:

1. **Phase 1:** Set `last_executed = current_time` (mark as executed).
2. **Phase 2:** Advance `next_due` by interval (only after marking executed).

This ensures a double-call at the same ledger timestamp will skip re-execution via the idempotency guard: `if last_executed >= next_due_original { skip }`.

---

## Function Signature

```rust
pub fn execute_due_remittance_schedules(env: Env) -> Vec<u32>
```

### Parameters
- `env: Env` — The Soroban environment (provides ledger timestamp, storage, events).

### Returns
- `Vec<u32>` — A vector of schedule IDs that were successfully executed in this call. Empty if no schedules were due or the contract is paused.

### Authorization
- **Permissionless:** Any account may call this function. No authorization check.
- **Reason:** Executors are utilities for maintaining schedule state, and blocking them would break automation workflows.

---

## Execution Flow

### 1. Preconditions Check

```rust
if Self::get_global_paused(&env) {
    return Vec::new(&env);  // Exit early if paused
}

let _config = match env.storage().instance().get(&symbol_short!("CONFIG")) {
    Some(c) => c,
    None => return Vec::new(&env),  // Exit if not initialized
};
```

### 2. Schedule Iteration

For each schedule ID from 1 to `NEXT_RSCH`:

```rust
for schedule_id in 1..=next_schedule_id {
    let mut schedule = match load_schedule(schedule_id) {
        Some(s) => s,
        None => continue,  // Skip if never created or deleted
    };
```

### 3. Due and Active Filter

```rust
if !schedule.active || schedule.next_due > current_time {
    continue;  // Skip inactive or not-yet-due
}
```

### 4. Idempotency Check

```rust
if let Some(last_exec) = schedule.last_executed {
    if last_exec >= schedule.next_due {
        continue;  // Already executed in this window
    }
}
```

This guard prevents double-execution at the same timestamp:
- After the first call, `last_executed = current_time`.
- On the second call (same timestamp), `current_time == last_executed` and `last_executed >= next_due_original`, so the schedule is skipped.

### 5. Mark Execution

```rust
schedule.last_executed = Some(current_time);
```

This is done **before** advancing `next_due`, ensuring the idempotency guard takes effect.

### 6. Advance next_due

#### For One-Off Schedules (`interval == 0`)
```rust
schedule.active = false;  // Deactivate
```

#### For Recurring Schedules (`interval > 0`)
```rust
let mut missed = 0u32;
let mut next = schedule.next_due + schedule.interval;
while next <= current_time {
    missed = missed.saturating_add(1);
    next = next.saturating_add(schedule.interval);
}
schedule.missed_count = schedule.missed_count.saturating_add(missed);
schedule.next_due = next;
```

**Drift Handling:** If the executor runs late (e.g., 3 intervals late), the loop "catches up" by advancing through all missed intervals, tracking each as `missed_count`.

### 7. Emit Events

```rust
// Execution event
RemitwiseEvents::emit(
    &env,
    EventCategory::State,
    EventPriority::Medium,
    symbol_short!("sch_exec"),
    (schedule_id, schedule.amount),
);

// Missed intervals event (if applicable)
if missed > 0 {
    RemitwiseEvents::emit(
        &env,
        EventCategory::State,
        EventPriority::Low,
        symbol_short!("sch_miss"),
        (schedule_id, missed),
    );
}
```

### 8. Persist and Record

```rust
env.storage().persistent().set(&DataKey::Schedule(schedule_id), &schedule);
executed.push_back(schedule_id);
```

---

## Security Properties

### 1. Idempotency Guarantee

**Claim:** Calling `execute_due_remittance_schedules` twice at the same ledger timestamp cannot double-execute a schedule.

**Proof:**
- On call N at time T, schedule S with `next_due = D` (where `D <= T`):
  - Check: `active == true`, `D <= T` → pass both.
  - Idempotency check: `last_executed >= D` → false (last_executed was None or < D).
  - Action: Set `last_executed = T`, advance `next_due` to `D + interval` (now > T).
- On call N+1 at time T (same timestamp):
  - Check: `active == true`, `next_due > T` → skip (no longer due).
  - No re-execution.
- Alternative: If `next_due == D` again (unchanged), the idempotency check `last_executed >= D` now passes → skip.

### 2. Paused Contract Rejection

If the contract is paused, `execute_due_remittance_schedules` immediately returns an empty Vec without processing any schedules. This ensures that during an emergency freeze, no schedule state is modified.

### 3. No Authorization Required

The function is permissionless by design, allowing off-chain systems and automated agents to call it without requiring owner keys. Security is maintained through:
- Schedule creation/modification/cancellation remain owner-only and authenticated.
- Once created, only this executor and cancellation can change `next_due` and `last_executed`.
- An attacker cannot bypass the idempotency guard by resetting `last_executed` (no function does that except after execution).

---

## Usage Patterns

### Off-Chain Orchestration

```typescript
// Pseudocode: Off-chain orchestrator running periodically
async function executorJob() {
    const result = await contract.execute_due_remittance_schedules();
    const executedIds = result;
    
    for (const scheduleId of executedIds) {
        const schedule = await contract.get_remittance_schedule(scheduleId);
        console.log(`Schedule ${scheduleId} executed at ${schedule.last_executed}`);
        
        // If needed, call distribute_usdc separately to move funds
        // (the executor does NOT transfer funds, only advances state)
    }
}

// Run every block, every minute, or based on monitoring
setInterval(executorJob, 60_000);  // Every 60 seconds
```

### On-Chain Integration

```rust
// Example: Call from another smart contract
let executed = RemittanceSplit::execute_due_remittance_schedules(&env);
if executed.len() > 0 {
    env.events().publish((symbol_short!("app"),), ("schedules_executed", executed));
}
```

---

## Edge Cases and Handling

### 1. Empty Schedule Set
- **Scenario:** No schedules have been created.
- **Behavior:** Function returns empty Vec immediately.
- **Cost:** Single instance storage read for `NEXT_RSCH`.

### 2. All Inactive Schedules
- **Scenario:** All schedules are cancelled or one-off and already executed.
- **Behavior:** Function skips each one during iteration.
- **Result:** Returns empty Vec.

### 3. Exactly Equal: `next_due == current_time`
- **Scenario:** Schedule is due at exact ledger timestamp (not just >).
- **Behavior:** Condition `next_due <= current_time` is true → execute.
- **Result:** Schedule is processed normally.

### 4. Drift: Multiple Missed Intervals
- **Scenario:** Executor runs 3 intervals late (e.g., `next_due = 3000`, `interval = 86400`, `now = 3000 + 86400*3 + 100`).
- **Behavior:** Loop advances `next_due` through all three intervals, incrementing `missed_count` by 3.
- **Result:** `next_due = 3000 + 86400*4`, `missed_count = 3`, event emitted.
- **Implication:** Drift is transparent and auditable on-chain.

### 5. Contract Paused
- **Scenario:** Admin calls `pause()` before executor runs.
- **Behavior:** Function returns empty Vec.
- **State:** Schedules remain unchanged (unpause resumes normal execution on next call).

---

## Integration with distribute_usdc

**Important:** `execute_due_remittance_schedules` does **not** transfer funds. It only advances schedule state for auditing and coordination.

### Recommended Workflow

1. Off-chain executor calls `execute_due_remittance_schedules()`.
2. Off-chain system observes `ScheduleExecuted` and `ScheduleMissed` events.
3. Off-chain system calls `distribute_usdc()` to perform the actual fund transfer.
4. On-chain audit trail reflects both execution and distribution separately.

### Why Separate Functions?

- **Flexibility:** Schedules can be tracked without requiring off-chain knowledge of account groups.
- **Atomicity:** Distribution is orthogonal to schedule advancement; a failed transfer does not corrupt schedule state.
- **Auditability:** Events for schedule execution are independent of transfer events, providing clear causality.

---

## Events Reference

### ScheduleExecuted Event
**Topic:** `("split", "sch_exec")`  
**Data:** `(schedule_id: u32, amount: i128)`  
**Emitted:** When a schedule is successfully executed.

### ScheduleMissed Event
**Topic:** `("split", "sch_miss")`  
**Data:** `(schedule_id: u32, missed_count: u32)`  
**Emitted:** When a schedule has missed one or more intervals during execution.

---

## Testing Coverage

The implementation includes comprehensive tests for:

1. **Basic execution:** One-off and recurring schedules.
2. **Recurring advancement:** `next_due` correctly advanced by interval.
3. **Missed intervals:** Correct `missed_count` and catch-up advancement.
4. **Idempotency:** Double-call at same timestamp is a no-op.
5. **Filtering:** Skips inactive schedules and not-yet-due schedules.
6. **Edge case: Exactly equal timestamp:** Executes when `next_due == now`.
7. **Empty sets:** Handles zero schedules gracefully.
8. **Paused contract:** Returns empty Vec when paused, does not modify state.
9. **Mixed due/not-due:** Correctly partitions schedules.

### Test Coverage Target
**≥ 95% line coverage** of the executor function and related schedule management code.

---

## Performance Considerations

### Gas Budget
- **Per-schedule iteration:** O(1) storage read.
- **Per-execution:** O(interval_count) for catch-up loop (bounded by practical time gaps).
- **Total:** O(total_schedules) storage reads + O(total_executions * average_interval_count) compute.

### Optimization Tips
1. **Batch off-chain:** If many schedules are due, consider pagination via `get_schedules_paginated()`.
2. **Monitor missed_count:** Drift indicates executor latency; adjust frequency if needed.
3. **Pause during maintenance:** Call `pause()` to prevent execution while performing upgrades.

---

## FAQ

**Q: Why is this function permissionless?**  
A: The executor is a utility for maintaining schedule state. Restricting it would break automation workflows. Security is maintained through the fact that schedule creation and modification remain owner-authenticated.

**Q: What if the same schedule is created twice with the same ID?**  
A: Schedule IDs are allocated sequentially via a monotonic counter (`NEXT_RSCH`), making ID collision impossible.

**Q: Can an attacker reset `last_executed` to force re-execution?**  
A: No. The only function that sets `last_executed` is `execute_due_remittance_schedules`, and it only sets it forward in time (to `current_time`). Once set, it can only increase.

**Q: What happens if `interval` overflows during the catch-up loop?**  
A: The code uses `saturating_add` to prevent panic. If `next = next.saturating_add(interval)` overflows, it caps at `u64::MAX`, ensuring the loop terminates safely.

---

## References

- **Issue:** #607 (Remittance Split: Add execute_due_remittance_schedules with idempotent next_due advancement)
- **Related Contract:** `savings_goals::execute_due_savings_schedules` (similar pattern)
- **Event Documentation:** See [EVENTS.md](../EVENTS.md) for full event schema.
- **Architecture:** See [ARCHITECTURE.md](../ARCHITECTURE.md) for system-wide contract relationships.
