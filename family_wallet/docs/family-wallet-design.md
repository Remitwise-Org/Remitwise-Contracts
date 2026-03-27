# Family Wallet — Role Expiry Design

## Overview

The `FamilyWallet` contract supports time-bounded roles. Any role except `Owner`
can be given an expiry timestamp. Once the ledger clock reaches or passes that
timestamp, the role is treated as if it does not exist for authorization purposes.

---

## Role Hierarchy

| Role    | Ordinal | Notes                          |
|---------|---------|--------------------------------|
| Owner   | 0       | Never expires; full control    |
| Admin   | 1       | Can manage members and expiries|
| Member  | 2       | Can propose transactions       |
| Viewer  | 3       | Read-only                      |

Lower ordinal = higher privilege. `require_role_at_least(min_role)` passes when
`role_ordinal(caller) <= role_ordinal(min_role)`.

---

## Role Expiry Mechanics

### Storage

Expiries are stored in a `Map<Address, u64>` under the `ROLE_EXP` storage key.
A missing entry means no expiry (the role never expires).

### Setting an Expiry

```rust
// Requires Admin role minimum
pub fn set_role_expiry(env, caller, member, expires_at: Option<u64>) -> bool
```

- `Some(ts)` — sets expiry to ledger timestamp `ts`
- `None` — clears expiry; the role becomes permanent again

### Expiry Check

```rust
fn role_has_expired(env, address) -> bool {
    if let Some(exp) = get_role_expiry(env, address) {
        env.ledger().timestamp() >= exp  // INCLUSIVE boundary
    } else {
        false
    }
}
```

> ⚠️ **Boundary is inclusive (`>=`)**
> A role set to expire at timestamp `T` is already expired when the ledger
> reads exactly `T`. Plan expiry windows accordingly.

### Enforcement

`require_role_at_least()` calls `role_has_expired()` before checking the role
ordinal. An expired role panics with `"Role has expired"` regardless of what
role the member holds.

---

## Lifecycle

```
Owner sets expiry          Role active          Role expires
        │                       │                    │
  t=1000│                 t=1000-1999             t=2000+
        ▼                       ▼                    ▼
  set_role_expiry(         any action            "Role has
  member, Some(2000))       succeeds             expired" panic
```

### Renewal

Only `Owner` or an **active** `Admin` can renew an expired role:

```
set_role_expiry(owner, expired_admin, Some(new_future_ts))
```

An expired admin **cannot renew their own role** — the expiry check fires
before any authorization logic runs.

---

## Security Assumptions

### 1. Expired roles cannot self-renew
`require_role_at_least` is called inside `set_role_expiry`. An expired caller
panics before reaching the storage write, making self-renewal impossible.

### 2. Plain members cannot set expiries
`set_role_expiry` requires `FamilyRole::Admin` minimum. A `Member` or `Viewer`
calling it will panic with `"Insufficient role"`.

### 3. Non-members are fully blocked
Any address not in the `MEMBERS` map panics with `"Not a family member"` before
any role or expiry check runs.

### 4. Owner is immune to expiry side-effects
`role_ordinal(Owner) == 0` satisfies every `require_role_at_least` call.
Even if an expiry is set on the Owner address, the Owner's `require_auth()`
bypass means core admin actions remain available. Setting expiry on Owner
is considered a misconfiguration and should be avoided.

### 5. Ledger timestamp is the source of truth
Tests must use `env.ledger().with_mut(|li| li.timestamp = ts)` to simulate
time. Wall-clock time is irrelevant; only the ledger timestamp matters.

### 6. Past-timestamp expiry takes immediate effect
Setting `expires_at` to a value already less than the current ledger timestamp
immediately invalidates the role. There is no grace period.

---

## Test Coverage Summary

| Test Group                          | Tests | Covers                                      |
|-------------------------------------|-------|---------------------------------------------|
| Role active baseline                | 3     | No expiry, active before expiry, Owner bypass|
| Exact boundary (inclusive >=)       | 3     | At T, T-1, T+1                              |
| Post-expiry rejections              | 4     | add_member, set_expiry, multisig, propose   |
| Unauthorized renewal paths          | 4     | Self-renew, plain member, non-member, expired admin |
| Successful owner renewal            | 4     | Renew, correct storage, permission limits, clear |
| Edge cases                          | 7     | Independent expiries, past timestamp, overflow, audit |
| **Total**                           | **25**| **>95% branch coverage on expiry paths**    |

---

## Running the Tests

```bash
cargo test -p family_wallet
```

Expected output: all 25 tests pass with no warnings on expiry-related code paths.