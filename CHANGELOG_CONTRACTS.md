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

### v0.1.0

- **Summary**: Initial release of the Reporting contract.
- **Breaking Changes**: None.
- **Migration Notes**: Baseline deployment.
