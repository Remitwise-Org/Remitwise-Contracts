# Family Spending Report

`ReportingContract::get_family_spending_report(caller, user, period_start, period_end)`
builds a family-wallet spending snapshot from the configured `family_wallet`
dependency.

## Authorization

- `user.require_auth()` is enforced, matching the other user-facing reporting
  endpoints.
- `caller` is currently unused and kept for signature consistency with the
  savings, bills, insurance, and financial-health report methods.

## Data source

The report reads two family-wallet views:

1. `get_member_addresses_page(cursor, limit)` to enumerate the member set
   without fixed-limit truncation.
2. `get_spending_tracker(member)` to read each member's current cumulative
   spending amount.

## Output semantics

`FamilySpendingReport` now includes:

- `member_breakdown`: one entry per unique member address.
- `total_members`: number of unique members observed from the dependency.
- `total_spending`: sum of successfully read member spending totals.
- `average_per_member`: `total_spending / total_members`, or `0` when there are
  no members.
- `data_availability`: report completeness signal.

Each `FamilyMemberSpending` entry contains:

- `member`: member address.
- `total_spending`: tracked spending amount, or `0` when no tracker exists or
  the per-member read failed.
- `data_available`: `false` when that member's spending read failed.

## DataAvailability rules

- `Complete`: member enumeration succeeded and every member spending read
  succeeded.
- `Partial`: pagination hit `MAX_DEP_PAGES`, a later member page failed, a
  per-member spending read failed, or aggregate addition overflowed and had to
  clamp with saturating arithmetic.
- `Missing`: the first member-page read failed or the dependency returned zero
  members on the first page.

## Arithmetic safety

- Aggregate `i128` totals use `checked_add`.
- On overflow, the report does not panic. It marks
  `data_availability = Partial` and clamps the aggregate with
  `saturating_add`.
