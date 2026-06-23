# Contract Changelog

This document tracks changes, versions, and migration notes for each of the smart contracts in the Remitwise suite.

## Remittance Split (`remittance_split`)

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

### v0.1.0

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
