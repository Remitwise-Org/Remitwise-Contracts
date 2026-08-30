# Observability Model

**Audience:** Downstream integrators — indexers, analytics pipelines, and
monitoring services that consume on-chain events from Remitwise contracts.

This document describes **what each contract emits**, the topic/payload schema
for every event, and what off-chain consumers must rely on to reconstruct
contract state without re-reading on-chain storage.

---

## How events are published

All Remitwise contracts use Soroban's `env.events().publish()` mechanism.
There are two emission styles in use:

**Style 1 — categorised (via `RemitwiseEvents::emit`):**

```rust
// topic: (String("Remitwise"), EventCategory, EventPriority, Symbol)
// data:  an arbitrary serialisable value
RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::High,
                       symbol_short!("flow"), (executor, amount));
```

**Style 2 — raw two-element topic:**

```rust
// topic: (Symbol, Symbol)
// data:  tuple
env.events().publish((symbol_short!("orch"), symbol_short!("epch_bump")),
                     (old_epoch, new_epoch));
```

Both styles are permanently on-chain and queryable through any Soroban RPC
node.  The topic structure is the stable public API; never rely on topic
ordinality alone — always match on symbol content.

---

## Orchestrator contract

### `flow` — remittance flow started

| Field | Value |
|---|---|
| Topic style | Categorised |
| Category | `Transaction` |
| Priority | `High` |
| Symbol | `"flow"` |
| Data | `(executor: Address, amount: i128)` |

Emitted at the start of `execute_remittance_flow` and
`execute_remittance_flow_signed` **after** all input validation passes (auth,
amount > 0, reentrancy guard, epoch check, nonce/hash check).

> Off-chain: a `flow` event with no subsequent `flow_ok` or `flow_fail` in the
> same transaction indicates a crash-abort — flag for investigation.

---

### `flow_ok` — flow completed successfully

| Field | Value |
|---|---|
| Topic style | Categorised |
| Category | `Transaction` |
| Priority | `High` |
| Symbol | `"flow_ok"` |
| Data | `(executor: Address, amount: i128)` |

Emitted when all downstream cross-contract calls succeed.

---

### `flow_fail` — flow failed

| Field | Value |
|---|---|
| Topic style | Categorised |
| Category | `Transaction` |
| Priority | `High` |
| Symbol | `"flow_fail"` |
| Data | `(executor: Address, error_code: u32)` |

The `amount` is **intentionally omitted** to avoid leaking sensitive financial
information in failure paths.  Map `error_code` to `OrchestratorError` using
the discriminant table:

| Code | Variant | Meaning |
|---|---|---|
| 1 | `Unauthorized` | Caller not authorised |
| 2 | `InvalidAmount` | Amount ≤ 0 |
| 3 | `Overflow` | Arithmetic overflow |
| 4 | `CrossContractCallFailed` | Downstream contract returned an error |
| 5 | `NonceAlreadyUsed` | Nonce was already consumed |
| 6 | `InvalidNonce` | Nonce or hash validation failed |
| 7 | `DeadlineExpired` | Deadline timestamp has passed |
| 8 | `ExecutionLocked` | Reentrancy guard active |
| 9 | `InvalidDependency` | Required contract dependency missing |
| 10 | `DuplicateDependency` | Dependency registered twice |
| 15 | `EpochMismatch` | Supplied actor_epoch ≠ current epoch |

---

### `init_ok` — orchestrator initialised

| Field | Value |
|---|---|
| Topic style | Categorised |
| Category | `System` |
| Priority | `High` |
| Symbol | `"init_ok"` |
| Data | `caller: Address` |

One-time event on successful `init`.

---

### `upgraded` — contract version changed

| Field | Value |
|---|---|
| Topic style | Raw two-element |
| Topic | `("orch", "upgraded")` |
| Data | `(previous_version: u32, new_version: u32)` |

Emitted by `set_version`.  Indexers should watch for this event to
invalidate any cached schema assumptions derived from `get_version`.

---

### `epch_bump` — actor epoch bumped

| Field | Value |
|---|---|
| Topic style | Raw two-element |
| Topic | `("orch", "epch_bump")` |
| Data | `(old_epoch: u64, new_epoch: u64)` |

Emitted by `bump_actor_epoch`.  Off-chain signers and relayers **must**
refresh their cached epoch after seeing this event — all tokens signed under
`old_epoch` are immediately invalid.

See [CROSS_CONTRACT_EPOCHS.md](CROSS_CONTRACT_EPOCHS.md) for the full epoch
coordination protocol.

---

### `clm_rwd` — rewards claimed

| Field | Value |
|---|---|
| Topic style | Raw two-element |
| Topic | `("orch", "clm_rwd")` |
| Data | `(caller: Address, amount: u64)` |

---

### `snap_pre` / `snap_rst` — upgrade snapshot lifecycle

| Symbol | Trigger | Data |
|---|---|---|
| `"snap_pre"` | `pre_upgrade` called | `snapshot_version: u32` |
| `"snap_rst"` | `restore_from_snapshot` called | `snapshot_version: u32` |

These are operational events for the upgrade runbook; see
[docs/UPGRADE_RUNBOOK.md](UPGRADE_RUNBOOK.md).

---

## Remittance Split contract

### `init` — split configured

| Field | Value |
|---|---|
| Symbol | `"init"` |
| Data | split configuration struct |

### `calc` — split calculated

| Field | Value |
|---|---|
| Symbol | `"calc"` |
| Data | `(total, spending, savings, bills, insurance): (i128, i128, i128, i128, i128)` |

Emitted by `calculate_split` with the four resulting allocations.

---

## Savings Goals contract

| Symbol | Trigger | Key data |
|---|---|---|
| `"create"` / `"created"` | `create_goal` | `(goal_id, owner, name, target_amount, target_date)` |
| `"add"` / `"funds_add"` | `add_to_goal` | `(goal_id, amount, new_total)` |
| `"completed"` | Goal target reached | `(goal_id, name, final_amount)` |
| `"archive"` | `archive_completed_goals` | `(owner, count)` |
| `"import"` | Data migration import | `(format, record_count)` |
| `"lock"` | Goal locked | `(goal_id, locked_until)` |

---

## Bill Payments contract

| Symbol | Trigger | Key data |
|---|---|---|
| `"created"` | `create_bill` | `(bill_id, owner, name, amount, due_date, recurring)` |
| `"paid"` | `pay_bill` | `(bill_id, owner, amount, paid_at)` |
| `"sched_crt"` | `create_bill_schedule` | `(schedule_id, owner)` |
| `"sched_exe"` | `execute_due_bill_schedules` | `schedule_id` |
| `"sched_mod"` | `modify_bill_schedule` | `schedule_id` |
| `"sched_ccl"` | `cancel_bill_schedule` | `schedule_id` |

---

## Insurance contract

| Symbol | Trigger | Key data |
|---|---|---|
| `"created"` | `create_policy` | `(policy_id, name, coverage_type, monthly_premium, coverage_amount)` |
| `"prem_pay"` / `"paid"` | `pay_premium` | `(policy_id, amount, next_payment_date)` |
| `"deactive"` | `deactivate_policy` | `(policy_id, name)` |
| `"react"` | `reactivate_policy` | `(policy_id)` |
| `"sched_crt"` | Schedule created | `(schedule_id, owner)` |
| `"sched_exe"` | Schedule executed | `schedule_id` |

---

## Family Wallet contract

| Symbol | Trigger | Key data |
|---|---|---|
| `"added"` / member events | `add_member` | `(wallet_id, member, role)` |
| `"updated"` / limit events | `update_spending_limit` | `(member, limit)` |
| `"emerg/*"` | Emergency mode / transfers | mode and transfer details |
| `"wallet/*"` | Multisig proposals / withdrawals | proposal id, amount |

---

## Reporting contract

| Symbol | Trigger | Key data |
|---|---|---|
| `"report"` | `get_remittance_summary` (write path) | `(owner, data_availability)` |

The `data_availability` value maps to `DataAvailability::Complete`,
`Partial`, or `Missing` — consumers should surface `Partial` and `Missing`
states to users rather than treating them as errors.

---

## Event lifecycle: successful remittance flow

The canonical cross-contract event sequence for a single remittance:

```
 Contract             Event         Meaning
 ─────────────────────────────────────────────────────────────
 orchestrator    →    flow          flow started (validated)
 remittance_split→    calc          allocation calculated
 savings_goals   →    add           savings slice deposited
 bill_payments   →    paid          bills slice applied
 insurance       →    prem_pay      insurance slice applied
 orchestrator    →    flow_ok       all cross-contract calls succeeded
```

An indexer that wants to detect partial failures should check whether a
`flow` event is followed by `flow_ok` or `flow_fail` **in the same
ledger** — Soroban guarantees atomicity within a single transaction.

---

## Off-chain consumption guidelines

### Guaranteed invariants

1. `flow_ok` is only emitted when all downstream calls succeeded — it is safe
   to use as a settlement trigger.
2. `flow_fail` carries an `error_code` instead of an `amount` — do not infer
   financial amounts from failure events.
3. Every `epch_bump` event makes all previously-signed tokens invalid — cache
   the epoch and refresh on every `epch_bump`.
4. `upgraded` events may change storage layout; re-read contract state after
   observing one.

### Event ordering

Events within a single Soroban transaction are ordered by emission order.
Cross-transaction ordering is determined by ledger sequence number.  Do not
assume events from different contracts in different transactions are ordered
relative to each other unless they share a ledger close.

### What is NOT emitted

The following state changes do **not** produce events and must be polled via
view functions if off-chain visibility is needed:

- TTL extension operations (`extend_ttl`, `bump_instance_ttl`)
- Internal nonce increments
- Storage key writes that are not part of a user-visible action

---

## Querying events

```bash
# Fetch events for the orchestrator contract from the RPC
curl -X POST https://soroban-testnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getEvents",
    "params": {
      "startLedger": "1000",
      "filters": [{
        "type": "contract",
        "contractIds": ["<ORCHESTRATOR_CONTRACT_ID>"],
        "topics": [["*", "*", "*", "AAAADQAAAARmbG93"]]
      }],
      "pagination": { "limit": 100 }
    }
  }'
```

Topic values are base64-encoded XDR `ScVal`s.  Use the Soroban SDK or the
[indexer](../indexer/README.md) to decode them into typed structs.

---

## See also

- [docs/CROSS_CONTRACT_EPOCHS.md](CROSS_CONTRACT_EPOCHS.md) — epoch coordination
  and the `epch_bump` event
- [docs/EVENTS.md](EVENTS.md) — complete event schema with full payload
  definitions for all contracts
- [docs/AUDIT_TRAIL.md](AUDIT_TRAIL.md) — how to reconstruct historical state
  from events alone
- [docs/EVENT_VERSIONING.md](EVENT_VERSIONING.md) — backward-compatibility
  rules and `_v2` suffix discipline
- [indexer/README.md](../indexer/README.md) — off-chain event indexing service
