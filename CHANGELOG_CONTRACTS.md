# Contract Changelog

This document tracks changes, versions, and migration notes for each of the smart contracts in the Remitwise suite.

## Remittance Split (`remittance_split`)

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

## Reporting (`reporting`)

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
