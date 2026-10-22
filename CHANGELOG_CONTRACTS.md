# Contract Changelog

This document tracks changes, versions, and migration notes for each of the smart contracts in the Remitwise suite.

## Remittance Split (`remittance_split`)

### v0.2.2

- **Summary**: Restored structured `DistributionCompletedEvent` emission on both `distribute_usdc` and `distribute_usdc_hashed` via a shared helper (#843).
- **New Features**:
  - `emit_distribution_completed`: shared helper publishing unstructured `dist_ok` (backward compat) plus structured `(split, DistributionCompleted)` with per-category amounts
- **Breaking Changes**: None — the unstructured `dist_ok` event is retained alongside the structured event.
- **Migration Notes**: Indexers keyed on `(split, DistributionCompleted)` now receive events from the hashed path as well as the legacy path.

### v0.2.1

- **Summary**: Removed orphaned `RemittanceSchedulePage` contracttype that had no producer, shrinking the exported ABI.
- **Breaking Changes**: `RemittanceSchedulePage` removed from the type surface. Consumers using `SchedulePage` are unaffected.
- **Migration Notes**: No migration needed unless a consumer was referencing the never-produced `RemittanceSchedulePage`.

### v0.2.0

- **Summary**: Added owner-indexed schedule pagination with ordering guarantees.
- **New Features**:
  - `get_remittance_schedules_paginated()`: Paginated schedule queries with stable cursors
  - Deterministic ID-ascending ordering for all schedule queries
  - Enhanced pagination support with limit clamping and cursor stability
- **Breaking Changes**: None (new function added).
- **Migration Notes**: Existing `get_remittance_schedules()` now returns results in ID-ascending order for consistency.

### v0.1.0

- **Summary**: Initial release of the Remittance Split contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Bill Payments (`bill_payments`)

### v0.2.0

- **Summary**: Aligned bill event emissions with declared `BillEvent` enum variants.
- **Breaking Changes**:
  - `cancel_bill` action symbol changed from `"canceled"` to `"cancelled"` to match `BillEvent::Cancelled`
  - `set_external_ref` action symbol changed from `"ext_ref"` to `"ext_upd"` to match `BillEvent::ExternalRefUpdated`
- **Migration Notes**: Indexers must update action symbol filters to match the new symbols above.
- **Event Alignment**: This change aligns emitted bill events with `EVENTS.md` and removes ad-hoc event symbols.

### v0.1.0

- **Summary**: Initial release of the Bill Payments contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Insurance (`insurance`)

### v0.1.0

- **Summary**: Initial release of the Insurance contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Savings Goals (`savings_goals`)

### v0.2.0

- **Summary**: Default `create_goal` to unlocked; explicit locked path for commitment devices.
- **Changes**:
  - `create_goal` now defaults to `locked: false` (immediately withdrawable). 
    Added explicit `locked: bool` parameter to support commitment-device use cases.
  - `GoalCreatedEvent` includes resulting `locked` state.
  - Updated tests (including large-amount stress tests) to cover both default and explicit locked paths.
- **Breaking Changes**: Yes — ABI and default behaviour change for `create_goal`. 
  Callers that relied on locked-by-default must now pass `false` explicitly for normal goals.
- **Migration Notes**: 
  - Update all integrator calls to `create_goal` to pass the new `locked` parameter (default `false`).
  - No storage migration required (existing goals unaffected).
  - Bump contract version and re-deploy.

### v0.1.0
...

- **Summary**: Initial release of the Savings Goals contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Family Wallet (`family_wallet`)

### v0.1.0

- **Summary**: Initial release of the Family Wallet contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Emergency Kill Switch (`emergency_killswitch`)

### v0.2.1

- **Summary**: Restored missing checked-arithmetic helpers (`checked_add_u64`, `checked_add_u32`, `recovery_ready_at`, `snapshot_age`), fixed error discriminants, and repaired pagination test compilation (Issue #1865).
- **Changes**:
  - `activate` now computes the recovery deadline with checked arithmetic in the validation phase (previously `saturating_add`, which silently pinned the deadline at `u64::MAX` on overflow); a near-`u64::MAX` clock returns `Error::Overflow` with **zero** writes — no orphaned activation markers.
  - `bump_kill_switch_epoch` / `configure_signers*` epoch increments and `migrate_storage` step advances return `Error::Overflow` without partially updating persistent state (all checks precede all writes).
  - `restore_from_snapshot` rejects future-dated snapshots (`now < taken_at`) with `Error::Overflow` instead of underflowing the snapshot age.
  - Pagination test module compiles again (`vec!` import, test-only `std` for `format!`, `&1` threshold args, `push_back` construction instead of collecting an iterator into `soroban_sdk::Vec`). No assertions or production behavior changed.
- **Error Discriminants (ABI decision)**:
  - `Error::Overflow = 20` — **keeps the pinned value.** `20` was already asserted by existing tests (`epoch_mismatch_error_discriminant_is_seven`, `error_discriminants_are_stable`) and documented in `ACTIVATION_RECOVERY_POLICY.md`, so downstream integrations treating `20` as the overflow code continue to work unchanged.
  - `Error::InvalidCursor = 21` — moved from `20` to make room. This is safe: `InvalidCursor` is declared but **never produced or matched** by any entry point, test, or indexer in this repository (pagination cursors out of range return an empty page, not an error), so no pinned contract exists for its value.
  - All other discriminants (`1..=19`) are preserved.
- **Migration Notes**: No storage migration required. Off-chain code keying on `Error::Overflow == 20` is unaffected; do not key on `InvalidCursor == 20` (it is now `21`).
- **Tests**: Added regression tests covering `checked_add_*` overflow/happy paths, `recovery_ready_at` at the `u64::MAX` boundary, future-dated snapshot rejection with state preservation, the exact `SNAPSHOT_TTL` / `SNAPSHOT_TTL + 1` restore boundary, and signer-set preservation when the signer epoch is `u64::MAX`.

### v0.2.0

- **Summary**: Added a versioned, correlation-tagged control/audit event stream for every committed emergency transition — pause, recovery, threshold approval, and administrator rotation (Issue #1761).
- **New Features**:
  - `("emergency", "control")` audit events: `ControlEvent { version, seq, kind, actor, timestamp }` emitted on every committed transition, with `seq` sourced from a new monotonic per-contract counter (`DataKey::EventSeq`) and observable via the new read-only `get_event_seq()`.
  - `schedule_unpause` is now auditable via the control stream (previously it emitted no event).
- **Breaking Changes**: None at the ABI/type level — every public entry point keeps its exact signature and error behavior; legacy granular events are emitted unchanged. `CONTRACT_VERSION` bumped `1 → 2` so tooling can detect the new audit-event behavior.
- **Behavior Change**: `EmergencyStateSnapshot.active_scope` was re-encoded from `Option<PauseScope>` (not spec-serializable in soroban-sdk) to spec-scalar fields `scope_kind` / `scope_module` / `scope_function`. Snapshot semantics are unchanged; snapshots captured by a pre-change build must be discarded (`discard_snapshot`) and re-captured after upgrade (see `docs/issue-1761-emergency-controls-audit-parity.md`).
- **Migration Notes**: No storage migration required — the new `EventSeq` key is additive and lazily initialized. `STORAGE_VERSION` remains `1`.
- **Gas**: Gas benchmarks re-measured; write paths grew ~10–40% (one storage read/write + one event publish per committed transition). Baselines in `tests/gas_bench.rs` updated.
- **Tests**: Added `tests/audit_parity.rs` (7 tests) proving state↔event parity, strict `seq` ordering/versioning, committed-only emission, and consensus-actor semantics at the integration boundary.

### v0.1.0

- **Summary**: Initial release of the Emergency Kill Switch contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Reporting (`reporting`)

### v0.3.0

- **Summary**: Migrated all per-user data (active reports, archives, indexes, and viewer grants) from single instance-storage maps to per-record persistent storage entries to eliminate unbounded instance data growth and host-budget reverts. Added active report caps, paginated per-user archiving/cleanup, incremental storage stats, and a zero-downtime migration path.
- **Changes**:
  - Moved per-user records from instance storage `Map`s to persistent storage keys:
    - `DataKey::Report(Address, u64)`: active report
    - `DataKey::ReportIndex(Address)`: per-user list of active period keys, capped at `MAX_ACTIVE_REPORTS_PER_USER = 120`
    - `DataKey::Archived(Address, u64)`: archived report
    - `DataKey::ArchiveIndex(Address)`: per-user archive index, capped at `MAX_ARCHIVE_PER_USER = 500`
    - `DataKey::ViewerGrant(Address, Address, ReportScope)`: per-grant expiry timestamp
  - `store_report`: now enforces `MAX_ACTIVE_REPORTS_PER_USER = 120`. Attempting to store a 121st active report returns typed error `ReportingError::TooManyActiveReports = 16`.
  - `archive_old_reports`: updated ABI to `(caller: Address, user: Address, before_timestamp: u64, limit: u32) -> u32`. Archiving is now paginated and scoped per user, bounded by `limit`.
  - `cleanup_old_reports`: updated ABI to `(caller: Address, user: Address, before_timestamp: u64, limit: u32) -> u32`. Cleanup is now paginated and scoped per user, bounded by `limit`.
  - `StorageStats`: counters (`active_reports`, `archived_reports`) are maintained incrementally (+/- 1) on every mutation instead of recounting maps.
  - `migrate_reports_to_persistent`: admin-only paginated migration entrypoint `(caller: Address, limit: u32) -> Result<u32, ReportingError>`. Drains legacy instance maps into persistent entries and deletes instance keys upon completion.
  - Read fallbacks: reads for active reports, archives, and viewer grants transparently fall back to legacy instance maps if not yet migrated.
- **Breaking Changes**:
  - `archive_old_reports` ABI changed from `(caller, before_timestamp)` to `(caller, user, before_timestamp, limit)`.
  - `cleanup_old_reports` ABI changed from `(caller, before_timestamp)` to `(caller, user, before_timestamp, limit)`.
  - `store_report` now returns `Result<bool, ReportingError>` (rejecting when active cap is reached).
- **Migration Procedure**:
  - Upgrade the contract WASM.
  - Contract continues to serve reads seamlessly via fallback to legacy instance maps.
  - Admin invokes `migrate_reports_to_persistent(admin, limit)` repeatedly (recommended batch limit `50` to `100`) until it returns `0`.
  - Once migration completes, legacy instance keys `REPORTS`, `ARCH_RPT`, `ARCH_IDX`, and `VIEW_ACL` are completely deleted, freeing instance storage.
- **Tests**:
  - `test_store_report_cost_independent_of_other_users` (proves CPU instruction cost stays within <= 10% regardless of number of users).
  - `test_store_report_enforces_per_user_cap` (verifies 121st active report fails with `TooManyActiveReports`).
  - `test_archive_is_paginated_and_bounded` (verifies 250 reports archived across batches of 100/100/50).
  - `test_migration_moves_legacy_data` (verifies legacy map migration and removal of legacy instance keys).
  - `test_reads_fall_back_before_migration` (verifies fallback reads before migration).
  - `test_storage_stats_stay_consistent` (verifies consistency after 200 random/deterministic operations).

### v0.2.0

- **Summary**: Bounded legacy `get_archived_reports` to prevent host-budget reverts and promoted `get_archived_reports_page` as the supported archive reader (Issue #832).
- **Changes**:
  - `get_archived_reports` is **deprecated** (marked `#[deprecated]`). It now delegates to `get_archived_reports_page(0, DEFAULT_PAGE_LIMIT)` internally, so it returns at most the first `DEFAULT_PAGE_LIMIT` (20) entries instead of the entire `ARCH_IDX(user)` list. Signature preserved for back-compat.
  - `get_archived_reports_page` now uses the canonical terminator convention: out-of-range cursors (`cursor >= count`) and empty archives both return `next_cursor == 0` instead of echoing the cursor back.
  - `get_archived_reports_page` now normalizes `limit` through `remitwise_common::clamp_limit`: `0` maps to `DEFAULT_PAGE_LIMIT` (20); values above `MAX_PAGE_LIMIT` (50) clamp to `MAX_PAGE_LIMIT`. This matches every other paginated read in the Remitwise suite.
- **Breaking Changes**: None at the ABI/type level. **Behaviour change**: `get_archived_reports` previously returned an unbounded `Vec<ArchivedReport>`; it is now bounded to `DEFAULT_PAGE_LIMIT` (20) and may truncate users with very long archive histories. Callers that need the full archive should migrate to `get_archived_reports_page` and walk until `next_cursor == 0`.
- **Migration Notes**:
  - Replace `get_archived_reports(user)` with a paged walk: start with `get_archived_reports_page(user, 0, DEFAULT_PAGE_LIMIT)` and continue until `next_cursor == 0`.
  - No storage migration required.
  - Compatibility: signatures are unchanged. Existing callers that only inspect the first page are unaffected.
- **Tests**: Added `reporting/src/tests_archived_pagination_bound.rs` covering bound enforcement, first-page equivalence (deprecated reader vs paged reader), full archival traversal with termination, out-of-range cursor, empty archive, `limit=0` normalization, `limit=u32::MAX` clamping, and user isolation under bound.

### v0.1.0

- **Summary**: Initial release of the Reporting contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.

## Orchestrator (`orchestrator`)

### v0.2.0

- **Summary**: Integrated emergency killswitch checking prior to executing any remittance flow (Issue feat(orchestrator): honour the emergency_killswitch contract before executing any remittance flow).
- **New Features**:
  - `KillswitchInterface` contract client trait for querying `emergency_killswitch` status (`is_paused`, `is_module_paused`, `is_function_paused`).
  - Owner-only `set_killswitch(caller, killswitch: Option<Address>)` entrypoint storing `KS_ADDR` (`symbol_short!("KS_ADDR")`) and publishing `("orch", "ks_set")`.
  - Defence-in-depth pause enforcement across all flow entrypoints: `execute_remittance_flow` (`"flow"`), `execute_remittance_flow_signed` (`"flow_sig"`), `execute_flow_fanout` (`"fanout"`), and `claim_rewards_summary_external` (`"clm_rwd"`).
  - Typed error `OrchestratorError::EmergencyPaused = 16`.
  - Fails closed on cross-contract query errors or contract absence.
- **Breaking Changes**: None at the ABI level; pause checking only takes effect when a killswitch address is explicitly configured.
- **Migration Notes**: No storage migration required. Optional killswitch address can be configured by the orchestrator owner.

