# Dispute Epoch Model

**Audience:** Contributors working on cross-contract dispute lifecycle operations and integrators handling dispute-related calls.

---

## What a dispute epoch is

The `remitwise-common` shared library introduces the concept of a **dispute epoch**, a monotonically increasing `u64` counter stored in instance storage under the key `DISP_EP` (`symbol_short!("DISP_EP")`).

This counter tracks the current active generation of dispute resolutions.

```
┌─────────────────────────────────┐
│  Contract Instance Storage      │
│                                 │
│  DISP_EP  →  u64 (e.g. 5)       │
└─────────────────────────────────┘
```

The epoch starts at 0 (implicitly, if unset) and is monotonically incremented when necessary. It never resets.

---

## Why dispute epochs exist

The primary threat mitigated by dispute epochs is **lifecycle bypass via stale operations**.

When a dispute is raised, evaluated, and resolved, various lifecycle rules apply (e.g., locking funds, reversing transactions). If an attacker could execute dispute-related operations tied to an *outdated* dispute generation, they might bypass lifecycle expiration rules, allowing them to manipulate resolutions, unlock funds prematurely, or lock funds unexpectedly.

Bumping the dispute epoch invalidates all pending dispute operations tied to previous epochs.

---

## Coordination and Semantics

### The Guard (`require_no_pending_dispute_epoch`)

```rust
// remitwise-common/src/lib.rs
pub fn require_no_pending_dispute_epoch(env: &Env, ep: u64) -> Result<(), DisputeError> {
    let current_epoch: u64 = env.storage().instance().get(&symbol_short!("DISP_EP")).unwrap_or(0);
    if ep < current_epoch {
        return Err(DisputeError::OutdatedEpoch);
    }
    Ok(())
}
```

Notice the specific check: `ep < current_epoch`.
Unlike the cross-contract coordination epoch which demands strict equality (`ep == current_epoch`), the dispute epoch allows operations from the *current* or *future* epochs, but explicitly rejects **outdated** epochs (`ep < current_epoch`).

### Epoch Tiers

| Tier | Definition | Guard outcome |
|---|---|---|
| **Same** | `supplied_epoch == current_epoch` | ✅ Accepted |
| **Future** | `supplied_epoch > current_epoch` | ✅ Accepted |
| **Prior (Outdated)** | `supplied_epoch < current_epoch` | ❌ `DisputeError::OutdatedEpoch` |

---

## Bumping the dispute epoch

Bumping the dispute epoch means advancing the `DISP_EP` counter in instance storage. 

**Effect on callers:** Any pending cross-contract dispute operations carrying an epoch smaller than the new `current_epoch` will instantly become stale and fail with `DisputeError::OutdatedEpoch`.

### When to bump

The dispute epoch should be bumped:
1. When a major dispute generation completes and the system needs to forcefully invalidate any straggling, unexecuted dispute commands from that generation.
2. During dispute-related emergency interventions where pending actions must be halted immediately.

---

## Worked example

```text
Time  On-chain epoch  Event
────  ─────────────  ─────────────────────────────────────────
 0    0              Contract initialized, DISP_EP implicitly 0
 1    0              Caller submits dispute operation with ep=0 → ✅ accepted
 2    1              Admin bumps dispute epoch → current_epoch = 1
 3    1              Attacker replays old dispute operation with ep=0 → ❌ DisputeError::OutdatedEpoch
 4    1              Caller submits new dispute operation with ep=1 → ✅ accepted
 5    1              Caller anticipates next epoch, submits ep=2 → ✅ accepted
```

---

## Error reference

| Error | Discriminant | When raised |
|---|---|---|
| `DisputeError::OutdatedEpoch` | 36 | `ep < current_epoch` in `require_no_pending_dispute_epoch` |

---

## See also

- [Cross-Contract Epochs](CROSS_CONTRACT_EPOCHS.md) - For the related but strictly-enforced orchestrator actor epochs.
