# Top-N report ordering contract (deterministic)

This document defines the ordering contract for the reporting contract’s Top-N endpoints:

- `get_top_bills_report`
- `get_top_savings_report`

## Determinism requirement

For the same inputs (same dependency data set, same report period, same configured dependency addresses), the contract must return results in a reproducible order across:

- repeated calls
- different networks
- different ledger environments

This contract guarantees that tie cases cannot produce non-deterministic output.

## Bills Top-N ordering

`get_top_bills_report` returns up to `MAX_ITEMS_PER_REPORT` `Bill` items.

Sorting rule:

1. Primary sort: `amount` **descending**
2. Tie-break (when `amount` is equal): `id` **ascending**

No padding is applied. If fewer than `MAX_ITEMS_PER_REPORT` matching items exist, the report contains exactly those items.

## Savings Top-N ordering

`get_top_savings_report` returns up to `MAX_ITEMS_PER_REPORT` `SavingsGoal` items.

Sorting rule:

1. Primary sort: `target_amount` **descending**
2. Tie-break (when `target_amount` is equal): `id` **ascending**

No padding is applied. If fewer than `MAX_ITEMS_PER_REPORT` matching items exist, the report contains exactly those items.

## Gas boundedness and partial data

Both Top-N implementations are gas-bounded by maintaining a bounded in-memory top list (capped to `MAX_ITEMS_PER_REPORT`).

If dependency paging is capped and the reporting contract cannot retrieve all dependency items, the report MUST still:

- return a bounded number of items (never more than `MAX_ITEMS_PER_REPORT`)
- set `data_availability` to `DataAvailability::Partial`

The ordering contract above still applies to the returned (possibly truncated) top list.

