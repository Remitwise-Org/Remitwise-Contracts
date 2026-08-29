# Ergonomic Helpers

> **Audience:** Contributors implementing or reviewing Soroban contract code.

`remitwise-common` contains small helpers for recurring boundary and arithmetic
rules. Use them instead of open-coding the same rule in each contract: the
shared implementation makes the intended behavior visible and keeps edge cases
consistent.

All of these helpers are `no_std` compatible. They use integer arithmetic and
Soroban-compatible types; none requires `std`.

## Pagination: `clamp_limit`

Normalize a caller-provided page size before reading storage:

```rust
use remitwise_common::{clamp_limit, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

assert_eq!(clamp_limit(0), DEFAULT_PAGE_LIMIT);
assert_eq!(clamp_limit(12), 12);
assert_eq!(clamp_limit(u32::MAX), MAX_PAGE_LIMIT);
```

The contract is:

- `0` means the default page size (`20`);
- `1..=50` passes through unchanged;
- anything larger is capped at `50`.

For example, `bill_payments::get_archived_bills_page` normalizes the request
before its storage loop:

```rust
let effective_limit = clamp_limit(limit);
// Read at most the normalized number of archived bills.
```

Do not reject `0` or duplicate the constants locally. For cursor behavior and
the complete pagination review checklist, see the
[Pagination Handbook](PAGINATION_HANDBOOK.md).

## Ordered ranges: `validate_period`

Use `validate_period` at an entrypoint boundary when equal timestamps are valid
but a reversed range is not:

```rust
use remitwise_common::{validate_period, TimeError};

assert_eq!(validate_period(100, 100), Ok(()));
assert_eq!(validate_period(100, 200), Ok(()));
assert_eq!(validate_period(200, 100), Err(TimeError::InvalidPeriod));
```

Contracts should translate the shared error into their public error type. The
reporting entrypoint `get_remittance_summary` uses this pattern:

```rust
remitwise_common::validate_period(period_start, period_end)
    .map_err(|_| ReportingError::InvalidPeriod)?;
```

This helper checks ordering only. It does not establish that either timestamp
came from the ledger, impose a maximum duration, or authorize the caller. Keep
those entrypoint-specific checks beside it. The workspace-wide time rules are
documented in [Period Invariants](PERIOD_INVARIANTS.md).

## Future distance: `Timestamp::seconds_until`

Use `Timestamp::seconds_until` when code needs a duration until a target and a
past target should produce zero:

```rust
use remitwise_common::Timestamp;

assert_eq!(Timestamp::seconds_until(1_000, 1_300), 300);
assert_eq!(Timestamp::seconds_until(1_300, 1_000), 0);
```

It is equivalent to `target.saturating_sub(now)`, so it cannot underflow.
`bill_payments::create_bill_schedule` first rejects a non-future due date, then
uses the helper to enforce the maximum lead time:

```rust
let current_time = env.ledger().timestamp();
if next_due <= current_time {
    return Err(BillPaymentsError::InvalidDueDate);
}
if Timestamp::seconds_until(current_time, next_due) > MAX_SCHEDULE_LEAD_TIME {
    return Err(BillPaymentsError::ScheduleLeadTimeTooLong);
}
```

The helper does not prove that a target is in the future—the saturated result
is also zero when the timestamps are equal or reversed. Add an explicit
ordering check when that distinction affects behavior.

## Rates: `Percent` and `Rate`

Use `Percent` for whole percentage input and `Rate` for calculations in basis
points. Conversion and multiplication are checked:

```rust
use remitwise_common::{Percent, Rate, RateError};

fn apply_percent(amount: i128, percent: u32) -> Result<i128, RateError> {
    let rate: Rate = Percent::from_percentage(percent).try_into()?;
    rate.apply_to(amount)
}

assert_eq!(apply_percent(20_000, 5), Ok(1_000));
```

Production contract code should likewise propagate conversion errors. For
untrusted entrypoint input, the core lines are:

```rust
let rate: Rate = Percent::from_percentage(requested_percent).try_into()?;
let fee = rate.apply_to(amount)?;
```

`Rate::from_bps` intentionally accepts every `u32`. If the domain requires a
maximum such as `10_000` (100%), enforce that semantic bound at the entrypoint.
See [Type-Safe Percent Conversion](type-safe-percent-conversion.md) for the
units, overflow behavior, and conversion API.

## Contributor checklist

When adding or reviewing a use of these helpers:

- normalize external pagination limits before allocating or reading;
- map shared validation errors to the contract's public error;
- pair `seconds_until` with an ordering check when past and present differ;
- keep percentage-to-basis-point conversion checked;
- add the contract-specific constraint the helper deliberately does not own;
- use `soroban_sdk` primitives in contract paths and preserve `#![no_std]`.

Run the shared helper tests and the WASM and workspace checks before pushing:

```bash
cargo test -p remitwise-common
cargo build --target wasm32-unknown-unknown --release
cargo clippy --workspace --all-targets -- -D warnings
```
