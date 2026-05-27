# Requirements Document

## Introduction

The Insurance smart contract (Soroban/Rust) maintains an external-reference index under
`KEY_EXT_REF_IDX` (storage symbol `EXT_IDX`). This index maps an opaque off-chain reference
string (`external_ref`) to the policy ID that currently holds it, enabling the
`get_policy_id_by_external_ref` lookup. The contract enforces two error codes related to
this index: `InsuranceError::DuplicateExternalRef` (code 5) when a ref is already held by
an active policy, and `InsuranceError::InvalidExternalRef` (code 4) when a ref string is
malformed (empty or longer than 128 bytes).

This feature adds:
1. The full index lifecycle — population on `create_policy`, cleanup on `archive_policy` /
   `deactivate_policy`, and atomic re-indexing on `set_external_ref`.
2. A comprehensive test suite in `insurance/src/test.rs` covering uniqueness, cleanup,
   re-indexing, lookup correctness, and edge cases.
3. Developer documentation in `docs/insurance-external-ref.md` describing the index
   lifecycle, security assumptions, and integration guidance.
4. Inline `///` doc-comments on all symbols involved in the index lifecycle.

## Glossary

- **Insurance**: The Soroban smart contract under `insurance/src/lib.rs` that manages
  micro-insurance policies.
- **EXT_IDX**: The instance-storage key `KEY_EXT_REF_IDX` (symbol `"EXT_IDX"`) that holds
  a `Map<String, u32>` mapping each active external reference to its owning policy ID.
- **external_ref**: An optional `String` field on `InsurancePolicy` carrying an opaque
  off-chain identifier (1–128 bytes when present).
- **active policy**: A policy whose `active` field is `true` and that has not been archived.
- **archived policy**: A policy that has been permanently removed from active service via
  `archive_policy`. Archived policies cannot receive premium payments or be re-activated.
- **InsuranceError**: The `#[contracterror]` enum that surfaces typed error codes to callers.
- **DuplicateExternalRef**: `InsuranceError` variant with code 5; returned when a caller
  attempts to assign an `external_ref` that is already held by another active policy.
- **InvalidExternalRef**: `InsuranceError` variant with code 4; returned when an
  `external_ref` string is empty or exceeds 128 bytes.
- **ExternalRefUpdatedEvent**: The event struct emitted by `set_external_ref`, carrying
  `policy_id`, `old_external_ref`, `new_external_ref`, and `timestamp`.
- **EVT_EXT_REF_UPDATED**: The `Symbol` constant used as the event topic for
  `ExternalRefUpdatedEvent` (value `"ext_upd"`).
- **Validator**: The internal validation logic within the Insurance contract that checks
  `external_ref` length and uniqueness before any write.
- **Index_Manager**: The internal index-maintenance logic within the Insurance contract
  responsible for inserting, updating, and removing entries in `EXT_IDX`.

## Requirements

### Requirement 1: External-Reference Index Population on Policy Creation

**User Story:** As a policy administrator, I want every policy created with an
`external_ref` to be immediately findable by that ref, so that off-chain systems can
resolve a provider reference to a policy ID without scanning all policies.

#### Acceptance Criteria

1. WHEN `create_policy` is called with a non-`None` `external_ref`, THE `Index_Manager`
   SHALL insert a mapping from that `external_ref` string to the new policy ID in `EXT_IDX`
   before the function returns.
2. WHEN `create_policy` is called with `external_ref = None`, THE `Index_Manager` SHALL
   leave `EXT_IDX` unchanged.
3. WHEN `create_policy` is called with an `external_ref` that is already present in
   `EXT_IDX` (held by an active policy), THE `Validator` SHALL return
   `InsuranceError::DuplicateExternalRef` and SHALL NOT create the policy or modify
   `EXT_IDX`.
4. WHEN `create_policy` is called with an `external_ref` whose byte length is zero or
   exceeds 128, THE `Validator` SHALL return `InsuranceError::InvalidExternalRef` and
   SHALL NOT create the policy or modify `EXT_IDX`.
5. FOR ALL valid `external_ref` strings `r` and all policy IDs `id` returned by
   `create_policy`, `get_policy_id_by_external_ref(r)` SHALL return `Some(id)` immediately
   after the call completes (round-trip property).

### Requirement 2: External-Reference Index Cleanup on Archive

**User Story:** As a policy administrator, I want archiving a policy to free its
`external_ref` for reuse, so that a replacement policy can be created with the same
provider reference without manual index cleanup.

#### Acceptance Criteria

1. WHEN `archive_policy` is called for a policy that has a non-`None` `external_ref`,
   THE `Index_Manager` SHALL remove the corresponding entry from `EXT_IDX` before the
   function returns.
2. WHEN `archive_policy` is called for a policy whose `external_ref` is `None`, THE
   `Index_Manager` SHALL leave `EXT_IDX` unchanged.
3. AFTER `archive_policy` completes, `get_policy_id_by_external_ref` called with the
   archived policy's former `external_ref` SHALL return `None`.
4. AFTER `archive_policy` completes for policy A with `external_ref = r`, THE `Insurance`
   SHALL allow `create_policy` to succeed with `external_ref = r`, and
   `get_policy_id_by_external_ref(r)` SHALL return the new policy's ID (reuse-after-archive
   property).

### Requirement 3: External-Reference Index Cleanup on Deactivation

**User Story:** As a security auditor, I want deactivating a policy to remove its
`external_ref` from the index, so that lookups never resolve to an inactive policy and
stale index entries cannot be exploited.

#### Acceptance Criteria

1. WHEN `deactivate_policy` is called for a policy that has a non-`None` `external_ref`,
   THE `Index_Manager` SHALL remove the corresponding entry from `EXT_IDX` before the
   function returns.
2. WHEN `deactivate_policy` is called for a policy whose `external_ref` is `None`, THE
   `Index_Manager` SHALL leave `EXT_IDX` unchanged.
3. AFTER `deactivate_policy` completes, `get_policy_id_by_external_ref` called with the
   deactivated policy's former `external_ref` SHALL return `None`.
4. IF `deactivate_policy` is called on a policy that is already inactive, THEN THE
   `Insurance` SHALL return `false` and SHALL NOT modify `EXT_IDX`.

### Requirement 4: Atomic Re-Indexing on set_external_ref

**User Story:** As a policy administrator, I want changing a policy's `external_ref` to
atomically update the index, so that the old ref is freed and the new ref is claimed in a
single operation with no window where both or neither are valid.

#### Acceptance Criteria

1. WHEN `set_external_ref` is called with a new non-`None` `external_ref` value `B` for a
   policy currently holding `external_ref = A`, THE `Index_Manager` SHALL remove the entry
   for `A` from `EXT_IDX` and insert an entry for `B` in the same operation, such that
   after the call `get_policy_id_by_external_ref(A)` returns `None` and
   `get_policy_id_by_external_ref(B)` returns `Some(policy_id)`.
2. WHEN `set_external_ref` is called with `None` for a policy currently holding
   `external_ref = A`, THE `Index_Manager` SHALL remove the entry for `A` from `EXT_IDX`,
   and after the call `get_policy_id_by_external_ref(A)` SHALL return `None`.
3. WHEN `set_external_ref` is called with a new `external_ref = B` that is already held by
   a different active policy, THE `Validator` SHALL return
   `InsuranceError::DuplicateExternalRef` and SHALL NOT modify `EXT_IDX` or the policy
   record.
4. WHEN `set_external_ref` is called with an `external_ref` whose byte length is zero or
   exceeds 128, THE `Validator` SHALL return `InsuranceError::InvalidExternalRef` and SHALL
   NOT modify `EXT_IDX` or the policy record.
5. WHEN `set_external_ref` is called with the same `external_ref` value the policy already
   holds (idempotent update), THE `Index_Manager` SHALL leave `EXT_IDX` unchanged and THE
   `Insurance` SHALL return `true` without emitting a duplicate event.
6. THE `Insurance` SHALL emit `ExternalRefUpdatedEvent` (topic `EVT_EXT_REF_UPDATED`) on
   every successful `set_external_ref` call, carrying the `old_external_ref` and
   `new_external_ref` values.
7. FOR ALL sequences of `set_external_ref` calls on the same policy (A→B→C), after the
   final call `get_policy_id_by_external_ref(C)` SHALL return `Some(policy_id)` and
   `get_policy_id_by_external_ref(A)` and `get_policy_id_by_external_ref(B)` SHALL both
   return `None` (sequential re-index consistency property).

### Requirement 5: get_policy_id_by_external_ref Lookup Correctness

**User Story:** As an off-chain integration developer, I want `get_policy_id_by_external_ref`
to always return the correct, current policy ID for a given ref, so that I can reliably
resolve provider references without stale or ambiguous results.

#### Acceptance Criteria

1. THE `Insurance` SHALL expose a `get_policy_id_by_external_ref(ext_ref: String) →
   Option<u32>` function that reads `EXT_IDX` and returns the mapped policy ID, or `None`
   if the ref is not present.
2. WHEN `get_policy_id_by_external_ref` is called with a ref that was never registered,
   THE `Insurance` SHALL return `None`.
3. WHEN `get_policy_id_by_external_ref` is called with a ref that belongs to an active
   policy, THE `Insurance` SHALL return `Some(policy_id)` where `policy_id` matches the
   policy returned by `get_policy(policy_id)`.
4. WHILE a policy is active and its `external_ref` has not been changed, THE `Insurance`
   SHALL return the same `Some(policy_id)` on every call to
   `get_policy_id_by_external_ref` with that ref (stability invariant).
5. THE `Insurance` SHALL NOT return a policy ID for a ref that belongs to an archived or
   deactivated policy (no-stale-lookup security invariant).

### Requirement 6: Test Coverage for External-Reference Index Lifecycle

**User Story:** As a contract maintainer, I want a comprehensive test suite for the
external-reference index, so that regressions in index correctness are caught before
deployment.

#### Acceptance Criteria

1. THE test suite in `insurance/src/test.rs` SHALL include a test verifying that
   `create_policy` with a duplicate `external_ref` returns `InsuranceError::DuplicateExternalRef`.
2. THE test suite SHALL include a test verifying that after `archive_policy`, the archived
   policy's `external_ref` can be reused by a new `create_policy` call (reuse-after-archive).
3. THE test suite SHALL include a test verifying that `set_external_ref` correctly
   re-indexes the policy (old ref removed, new ref added).
4. THE test suite SHALL include a test verifying that `get_policy_id_by_external_ref`
   returns the correct policy ID for an active policy.
5. THE test suite SHALL include a test verifying that a malformed `external_ref` (empty
   string or length > 128) is rejected with `InsuranceError::InvalidExternalRef`.
6. THE test suite SHALL include a test verifying that after `deactivate_policy`, the
   deactivated policy's `external_ref` is no longer resolvable via
   `get_policy_id_by_external_ref`.
7. THE test suite SHALL include a test verifying that `ExternalRefUpdatedEvent` is emitted
   with correct `old_external_ref` and `new_external_ref` fields on `set_external_ref`.
8. THE test suite SHALL include a test verifying sequential ref changes (A→B→C) leave only
   C in the index.
9. WHERE `proptest` is available, THE test suite SHALL include a property-based test
   verifying that for any valid `external_ref` string, `create_policy` followed by
   `get_policy_id_by_external_ref` returns the correct policy ID (round-trip property).

### Requirement 7: Documentation of External-Reference Index Lifecycle

**User Story:** As a developer integrating with the Insurance contract, I want clear
documentation of the external-reference index lifecycle, so that I understand when refs
are claimed, freed, and how to use the lookup function safely.

#### Acceptance Criteria

1. THE `docs/insurance-external-ref.md` file SHALL document the purpose and structure of
   `KEY_EXT_REF_IDX` (`EXT_IDX`).
2. THE documentation SHALL describe the full index lifecycle: creation (via `create_policy`
   and `set_external_ref`), cleanup (via `archive_policy`, `deactivate_policy`, and
   `set_external_ref` with `None`), and lookup (via `get_policy_id_by_external_ref`).
3. THE documentation SHALL document the security assumption: `get_policy_id_by_external_ref`
   SHALL never return a policy ID for an archived or deactivated policy.
4. THE documentation SHALL include the `InsuranceError` codes `DuplicateExternalRef` (5)
   and `InvalidExternalRef` (4) with their trigger conditions.
5. THE documentation SHALL include a code example showing the full lifecycle: create with
   ref → lookup → archive → reuse ref.
6. THE `Insurance` contract source SHALL include `///` doc-comments on `KEY_EXT_REF_IDX`,
   `get_policy_id_by_external_ref`, `set_external_ref`, `archive_policy`,
   `ExternalRefUpdatedEvent`, and `EVT_EXT_REF_UPDATED` describing their role in the index
   lifecycle.
