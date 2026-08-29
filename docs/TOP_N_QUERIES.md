# Top-N Queries

This document catalogues every entrypoint that returns a bounded, sorted subset of
records — "top-N queries" — across the Remitwise contracts. It is written for
**contributors** who need to know which functions cap results, what the cap is,
and how the results are ordered.

## True top-N reports (reporting crate)

These entrypoints sort all matching records by a **value field** then return the
top `N` items. They live in the `reporting` contract.

| Entrypoint | N | Primary sort | Tie-break | Lines |
|---|---|---|---|---|
| `get_top_bills_report` | `MAX_ITEMS_PER_REPORT` (10) | `amount` descending | `id` ascending | `reporting/src/lib.rs:1618` |
| `get_top_savings_report` | `MAX_ITEMS_PER_REPORT` (10) | `target_amount` descending | `id` ascending | `reporting/src/lib.rs:1713` |

- The cap is **hardcoded** via `MAX_ITEMS_PER_REPORT = 10` (`reporting/src/lib.rs:37`).
  There is no caller-supplied `limit` parameter.
- Both entrypoints iterate all pages from their dependency contract and maintain a
  bounded in-memory list. If the dependency pagination reaches `MAX_DEP_PAGES` (20)
  before exhausting all records, the report sets `data_availability: Partial`.
- The ordering contract is documented in detail in [`top-n-report-ordering.md`](top-n-report-ordering.md).

## Paginated list queries (all crates)

Every other bounded-list entrypoint uses a **caller-supplied `limit`** parameter
that is normalised via `remitwise_common::clamp_limit` (or a local equivalent).
Results are sorted by **record ID ascending** (insertion order), not by a value
field. These are paginated list queries, not true "top-N" reports.

### Shared limit contract (`remitwise-common`)

Defined in `remitwise-common/src/lib.rs:124`:

| Input | Result |
|---|---|
| `0` | `DEFAULT_PAGE_LIMIT` (20) |
| `1 ..= MAX_PAGE_LIMIT` | passthrough |
| `> MAX_PAGE_LIMIT` | `MAX_PAGE_LIMIT` (50) |

See [`pagination-limit-contract.md`](pagination-limit-contract.md) for the full
specification.

### Bill payments (`bill_payments` crate)

All functions use `remitwise_common::clamp_limit` for normalisation.

| Entrypoint | Sorted by | Lines |
|---|---|---|
| `get_unpaid_bills(owner, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:1964` |
| `get_all_bills_for_owner(owner, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:2003` |
| `get_overdue_bills(cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:2062` |
| `get_overdue_bills_for_owner(owner, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:2128` |
| `get_all_bills_page(caller, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:2205` |
| `get_archived_bills_page(owner, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:2461` |
| `get_bills_by_currency(owner, currency, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:3027` |
| `get_unpaid_bills_by_currency(owner, currency, cursor, limit)` | bill ID ascending | `bill_payments/src/lib.rs:3087` |

### Savings goals (`savings_goals` crate)

Defines its own `clamp_limit` locally (`savings_goals/src/lib.rs:333`) with the
same contract (`DEFAULT_PAGE_LIMIT = 20`, `MAX_PAGE_LIMIT = 50`).

| Entrypoint | Sorted by | Lines |
|---|---|---|
| `get_goals(owner, cursor, limit)` | goal ID ascending | `savings_goals/src/lib.rs:1595` |
| `get_goals_by_tag(owner, tag, cursor, limit)` | goal ID ascending | `savings_goals/src/lib.rs:1668` |
| `get_archived_goals_page(owner, cursor, limit)` | goal ID ascending | `savings_goals/src/lib.rs:1877` |
| `get_archived_goals(owner, cursor, limit)` | goal ID ascending | `savings_goals/src/lib.rs:1941` |

### Family wallet (`family_wallet` crate)

Defines per-feature limits (constants at `family_wallet/src/lib.rs:34-40`).

| Entrypoint | Limit bounds | Lines |
|---|---|---|
| `get_member_addresses_page(cursor, limit)` | `0→20`, cap `100` | `family_wallet/src/lib.rs:1870` |
| `get_pending_transactions_page(caller, cursor, limit)` | `0→20`, cap `100` | `family_wallet/src/lib.rs:1383` |
| `get_access_audit_page(caller, from_index, limit)` | `0→20`, cap `50` | `family_wallet/src/lib.rs:2479` |
| `get_access_audit(limit)` | tail slice, no clamp | `family_wallet/src/lib.rs:2442` |

### Insurance (`insurance` crate)

| Entrypoint | Limit bounds | Lines |
|---|---|---|
| `get_active_policies(owner, cursor, limit)` | `0→20`, cap `50` | `insurance/src/lib.rs:560` |

### Remittance split (`remittance_split` crate)

| Entrypoint | Limit bounds | Lines |
|---|---|---|
| `get_schedules_paginated(owner, from_index, limit)` | `0→20`, cap `50` | `remittance_split/src/lib.rs:2582` |
| `get_remittance_schedules_page(owner, cursor, limit)` | `0→20`, cap `50` | `remittance_split/src/lib.rs:2660` |

### Reporting — archived reports

| Entrypoint | Limit bounds | Lines |
|---|---|---|
| `get_archived_reports_page(user, cursor, limit)` | `0→20`, cap `50` | `reporting/src/lib.rs:2108` |

## Audit log entrypoints

These return audit trail entries from a ring buffer, sorted oldest-first.

| Contract | Entrypoint | Ring-buffer cap | Page limit | Lines |
|---|---|---|---|---|
| orchestrator | `get_audit_log(from_index, limit)` | `MAX_AUDIT_ENTRIES = 100` | `0→20`, cap `100` | `orchestrator/src/lib.rs:524` |
| savings_goals | `get_audit_log(from_index, limit)` | `MAX_AUDIT_ENTRIES = 5` | hard cap `5` | `savings_goals/src/lib.rs:2181` |
| remittance_split | `get_audit_log(from_index, limit)` | `MAX_AUDIT_ENTRIES = 100` | `0→20`, cap `50` | `remittance_split/src/lib.rs:1705` |
| family_wallet | `get_access_audit_page(caller, from_index, limit)` | `MAX_ACCESS_AUDIT_ENTRIES = 200` | `0→20`, cap `50` | `family_wallet/src/lib.rs:2479` |

## Related documentation

- [`top-n-report-ordering.md`](top-n-report-ordering.md) — ordering contract for
  the two true top-N report entrypoints.
- [`pagination-limit-contract.md`](pagination-limit-contract.md) — the shared
  `clamp_limit` normalisation contract.
