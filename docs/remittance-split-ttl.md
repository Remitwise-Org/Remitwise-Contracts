# Remittance Split TTL Policy

This document describes the Time-To-Live (TTL) extension policy for the `remittance_split` contract. All persistent and instance storage entries are proactively extended on access to prevent silent expiration.

## TTL Constants

The contract uses standardized constants from `remitwise-common`:

| Constant | Value (Ledgers) | Approx. Duration |
|----------|-----------------|------------------|
| `INSTANCE_LIFETIME_THRESHOLD` | 120,960 | 7 Days |
| `INSTANCE_BUMP_AMOUNT` | 518,400 | 30 Days |
| `PERSISTENT_LIFETIME_THRESHOLD` | 259,200 | 15 Days |
| `PERSISTENT_BUMP_AMOUNT` | 1,036,800 | 60 Days |

## Instance Storage (Active Data)

Instance storage entries are extended to `INSTANCE_BUMP_AMOUNT` whenever they have less than `INSTANCE_LIFETIME_THRESHOLD` remaining.

| Key | Governance | Trigger |
|-----|------------|---------|| `CONFIG`  | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| *(SPLIT removed)* | — | Percentages now derived from CONFIG |
| `NONCES` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `AUDIT` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `VERSION` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `PAUSE_ADM` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `PAUSED` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `UPG_ADM` | `INSTANCE_BUMP_AMOUNT` | Every read/write |
| `NEXT_RSCH` | `INSTANCE_BUMP_AMOUNT` | Every read/write |

## Persistent Storage (Schedules)

Persistent storage entries are extended to `PERSISTENT_BUMP_AMOUNT` whenever they have less than `PERSISTENT_LIFETIME_THRESHOLD` remaining.

| Key | Governance | Trigger |
|-----|------------|---------|
| `Schedule(id)` | `PERSISTENT_BUMP_AMOUNT` | Every read/write |
| `OwnerSchedules(addr)` | `PERSISTENT_BUMP_AMOUNT` | Every read/write |

## Implementation

The contract implements two helper functions to centralize TTL logic:

- `extend_instance_ttl(env)`: Extends the entire instance storage.
- `extend_persistent_ttl(env, key)`: Extends a specific persistent entry.

### Example Audit

Every storage interaction in `lib.rs` must be accompanied by a TTL extension.

```rust
// Persistent Read
let sch_key = DataKey::Schedule(schedule_id);
let schedule: RemittanceSchedule = env.storage().persistent().get(&sch_key).ok_or(...)?;
Self::extend_persistent_ttl(&env, &sch_key);

// Instance Read
Self::extend_instance_ttl(&env);
let config: SplitConfig = env.storage().instance().get(&symbol_short!("CONFIG")).ok_or(...)?;
```
