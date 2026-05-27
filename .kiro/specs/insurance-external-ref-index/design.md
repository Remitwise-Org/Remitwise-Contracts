# Design Document

## Overview

This design adds the external-reference index (`EXT_IDX`) to the Insurance smart contract,
along with the new contract functions, error types, event types, and test/documentation
artifacts required by the requirements. All changes are confined to
`insurance/src/lib.rs`, `insurance/src/test.rs`, and `docs/insurance-external-ref.md`.

The implementation follows the same patterns already established in `bill_payments/src/lib.rs`:
`#[contracterror]` for typed errors, `#[contracttype]` for event structs, `symbol_short!`
for storage keys, and `env.events().publish(...)` for event emission.

## Architecture

### New Storage Key

```
KEY_EXT_REF_IDX: Symbol = symbol_short!("EXT_IDX")
```

Holds a `Map<String, u32>` in instance storage. Each entry maps one active
`external_ref` string to the policy ID that currently owns it.

### New Types

#### InsuranceError (`#[contracterror]`)

```rust
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum InsuranceError {
    PolicyNotFound        = 1,
    Unauthorized          = 2,
    PolicyInactive        = 3,
    InvalidExternalRef    = 4,   // empty or > 128 bytes
    DuplicateExternalRef  = 5,   // ref already held by an active policy
}
```

#### ExternalRefUpdatedEvent (`#[contracttype]`)

```rust
#[contracttype]
#[derive(Clone)]
pub struct ExternalRefUpdatedEvent {
    pub policy_id:        u32,
    pub old_external_ref: Option<String>,
    pub new_external_ref: Option<String>,
    pub timestamp:        u64,
}
```

#### EVT_EXT_REF_UPDATED constant

```rust
const EVT_EXT_REF_UPDATED: Symbol = symbol_short!("ext_upd");
```

### New / Modified Contract Functions

#### `create_policy` (modified)

Before inserting the policy, the function now:
1. Validates `external_ref` length (1–128 bytes) if `Some`.
2. Checks `EXT_IDX` for a duplicate; returns `InsuranceError::DuplicateExternalRef` if found.
3. After inserting the policy, inserts `(external_ref → policy_id)` into `EXT_IDX`.

Return type changes from `u32` to `Result<u32, InsuranceError>`.

#### `deactivate_policy` (modified)

After setting `policy.active = false`, removes the policy's `external_ref` entry from
`EXT_IDX` (if `Some`).

Return type changes from `bool` to `Result<bool, InsuranceError>`.

#### `archive_policy` (new)

```rust
pub fn archive_policy(env: Env, caller: Address, policy_id: u32)
    -> Result<bool, InsuranceError>
```

Permanently removes the policy from `KEY_POLICIES` and removes its `external_ref` from
`EXT_IDX`. Only the policy owner may archive. Returns `false` if the policy does not exist.

#### `set_external_ref` (new)

```rust
pub fn set_external_ref(
    env: Env,
    caller: Address,
    policy_id: u32,
    new_ref: Option<String>,
) -> Result<bool, InsuranceError>
```

Atomically:
1. Validates `new_ref` length if `Some`.
2. Checks for duplicate in `EXT_IDX` (skipping the current policy's own entry).
3. Removes old `external_ref` from `EXT_IDX` if `Some`.
4. Inserts new `external_ref` into `EXT_IDX` if `Some`.
5. Updates `policy.external_ref`.
6. Emits `ExternalRefUpdatedEvent` via `env.events().publish(EVT_EXT_REF_UPDATED, ...)`.

Idempotent: if `new_ref == policy.external_ref`, returns `true` without modifying storage
or emitting an event.

#### `get_policy_id_by_external_ref` (new)

```rust
pub fn get_policy_id_by_external_ref(env: Env, ext_ref: String) -> Option<u32>
```

Reads `EXT_IDX` and returns the mapped policy ID, or `None`.

### Internal Helpers

```rust
fn validate_external_ref(ext_ref: &String) -> Result<(), InsuranceError>
fn ext_idx_insert(env: &Env, ext_ref: &String, policy_id: u32)
fn ext_idx_remove(env: &Env, ext_ref: &String)
fn ext_idx_get(env: &Env, ext_ref: &String) -> Option<u32>
```

These are private `fn` items on `impl Insurance` (not `pub fn` contract entry points).

## Data Flow

### create_policy with external_ref

```
caller → create_policy(owner, name, ..., Some("ref-A"))
  → validate_external_ref("ref-A")          // length check
  → ext_idx_get("ref-A") == None?           // duplicate check
  → insert policy into KEY_POLICIES
  → ext_idx_insert("ref-A", new_id)
  → return Ok(new_id)
```

### deactivate_policy

```
caller → deactivate_policy(caller, policy_id)
  → load policy
  → policy.active = false
  → save policy
  → if policy.external_ref == Some(r): ext_idx_remove(r)
  → return Ok(true)
```

### archive_policy

```
caller → archive_policy(caller, policy_id)
  → load policy (None → Ok(false))
  → check owner == caller
  → remove policy from KEY_POLICIES
  → if policy.external_ref == Some(r): ext_idx_remove(r)
  → return Ok(true)
```

### set_external_ref (A → B)

```
caller → set_external_ref(caller, policy_id, Some("ref-B"))
  → load policy
  → check owner == caller, policy.active
  → validate_external_ref("ref-B")
  → if "ref-B" == policy.external_ref: return Ok(true)  // idempotent
  → ext_idx_get("ref-B") == None?                       // duplicate check
  → ext_idx_remove("ref-A")                             // remove old
  → ext_idx_insert("ref-B", policy_id)                  // insert new
  → policy.external_ref = Some("ref-B")
  → save policy
  → emit ExternalRefUpdatedEvent { old: Some("ref-A"), new: Some("ref-B"), ... }
  → return Ok(true)
```

## Test Strategy

All tests live in `insurance/src/test.rs` and use `soroban_sdk::testutils`.

| Test name | Requirement |
|---|---|
| `test_create_policy_indexes_external_ref` | R1.1, R1.5 |
| `test_create_policy_none_ref_no_index` | R1.2 |
| `test_create_policy_duplicate_ref_rejected` | R1.3, R6.1 |
| `test_create_policy_invalid_ref_rejected` | R1.4, R6.5 |
| `test_deactivate_removes_ref_from_index` | R3.1, R3.3, R6.6 |
| `test_deactivate_none_ref_no_index_change` | R3.2 |
| `test_deactivate_already_inactive_no_index_change` | R3.4 |
| `test_archive_removes_ref_from_index` | R2.1, R2.3 |
| `test_archive_none_ref_no_index_change` | R2.2 |
| `test_reuse_after_archive` | R2.4, R6.2 |
| `test_set_external_ref_reindex` | R4.1, R6.3 |
| `test_set_external_ref_to_none` | R4.2 |
| `test_set_external_ref_duplicate_rejected` | R4.3 |
| `test_set_external_ref_invalid_rejected` | R4.4 |
| `test_set_external_ref_idempotent` | R4.5 |
| `test_set_external_ref_emits_event` | R4.6, R6.7 |
| `test_set_external_ref_sequential_abc` | R4.7, R6.8 |
| `test_lookup_active_policy` | R5.1, R5.3, R6.4 |
| `test_lookup_unknown_ref_returns_none` | R5.2 |
| `test_lookup_stability` | R5.4 |
| `test_lookup_no_stale_after_deactivate` | R5.5 |
| `test_lookup_no_stale_after_archive` | R5.5 |
| `proptest_round_trip` | R6.9 |

## File Changes Summary

| File | Change |
|---|---|
| `insurance/src/lib.rs` | Add `InsuranceError`, `ExternalRefUpdatedEvent`, `EVT_EXT_REF_UPDATED`, `KEY_EXT_REF_IDX`; add `archive_policy`, `set_external_ref`, `get_policy_id_by_external_ref`; modify `create_policy`, `deactivate_policy`; add `///` doc-comments |
| `insurance/src/test.rs` | Create full test suite (22+ tests + 1 proptest) |
| `docs/insurance-external-ref.md` | New documentation file |
