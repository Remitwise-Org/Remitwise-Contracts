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

### Unreleased

- **Summary**: Aligns Bill Payments event emissions with the declared `BillEvent` taxonomy.
- **Indexer Notes**:
  - `cancel_bill` now emits `BillEvent::Cancelled` with `(bill_id, owner, cancelled_at)`.
  - `restore_bill` now emits `BillEvent::Restored` with `(bill_id, owner, restored_at)`.
  - `set_external_ref` now emits `BillEvent::ExternalRefUpdated` with `(bill_id, owner, external_ref, updated_at)`.
  - `batch_pay_bills` emits per-bill `BillEvent::Paid` events for successful items, matching `pay_bill`.
- **Migration Notes**: Indexers should subscribe to the direct `("bill", BillEvent::...)` topics for canonical bill lifecycle events; generic `Remitwise` audit topics remain available for compatibility.

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
