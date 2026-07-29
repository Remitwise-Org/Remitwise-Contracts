# Cross-Contract Epochs

**Audience:** Contributors and downstream integrators building services that call
`execute_remittance_flow_signed` or coordinate with the orchestrator contract.

---

## What an epoch is

The orchestrator contract keeps a single `u64` counter in instance storage
under the key `ACT_EPOCH` (symbol `"ACT_EPOCH"`).  That counter is called the
**actor epoch**.  Every signed request to `execute_remittance_flow_signed` must
carry the current epoch value; if it does not match exactly, the call is
rejected with `OrchestratorError::EpochMismatch`.

```
┌─────────────────────────────────┐
│  Orchestrator contract          │
│  instance storage               │
│                                 │
│  ACT_EPOCH  →  u64 (e.g. 3)   │
└─────────────────────────────────┘
```

The epoch starts at 0 after `init` and is monotonically incremented by the
owner via `bump_actor_epoch`.  There is no automatic rollover and the value
never resets except through a re-deploy.

---

## Why epochs exist

The primary threat is **stale token replay**: an attacker who obtains a signed
request (e.g. from a compromised relayer or a leaked API key) should not be
able to replay it after the signing service is rotated.

Bumping the epoch atomically invalidates every token signed under the previous
value without requiring individual per-actor revocation lists.

---

## Coordination protocol

### Producing a signed request (caller side)

```
1. Read the current epoch:
       epoch = orchestrator.get_actor_epoch_public()

2. Build the request hash (binds operation, nonce, amount, deadline, routing):
       hash = compute_request_hash("flow", nonce, amount, deadline,
                                   goal_id, bill_id, policy_id)

3. Submit:
       orchestrator.execute_remittance_flow_signed(
           executor, amount, nonce, deadline, hash,
           actor_epoch = epoch   ← must match on-chain value exactly
       )
```

`get_actor_epoch_public` is a read-only view — it costs no auth and has no
side effects.  Call it immediately before building each signed request so the
embedded epoch is never stale.

### What the guard checks (contract side)

```rust
// orchestrator/src/lib.rs — verify_matching_epoch (private helper)
fn verify_matching_epoch(env: &Env, actor_epoch: u64) -> Result<(), OrchestratorError> {
    let current_epoch = Self::get_actor_epoch(env);    // reads ACT_EPOCH
    if actor_epoch != current_epoch {
        return Err(OrchestratorError::EpochMismatch);  // strict equality
    }
    Ok(())
}
```

The check is **strict equality**: there is no tolerance window, no off-by-one
leniency, no grace period for "immediately prior" tokens.  The same
`EpochMismatch` error is returned whether the supplied value is one behind,
one ahead, or `u64::MAX` away from the current epoch.

---

## Epoch tiers

| Tier | Definition | Guard outcome |
|---|---|---|
| **Same** | `actor_epoch == current_epoch` | ✅ Accepted |
| **Off-by-one (prior)** | `actor_epoch == current_epoch - 1` | ❌ `EpochMismatch` |
| **Off-by-one (future)** | `actor_epoch == current_epoch + 1` | ❌ `EpochMismatch` |
| **Ancient** | `actor_epoch << current_epoch` (e.g. 0 after 10 bumps) | ❌ `EpochMismatch` |

---

## Bumping the epoch

Only the contract owner may call `bump_actor_epoch`.  The call increments the
counter by exactly 1 and emits:

```
topic: ("orch", "epoch_bump")
data:  (old_epoch: u64, new_epoch: u64)
```

**Effect on callers:** every cached epoch value is immediately stale.  All
in-flight requests that have not yet been submitted will fail with
`EpochMismatch`.  Callers must call `get_actor_epoch_public()` again before
retrying.

### When to bump

- After rotating a signing service or API key
- After detecting a suspicious pattern of requests from an old token
- As part of a scheduled key rotation cadence

There is no cost to bump frequently; the only consequence is that callers
holding pre-computed tokens must refresh them.

---

## Worked example

```text
Time  On-chain epoch  Event
────  ─────────────  ─────────────────────────────────────────
 0    0              init() — epoch initialised to 0
 1    0              Caller queries get_actor_epoch_public() → 0
 2    0              Caller submits with actor_epoch=0 → ✅ accepted
 3    1              Owner calls bump_actor_epoch() → epoch = 1
 4    1              Caller submits with actor_epoch=0 → ❌ EpochMismatch
 5    1              Caller queries get_actor_epoch_public() → 1
 6    1              Caller submits with actor_epoch=1 → ✅ accepted
```

---

## Relationship to nonce and deadline

The epoch guard fires **before** the nonce and deadline checks:

```
1. executor.require_auth()
2. Validate initialization
3. Check amount > 0
4. Reentrancy guard
5. ← verify_matching_epoch (epoch check)
6. Validate nonce + deadline + request hash
7. Execute downstream cross-contract calls
```

A request that arrives with the correct epoch but a consumed nonce still fails
(at step 6).  A request with a wrong epoch never reaches step 6.  This
ordering means epoch mismatches are surfaced early and are easily distinguished
from nonce-replay errors in indexer logs.

---

## Error reference

| Error | Discriminant | When raised |
|---|---|---|
| `EpochMismatch` | 15 | `actor_epoch != current_epoch` in `verify_matching_epoch` |

The error discriminant is stable: it is tested in the compatibility-guard suite
and must not be renumbered.

---

## Testing

Cross-contract epoch guard behaviour is covered in two test files:

| File | What it covers |
|---|---|
| `orchestrator/tests/dispute_epoch_guard.rs` | Intra-contract same / prior / ancient / future / sweep |
| `orchestrator/tests/cross_contract_epoch_guard.rs` | Cross-contract same / off-by-one / ancient / sweep + bump-transition |

Run with:

```bash
cargo test -p orchestrator
```

---

## See also

- [docs/OBSERVABILITY_MODEL.md](OBSERVABILITY_MODEL.md) — `epoch_bump` event
  schema and off-chain consumption guidelines
- [orchestrator/src/lib.rs](../orchestrator/src/lib.rs) — `verify_matching_epoch`,
  `bump_actor_epoch`, `get_actor_epoch_public`
- [docs/AUDIT_TRAIL.md](AUDIT_TRAIL.md) — How to reconstruct historical state
  from events alone
