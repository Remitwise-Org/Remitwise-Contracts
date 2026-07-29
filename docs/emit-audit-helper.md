# `emit_audit(op, actor, meta)` — Shared Audit-Event Helper

**Crate:** `remitwise-common`  
**Issue:** #1268  
**Status:** Stable

---

## Motivation

Before this helper existed each contract emitted audit events via inline
`env.events().publish(...)` calls with slightly different topic shapes:

```rust
// remittance_split — one-off inline call (pre-#1268)
env.events().publish(
    (symbol_short!("split"), symbol_short!("audit")),
    (caller.clone(), amount, success),
);

// orchestrator — different shape
env.events().publish(
    (symbol_short!("orch"), symbol_short!("flow_exec")),
    (&entry.operation, &entry.executor, entry.success),
);
```

This made it impossible for indexers and compliance tools to subscribe to a
single canonical audit stream. An indexer had to know the bespoke topic tuple
for every contract. A new contract might forget to emit any audit event at all
and there was no compile-time enforcement.

`emit_audit` provides **one place** where the schema is defined, enforced, and
tested.

---

## API

```rust
pub fn emit_audit<T>(env: &Env, op: Symbol, actor: &Address, meta: T)
where
    T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
```

### Arguments

| Parameter | Type           | Description                                                                     |
|-----------|----------------|---------------------------------------------------------------------------------|
| `env`     | `&Env`         | Soroban environment.                                                            |
| `op`      | `Symbol`       | Short symbol (≤ 9 bytes) identifying the operation, e.g. `symbol_short!("settle")`. |
| `actor`   | `&Address`     | The principal that triggered the operation.                                      |
| `meta`    | `T: IntoVal`   | Compact operation-specific payload. Must serialise to ≤ 256 XDR bytes.         |

### Event schema

```text
topics = ("Remitwise", 5 /*Compliance*/, 2 /*High*/, "audit")
data   = meta   (caller-supplied IntoVal)
```

The topic tuple is always **identical** for every call to `emit_audit` regardless
of which contract, operation, or actor is involved. Indexers and compliance tools
can subscribe with a single filter:

```json
{ "topic": ["Remitwise", 5, 2, "audit"] }
```

The operation name, actor, and context are encoded inside `meta`, keeping the
topic layer stable across contract upgrades.

---

## Usage examples

```rust
use remitwise_common::emit_audit;
use soroban_sdk::symbol_short;

// --- Settlement audit ---
emit_audit(&env, symbol_short!("settle"), &caller, (amount, success));

// --- Access-control audit ---
emit_audit(&env, symbol_short!("access"), &member, role_u32);

// --- Flow execution audit ---
emit_audit(&env, symbol_short!("flow_exec"), &executor, (nonce, amount, true));
```

### Replacing inline calls

Before:
```rust
env.events().publish(
    (symbol_short!("orch"), symbol_short!("flow_exec")),
    (&entry.executor, entry.success),
);
```

After:
```rust
emit_audit(&env, symbol_short!("flow_exec"), &entry.executor, entry.success);
```

---

## Constraints

1. **`op` must be a short symbol (≤ 9 bytes).** Use `symbol_short!("…")` at call
   sites; a compile-time macro error occurs for strings longer than 9 bytes.

2. **`meta` must serialise to ≤ 256 XDR bytes.** In `#[cfg(test)]` builds the
   helper panics with
   `"emit_audit: meta payload size N exceeds 256-byte budget."` if this limit is
   exceeded. Keep meta payloads to IDs, amounts, and boolean result flags.

3. **`emit_audit` does not write to contract storage.** It is a pure event
   emitter. Contracts that need a bounded in-storage audit ring-buffer (e.g.
   the orchestrator's `AuditEntry` ring) must maintain that separately.

---

## Migration notes

`emit_audit` is **additive** — it emits a new event under the canonical topic
tuple. Existing ad-hoc audit publish calls in individual contracts can be left
in place or replaced, depending on whether the old topic tuple is monitored
downstream. If replacing an existing call, update any indexer queries that
subscribed to the old bespoke topic.

---

## Test coverage

`remitwise-common/src/lib.rs` contains an inline `emit_audit_tests` module
(gated with `#[cfg(test)]`) that asserts:

- Sentinel topic is `"Remitwise"`.
- Category is `EventCategory::Compliance` (discriminant 5).
- Priority is `EventPriority::High` (discriminant 2).
- Action topic is `symbol_short!("audit")`.
- Scalar and tuple meta payloads are accepted.
- Multiple sequential events are all recorded.
- Oversized payloads panic in test builds.

Run with:

```bash
cargo test -p remitwise-common emit_audit
```

---

## Related

- [`RemitwiseEvents::emit`](../remitwise-common/src/lib.rs) — generic event emitter.
- [`docs/EVENT_TAXONOMY.md`](EVENT_TAXONOMY.md) — category/priority encoding.
- [`docs/AUDIT_TRAIL.md`](AUDIT_TRAIL.md) — how to reconstruct historical state from events.
- [`docs/orchestrator-events.md`](orchestrator-events.md) — orchestrator lifecycle events.
