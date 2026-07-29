# Remittance Split Admin Roles

## Overview

The Remittance Split contract implements two administrative roles for managing critical contract operations:

1. **Pause Admin (`PAUSE_ADM`)**: Responsible for emergency pause/unpause functionality
2. **Upgrade Admin (`UPG_ADM`)**: Responsible for contract upgrade management

Both roles implement secure transfer mechanisms with audit trail event emission.

## Access Control Matrix

| Function | Initial Setter | Subsequent Transfer | Event Emitted |
|----------|---------------|-------------------|---------------|
| `set_pause_admin` | Owner only | Owner only | `adm_xfr` |
| `set_upgrade_admin` | Owner only | Current upgrade admin only | `adm_xfr` |

*Cross-reference: [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md)*

## Pause Admin Role

### Purpose
The pause admin has authority to:
- Pause all state-changing contract operations in emergencies
- Unpause the contract when the emergency is resolved
- Transfer the pause admin role to another address

### Setting the Pause Admin

#### Initial Assignment
Only the contract owner can set the initial pause admin:

```rust
fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), RemittanceSplitError>
```

**Authorization Requirements:**
- Caller must be the contract owner
- Contract must not be paused
- Contract must be initialized

**Event Emission:**
- Emits `adm_xfr` event with `(old_admin, new_admin)` tuple
- For initial assignment, `old_admin` is `None`

#### Role Transfer
Only the contract owner can transfer the pause admin role (unlike upgrade admin, which can be transferred by the current admin).

**Security Properties:**
- No storage mutation on authorization failure
- Idempotent self-transfer allowed (new_admin == current_admin)
- Transfer blocked when contract is paused

### Usage Example

```rust
// Set initial pause admin
client.set_pause_admin(&owner, &pause_admin_address).unwrap();

// Verify admin was set
assert_eq!(client.get_pause_admin_public(), Some(pause_admin_address));

// Transfer to new admin
client.set_pause_admin(&owner, &new_pause_admin).unwrap();
```

## Upgrade Admin Role

### Purpose
The upgrade admin has authority to:
- Set contract version for upgrade management
- Transfer the upgrade admin role to another address

### Setting the Upgrade Admin

#### Initial Assignment
Only the contract owner can set the initial upgrade admin:

```rust
fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), RemittanceSplitError>
```

**Authorization Requirements:**
- Caller must be the contract owner (if no upgrade admin exists)
- Caller must be the current upgrade admin (if upgrade admin exists)
- Contract must be initialized

**Event Emission:**
- Emits `adm_xfr` event with `(old_admin, new_admin)` tuple
- For initial assignment, `old_admin` is `None`

#### Role Transfer
Only the current upgrade admin can transfer the role to a new address. The owner cannot override the upgrade admin once it has been set.

**Security Properties:**
- No storage mutation on authorization failure
- Idempotent self-transfer allowed (new_admin == current_admin)
- Owner cannot override after initial assignment (privilege escalation prevention)

### Usage Example

```rust
// Set initial upgrade admin (by owner)
client.set_upgrade_admin(&owner, &upgrade_admin_address).unwrap();

// Verify admin was set
assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin_address));

// Transfer to new admin (by current upgrade admin only)
client.set_upgrade_admin(&upgrade_admin_address, &new_upgrade_admin).unwrap();

// Owner cannot override after initial set
let result = client.try_set_upgrade_admin(&owner, &another_address);
assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
```

## Security Considerations

### Privilege Escalation Prevention
- **Upgrade Admin**: Once set, the owner cannot override the upgrade admin. This prevents privilege escalation attacks where a compromised owner could regain upgrade authority.
- **Pause Admin**: The owner retains transfer authority to ensure emergency response capability is maintained.

### Authorization Failures
Both functions implement strict authorization checks that:
1. Validate caller identity before any storage access
2. Return `Unauthorized` error without mutating storage on failure
3. Maintain consistent security boundaries across all scenarios

### Event Emission
Both admin transfer functions emit `adm_xfr` events containing:
- `old_admin`: The previous admin address (or `None` for initial assignment)
- `new_admin`: The new admin address

This provides an audit trail for all role transfers.

### Idempotent Operations
Both functions support idempotent self-transfer (setting the admin to the same address), which:
- Succeeds without error
- Emits the transfer event
- Does not change storage state

## Testing Coverage

Comprehensive authorization tests ensure:

1. **Authorized transfers succeed**: Owner and current admin can perform valid transfers
2. **Unauthorized callers rejected**: Non-authorized callers receive `Unauthorized` error
3. **No storage mutation on rejection**: Failed transfers do not modify storage
4. **Event emission verification**: Successful transfers emit `adm_xfr` events with correct data
5. **Self-transfer idempotency**: Setting admin to current address succeeds without changes
6. **Double transfer handling**: Sequential transfers work correctly
7. **Pause state blocking**: Transfers blocked when contract is paused (pause admin only)
8. **Owner override prevention**: Owner cannot override upgrade admin after initial set

See `remittance_split/src/test.rs` for complete test coverage.

## References

- Contract Implementation: `remittance_split/src/lib.rs`
- Test Suite: `remittance_split/src/test.rs`
- Access Control Matrix: `ACCESS_CONTROL_MATRIX.md`
- Error Types: `RemittanceSplitError::Unauthorized`, `RemittanceSplitError::NotInitialized`

## Storage Keys

- `PAUSE_ADM`: Pause admin address (stored in instance storage)
- `UPG_ADM`: Upgrade admin address (stored in instance storage)

## Related Functions

- `get_pause_admin_public()`: Returns current pause admin (or owner if not set)
- `get_upgrade_admin_public()`: Returns current upgrade admin (or None if not set)
- `pause()`: Pauses contract (requires pause admin or owner)
- `unpause()`: Unpauses contract (requires pause admin or owner)
- `set_version()`: Sets contract version (requires upgrade admin)