# Implementation Tasks

## Task 1: Add InsuranceError, ExternalRefUpdatedEvent, EVT_EXT_REF_UPDATED, and KEY_EXT_REF_IDX to lib.rs

Add the foundational types and constants required by the external-reference index feature.

- [x] 1.1 Add `#[contracterror]` enum `InsuranceError` with variants: `PolicyNotFound = 1`, `Unauthorized = 2`, `PolicyInactive = 3`, `InvalidExternalRef = 4`, `DuplicateExternalRef = 5`
- [x] 1.2 Add `#[contracttype]` struct `ExternalRefUpdatedEvent` with fields: `policy_id: u32`, `old_external_ref: Option<String>`, `new_external_ref: Option<String>`, `timestamp: u64`
- [x] 1.3 Add `const EVT_EXT_REF_UPDATED: Symbol = symbol_short!("ext_upd")` and `const KEY_EXT_REF_IDX: Symbol = symbol_short!("EXT_IDX")` to the storage-key constants block
- [x] 1.4 Add `///` doc-comments on `KEY_EXT_REF_IDX`, `ExternalRefUpdatedEvent`, and `EVT_EXT_REF_UPDATED` describing their role in the index lifecycle

## Task 2: Add internal index-management helpers to impl Insurance

Add private helper functions that encapsulate all reads and writes to `EXT_IDX`.

- [x] 2.1 Add `fn validate_external_ref(ext_ref: &String) -> Result<(), InsuranceError>` — returns `Err(InsuranceError::InvalidExternalRef)` if byte length is 0 or > 128
- [x] 2.2 Add `fn ext_idx_get(env: &Env, ext_ref: &String) -> Option<u32>` — reads `KEY_EXT_REF_IDX` and returns the mapped policy ID
- [x] 2.3 Add `fn ext_idx_insert(env: &Env, ext_ref: &String, policy_id: u32)` — loads `EXT_IDX`, inserts the mapping, and saves it back
- [x] 2.4 Add `fn ext_idx_remove(env: &Env, ext_ref: &String)` — loads `EXT_IDX`, removes the entry for `ext_ref`, and saves it back

## Task 3: Modify create_policy to validate, deduplicate, and index external_ref

Update `create_policy` to enforce uniqueness and populate `EXT_IDX` on success.

- [x] 3.1 Change the return type of `create_policy` from `u32` to `Result<u32, InsuranceError>`
- [x] 3.2 Before creating the policy, call `validate_external_ref` when `external_ref` is `Some`; return `Err(InsuranceError::InvalidExternalRef)` on failure
- [x] 3.3 Before creating the policy, call `ext_idx_get` when `external_ref` is `Some`; return `Err(InsuranceError::DuplicateExternalRef)` if the ref is already present
- [x] 3.4 After inserting the policy into `KEY_POLICIES`, call `ext_idx_insert` when `external_ref` is `Some`
- [x] 3.5 Add `///` doc-comment on `create_policy` describing the `InvalidExternalRef` and `DuplicateExternalRef` error conditions

## Task 4: Modify deactivate_policy to clean up EXT_IDX

Update `deactivate_policy` to remove the policy's `external_ref` from `EXT_IDX` on deactivation.

- [x] 4.1 Change the return type of `deactivate_policy` from `bool` to `Result<bool, InsuranceError>`
- [x] 4.2 After setting `policy.active = false` and saving, call `ext_idx_remove` if `policy.external_ref` is `Some`
- [x] 4.3 Preserve the existing early-return `false` behaviour for missing policy and wrong owner (wrap as `Ok(false)`)
- [x] 4.4 Add `///` doc-comment on `deactivate_policy` describing the index cleanup behaviour

## Task 5: Add archive_policy function

Add a new `archive_policy` function that permanently removes a policy and frees its `external_ref`.

- [x] 5.1 Implement `pub fn archive_policy(env: Env, caller: Address, policy_id: u32) -> Result<bool, InsuranceError>` that requires `caller.require_auth()`
- [x] 5.2 Load the policy from `KEY_POLICIES`; return `Ok(false)` if not found
- [x] 5.3 Return `Err(InsuranceError::Unauthorized)` if `policy.owner != caller`
- [x] 5.4 Remove the policy entry from `KEY_POLICIES` and save
- [x] 5.5 Call `ext_idx_remove` if `policy.external_ref` is `Some`
- [x] 5.6 Add `///` doc-comment on `archive_policy` describing the index cleanup and permanence

## Task 6: Add get_policy_id_by_external_ref function

Add the public lookup function that reads `EXT_IDX`.

- [x] 6.1 Implement `pub fn get_policy_id_by_external_ref(env: Env, ext_ref: String) -> Option<u32>` that calls `Self::extend_instance_ttl` and then `ext_idx_get`
- [x] 6.2 Add `///` doc-comment describing the stability and no-stale-lookup security invariants

## Task 7: Add set_external_ref function

Add the atomic re-indexing function for changing a policy's `external_ref`.

- [x] 7.1 Implement `pub fn set_external_ref(env: Env, caller: Address, policy_id: u32, new_ref: Option<String>) -> Result<bool, InsuranceError>` that requires `caller.require_auth()`
- [x] 7.2 Load the policy; return `Err(InsuranceError::PolicyNotFound)` if missing
- [x] 7.3 Return `Err(InsuranceError::Unauthorized)` if `policy.owner != caller`
- [x] 7.4 Return `Err(InsuranceError::PolicyInactive)` if `!policy.active`
- [x] 7.5 If `new_ref == policy.external_ref`, return `Ok(true)` immediately (idempotent path — no storage write, no event)
- [x] 7.6 Validate `new_ref` length if `Some`; return `Err(InsuranceError::InvalidExternalRef)` on failure
- [x] 7.7 Check `EXT_IDX` for duplicate when `new_ref` is `Some`; skip the current policy's own entry; return `Err(InsuranceError::DuplicateExternalRef)` if found
- [x] 7.8 Call `ext_idx_remove` for the old `external_ref` if `Some`
- [x] 7.9 Call `ext_idx_insert` for `new_ref` if `Some`
- [x] 7.10 Update `policy.external_ref = new_ref` and save the policy
- [x] 7.11 Emit `ExternalRefUpdatedEvent` via `env.events().publish((EVT_EXT_REF_UPDATED,), event_struct)`
- [x] 7.12 Add `///` doc-comment on `set_external_ref` describing atomicity, idempotency, and the emitted event

## Task 8: Write the test suite in insurance/src/test.rs

Create `insurance/src/test.rs` with the full test suite covering all requirements.

- [x] 8.1 Add the `#[cfg(test)]` module header, imports (`soroban_sdk::testutils::*`, `soroban_sdk::Env`, contract client, etc.), and a `setup()` helper that creates an `Env`, registers the contract, and returns `(env, client, owner_address)`
- [x] 8.2 Write `test_create_policy_indexes_external_ref` — creates a policy with `Some("ref-A")`, asserts `get_policy_id_by_external_ref("ref-A")` returns the correct ID
- [x] 8.3 Write `test_create_policy_none_ref_no_index` — creates a policy with `None`, asserts `get_policy_id_by_external_ref` for any string returns `None`
- [x] 8.4 Write `test_create_policy_duplicate_ref_rejected` — creates policy with `"ref-A"`, then attempts a second `create_policy` with `"ref-A"`, asserts `Err(InsuranceError::DuplicateExternalRef)`
- [x] 8.5 Write `test_create_policy_invalid_ref_rejected` — attempts `create_policy` with empty string and with a 129-byte string, asserts `Err(InsuranceError::InvalidExternalRef)` for both
- [x] 8.6 Write `test_deactivate_removes_ref_from_index` — creates policy with `"ref-A"`, deactivates it, asserts `get_policy_id_by_external_ref("ref-A")` returns `None`
- [x] 8.7 Write `test_deactivate_none_ref_no_index_change` — creates policy with `None`, deactivates it, asserts no panic and index is empty
- [x] 8.8 Write `test_deactivate_already_inactive_no_index_change` — deactivates a policy twice, asserts second call returns `Ok(false)` and index is unchanged
- [x] 8.9 Write `test_archive_removes_ref_from_index` — creates policy with `"ref-A"`, archives it, asserts `get_policy_id_by_external_ref("ref-A")` returns `None` and `get_policy` returns `None`
- [x] 8.10 Write `test_archive_none_ref_no_index_change` — creates policy with `None`, archives it, asserts no panic
- [x] 8.11 Write `test_reuse_after_archive` — creates policy A with `"ref-A"`, archives it, creates policy B with `"ref-A"`, asserts `get_policy_id_by_external_ref("ref-A")` returns B's ID
- [x] 8.12 Write `test_set_external_ref_reindex` — creates policy with `"ref-A"`, calls `set_external_ref` to `"ref-B"`, asserts `get_policy_id_by_external_ref("ref-A")` is `None` and `get_policy_id_by_external_ref("ref-B")` is `Some(id)`
- [x] 8.13 Write `test_set_external_ref_to_none` — creates policy with `"ref-A"`, calls `set_external_ref` to `None`, asserts `get_policy_id_by_external_ref("ref-A")` is `None`
- [x] 8.14 Write `test_set_external_ref_duplicate_rejected` — creates two policies with `"ref-A"` and `"ref-B"`, attempts to set policy 1's ref to `"ref-B"`, asserts `Err(InsuranceError::DuplicateExternalRef)`
- [x] 8.15 Write `test_set_external_ref_invalid_rejected` — attempts `set_external_ref` with empty string and 129-byte string, asserts `Err(InsuranceError::InvalidExternalRef)`
- [x] 8.16 Write `test_set_external_ref_idempotent` — calls `set_external_ref` with the same value the policy already holds, asserts `Ok(true)` and no event emitted
- [x] 8.17 Write `test_set_external_ref_emits_event` — calls `set_external_ref`, reads `env.events().all()`, asserts the event topic is `EVT_EXT_REF_UPDATED` and the payload contains correct `old_external_ref` and `new_external_ref`
- [x] 8.18 Write `test_set_external_ref_sequential_abc` — calls `set_external_ref` A→B→C on the same policy, asserts only C is in the index and A, B return `None`
- [x] 8.19 Write `test_lookup_active_policy` — asserts `get_policy_id_by_external_ref` returns `Some(id)` matching `get_policy(id).unwrap().id`
- [x] 8.20 Write `test_lookup_unknown_ref_returns_none` — asserts `get_policy_id_by_external_ref("never-registered")` returns `None`
- [x] 8.21 Write `test_lookup_stability` — calls `get_policy_id_by_external_ref` three times for the same active policy, asserts all three return the same `Some(id)`
- [x] 8.22 Write `test_lookup_no_stale_after_deactivate` and `test_lookup_no_stale_after_archive` — assert `None` after the respective lifecycle operation
- [x] 8.23 Write `proptest_round_trip` using the `proptest` crate — for any valid `external_ref` string (1–128 ASCII bytes), `create_policy` followed by `get_policy_id_by_external_ref` returns the correct ID

## Task 9: Write docs/insurance-external-ref.md

Create the developer documentation for the external-reference index lifecycle.

- [x] 9.1 Document the purpose and structure of `KEY_EXT_REF_IDX` (`EXT_IDX`): storage type `Map<String, u32>`, instance storage, symbol `"EXT_IDX"`
- [x] 9.2 Document the full index lifecycle: creation via `create_policy` and `set_external_ref`, cleanup via `archive_policy`, `deactivate_policy`, and `set_external_ref` with `None`, and lookup via `get_policy_id_by_external_ref`
- [x] 9.3 Document the security invariant: `get_policy_id_by_external_ref` never returns a policy ID for an archived or deactivated policy
- [x] 9.4 Document `InsuranceError` codes `InvalidExternalRef` (4) and `DuplicateExternalRef` (5) with their trigger conditions
- [x] 9.5 Include a Rust code example showing the full lifecycle: create with ref → lookup → archive → reuse ref
- [x] 9.6 Include an index-integrity note explaining that all index mutations are atomic within a single contract invocation (Soroban's single-writer model)

## Task 10: Run cargo test -p insurance and verify all tests pass

Validate the implementation by running the full test suite.

- [ ] 10.1 Run `cargo test -p insurance` and confirm all tests pass with zero failures
- [ ] 10.2 Confirm `cargo clippy -p insurance` produces no new warnings related to the added code
- [ ] 10.3 Record the test output summary (test count, pass/fail) as a comment in this task
