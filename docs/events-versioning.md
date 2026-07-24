# Architecture Decision Record: Event Versioning via `_v2` Suffix

**Status:** Accepted
**Audience:** Contributor

## Summary

When an existing Soroban contract event schema must change (e.g., adding a new field, changing a type), we append a version suffix (like `_v2`) to the primary event topic instead of mutating the existing event payload or using internal payload version fields. 

## Background

Soroban contract events are critical for off-chain indexing (like the `quicklendx-backend` or `Remitwise` indexer). Downstream integrators, operators, and our own backend rely on predictable event structures. If we modify the `BillPaid` event to include a new `platform_fee` field, existing indexers that rigidly deserialize the payload into an older struct will crash or drop the event. 

We need a way to evolve event schemas without breaking backwards compatibility for running indexers. 

## Decision

We version events by changing the event topic itself. We add a `_v2` (or `_v3`, etc.) suffix to the event name in the topic tuple. 

### Concrete Example

**Before (v1):**
```rust
use soroban_sdk::{Env, Symbol, symbol_short, Address};

pub fn emit_paid(env: &Env, bill_id: u32, amount: i128) {
    let topics = (Symbol::new(env, "paid"),);
    let payload = (bill_id, amount);
    env.events().publish(topics, payload);
}
```

**After (v2):**
Adding a `platform_fee` to the settlement event.
```rust
use soroban_sdk::{Env, Symbol, Address};

pub fn emit_paid_v2(env: &Env, bill_id: u32, amount: i128, platform_fee: i128) {
    // Topic changed to include _v2 suffix
    let topics = (Symbol::new(env, "paid_v2"),);
    let payload = (bill_id, amount, platform_fee);
    env.events().publish(topics, payload);
}
```

By doing this, an indexer listening for `paid` will simply ignore `paid_v2` until it is upgraded, rather than panicking on a payload size mismatch.

## Alternatives Considered

1. **Version field in the payload:**
   ```rust
   // e.g. payload = (version_u32, bill_id, amount)
   ```
   *Trade-offs:* Horizon and RPC node filters work on *topics*, not payloads. If we put the version in the payload, an indexer must fetch and deserialize the payload to know if it can handle the event. This wastes RPC bandwidth and CPU cycles.

2. **Emitting both v1 and v2 events simultaneously:**
   *Trade-offs:* While extremely safe for downstream consumers, this doubles the event emission cost (gas/fees) for the contract on every invocation. In high-throughput DeFi protocols, this cost is prohibitive.

3. **Deploying a new contract instance:**
   *Trade-offs:* Migrating liquidity and state to a new contract just to change an event schema is operationally heavy and disrupts the protocol. 

## Implementation Guidelines

- Keep `#![no_std]` discipline: ensure you use `soroban_sdk::Symbol::new(&env, "Event_v2")` rather than standard string manipulation.
- When introducing a `_v2` event, clearly document the new fields in `docs/EVENTS.md`.
- Ensure the backend indexer is updated to listen to the new `_v2` topic before the contract upgrade is executed on Mainnet.
