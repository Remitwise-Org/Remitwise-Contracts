# Family Spending Report

The reporting contract exposes `get_family_spending_report` to aggregate per-member
spending from the configured `family_wallet` dependency.

## Entrypoint

```rust
get_family_spending_report(
    env,
    caller,
    user,
    period_start,
    period_end,
) -> Result<FamilySpendingReport, ReportingError>
```

- **`user`**: Must authorize via `user.require_auth()`. The report is scoped to
  the authenticated caller's request context (same pattern as other report endpoints).
- **`period_start` / `period_end`**: Inclusive reporting window metadata stored on
  the returned struct. Must satisfy `period_start <= period_end` or the call returns
  `ReportingError::InvalidPeriod`.
- **Dependency**: Requires `configure_addresses` to have stored a `family_wallet`
  contract ID. Otherwise returns `ReportingError::AddressesNotConfigured`.

## Schema: `FamilySpendingReport`

| Field | Type | Description |
| --- | --- | --- |
| `member_breakdown` | `Vec<FamilyMemberSpending>` | One entry per unique member address visited while paging. |
| `total_members` | `u32` | Length of `member_breakdown` after deduplication. |
| `total_spending` | `i128` | Sum of per-member `total_spending` values (checked add; saturates on overflow). |
| `average_per_member` | `i128` | `total_spending / total_members`, or `0` when `total_members == 0`. |
| `period_start` | `u64` | Echo of the request window start. |
| `period_end` | `u64` | Echo of the request window end. |
| `data_availability` | `DataAvailability` | Completeness indicator (see below). |

### `FamilyMemberSpending`

| Field | Type | Description |
| --- | --- | --- |
| `member` | `Address` | Family-wallet member address. |
| `total_spending` | `i128` | `SpendingTracker.current_spent` when available, else `0`. |
| `data_available` | `bool` | `true` when the spending tracker read succeeded (`Some` or `None`); `false` on cross-contract failure. |

## Downstream calls

The reporting contract uses a generated [`FamilyWalletClient`](../../reporting/src/lib.rs)
trait with:

1. `get_member_addresses_page(cursor, DEP_PAGE_LIMIT)` — paginated member enumeration.
2. `get_spending_tracker(member)` — per-member spending snapshot.

Members appearing on multiple pages are deduplicated before spending is fetched.

## DataAvailability rules

| Value | When set |
| --- | --- |
| `Complete` | All member pages drained within [`MAX_DEP_PAGES`](../../reporting/src/lib.rs) and every spending tracker read completed without error. |
| `Partial` | Any of: member paging reached `MAX_DEP_PAGES` before `next_cursor == 0`; a member-page fetch failed after at least one successful page; a spending tracker read failed; `total_spending` overflowed checked addition. |
| `Missing` | The first member page is empty, or the family wallet is unreachable on the initial member fetch. |

## Constants

- [`DEP_PAGE_LIMIT`](../../reporting/src/lib.rs) (`50`): page size for member address queries.
- [`MAX_DEP_PAGES`](../../reporting/src/lib.rs) (`20`): maximum member pages fetched per report call.

## Example

```rust
let report = client.get_family_spending_report(
    &user,
    &user,
    &period_start,
    &period_end,
)?;

match report.data_availability {
    DataAvailability::Complete => { /* use totals */ }
    DataAvailability::Partial => { /* totals may be truncated */ }
    DataAvailability::Missing => { /* dependency unavailable or no members */ }
}
```

## Testing

```bash
cargo test -p reporting get_family_spending
```

Edge cases covered in tests:

- Zero members → `Missing`, `average_per_member == 0`
- Unreachable family wallet → `Missing`
- Tracker read failure → `Partial`
- Spending sum overflow → `Partial`
- Member paging beyond `MAX_DEP_PAGES` → `Partial`
- Duplicate member addresses across pages → counted once
- Invalid period → `InvalidPeriod`
- Unconfigured addresses → `AddressesNotConfigured`
