# Insurance External Reference Index

This document describes the `insurance` contract's external reference index feature.
It explains the purpose and lifecycle of `KEY_EXT_REF_IDX` (`EXT_IDX`), the public lookup function, and the new error codes used for validation and deduplication.

## Purpose

The external reference index allows off-chain systems to look up an active insurance policy by a stable external identifier.
This index is stored in contract instance storage and maps each active `external_ref` string to the owning policy ID.

The feature supports:

- unique, optional policy-level `external_ref`
- fast lookup from `external_ref` → policy ID
- safe reuse of references after a policy is archived
- cleanup on deactivation and archival
- atomic ref changes with event emission

## Storage

The index is stored under instance storage key:

- `KEY_EXT_REF_IDX` = `symbol_short!("EXT_IDX")`

The value stored at `KEY_EXT_REF_IDX` is:

- `Map<String, u32>`

The map tracks all active policies that currently expose an external reference.

## Lifecycle

### Creation

- `create_policy(...)` accepts `external_ref: Option<String>`.
- If `external_ref` is `Some`, the contract validates its length and rejects duplicates.
- On success, the new policy is stored in `KEY_POLICIES` and the index is populated via `ext_idx_insert`.

### Lookup

- `get_policy_id_by_external_ref(env, ext_ref)` returns `Some(policy_id)` if `ext_ref` is currently mapped in `EXT_IDX`.
- The lookup function calls `Self::extend_instance_ttl` and then reads the index.
- If the ref is unregistered or the policy has been removed/deactivated, it returns `None`.

### Deactivation

- `deactivate_policy(...)` sets `policy.active = false`.
- If the policy had an `external_ref`, the contract removes it from `EXT_IDX`.
- This ensures the lookup cannot return stale IDs for inactive policies.

### Archival

- `archive_policy(...)` permanently removes the policy from `KEY_POLICIES`.
- If the archived policy had an `external_ref`, the contract removes it from `EXT_IDX`.
- After archival, the same reference can be reused by another policy.

### Updating the external_ref

- `set_external_ref(...)` changes the policy's `external_ref` atomically.
- If the new value is equal to the current value, the operation is idempotent and returns `Ok(true)` without changing storage or emitting an event.
- If the new value differs:
  - the old mapping is removed from `EXT_IDX` if present
  - the new mapping is inserted if it is `Some`
  - the policy record is updated in `KEY_POLICIES`
  - an `ExternalRefUpdatedEvent` is emitted with topic `EVT_EXT_REF_UPDATED`

## Security invariants

- `EXT_IDX` only contains refs for active policies.
- A deactivated or archived policy is removed from the index.
- `get_policy_id_by_external_ref` therefore never returns an ID for a stale or inactive policy.
- `set_external_ref` rejects duplicate refs held by any other active policy.

## Error codes

The feature introduces two new `InsuranceError` variants:

- `InsuranceError::InvalidExternalRef = 4`
  - returned when a provided `external_ref` is empty or longer than 128 bytes.

- `InsuranceError::DuplicateExternalRef = 5`
  - returned when a provided `external_ref` is already held by another active policy.

These errors may occur during:

- `create_policy(...)`
- `set_external_ref(...)`

## Example usage

```rust
let policy_id = client
    .create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &CoverageType::Health,
        &100,
        &10_000,
        &Some(String::from_str(&env, "ref-A")),
    )
    .unwrap();

assert_eq!(
    client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A")),
    Some(policy_id),
);

client.set_external_ref(
    &owner,
    &policy_id,
    &Some(String::from_str(&env, "ref-B")),
)
.unwrap();

assert_eq!(
    client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-A")),
    None,
);
assert_eq!(
    client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-B")),
    Some(policy_id),
);

client.archive_policy(&owner, &policy_id).unwrap();
assert_eq!(
    client.get_policy_id_by_external_ref(&String::from_str(&env, "ref-B")),
    None,
);
```

## Notes

- `EXT_IDX` is kept in instance storage so the index is scoped to the contract instance.
- Event topic `EVT_EXT_REF_UPDATED` is defined as `symbol_short!("ext_upd")`.
- The feature is designed to prevent stale lookups while still allowing safe ref reuse after a policy is archived.
