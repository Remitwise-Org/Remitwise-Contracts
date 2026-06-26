# Cross-Contract `try_*` Patterns

This guide is for contributors reading and modifying the contracts. It explains
when to call the panicking variant of a cross-contract method versus `try_*`,
how to surface partial failure, and how these patterns are enforced across the
workspace.

---

## Background: two flavours of every cross-contract call

Soroban's `#[contractclient]` macro generates two method variants for every
trait function:

| Variant | Return type | Panics on trap? | Use when |
|---------|-------------|-----------------|----------|
| `client.method(...)` | `ReturnType` directly | **Yes** — host trap rolls back the whole transaction | Failure means the whole transaction should abort |
| `client.try_method(...)` | `Result<Result<ReturnType, ContractError>, InvokeError>` | **No** — trap is caught and returned as `Err(InvokeError::Contract(...))` | Caller can degrade gracefully without aborting |

The double-`Result` wrapping is intentional. The outer `Result` captures host-
level failures (contract not found, trap, budget exceeded). The inner `Result`
carries the callee's own `#[contracterror]` value when the contract returned an
error without trapping.

### Matching on `try_*` output

```rust
// reporting/src/lib.rs — dependency health probe
let split_ok = matches!(split_client.try_get_split(), Ok(Ok(_)));

// Richer pattern for surfacing partial failure
match bill_client.try_get_total_unpaid(&owner) {
    Ok(Ok(amount))  => { /* use amount */ }
    Ok(Err(e))      => { /* callee returned a typed BillPaymentsError */ }
    Err(_)          => { /* host trap: contract not found, budget exceeded, etc. */ }
}
```

---

## Rule: match the call variant to the error contract

### Use the panicking variant when failure means "abort the whole transaction"

The orchestrator's `execute_remittance_flow` executes a multi-step remittance.
If the spending-limit check fails, the entire flow must abort — there is no
meaningful partial state to return. Panicking (and letting the VM roll back) is
the right call:

```rust
// orchestrator/src/lib.rs — perform_remittance_flow
let fw_client = interface::FamilyWalletClient::new(env, &params.family_wallet);
if !fw_client.check_spending_limit(&params.caller, &params.total_amount) {
    return Err(OrchestratorError::Unauthorized);
}

let rs_client = interface::RemittanceSplitClient::new(env, &params.remittance_split);
let allocations = rs_client.calculate_split(&params.total_amount);
// ^ panics on trap — rolls back everything including the EXEC_LOCK write
```

The same reasoning applies to the downstream write steps:

```rust
// orchestrator/src/lib.rs — execute_flow_internal
let s_client = interface::SavingsGoalsClient::new(env, &sg_addr);
if !s_client.add_to_goal(executor, &goal_id, &savings_amt) {
    // First write failed — nothing to compensate yet
    return Err(OrchestratorError::CrossContractCallFailed);
}
savings_done = true;

let b_client = interface::BillPaymentsClient::new(env, &bp_addr);
if !b_client.pay_bill(executor, &bill_id, &bills_amt) {
    // Bills failed — savings was already applied, compensate it
    Self::compensate_savings(env, executor, goal_id, savings_amt, savings_done);
    return Err(OrchestratorError::RemittanceFlowRolledBack);
}
```

The panicking variant lets Soroban's atomic rollback handle unexpected traps
(e.g., the callee ran out of budget). Explicit `OrchestratorError` values handle
expected logical failures.

### Use `try_*` when failure is a valid degraded outcome

The reporting contract queries up to five independent contracts to assemble one
`FinancialHealthReport`. If the insurance contract is temporarily unreachable,
the report can still return a meaningful result for the other four components.
Panicking on every call would make the report all-or-nothing:

```rust
// reporting/src/lib.rs — check_dependencies (admin health probe)
let split_ok = matches!(split_client.try_get_split(), Ok(Ok(_)));

// reporting/src/lib.rs — get_remittance_summary_internal
let split_percentages = match split_client.try_get_split() {
    Ok(Ok(res)) => res,
    _ => {
        availability = DataAvailability::Partial;
        Vec::new(env)  // safe default — continue without split data
    }
};

// reporting/src/lib.rs — get_family_spending_report_internal
let tracker_result = family_client.try_get_spending_tracker(&member);
let (member_spending, data_available) = match tracker_result {
    Ok(Ok(Some(tracker))) => (tracker.current_spent, true),
    Ok(Ok(None))          => (0, true),
    _                     => {
        availability = DataAvailability::Partial;  // degrade, don't abort
        (0, false)
    }
};
```

`try_*` is also used everywhere in test code to assert specific error types
without panicking the test harness:

```rust
// orchestrator/src/lib.rs (test module)
let replay = client.try_execute_remittance_flow_signed(
    &executor, &FLOW_AMOUNT, &0, &deadline, &replay_hash,
);
assert_eq!(replay, Err(Ok(OrchestratorError::NonceAlreadyUsed)));
```

---

## How partial failure is surfaced

### Orchestrator — `OrchestratorError::RemittanceFlowRolledBack`

When a write step in `execute_flow_internal` fails after earlier writes have
been applied, the orchestrator attempts best-effort compensation and then
returns `OrchestratorError::RemittanceFlowRolledBack` to the caller:

```rust
// orchestrator/src/lib.rs — execute_flow_internal (step 2 failure)
if !b_client.pay_bill(executor, &bill_id, &bills_amt) {
    // bills_amt > 0, bills step failed
    // savings step already succeeded — reverse it
    Self::compensate_savings(env, executor, goal_id, savings_amt, savings_done);
    return Err(OrchestratorError::RemittanceFlowRolledBack);
}

// step 3 failure: compensate both steps 1 and 2
if !i_client.pay_premium(executor, &policy_id, &insurance_amt) {
    Self::compensate_savings(env, executor, goal_id, savings_amt, savings_done);
    Self::compensate_bill(env, executor, bill_id, bills_amt, bills_done);
    return Err(OrchestratorError::RemittanceFlowRolledBack);
}
```

Compensation calls go through the companion interfaces — `SavingsGoalsCompClient`
(`remove_from_goal`) and `BillPaymentsCompClient` (`reverse_payment`). If a
compensation call itself traps, Soroban's atomic rollback handles it; the
`RemittanceFlowRolledBack` error is what the original caller observes.

The audit log records the outcome of every execution regardless of the path:

```rust
// orchestrator/src/lib.rs — record_flow_outcome
Err(e) => {
    Self::update_execution_stats(env, false);
    Self::append_audit(env, FLOW_EXEC_AUDIT, executor, false);
    RemitwiseEvents::emit(env, EventCategory::Transaction, EventPriority::High,
        symbol_short!("flow_fail"), (executor.clone(), e as u32));
    Err(e)
}
```

### Reporting — `DataAvailability` enum

The reporting contract never panics on a failed downstream call. Instead it
degrades the result using the `DataAvailability` enum:

```rust
// reporting/src/lib.rs
pub enum DataAvailability {
    Complete = 0,  // all upstream calls succeeded
    Partial  = 1,  // some calls failed or pagination cap reached
    Missing  = 2,  // critical call failed; data is default/empty
}
```

The `paginate_dependency` helper uses this convention:

```rust
// reporting/src/lib.rs — paginate_dependency
pub(crate) fn paginate_dependency<T>(
    env: &Env,
    mut fetch_page: impl FnMut(u32) -> (Vec<T>, u32),
) -> PaginatedResult<T> {
    // ...
    if pages_fetched >= MAX_DEP_PAGES {
        return PaginatedResult {
            items,
            data_availability: DataAvailability::Partial,  // cap reached
        };
    }
    // ...
    let data_availability = if items.is_empty() {
        DataAvailability::Missing   // nothing came back at all
    } else {
        DataAvailability::Complete
    };
}
```

The top-level `FinancialHealthReport` aggregates availability across components:

```rust
// reporting/src/lib.rs — get_financial_health_report
let data_availability = Self::worst_data_availability(
    remittance_summary.data_availability,
    Self::worst_data_availability(
        bill_compliance.data_availability,
        insurance_report.data_availability,
    ),
);
```

`worst_data_availability` returns `Missing` if any component is `Missing`,
`Partial` if any is `Partial`, and `Complete` only when all are `Complete`.
Callers must check `data_availability` on any report before relying on its
aggregate numbers.

---

## `#![no_std]` discipline

All contracts in this workspace declare `#![no_std]`. That means:

- No `std::` prefix anywhere. Use `core::` for anything from Rust core.
- No `String`, `Vec`, `HashMap` from the standard library. Use the SDK's
  `soroban_sdk::{String, Vec, Map}` instead — they allocate in the Soroban host.
- No `println!`, `eprintln!`, or `std::io`. Events through `env.events().publish`
  or `RemitwiseEvents::emit` are the only output channel.
- `Option` comes from `core::option::Option`. The SDK re-exports it so in
  practice you write `Option<T>` as normal; the explicit path is only needed
  inside `#[contracttype]` structs where `core::option::Option<T>` is required.

Cross-contract calls themselves respect this — the generated client code is
`no_std` compatible. All type conversions go through
`soroban_sdk::IntoVal` / `soroban_sdk::TryFromVal`.

---

## Decision checklist for contributors

Before writing a cross-contract call, ask:

1. **Should a failure abort the transaction?**
   - Yes → panicking variant. A host trap rolls back all state changes atomically.
   - No → `try_*`. Match on `Ok(Ok(_))` / `Ok(Err(e))` / `Err(_)` explicitly.

2. **Does this call appear in a multi-step write sequence?**
   - Yes → consider whether you need compensation logic if a later step fails.
     See `execute_flow_internal` in `orchestrator/src/lib.rs` for the pattern.

3. **Does this call appear in a read-only aggregation (reporting, health score)?**
   - Yes → always use `try_*` and degrade to `DataAvailability::Partial` or
     `DataAvailability::Missing`. Never let a single dependency make the whole
     report fail.

4. **Does this call appear in tests?**
   - Yes → always use `try_*` so errors can be pattern-matched without panicking
     the test harness. See the nonce-eviction tests in `orchestrator/src/lib.rs`
     for examples.

5. **Will the new code run in production (not tests)?**
   - Yes → `#[cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]`
     is active on every contract. Do not use `.unwrap()` or `.expect()`.

---

## Where to look for real examples

| Pattern | File | Function |
|---------|------|----------|
| Panicking multi-step flow | `orchestrator/src/lib.rs` | `execute_flow_internal` |
| Compensation on partial failure | `orchestrator/src/lib.rs` | `compensate_savings`, `compensate_bill` |
| `try_*` health probe | `reporting/src/lib.rs` | `check_dependencies` |
| `try_*` graceful degradation | `reporting/src/lib.rs` | `get_remittance_summary_internal`, `get_family_spending_report_internal` |
| `DataAvailability` aggregation | `reporting/src/lib.rs` | `get_financial_health_report`, `worst_data_availability` |
| `try_*` in tests | `orchestrator/src/lib.rs` (test module) | `signed_flow_replay_uses_used_set_and_old_nonce_uses_sequential_counter` |
| `paginate_dependency` helper | `reporting/src/lib.rs` | `paginate_dependency` |

---

## Related docs

- [Orchestrator Reentrancy Model](orchestrator-reentrancy.md) — how the
  `EXEC_LOCK` guard interacts with panicking cross-contract calls
- [Orchestrator Nonce Replay Protection](orchestrator-nonce.md) — request-hash
  binding and the deadline window used in `execute_remittance_flow_signed`
- [Reporting `check_dependencies` health schema](reporting-check-dependencies.md) —
  the `try_*`-based probe calls and `DependencyStatus` output format
- [Event Taxonomy](EVENT_TAXONOMY.md) — the `flow`, `flow_ok`, `flow_fail`
  lifecycle events emitted by the orchestrator on each execution path
