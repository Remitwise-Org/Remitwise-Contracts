# Audit Trail: Reconstructing Historical State from Events

**Audience:** Downstream integrators — indexers, compliance tools, and
analytics pipelines that need to reproduce any past state of the Remitwise
contracts from on-chain data alone, without access to live contract storage.

---

## Background

Every Remitwise contract emits structured Soroban events for each
state-changing operation. Those events are the authoritative, immutable record
of what happened and when. This document explains how to replay those events in
order to reproduce any snapshot of user data (bills, savings goals, insurance
policies, remittance flows) as it existed at any past ledger.

Live contract storage is a cache. The event log is the truth.

> **Related reading:**
> - [EVENTS.md](../EVENTS.md) — complete event schema reference
> - [docs/EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) — category/priority encoding
> - [docs/orchestrator-events.md](orchestrator-events.md) — orchestrator lifecycle events
> - [docs/orchestrator-audit-retention.md](orchestrator-audit-retention.md) — on-chain audit log rotation
> - [indexer/README.md](../indexer/README.md) — reference off-chain indexer implementation

---

## How Events Are Published

All contracts use `env.events().publish()` with a consistent four-element topic
tuple:

```rust
// from remitwise-common/src/lib.rs  (RemitwiseEvents::emit)
let topics = (
    symbol_short!("Remitwise"), // constant namespace
    category.to_u32(),          // EventCategory  (0–4)
    priority.to_u32(),          // EventPriority  (0–2)
    action,                     // Symbol, e.g. symbol_short!("paid")
);
env.events().publish(topics, data);
```

A secondary `(namespace, variant)` topic is also emitted by most contracts for
finer-grained subscriptions:

```rust
// e.g. bill_payments
env.events().publish((symbol_short!("bill"), BillEvent::Paid), data);
```

Batch operations emit one per-item event **plus** a single summary event:

```rust
// RemitwiseEvents::emit_batch
let topics = (
    symbol_short!("Remitwise"),
    category.to_u32(),
    EventPriority::Low.to_u32(),
    symbol_short!("batch"),
);
env.events().publish(topics, (action, count));
```

The `category` and `priority` integer encodings are:

| Variant | Value |
|---------|-------|
| `Transaction` | `0` |
| `State` | `1` |
| `Alert` | `2` |
| `System` | `3` |
| `Access` | `4` |

| Variant | Value |
|---------|-------|
| `Low` | `0` |
| `Medium` | `1` |
| `High` | `2` |

---

## Fetching Events from the RPC

Use the Stellar RPC `getEvents` method, filtering by contract address and
optionally by topic segment. Pagination is ledger-range based.

```bash
# Fetch all events from the bill_payments contract between ledger 1000 and 2000
curl -X POST https://soroban-testnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "getEvents",
    "params": {
      "startLedger": 1000,
      "filters": [
        {
          "type": "contract",
          "contractIds": ["<BILL_PAYMENTS_CONTRACT_ID>"],
          "topics": [["*", "*", "*", "*"]]
        }
      ],
      "pagination": { "limit": 200 }
    }
  }'
```

To narrow to a single event type, pin the action symbol in position 4:

```bash
# Only BillPaid events (category=0 / Transaction, priority=2 / High, action="paid")
"topics": [["AAAADQAAAAlSZW1pdHdpc2U=", "AAAABAAAAAA=", "AAAABAAAAAI=", "AAAADQAAAARwYWlk"]]
```

> The base64-encoded values above are the XDR `ScVal` representations of
> `symbol_short!("Remitwise")`, `u32(0)`, `u32(2)`, and `symbol_short!("paid")`
> respectively. Use the `soroban-sdk` `IntoVal` helpers or the Stellar SDK's
> `xdr` module to produce them programmatically.

---

## Replay Walkthrough: Bill Payments

This section shows, step by step, how to reconstruct the full state of all
bills owned by a given address from events alone.

### Events involved

| Event (secondary topic) | Payload fields | Effect on state |
|-------------------------|----------------|-----------------|
| `(bill, Created)` | `(bill_id, owner, name?, amount, due_date, recurring, created_at)` | Insert bill record |
| `(bill, Paid)` | `(bill_id, owner, external_ref?)` | Set `paid = true`, record `paid_at` |
| `(bill, RecurringBillCreated)` | `(bill_id, parent_bill_id, ...)` | Insert next-cycle bill |
| `(bill, ExternalRefUpdated)` | `(bill_id, owner, external_ref?)` | Update `external_ref` |
| `(bill, Cancelled)` | `(bill_id, owner, cancelled_at)` | Remove bill from active set |
| `(bill, Archived)` | `(count, archived_at)` | Move paid bills to archive |
| `(bill, Restored)` | `(bill_id, owner, restored_at)` | Move bill back from archive |

### Reconstruction algorithm

```
state = {
  active:   Map<bill_id, Bill>,
  archived: Map<bill_id, Bill>,
}

for each event E in ledger order:
  match secondary topic variant:

    Created:
      state.active[bill_id] = new Bill from payload

    Paid:
      state.active[bill_id].paid    = true
      state.active[bill_id].paid_at = E.ledger_timestamp

    RecurringBillCreated:
      state.active[new_bill_id] = new Bill cloned from parent with updated due_date

    ExternalRefUpdated:
      state.active[bill_id].external_ref = payload.external_ref

    Cancelled:
      delete state.active[bill_id]

    Archived:
      for each bill in state.active where bill.paid == true:
        state.archived[bill.id] = bill
        delete state.active[bill.id]

    Restored:
      state.active[bill_id] = state.archived[bill_id]
      delete state.archived[bill_id]
```

To answer "what were Alice's unpaid bills at ledger 1500?", replay all events
up to and including ledger 1500 then read `state.active` filtered by
`owner == Alice && paid == false`.

### Concrete example

```
Ledger 1010  (bill, Created)    → bill_id=7, owner=Alice, amount=150, due_date=1735689600
Ledger 1020  (bill, Created)    → bill_id=8, owner=Alice, amount=80,  due_date=1735776000
Ledger 1050  (bill, Paid)       → bill_id=7, owner=Alice
Ledger 1100  (bill, Archived)   → count=1
Ledger 1120  (bill, Restored)   → bill_id=7, owner=Alice
```

After replaying to ledger 1120:
- `state.active`   = `{ 7: paid=true, 8: paid=false }`
- `state.archived` = `{}`

Alice's unpaid bills at ledger 1100 (before Restored): `{ 8 }`.

---

## Replay Walkthrough: Savings Goals

| Event | Payload | Effect |
|-------|---------|--------|
| `created` | `goal_id, owner, name, target_amount, target_date, timestamp` | Insert goal |
| `added` | `goal_id, owner, amount, new_total, timestamp` | `current_amount += amount` |
| `completed` | `goal_id, name, final_amount, timestamp` | Set `completed = true` (fires once) |
| `withdrawn` | `goal_id, owner, amount, new_total, timestamp` | `current_amount -= amount` |
| `GoalLocked` / `GoalUnlocked` | `goal_id, locked, timestamp` | Set lock flag |

The `completed` event fires **exactly once per goal** — the first time
`current_amount >= target_amount`. Downstream consumers can treat it as an
idempotent edge trigger: receiving it twice for the same `goal_id` indicates a
replay of old ledgers, not a new completion.

---

## Replay Walkthrough: Remittance Flows (Orchestrator)

The orchestrator emits three lifecycle events per execution attempt:

```
flow      → execution started  (executor, amount)
flow_ok   → completed          (executor, amount)
flow_fail → failed             (executor, error_code)
```

Every execution also writes an `AuditEntry` to the on-chain ring buffer
(key `AUDIT`, capped at `MAX_AUDIT_ENTRIES = 100`). Because the ring buffer
rotates, integrators that need full history **must** archive entries externally
before they are evicted. The recommended pattern:

```
1. Call get_execution_stats() and read evicted_entries counter.
2. If evicted_entries advanced since last checkpoint:
     a. Call get_audit_log(0, 100) to read the full current window.
     b. Persist any entries not yet in your local store.
3. Record the new evicted_entries value as the baseline.
```

To reconstruct "did flow F succeed?", find the `flow` event with the matching
`(executor, amount)` in the target ledger range, then look for a subsequent
`flow_ok` or `flow_fail` before the next `flow` from the same executor.

---

## Replay Walkthrough: Insurance Premiums

| Event | Payload | Effect |
|-------|---------|--------|
| `PolicyCreated` | `policy_id, name, coverage_type, monthly_premium, coverage_amount, timestamp` | Insert policy |
| `PremiumPaid` | `policy_id, owner, amount, next_payment_date, timestamp` | Advance `next_payment_date` by 30 days |
| `PolicyDeactivated` | `policy_id, name, timestamp` | Set `active = false` |
| `ScheduleMissed` | `schedule_id, policy_id, missed_count, timestamp` | Log gap; `next_payment_date` was already advanced past missed intervals |

The `next_payment_date` in each `PremiumPaid` payload is the authoritative
value after that payment. Do not compute it yourself — use the emitted value.

---

## System Events That Affect All State

These events alter the operational mode of a contract and must be honoured
during replay:

| Event | Symbol | Meaning for replay |
|-------|--------|--------------------|
| Contract paused | `paused` | Operations after this ledger will have failed; no state changes expected |
| Contract unpaused | `unpaused` | Operations resume |
| Contract upgraded | `upgraded` | Payload carries `previous_version` and `new_version`; schema may change at this boundary |

When replaying across an `upgraded` event, check [EVENTS.md](../EVENTS.md)
§ Version Compatibility for any field additions introduced in the new version.
New fields appended to existing payloads are optional and absent in events
emitted before the upgrade.

---

## Verifying Your Reconstruction

The `bill_payments` crate ships event schema stability tests that pin every
topic symbol, payload shape, and variant set:

```bash
cargo test -p bill_payments --test events_schema_test
```

For savings goals:

```bash
cargo test -p savings_goals
```

These tests run against the live `Env` and assert exact round-trip
serialization, so a passing suite means the events you read from chain will
deserialize cleanly with the same types shown in [EVENTS.md](../EVENTS.md).

---

## Checklist for a New Integrator

- [ ] Identify which contract addresses to watch (see deployment JSON or
      [DEPLOYMENT.md](../DEPLOYMENT.md)).
- [ ] Record the deployment ledger for each contract; use it as `startLedger`
      when calling `getEvents`.
- [ ] Subscribe to both topic forms: the primary `("Remitwise", cat, pri, action)`
      tuple and the secondary `(namespace, BillEvent::Variant)` tuple.
      They carry different field sets for the same operation.
- [ ] Store raw events as-is before processing; it lets you re-derive state
      without re-fetching from the RPC.
- [ ] Poll `get_execution_stats()` on the orchestrator regularly and archive
      `AuditEntry` records before the ring buffer rotates them out.
- [ ] Handle `upgraded` events by refreshing your schema reference and
      treating new optional fields as absent for older events.
- [ ] Validate your reconstruction against the on-chain query result for a
      known address to confirm parity before going to production.
