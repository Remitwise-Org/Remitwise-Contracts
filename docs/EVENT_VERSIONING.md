# Event Versioning Discipline

**Audience:** Downstream integrator — indexers, analytics pipelines, and any
service that subscribes to Remitwise on-chain events.

**Related docs:**
- [docs/EVENTS.md](EVENTS.md) — complete current event schema for all contracts
- [docs/EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) — `EventCategory` / `EventPriority` taxonomy
- [docs/events-versioning.md](events-versioning.md) — ADR explaining the original design decision
- [docs/INDEXING.md](INDEXING.md) — mapping contract events to off-chain tables
- [docs/AUDIT_TRAIL.md](AUDIT_TRAIL.md) — replaying events to reconstruct past state

---

## What this document covers

Remitwise contract events are consumed by off-chain indexers, analytics services,
and third-party integrators. This document states the rules contributors must
follow when changing an event schema, and the guarantees integrators can rely on.

---

## Stability guarantees

A _stable_ event satisfies all of the following:

1. **Topic symbols are immutable.** Once an action symbol (e.g. `"paid"`,
   `"created"`) appears in a merged contract, it will never be renamed or
   removed without a version suffix. An indexer that subscribes to `"paid"`
   today will continue to receive the same topic after any non-breaking upgrade.

2. **Payload fields are append-only.** Existing fields keep their name,
   type, and tuple position. New fields may be added to the *end* of the tuple;
   older indexers that decode only the leading fields continue to work.

3. **`EventCategory` and `EventPriority` discriminants are locked.** The `u32`
   values emitted in topic positions 2 and 3 (see
   [EVENT_TAXONOMY.md](EVENT_TAXONOMY.md)) are the same values tested in
   `remitwise-common` and must not be renumbered.

4. **Batch events always use `EventPriority::Low` (discriminant `0`).** The
   fourth topic element of a batch event is always `symbol_short!("batch")`.
   Both facts are locked by `emit_tests` in `remitwise-common/src/emit_tests.rs`.

---

## Breaking vs. non-breaking changes

| Change | Classification | Required action |
|--------|---------------|-----------------|
| Add a field at the *end* of the tuple | Non-breaking | Document in [EVENTS.md](EVENTS.md); bump minor version |
| Remove a field | **Breaking** | Version the event (`_v2`); announce migration window |
| Rename a field | **Breaking** | Version the event (`_v2`); announce migration window |
| Change a field's type | **Breaking** | Version the event (`_v2`); announce migration window |
| Reorder existing fields | **Breaking** | Version the event (`_v2`); announce migration window |
| Change the action symbol | **Breaking** | Version the event (`_v2`); announce migration window |
| Add a brand-new event (new topic) | Non-breaking | Document in [EVENTS.md](EVENTS.md) |
| Change `EventCategory` / `EventPriority` discriminant | **Breaking** | Forbidden — update [EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) instead |

---

## Versioning an event: step-by-step

When a breaking change is unavoidable, version the event by appending a `_v2`
suffix to the action symbol. Do **not** mutate the existing event in place.

### 1. Introduce the `_v2` emission

```rust
// bill_payments/src/lib.rs

// v1 (keep in place — do not remove until deprecation window has passed)
RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    symbol_short!("paid"),           // topic: "paid"
    (bill_id, caller.clone(), paid_amount),
);

// v2 (new) — adds platform_fee field at the end
RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    Symbol::new(&env, "paid_v2"),    // topic: "paid_v2"
    (bill_id, caller.clone(), paid_amount, platform_fee),
);
```

> **`#![no_std]` rule:** Use `Symbol::new(&env, "paid_v2")` for action symbols
> longer than nine characters — `symbol_short!` panics for names exceeding the
> nine-character limit at compile time. Short names (≤ 9 chars) may continue to
> use `symbol_short!`.

### 2. Update `docs/EVENTS.md`

Document the old and new schemas side-by-side in
[docs/EVENTS.md](EVENTS.md). Mark the v1 section **Deprecated (see `paid_v2`)**.

### 3. Update the schema-stability test

Each contract has an `events_schema_test` module. Update it to cover the new
`_v2` shape as a separate commit on top of the version bump so reviewers can
audit the diff in isolation:

```rust
// bill_payments/src/events_schema_test.rs
#[test]
fn paid_v2_schema_is_stable() {
    let env = Env::default();
    // Verify topic symbol
    assert_eq!(
        Symbol::new(&env, "paid_v2"),
        Symbol::new(&env, "paid_v2"),
    );
    // Verify payload field count: (bill_id, caller, amount, platform_fee)
    let payload = (1_u32, Address::generate(&env), 5000_i128, 50_i128);
    let val: Val = payload.into_val(&env);
    let _roundtrip: (u32, Address, i128, i128) = val.try_into_val(&env).unwrap();
}
```

### 4. Coordinate the migration window

Before the contract upgrade is executed on mainnet:

1. Notify all known indexer operators (open a migration-tracking issue).
2. Give operators at least **two ledger upgrade cycles** (typically ≥ 2 weeks)
   to deploy parsers that handle the new topic.
3. Both v1 and v2 events are emitted simultaneously during the transition
   window.
4. After all operators confirm readiness, remove the v1 emission in the next
   scheduled upgrade.

### 5. Remove the old emission

Once the migration window closes:

- Delete the v1 `RemitwiseEvents::emit` call.
- Mark the v1 schema in `docs/EVENTS.md` as **Removed in contract v{N}**.
- Remove the v1 schema-stability test assertions.
- Keep the v2 stability test permanently.

---

## Concrete example: adding `platform_fee` to `BillPaid`

This example walks through the full lifecycle of a real breaking change.

### Before the change (v1 shape)

```rust
// Emitted by: pay_bill, batch_pay_bills
// Topic: ("Remitwise", 0 /*Transaction*/, 2 /*High*/, "paid")
// Payload: (bill_id: u32, caller: Address, amount: i128)

RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    symbol_short!("paid"),
    (bill_id, caller.clone(), paid_amount),
);
```

An indexer table might look like:

```sql
CREATE TABLE bill_paid_events (
    bill_id  INTEGER,
    caller   TEXT,
    amount   BIGINT,
    ledger   INTEGER,
    tx_hash  TEXT
);
```

### The breaking change (v2 shape)

The contract needs to include a `platform_fee` deducted before the net payout.
Because `platform_fee` changes the meaning of `amount` and is injected between
`amount` and the end, it is a **breaking change**.

```rust
// During the transition window: emit BOTH v1 and v2

// v1 — unchanged, still emitted for running indexers
RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    symbol_short!("paid"),
    (bill_id, caller.clone(), paid_amount),
);

// v2 — new shape; platform_fee appended
RemitwiseEvents::emit(
    &env,
    EventCategory::Transaction,
    EventPriority::High,
    Symbol::new(&env, "paid_v2"),
    (bill_id, caller.clone(), paid_amount, platform_fee),
);
```

The indexer adds a parallel table and migrates once the window closes:

```sql
CREATE TABLE bill_paid_v2_events (
    bill_id       INTEGER,
    caller        TEXT,
    amount        BIGINT,
    platform_fee  BIGINT,
    ledger        INTEGER,
    tx_hash       TEXT
);
```

After the window closes, the v1 emission is removed and only `paid_v2` is emitted.

---

## Subscribing to events: recommended patterns

All Remitwise events share the same outer topic structure emitted by
`RemitwiseEvents::emit`:

```
topics[0]  symbol_short!("Remitwise")          — contract namespace
topics[1]  EventCategory::Transaction.to_u32() — u32 category discriminant
topics[2]  EventPriority::High.to_u32()        — u32 priority discriminant
topics[3]  symbol_short!("paid")               — action symbol (version-specific)
```

Batch events (emitted by `RemitwiseEvents::emit_batch`) always use:

```
topics[3]  symbol_short!("batch")
data       (action: Symbol, count: u32)
```

### Filtering by topic on the Horizon / RPC API

To subscribe to all `BillPaid` events regardless of version, subscribe to the
`Remitwise` contract address and filter topics[1] for `EventCategory::Transaction`
(`0`) in your indexer, then branch on the action symbol:

```typescript
function handleBillEvent(topics: string[], data: unknown[]) {
    const action = topics[3]; // "paid" or "paid_v2"

    if (action === "paid") {
        const [bill_id, caller, amount] = data as [number, string, bigint];
        insertBillPaidV1({ bill_id, caller, amount });
    } else if (action === "paid_v2") {
        const [bill_id, caller, amount, platform_fee] = data as [number, string, bigint, bigint];
        insertBillPaidV2({ bill_id, caller, amount, platform_fee });
    }
    // Unknown action: log and skip — do not crash
}
```

> **Do not crash on unknown action symbols.** New event versions will appear
> before your indexer is updated. Log the unknown symbol and continue.

---

## Verifying schema stability locally

The `events_schema_test` modules pin each event's topic symbols, payload field
set, and enum discriminants. A failing test is the signal that a change is
breaking for indexers.

```bash
# Run all schema-stability tests across the workspace
cargo test --workspace events_schema_test

# Run just the remitwise-common emit tests
cargo test -p remitwise-common emit_tests
```

See [docs/EVENTS.md — Schema Stability Tests](EVENTS.md#schema-stability-tests)
for the full per-contract test module table.

---

## FAQ

**Q: Why append `_v2` to the topic symbol instead of a version field in the payload?**  
A: Horizon and RPC nodes filter events on topics, not payloads. A version field
in the payload requires fetching and decoding every event before knowing whether
the indexer can handle it — wasting bandwidth and CPU. Topic-level versioning
lets the RPC layer do the filtering. See
[docs/events-versioning.md](events-versioning.md) for the full ADR.

**Q: Why not emit both v1 and v2 permanently?**  
A: Each `env.events().publish()` call consumes contract execution budget. In
high-throughput flows (e.g. `execute_due_bill_schedules` processing many
schedules), doubled event costs accumulate. Dual emission is a migration aid,
not a permanent configuration.

**Q: What if I need to add a field that is not at the end?**  
A: You cannot. Inserting at a non-terminal position shifts all subsequent field
offsets, which is equivalent to changing every downstream field's type from
the indexer's perspective. Model the new field such that it appends naturally,
or start a new event with a `_v2` suffix.

**Q: How do I know which version a running contract emits?**  
A: Query `get_version()` on the contract, then cross-reference with the version
table in [docs/EVENTS.md — Version Compatibility](EVENTS.md#version-compatibility).
Subscribe to `ContractUpgraded` (`"upgraded"`) events to receive automatic
notifications when the contract version changes.

**Q: Are `EventCategory` and `EventPriority` discriminants stable?**  
A: Yes. They are asserted in `remitwise-common` tests and are part of the
on-wire topic format. Changing them would silently break any indexer filtering
on topic positions 2 or 3. See [EVENT_TAXONOMY.md](EVENT_TAXONOMY.md).
