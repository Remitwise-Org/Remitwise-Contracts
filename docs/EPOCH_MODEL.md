# Epoch Model: Authorization Invalidation via Counter Bumps

## Audience

This document is written for **contributors** (engineers creating, modifying, or reviewing Soroban smart contracts in `Remitwise-Contracts`). It explains why and how the workspace uses monotonically increasing epoch counters to invalidate stale authorizations, and what each epoch bump invalidates downstream.

---

## Overview

An **epoch** is a `u64` counter stored in contract instance storage that is atomically incremented by an authorized caller (admin or owner). Any signed authorization or actor token that was created at or before the old epoch becomes **permanently invalid** once the epoch is bumped.

RemitWise currently defines two independent epoch mechanisms:

| Contract | Epoch Name | Storage Key | Initial Value | Who Bumps | What It Invalidates |
|---|---|---|---|---|---|
| `emergency_killswitch` | KillSwitchEpoch | `DataKey::KillSwitchEpoch` | `0` | Admin | Stale `transfer_admin` authorization payloads |
| `orchestrator` | ActorEpoch | `symbol_short!("ACT_EPOCH")` | `0` | Owner | Stale actor tokens for signed remittance flows |

Both follow the same pattern: a monotonic `u64` counter, initialized at contract deployment, bumped on demand by a privileged caller, and verified at every sensitive entry point that consumes an externally supplied authorization.

---

## Threat Model (Why Epochs Exist)

Without an epoch check, a signed authorization payload can be captured by an observer or exfiltraded from a compromised signing service and replayed indefinitely. The contract has no way to distinguish a fresh authorization from a replay of one that the signer intended to be revoked.

By embedding the current epoch **inside the authorization payload** (as a field that the signer signs over) and checking it against the contract's current epoch, the contract gains the ability to:

1. **Revoke all prior authorizations atomically** — a single bump invalidates every payload signed at or before the old epoch.
2. **Prevent indefinite replay** — an attacker who obtained a payload before a bump cannot use it after.
3. **Audit epoch changes** — every bump emits an event with the old and new values, giving operators a verifiable trail.

---

## How Epochs Bump

Both epoch mechanisms follow the same bump procedure:

1. **Authorization**: The caller must be the current admin (killswitch) or owner (orchestrator), verified via `require_auth()` and an identity check.
2. **Read current**: Retrieve the stored epoch (defaulting to `0` if unset).
3. **Increment**: `old_epoch.checked_add(1)`. Returns `Overflow` on wrap (practically unreachable — `u64::MAX` bumps would take billions of years at once-per-ledger).
4. **Write**: Store the new epoch back to instance storage.
5. **Emit event**: Publish an `(action, "epch_bump")` event with `(old_epoch, new_epoch)` for off-chain observability.

The bump is **monotonic**: each new value is strictly greater than the previous. Epochs never reset, wrap, or decrease.

---

## Concrete Examples

### 1. Emergency Killswitch: Stale Admin-Transfer Replay

The `emergency_killswitch` contract uses its epoch to protect `transfer_admin`. Without this check, an attacker who obtains a signed `transfer_admin` payload (e.g., by compromising a hot wallet that signed it before the admin realised the compromise) could replay it to seize admin authority.

#### Storage Initialization

```rust
// In initialize():
env.storage()
    .instance()
    .set(&DataKey::KillSwitchEpoch, &0u64);
```

#### Verification Guard

```rust
pub fn require_matching_kill_switch_epoch(env: Env, ep: u64) -> Result<(), Error> {
    let current: u64 = env
        .storage()
        .instance()
        .get(&DataKey::KillSwitchEpoch)
        .unwrap_or(0);
    if ep != current {
        return Err(Error::EpochMismatch);
    }
    Ok(())
}
```

#### Entry Point That Consumes the Epoch

```rust
pub fn transfer_admin(env: Env, new_admin: Address, ep: u64) -> Result<(), Error> {
    // Reject if the caller-supplied epoch does not match the current contract epoch.
    // This invalidates every transfer_admin payload signed at or before the old epoch.
    Self::require_matching_kill_switch_epoch(env.clone(), ep)?;

    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();

    // ... validate new_admin, update storage, emit event ...
    Ok(())
}
```

#### Bump Function

```rust
/// Admin-only. Atomically invalidates all prior transfer_admin authorizations.
pub fn bump_kill_switch_epoch(env: Env, caller: Address) -> Result<u64, Error> {
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)?;
    admin.require_auth();
    if caller != admin {
        return Err(Error::Unauthorized);
    }

    let old_epoch: u64 = env
        .storage()
        .instance()
        .get(&DataKey::KillSwitchEpoch)
        .unwrap_or(0);
    let new_epoch = old_epoch
        .checked_add(1)
        .ok_or(Error::InvalidAdmin)?; // overflow guard

    env.storage()
        .instance()
        .set(&DataKey::KillSwitchEpoch, &new_epoch);

    env.events().publish(
        (symbol_short!("emergency"), symbol_short!("epch_bump")),
        (old_epoch, new_epoch),
    );
    Ok(new_epoch)
}
```

#### Concrete Test

```rust
#[test]
fn test_stale_epoch_rejected_after_bump() {
    let (env, client) = setup_env();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);

    // Bump the epoch to 1
    let new_epoch = client.bump_kill_switch_epoch(&admin);
    assert_eq!(new_epoch, 1);

    // Transfer with old epoch 0 should now fail
    let res = client.try_transfer_admin(&new_admin, &0);
    assert_eq!(res, Err(Ok(Error::EpochMismatch)));

    // Transfer with new epoch 1 should succeed
    let res = client.try_transfer_admin(&new_admin, &1);
    assert_eq!(res, Ok(Ok(())));
}
```

---

### 2. Orchestrator: Stale Actor-Token Replay

The `orchestrator` contract uses its actor epoch to protect `execute_remittance_flow_signed`. An **actor token** is a signed authorization that delegates execution rights to a specific executor for a specific amount, nonce, deadline, and request hash. Without the epoch check, a captured token could be replayed forever.

#### Storage Initialization

```rust
// In initialize():
env.storage().instance().set(&ACTOR_EPOCH, &0u64);
```

#### Internal Verification

```rust
fn verify_matching_epoch(env: &Env, actor_epoch: u64) -> Result<(), OrchestratorError> {
    let current_epoch = Self::get_actor_epoch(env);
    if actor_epoch != current_epoch {
        return Err(OrchestratorError::EpochMismatch);
    }
    Ok(())
}
```

#### Entry Point That Consumes the Epoch

```rust
pub fn execute_remittance_flow_signed(
    env: Env,
    executor: Address,
    amount: i128,
    nonce: u64,
    deadline: u64,
    request_hash: u64,
    actor_epoch: u64,  // <-- signed as part of the actor token
) -> Result<bool, OrchestratorError> {
    executor.require_auth();

    // ... validate initialization, nonce, deadline, request hash ...

    // Reject if the actor epoch embedded in the token does not match
    // the contract's current epoch. After an owner bumps the epoch,
    // all tokens created at or before the old epoch are invalid.
    Self::verify_matching_epoch(&env, actor_epoch)?;

    // ... execute flow ...
}
```

#### Bump Function

```rust
/// Owner-only. Atomically invalidates all prior actor tokens.
pub fn bump_actor_epoch(env: Env, caller: Address) -> Result<u64, OrchestratorError> {
    caller.require_auth();

    let owner: Address = env
        .storage()
        .instance()
        .get(&symbol_short!("OWNER"))
        .ok_or(OrchestratorError::Unauthorized)?;
    if caller != owner {
        return Err(OrchestratorError::Unauthorized);
    }

    let old_epoch = Self::get_actor_epoch(&env);
    let new_epoch = old_epoch
        .checked_add(1)
        .ok_or(OrchestratorError::Overflow)?;

    env.storage().instance().set(&ACTOR_EPOCH, &new_epoch);

    env.events().publish(
        (symbol_short!("orch"), symbol_short!("epch_bump")),
        (old_epoch, new_epoch),
    );
    Ok(new_epoch)
}
```

#### Epoch as Part of Pre-Upgrade Snapshot

The orchestrator also captures the actor epoch inside its `ExecutionSnapshot` so that a rollback restores the epoch to its pre-upgrade value, preventing a race where an upgrade bumps the epoch but a rollback loses that state:

```rust
#[contracttype]
#[derive(Clone)]
pub struct ExecutionSnapshot {
    pub policy_id: u32,
    pub actor_epoch: u64,  // <-- captured here
    // ... other fields ...
}

pub fn capture_snapshot(env: Env) -> Result<(), OrchestratorError> {
    // ...
    let snapshot = ExecutionSnapshot {
        policy_id: env.storage().instance().get(&symbol_short!("POL_ID")).unwrap_or(1),
        actor_epoch: Self::get_actor_epoch(&env),
    };
    env.storage().persistent().set(&SNAPSHOT_KEY, &snapshot);
    // ...
}

pub fn restore_from_snapshot(env: Env) -> Result<(), OrchestratorError> {
    // ...
    let snapshot: ExecutionSnapshot = env.storage().persistent()
        .get(&SNAPSHOT_KEY).ok_or(OrchestratorError::SnapshotNotFound)?;
    env.storage().instance().set(&ACTOR_EPOCH, &snapshot.actor_epoch);
    // ...
}
```

---

## Invariant Summary

| Invariant | Description | Enforced By |
|---|---|---|
| **Monotonicity** | Each bump produces a strictly larger value: `new_epoch == old_epoch + 1` | `checked_add(1)` with overflow error |
| **Non-decreasing** | Epochs never reset or wrap in normal operation | `u64::MAX` unreachable at one bump per ledger |
| **Authorization** | Only the designated privilege holder (admin/owner) may bump | `require_auth()` + caller identity check |
| **Observability** | Every bump emits a publicly verifiable event | `env.events().publish(...)` |
| **Freshness** | A consumed epoch value must equal the current contract epoch | `require_matching_kill_switch_epoch` / `verify_matching_epoch` |
| **Snapshot recovery** | Actor epoch is preserved across upgrade/rollback cycles | Captured in `ExecutionSnapshot` and restored on rollback |

---

## Design Rationale

### Why a `u64` counter instead of a timestamp?

A monotonic counter is independent of ledger time and cannot be influenced by transaction ordering or clock drift. Two bumps in the same ledger produce distinct epoch values (`0 → 1 → 2`) whereas two timestamps in the same ledger would be identical, making revocation granularity ambiguous.

### Why check inside the entry point instead of a modifier pattern?

Soroban contracts do not support Solidity-style function modifiers. The epoch check is called explicitly at the top of each protected entry point, before any state mutation, ensuring fail-fast semantics.

### Why include the epoch in the signed payload?

Embedding the epoch inside the data that the signer signs (rather than, say, checking it on-chain and rejecting if stale) means the signer explicitly commits to a specific epoch value. This prevents a class of attacks where a relayer extracts a signed payload, waits for an epoch bump, and then claims the contract should accept the old value.

---

## Related Documentation

- [Killswitch Trust Model](killswitch-trust-model.md) — Admin authority, pause lifecycle, and the epoch-protected `transfer_admin` entry point
- [Orchestrator Reentrancy](orchestrator-reentrancy.md) — Execution lock and reentrancy protection that works alongside epoch guards
- [Orchestrator Nonce](orchestrator-nonce.md) — Nonce-based replay protection that complements the epoch mechanism
- [Period Invariants](PERIOD_INVARIANTS.md) — Time-bound invariants (periods, deadlines, expiry) — conceptually distinct from epoch-based authorization invalidation
- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) — Onboarding guide, development standards, and testing workflow
- [Threat Model](../THREAT_MODEL.md) — Broader security analysis covering authorization, reentrancy, and DoS mitigations

