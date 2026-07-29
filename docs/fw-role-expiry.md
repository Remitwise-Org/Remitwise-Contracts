# Family Wallet: Role Expiry Semantics

## Overview

`family_wallet` supports optional per-member role expiry. When a role expires, the member loses all privileges associated with that role and is treated as if they no longer hold it — even though their membership record still exists.

## Setting Role Expiry

```rust
pub fn set_role_expiry(
    env: Env,
    caller: Address,    // Must be Owner or Admin (non-expired)
    member: Address,    // Target family member (must exist)
    expires_at: Option<u64>,  // Unix timestamp in seconds; None clears expiry
) -> bool
```

- Only an Owner or Admin with a **non-expired** role may call this.
- `member` must already be in the `MEMBERS` map; non-members are rejected with `"Member not found"`.
- `Some(t)` sets the expiry; `None` clears it (role becomes permanent again).
- The operation is recorded in `ACC_AUDIT` with operation `role_exp`.

## Expiry Boundary

The expiry is **inclusive**: a member is considered expired when:

```
ledger.timestamp() >= expires_at
```

At `expires_at - 1` the role is still active. At `expires_at` it is expired.

## Storage

Expiry timestamps are stored in `ROLE_EXP` (`Map<Address, u64>`) in instance storage. Members without an entry in this map have no expiry (their role is permanent).

## Enforcement Points

Role expiry is checked at every privileged operation via two internal helpers:

### `require_role_at_least(env, caller, min_role)`

Called by all privileged public functions. Panics with:

- `"Role has expired"` if the caller's role has expired
- `"Insufficient role"` if the caller's role is below the required minimum
- `"Not a family member"` if the caller is not in `MEMBERS`

### `is_owner_or_admin_in_members(env, members, address)`

Returns `false` if the member's role has expired, even if they hold Owner or Admin role. Used by `configure_multisig`, `archive_old_transactions`, `cleanup_expired_pending`, and other admin-gated operations.

## Affected Operations

The following operations reject an expired role:

| Operation                     | Required role            | Enforcement path               |
| ----------------------------- | ------------------------ | ------------------------------ |
| `propose_transaction`         | Member                   | `require_role_at_least`        |
| `sign_transaction`            | Member                   | `require_role_at_least`        |
| `configure_multisig`          | Admin                    | `is_owner_or_admin_in_members` |
| `configure_emergency`         | Admin                    | `is_owner_or_admin_in_members` |
| `set_emergency_mode`          | Admin                    | `is_owner_or_admin_in_members` |
| `pause` / `unpause`           | Pause admin (role check) | `require_role_at_least`        |
| `archive_old_transactions`    | Admin                    | `is_owner_or_admin_in_members` |
| `cleanup_expired_pending`     | Admin                    | `is_owner_or_admin_in_members` |
| `batch_add_family_members`    | Admin                    | `is_owner_or_admin_in_members` |
| `batch_remove_family_members` | Owner                    | `is_owner_or_admin_in_members` |
| `set_proposal_expiry`         | Owner                    | `require_role_at_least`        |
| `set_upgrade_admin`           | Owner                    | `require_role_at_least`        |
| `set_version`                 | Upgrade admin            | `require_role_at_least`        |
| `set_role_expiry`             | Admin                    | `require_role_at_least`        |

## Renewal

An expired admin **cannot renew their own expiry** — `set_role_expiry` calls `require_role_at_least(Admin)` which will panic with `"Role has expired"`. Only a non-expired Owner or Admin can renew another member's expiry.

## Tests

Role expiry tests are in `family_wallet/src/test.rs`:

- `test_role_expiry_boundary_allows_before_expiry` — active at `expires_at - 1`
- `test_role_expiry_boundary_revokes_at_expiry_timestamp` — expired at `expires_at`
- `test_role_expiry_renewal_restores_permissions` — renewal by Owner restores access
- `test_role_expiry_unauthorized_member_cannot_renew` — regular members cannot set expiry
- `test_role_expiry_expired_admin_cannot_renew_self` — expired admin cannot self-renew
- `test_role_expiry_cannot_be_set_for_non_member` — non-members rejected
- `test_expired_admin_cannot_pause` / `test_expired_admin_cannot_unpause`
- `test_expired_admin_cannot_archive_transactions`
- `test_expired_admin_cannot_cleanup_expired_pending`
- `test_expired_admin_cannot_configure_multisig`
- `test_expired_admin_cannot_configure_emergency`
- `test_expired_admin_cannot_set_emergency_mode`
- `test_expired_admin_cannot_batch_add_members`
- `test_expired_owner_cannot_batch_remove_members`
- `test_expired_owner_cannot_set_proposal_expiry`
- `test_expired_owner_cannot_set_upgrade_admin`
- `test_expired_owner_cannot_set_version`
- `test_non_expired_admin_can_perform_privileged_operations`

## Security Notes

- Role expiry does **not** remove the member from `MEMBERS`. The member record persists; only their privilege level is revoked.
- Expiry is evaluated against `ledger.timestamp()` (ledger seconds), not wall-clock time. Ledger timestamps are set by validators and cannot be manipulated by contract callers.
- An expired Owner can still be renewed by another Owner (if one exists). If the only Owner's role expires, the wallet is effectively locked until the expiry is cleared via a governance process.

## References

- `family_wallet/src/lib.rs` — `set_role_expiry`, `role_has_expired`, `require_role_at_least`, `is_owner_or_admin_in_members`
- `family_wallet/src/test.rs` — role expiry test suite
- `STORAGE_LAYOUT.md` — `ROLE_EXP` key documentation
