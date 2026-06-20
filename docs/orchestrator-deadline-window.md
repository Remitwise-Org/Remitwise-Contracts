# Orchestrator: Signed-Flow Deadline Window Semantics

## Overview
`Orchestrator::execute_remittance_flow_signed` (orchestrator/src/lib.rs) accepts
a signed authorization that is valid only for a bounded window after the ledger
timestamp at submission. The window bounds how long a signed flow authorization
remains exploitable if a signature leaks, limiting the blast radius of a captured
signature. The bound is enforced in `require_nonce_hardened` together with the
per-address nonce replay protection.

## Constants
- `MAX_DEADLINE_WINDOW_SECS` = 3,600 (1 hour)

## Deadline Validation Rules
The check is performed before the execution lock is taken and before any
`ExecutionStats` mutation:

```rust
let now = env.ledger().timestamp();
if deadline <= now {
    return Err(OrchestratorError::DeadlineExpired);
}
if deadline > now + MAX_DEADLINE_WINDOW_SECS {
    return Err(OrchestratorError::DeadlineExpired);
}
```

| Condition                                       | Result            |
|-------------------------------------------------|-------------------|
| `deadline < now`                                | `DeadlineExpired` |
| `deadline == now`                               | `DeadlineExpired` |
| `deadline == now + 1`                           | Accepted          |
| `deadline == now + MAX_DEADLINE_WINDOW_SECS`    | Accepted (edge)   |
| `deadline == now + MAX_DEADLINE_WINDOW_SECS + 1`| `DeadlineExpired` |

**Inclusive upper edge:** the window comparison is strictly
`deadline > now + MAX_DEADLINE_WINDOW_SECS`, so a deadline exactly at
`now + MAX_DEADLINE_WINDOW_SECS` is **accepted**. The lower bound is strict
(`deadline <= now` rejected), so `now` and any past timestamp are rejected.

## Security Properties
- A deadline-rejected call returns `DeadlineExpired` **before** the execution
  lock is set, before the nonce advances, and before `ExecutionStats` is touched.
  Rejected calls therefore leave both the nonce counter and the stats counters
  unchanged.
- The deadline window operates alongside the nonce replay protection: even a
  signature whose deadline is still in-window cannot be replayed once its nonce
  has been consumed — the advanced per-address counter rejects the stale nonce
  with `InvalidNonce`. See [orchestrator-nonce.md](orchestrator-nonce.md).
- The deadline is one of the fields folded into the request hash binding, so a
  captured signature cannot be re-pointed at a different deadline without
  breaking the hash check.

## Replay Window
A signed request is valid for at most 1 hour from the ledger timestamp at
submission. Requests with deadlines beyond this window are rejected as
`DeadlineExpired` to prevent unbounded replay windows.

## Test Coverage
Boundary semantics are pinned in
[orchestrator/src/test.rs](../orchestrator/src/test.rs):

- `test_signed_deadline_at_window_edge_accepted` — inclusive upper edge accepted
- `test_signed_deadline_one_past_window_rejected` — one second beyond rejected
- `test_signed_deadline_in_past_rejected` — past deadline rejected
- `test_signed_in_window_replay_with_used_nonce_rejected` — nonce uniqueness
  enforced alongside the still-valid deadline
- `test_signed_deadline_rejected_does_not_mutate_stats` — no `ExecutionStats`
  mutation on a deadline-rejected call

Pre-existing related tests: `test_execute_flow_deadline_expired`,
`test_execute_flow_deadline_too_far`, `test_deadline_window_prevents_old_requests`.

## Events
The deadline check runs before the `flow` lifecycle event is emitted, so a
deadline-rejected signed call emits no flow events. See the orchestrator entries
in [EVENTS.md](../EVENTS.md) and [orchestrator-events.md](orchestrator-events.md).
