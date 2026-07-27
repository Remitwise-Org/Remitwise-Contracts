# Per-Entrypoint N Caps

**Audience:** contributor / reviewer

This document lists every hard N-cap that the contracts enforce at a public entrypoint — the static upper bound on how many records a single address (or the contract as a whole) may hold before a write is rejected. It is the authoritative reference for reviewers who want to verify behaviour against the documented intent and for contributors adding new entrypoints.

For pagination semantics see [PAGINATION_HANDBOOK.md](PAGINATION_HANDBOOK.md).
For amount zero-handling see [ZERO_AMOUNT_POLICY.md](ZERO_AMOUNT_POLICY.md).
For time-window caps see [PERIOD_INVARIANTS.md](PERIOD_INVARIANTS.md).

---

## How to read this document

Each entry shows:

| Field | Meaning |
|---|---|
| **Entrypoint** | Function name as it appears in the contract `impl` block |
| **Scope** | Whether the cap is per-owner or global (whole contract) |
| **Cap value** | Constant from the source file |
| **Error returned** | `ContractError` variant or panic message when the cap is hit |
| **Source constant** | Symbol and crate so you can `grep` to the exact line |

"N" means the maximum inclusive count of records; the (N+1)-th create attempt is rejected.

---

## remittance_split

### Schedules per owner

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_schedule` | per-owner | **50** | `ScheduleCapExceeded` (22) | `MAX_SCHEDULES_PER_OWNER` in `remittance_split/src/lib.rs` |

**Enforcement:**

```rust
// remittance_split/src/lib.rs
pub const MAX_SCHEDULES_PER_OWNER: u32 = 50;

// inside create_schedule():
if owner_schedules.len() >= MAX_SCHEDULES_PER_OWNER {
    return Err(RemittanceSplitError::ScheduleCapExceeded);
}
```

Cancelling a schedule via `cancel_schedule` removes the entry from the owner's list and frees one slot, allowing a new `create_schedule` to succeed immediately.

**Import path:** `import_snapshot` also enforces this cap — a snapshot containing more than 50 schedules returns `ScheduleCapExceeded` without any partial state change.

### Replay-protection nonce set

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| `execute_signed_split` | per-address | **256** | Oldest nonce pruned (ring buffer — no error) | `MAX_USED_NONCES_PER_ADDR` in `remittance_split/src/lib.rs` |

This is not a hard rejection cap — when the per-address used-nonce set reaches 256 the oldest nonce is silently evicted to make room. The practical effect is that a nonce older than the last 256 used nonces becomes replayable. For normal usage this window is safe; replay-attack vectors are analysed in [COMMITTED_HASHES.md](COMMITTED_HASHES.md).

---

## savings_goals

### Goals per owner

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_goal` | per-owner | **2 000** | `GoalCapReached` (12) | `MAX_GOALS_PER_OWNER` in `savings_goals/src/lib.rs` |

**Enforcement:**

```rust
// savings_goals/src/lib.rs
const MAX_GOALS_PER_OWNER: u32 = 2000;

// inside create_goal():
if Self::get_owner_goal_count(&env, &owner) >= MAX_GOALS_PER_OWNER {
    return Err(SavingsGoalError::GoalCapReached);
}
```

The count includes **both active and archived goals** for the owner. Archiving a goal does not free a slot for a new `create_goal`. The cap prevents storage-bloat DoS regardless of lifecycle state.

**Import path:** `import_snapshot` also validates this: if the snapshot would push the owner above `MAX_GOALS_PER_OWNER` the import is rejected.

### Batch contribution items

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `batch_add_to_goals` | per-call | **50** | `BatchTooLarge` (14) | `MAX_BATCH_SIZE` in `savings_goals/src/lib.rs` |

This is a per-call limit on the number of `ContributionItem` entries, not a persistent storage cap. There is no minimum; an empty batch (`len == 0`) returns successfully with count 0.

### Audit log (internal ring buffer)

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| All write entrypoints | global | **5** entries | Oldest evicted (ring buffer — no error) | `MAX_AUDIT_ENTRIES` in `savings_goals/src/lib.rs` |

The on-chain audit log is intentionally short (5 entries). It is meant for immediate diagnostics, not long-term retention. Use the off-chain event indexer for historical auditing.

---

## bill_payments

### Bills per owner

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_bill` | per-owner | **1 000** | `OwnerBillCapExceeded` (18) | `MAX_BILLS_PER_OWNER` in `bill_payments/src/lib.rs` |

**Enforcement:**

```rust
// bill_payments/src/lib.rs
pub const MAX_BILLS_PER_OWNER: u32 = 1_000;

// inside create_bill():
if owner_bill_count >= MAX_BILLS_PER_OWNER {
    return Err(BillPaymentsError::OwnerBillCapExceeded);
}
```

Paid bills do not free a slot — they remain in storage (pending archival via `archive_paid_bills`). Archiving also does not free a slot because the cap counts all bills ever created by the owner (active and paid, but not including archived bills that have been removed by `bulk_cleanup_bills`). Reviewers should note this asymmetry: once an owner reaches 1 000 bills they must clean up via `bulk_cleanup_bills` before new bills can be created.

**Schedule-triggered creation:** `execute_due_bill_schedules` creates recurring bills automatically. It skips the creation and logs a missed execution if the owner is already at the cap:

```rust
// bill_payments/src/lib.rs, inside execute_due_bill_schedules():
if owner_bill_count < MAX_BILLS_PER_OWNER {
    // create next recurring bill
} // else: silently skip this cycle
```

### Bill schedules per owner

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_bill_schedule` | per-owner | **50** | `ScheduleCapExceeded` (23) | `MAX_BILL_SCHEDULES_PER_OWNER` in `bill_payments/src/lib.rs` |

**Enforcement:**

```rust
// bill_payments/src/lib.rs
const MAX_BILL_SCHEDULES_PER_OWNER: u32 = 50;

// inside create_bill_schedule():
if owner_schedule_count >= MAX_BILL_SCHEDULES_PER_OWNER {
    return Err(BillPaymentsError::ScheduleCapExceeded);
}
```

`cancel_bill_schedule` removes the schedule from the owner index, freeing one slot.

### Rate limits (per-address, 24-hour window)

These are not persistent record caps but per-address call-rate caps enforced over a rolling 24-hour window. They are documented here because they share the same "N cap at an entrypoint" structure:

| Entrypoint | Cap | Error | Constant |
|---|---|---|---|
| `create_bill` | **100** calls / 24 h | `RateLimitExceeded` (20) | `CREATE_BILL_RATE_LIMIT` in `bill_payments/src/lib.rs` |
| `pay_bill` | **200** calls / 24 h | `RateLimitExceeded` (20) | `PAY_BILL_RATE_LIMIT` in `bill_payments/src/lib.rs` |
| `cancel_bill_schedule` | **50** calls / 24 h | `RateLimitExceeded` (20) | `CANCEL_BILL_RATE_LIMIT` in `bill_payments/src/lib.rs` |

Rate-limit state is reset automatically after `RATE_LIMIT_WINDOW_SECONDS` (86 400 s) using the shared `check_and_increment_rate_limit` utility in `remitwise-common`.

---

## insurance

### Active policies (global, whole contract)

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_policy` | global | **1 000** | `MaxPoliciesReached` (11) | `MAX_POLICIES` in `insurance/src/lib.rs` |

**Enforcement:**

```rust
// insurance/src/lib.rs
const MAX_POLICIES: u32 = 1_000;

// inside create_policy():
if active.len() >= MAX_POLICIES {
    return Err(InsuranceError::MaxPoliciesReached);
}
```

Unlike the other record caps, this is a **global contract cap** on the active-policy index (`DataKey::ActivePolicies`), not a per-owner cap. Any caller beyond the 1 000 active-policy total receives `MaxPoliciesReached` regardless of how many policies they personally hold. Deactivating a policy via `deactivate_policy` removes it from the active index and frees one global slot.

> **Reviewer note:** this differs from `docs/insurance-policy-cap.md` (which describes the per-owner OWN_ACT index, MAX_POLICIES_PER_OWNER = 50). That per-owner index is managed by a **separate constant** in the same file and enforces a different invariant. Both caps must pass for a `create_policy` to succeed.

### Premium schedules per owner

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `create_premium_schedule` | per-owner | **50** | `MaxPoliciesReached` (11)* | `MAX_SCHEDULES_PER_OWNER` in `insurance/src/lib.rs` |

\* The insurance contract reuses `MaxPoliciesReached` for schedule overflow (see `insurance/src/lib.rs` line ~1147). This is a known code smell — the error is correct in practice but semantically misleading. Open a follow-up to introduce a dedicated `ScheduleCapExceeded` error code.

**Enforcement:**

```rust
// insurance/src/lib.rs
const MAX_SCHEDULES_PER_OWNER: u32 = 50;

if owner_ids.len() >= MAX_SCHEDULES_PER_OWNER {
    return Err(InsuranceError::MaxPoliciesReached); // <- misleading variant; see note above
}
```

---

## family_wallet

### Signers per multisig config

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `configure_multisig` | per-config | **20** | `TooManySigners` (19) | `MAX_SIGNERS` in `family_wallet/src/lib.rs` |

**Enforcement:**

```rust
// family_wallet/src/lib.rs
const MAX_SIGNERS: u32 = 20;

// inside configure_multisig():
if signer_count > MAX_SIGNERS {
    return Err(Error::TooManySigners);
}
```

### Family members (add_member / batch_add_family_members)

| Entrypoint | Scope | Cap | Behaviour on violation | Constant |
|---|---|---|---|---|
| `add_member` | global wallet | **30** total | `panic!("Member cap exceeded")` | `MAX_FAMILY_MEMBERS` in `family_wallet/src/lib.rs` |
| `batch_add_family_members` | global wallet | batch ≤ **30** items AND total ≤ **30** | `panic!("Batch too large")` / `panic!("Member cap exceeded")` | `MAX_BATCH_MEMBERS` / `MAX_FAMILY_MEMBERS` |

`MAX_FAMILY_MEMBERS` is defined as `MAX_BATCH_MEMBERS` (both are 30). The two constants exist separately to make the two distinct checks readable at their enforcement sites.

> **Note:** `add_member` panics rather than returning a typed error. This is a known gap: panics abort the transaction with an opaque error code. A follow-up should introduce a typed `MemberCapExceeded` error.

### Archived transactions ring buffer

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| `archive_old_transactions` | global wallet | **500** archived entries | Oldest entry evicted before insertion | `MAX_ARCHIVE_ENTRIES` in `family_wallet/src/lib.rs` |

No error is returned. The archive is a ring buffer: when `ARCH_TX` reaches 500 entries, the entry with the lowest `tx_id` is removed before the new one is inserted.

### Access audit ring buffer

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| All write entrypoints | global wallet | **200** audit entries | Oldest entry evicted | `MAX_ACCESS_AUDIT_ENTRIES` in `family_wallet/src/lib.rs` |

### Proposal expiry

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `propose_transaction` | per-proposal | **604 800 s** (7 days) | `InvalidProposalExpiry` (21) | `MAX_PROPOSAL_EXPIRY` in `family_wallet/src/lib.rs` |

Callers may choose any expiry up to 7 days. The default is 24 h (`DEFAULT_PROPOSAL_EXPIRY`).

---

## orchestrator

### Audit log ring buffer

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| `execute_remittance_flow`, `execute_signed_flow` | global | **100** entries | Oldest entry evicted | `MAX_AUDIT_ENTRIES` in `orchestrator/src/lib.rs` |

`get_audit_log(from_index, limit)` returns at most `MAX_AUDIT_ENTRIES` entries per call. Passing `limit = 0` maps to the default of 20; any value above `MAX_AUDIT_ENTRIES` is clamped down.

### Request deadline window

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `execute_signed_flow` | per-request | **3 600 s** from current ledger time | `InvalidDeadline` | `MAX_DEADLINE_WINDOW_SECS` in `orchestrator/src/lib.rs` |

A signed request whose `deadline` field is more than 3 600 s in the future from `env.ledger().timestamp()` is rejected. This matches the analogous deadline window in `remittance_split`.

### Replay-protection nonce set

| Entrypoint | Scope | Cap | Behaviour | Constant |
|---|---|---|---|---|
| `execute_signed_flow` | per-address | **256** | Oldest nonce pruned (ring buffer — no error) | `MAX_USED_NONCES_PER_ADDR` in `orchestrator/src/lib.rs` |

Same semantics as in `remittance_split`; see note above.

---

## emergency_killswitch

### Paused functions per module

| Entrypoint | Scope | Cap | Error | Constant |
|---|---|---|---|---|
| `pause_function` | per-module | **10** distinct function names | `LimitExceeded` (4) | `MAX_PAUSED_FUNCTIONS` in `emergency_killswitch/src/lib.rs` |

**Enforcement:**

```rust
// emergency_killswitch/src/lib.rs
// (MAX_PAUSED_FUNCTIONS = 10)
if paused_list.len() >= MAX_PAUSED_FUNCTIONS {
    return Err(Error::LimitExceeded);
}
```

The cap is per `module_id` — each module has its own independent list. Re-pausing an already-paused function is a no-op and does not consume a slot. `unpause_function` removes the entry and frees one slot. See [docs/killswitch-paused-functions-cap.md](killswitch-paused-functions-cap.md) for the full invariant test matrix.

---

## remitwise-common (shared limits)

These constants are imported by all contracts and apply wherever paginated reads are offered:

| Symbol | Value | Meaning |
|---|---|---|
| `DEFAULT_PAGE_LIMIT` | 20 | Default page size when caller passes `limit = 0` |
| `MAX_PAGE_LIMIT` | 50 | Maximum page size; higher values are clamped |
| `MAX_BATCH_SIZE` | 50 | Maximum items in a single batch write call |

The `clamp_limit()` utility normalises caller-supplied limits to `[1, MAX_PAGE_LIMIT]`. See [docs/pagination-limit-contract.md](pagination-limit-contract.md) for the full normalisation contract.

---

## Quick-reference table

| Contract | Entrypoint | Scope | N | Unit | Error on violation |
|---|---|---|---|---|---|
| `remittance_split` | `create_schedule` | per-owner | 50 | schedules | `ScheduleCapExceeded` (22) |
| `remittance_split` | `execute_signed_split` nonce set | per-address | 256 | nonces | evict oldest (no error) |
| `savings_goals` | `create_goal` | per-owner | 2 000 | goals | `GoalCapReached` (12) |
| `savings_goals` | `batch_add_to_goals` | per-call | 50 | items | `BatchTooLarge` (14) |
| `bill_payments` | `create_bill` | per-owner | 1 000 | bills | `OwnerBillCapExceeded` (18) |
| `bill_payments` | `create_bill` rate limit | per-address | 100 / 24 h | calls | `RateLimitExceeded` (20) |
| `bill_payments` | `pay_bill` rate limit | per-address | 200 / 24 h | calls | `RateLimitExceeded` (20) |
| `bill_payments` | `cancel_bill_schedule` rate limit | per-address | 50 / 24 h | calls | `RateLimitExceeded` (20) |
| `bill_payments` | `create_bill_schedule` | per-owner | 50 | schedules | `ScheduleCapExceeded` (23) |
| `insurance` | `create_policy` (global index) | global | 1 000 | active policies | `MaxPoliciesReached` (11) |
| `insurance` | `create_premium_schedule` | per-owner | 50 | schedules | `MaxPoliciesReached` (11)* |
| `family_wallet` | `configure_multisig` | per-config | 20 | signers | `TooManySigners` (19) |
| `family_wallet` | `add_member` / `batch_add_family_members` | global wallet | 30 | members | panic |
| `family_wallet` | `archive_old_transactions` | global wallet | 500 | archived txs | evict oldest (no error) |
| `family_wallet` | `propose_transaction` expiry | per-proposal | 604 800 s | seconds | `InvalidProposalExpiry` (21) |
| `orchestrator` | `execute_*_flow` audit log | global | 100 | audit entries | evict oldest (no error) |
| `orchestrator` | `execute_signed_flow` deadline | per-request | 3 600 s | seconds | `InvalidDeadline` |
| `orchestrator` | `execute_signed_flow` nonce set | per-address | 256 | nonces | evict oldest (no error) |
| `emergency_killswitch` | `pause_function` | per-module | 10 | paused fns | `LimitExceeded` (4) |

\* Misuse of `MaxPoliciesReached`; see the insurance section above.

---

## Reviewer checklist

When adding a new entrypoint that stores records, verify:

1. **A named constant exists** — never embed magic numbers at enforcement sites.
2. **The enforcement predicate is `>= CAP` (not `> CAP`)** — off-by-one errors here silently allow N+1 records.
3. **The correct error variant is returned** — using an unrelated variant (like the insurance schedule / `MaxPoliciesReached` case) makes on-chain error codes unreliable for indexers.
4. **Slot release is documented** — if the cap can be freed (cancel schedule, deactivate policy…), state this explicitly so operators know the escape hatch.
5. **Import/snapshot paths respect the same cap** — import functions that bypass the normal create path must independently check the cap.
6. **This file is updated** — add the new entrypoint to the quick-reference table and the detailed section.

---

## Related documents

- [PAGINATION_HANDBOOK.md](PAGINATION_HANDBOOK.md) — page-size clamping and cursor semantics
- [ZERO_AMOUNT_POLICY.md](ZERO_AMOUNT_POLICY.md) — which entrypoints reject zero amounts
- [AMOUNT_INVARIANTS.md](AMOUNT_INVARIANTS.md) — amount validation rules
- [PERIOD_INVARIANTS.md](PERIOD_INVARIANTS.md) — time-window rules
- [docs/insurance-policy-cap.md](insurance-policy-cap.md) — per-owner OWN_ACT accounting for insurance
- [docs/killswitch-paused-functions-cap.md](killswitch-paused-functions-cap.md) — killswitch cap invariant tests
- [ARCHITECTURE.md](../ARCHITECTURE.md#operational-limits-and-monitoring) — operational limits and u32 overflow analysis
