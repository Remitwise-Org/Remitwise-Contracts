# Bill Scheduling & Execution: Amount Precision and Overflow — Issue #1737

## Overview

This change makes scheduled bill execution **deterministic across due dates,
retries, missed windows, and partial infrastructure failures** by enforcing
exact-integer amount rules and checked arithmetic at every boundary of the
`bill_payments` contract.

Before this change, the following could silently corrupt balances or leave a
plan unrecoverable:

1. `create_bill_schedule` accepted **any** `i128` amount — including `0`,
   negatives, and values large enough to threaten per-owner total overflow —
   with no validation at all.
2. `adjust_unpaid_total` used `saturating_add`, silently clamping an owner's
   unpaid balance on overflow instead of rejecting the operation.
3. `execute_due_bill_schedules` used `saturating_add` for the next-bill id
   counter: at `u32::MAX` it would silently **reuse an existing bill id and
   overwrite a live record**.
4. `execute_due_bill_schedules` used unchecked `next_due + interval` and
   `saturating_add` in the missed-window catch-up loop, risking wrap-around
   (release builds) or silent clamping of the next due date.
5. `batch_pay_bills` did not compile (undefined `next_id`, mutation of an
   immutable `bills` map) and, in the merged form, computed an unpaid-total
   delta that was **never applied** — the cached total went stale after every
   batch payment.
6. A schedule with a `u64`-sized interval could be created and would then
   brick the permissionless executor forever (unrecoverable state).
7. The settlement-window guard on `pay_bill`/`batch_pay_bills` (rejecting
   payment more than 30 days past due) was lost in a bad merge, silently
   permitting stale obligations to be paid.

## Canonical Amount Rules (shared)

Added in `remitwise-common/src/amount.rs`, consumed by every contract:

| Constant / Function | Value / Behavior |
| --- | --- |
| `MIN_AMOUNT` | `1` — one unit of the asset's smallest denomination. |
| `MAX_AMOUNT` | `10^30` — one sextillion stroops (≈ `10^23` XLM). |
| `validate_amount(i128)` | `Ok` iff `MIN_AMOUNT <= amount <= MAX_AMOUNT`; otherwise `Err(AmountValidationError::NonPositive)` (zero/negative) or `Err(AmountValidationError::ExceedsMaximum)`. |
| `checked_add_amount` / `checked_sub_amount` | Checked `i128` arithmetic returning `AmountOverflowError` instead of wrapping/saturating. |

**Why `10^30`?** All amounts are exact integers in smallest units. `10^30`
is a round power of ten chosen so that every aggregation over validated
amounts is provably free of `i128` overflow:

| Aggregation | Worst case | Headroom vs `i128::MAX` (~1.7 × 10³⁸) |
| --- | --- | --- |
| Per-owner unpaid total | `1000 × 10³⁰ = 10³³` | ~1.7 × 10⁵ × |
| Single batch delta | `50 × 10³⁰ = 5 × 10³¹` | ~3.4 × 10⁶ × |

## Invariants

1. **Exact integer arithmetic.** Amounts are exact `i128` integers in the
   asset's smallest unit. There is no floating-point or fixed-point
   representation anywhere in storage, scheduling math, or totals — a value
   is **rejected**, never rounded, truncated, or scaled.
2. **Sign and scale at every boundary.** `create_bill`,
   `create_bill_schedule`, `modify_bill_schedule`, schedule execution
   (defence-in-depth on stored schedules), and `check_invariants` (pay /
   batch / cancel / archive) all enforce `[MIN_AMOUNT, MAX_AMOUNT]`
   **before any state change**.
3. **Checked arithmetic.** Unpaid-total adjustments, id counters
   (`NEXT_ID`, `NEXT_BSCH`, the `EXE_CURS` cursor), and `next_due + interval`
   advancement are all checked. Overflow is rejected deterministically —
   a typed error where the entry point returns `Result`, or a panic (full
   invocation revert) where it does not — never silently saturating.
4. **No partial state.** Rejected, stale, repeated, and failed operations
   leave no partial state: validation and computation run before writes, and
   any panic reverts the whole invocation.
5. **Unpaid-total cache == oracle.** The per-owner unpaid-total cache is
   maintained by the (checked) `adjust_unpaid_total` and always equals an
   independent sum of unpaid bill amounts. Regression tests verify this
   against an independent oracle after create, pay, cancel, batch-pay, and
   schedule execution.
6. **Bounded intervals.** Schedule intervals are capped at
   `MAX_SCHEDULE_INTERVAL = MAX_FREQUENCY_DAYS × SECONDS_PER_DAY` (100 years)
   at creation/modification, so the interval → `frequency_days` conversion is
   always exact and the executor can always make progress.
7. **Missed-window determinism.** `pay_bill` and `batch_pay_bills` reject
   settlement more than `MAX_SETTLEMENT_WINDOW_SECS` (30 days) past due with
   `SettlementWindowExpired` before any state change.

## Failure Behavior

| Condition | Behavior |
| --- | --- |
| `amount <= 0` | `BillPaymentsError::InvalidAmount` (unchanged for `create_bill`/`modify_bill_schedule`; **new** for `create_bill_schedule`). |
| `amount > MAX_AMOUNT` | `BillPaymentsError::AmountExceedsMax` (**new**, discriminant 38). |
| Unpaid-total / batch-delta overflow | `BillPaymentsError::AmountOverflow` (**new**, discriminant 37); in non-`Result` paths a panic reverts the invocation. |
| `interval > MAX_SCHEDULE_INTERVAL` | `BillPaymentsError::ScheduleIntervalTooLong` (**new**, discriminant 39). |
| Bill/schedule id counter exhausted (`u32::MAX`) | `BillPaymentsError::IdSpaceExhausted` (**new**, discriminant 40); in `execute_due_bill_schedules` a panic reverts. |
| Stored schedule with invalid amount (legacy/corrupt) | `execute_due_bill_schedules` panics (revert) — the schedule owner must modify or cancel it. |
| Stored schedule with unrepresentable interval | Same as above (`InvariantViolation`). |
| Settlement more than 30 days past due | `SettlementWindowExpired` (**restored** in `pay_bill`/`batch_pay_bills`). |

## Compatibility Impact

- **Additive API:** four new `BillPaymentsError` variants; all existing error
  codes and entry-point signatures are unchanged.
- **Behavior changes (documented):**
  - `create_bill_schedule` now rejects zero/negative/oversized amounts (it
    previously accepted them silently — an obvious bug).
  - Amounts above `MAX_AMOUNT` (10³⁰ stroops, ≈10²³ XLM) are rejected
    everywhere. No realistic deployment is affected; any external caller that
    previously passed such a value now receives an error instead of
    corrupting totals.
  - Schedule intervals above 100 years are rejected at create/modify.
  - Unpaid-total overflow now errors/reverts instead of silently saturating
    (only reachable with pre-existing legacy data exceeding `MAX_AMOUNT`).
  - Payment more than 30 days past due is rejected again in `pay_bill` and
    `batch_pay_bills` (the guard lost in a bad merge is restored; the
    existing `test_pay_bill_settlement_window_expired` pins this).
- **`batch_pay_bills`** keeps its `Result<(), BillPaymentsError>` signature
  and skip-not-fail semantics for non-existent / cross-owner / already-paid
  ids, but now **compiles**, stages all side effects, and applies the
  unpaid-total delta exactly.

## Migration & Rollback

- No storage migration is required: existing bills and schedules remain
  valid as long as their amounts are within `[1, MAX_AMOUNT]` and intervals
  are representable. Out-of-range legacy records are rejected at the next
  interaction (see Failure Behavior) and can be fixed by the owner via
  `modify_bill_schedule` / `cancel_bill` — they never brick the contract,
  because every rejection is a full revert that leaves the record intact and
  the executor skipping past the affected schedule is bounded by the
  `MAX_BATCH_SIZE` cursor.
- Rollback: reverting to a pre-change build restores the previous (buggy)
  behavior; the new error variants and constants are additive, so a rollback
  is a plain redeploy.

## Operational Limitations

- `execute_due_bill_schedules` is permissionless and paginated at
  `MAX_BATCH_SIZE` (50) per call via the `EXE_CURS` cursor; a corrupt stored
  schedule aborts the current window (full revert) and must be repaired by
  its owner before the executor can pass it.
- `get_total_unpaid`, `get_total_unpaid_by_currency`, and
  `update_storage_stats` use **checked** aggregation reads (matching the
  checked write path); with all writes bounded by `MAX_AMOUNT`, those reads
  can never overflow in practice — the panic is defence-in-depth only.
- The CI **gas-benchmarks** job compares `bill_payments` against
  `benchmarks/baseline.json` (default 10% threshold; `batch_pay_bills` is
  method-specific at 15% CPU / 12% mem). `batch_pay_bills` was rewritten
  (staged, atomic) as part of this issue and `pay_bill` gained the recurring
  child-amount netting; if the merge shows a regression over threshold, run
  `./scripts/update_baseline.sh` to record the new legitimate baseline before
  merging. `pay_bill` itself has no baseline entry, so it is compared
  leniently (reported as new, never failed).

## Security Assumptions

- Amounts are trusted as exact smallest-unit integers; no contract in this
  workspace converts them to floating point.
- The shared `MAX_AMOUNT` bound is a platform-wide policy: changing it in
  `remitwise-common` affects every consumer (only `bill_payments` consumes it
  today).
- The cross-contract epoch / trusted-orchestrator guard on `pay_bill` is
  unchanged; tests configure a trusted orchestrator to exercise the guarded
  path.

## Validation

Run from the repository root (toolchain: `rust-toolchain.toml`):

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p remitwise-common
cargo test -p bill_payments
cargo build --release --target wasm32-unknown-unknown --workspace
```

**Test-suite compatibility audit:** the restored 30-day settlement check was
audited against every existing `pay_bill`/`batch_pay_bills` call site. The
default test ledger timestamp is `0` (soroban-env-host
`set_test_ledger_info_with_current_test_protocol`), and all pre-existing tests
create bills with future due dates and pay within the 30-day window — only
`test_pay_bill_settlement_window_expired` (which explicitly advances 46 days
past due) expects the rejection.

**Pre-existing #1776 breakage migrated (test-only):** the whole bill_payments
test suite (unit tests in `src/test.rs`, `events_schema_test.rs`, and all 13
integration files under `tests/` plus `examples/bill_payments_example.rs`)
still called the pre-#1776 two-argument `pay_bill(owner, id)` signature and
would not compile. Every call site now configures a trusted orchestrator and
passes `(orchestrator, epoch, caller, id)`. The large-amount stress tests that
asserted the removed saturating behavior or used amounts above `MAX_AMOUNT`
were updated to the new documented bounds (see `stress_test_large_amounts.rs`).
These are test-only changes required by the "complete repository test suite"
validation criterion; no contract behavior changed beyond what is documented
in Compatibility Impact.

Regression coverage added beside the implementation:

- `remitwise-common/src/amount.rs` — unit tests (zero, negative, min, max,
  max+1, `i128::MAX`, checked add/sub) plus a **proptest** sweeping the full
  `[MIN_AMOUNT, MAX_AMOUNT]` range invariant against an independent range
  check.
- `bill_payments/src/tests_amount_precision.rs` — contract-level integration
  tests proving the invariant at the actual storage boundary:
  - rejection of zero / negative / near-overflow amounts at bill, schedule,
    and modify boundaries with **no partial state** (no records, no consumed
    ids, unchanged stored schedules);
  - acceptance of minimum and maximum amounts, with exact totals;
  - interval cap boundary (accepted at max, rejected one second above);
  - interval → `frequency_days` conversion boundaries (1 and 36 500) and
    exact seconds-per-day due-date math;
  - schedule execution with fractional-scale and max amounts — exact
    round-trip, exact unpaid total;
  - defence-in-depth: a corrupt stored schedule amount is rejected
    (panic/revert) instead of minting an invalid bill;
  - unpaid-total **oracle** tests: the cached total equals an independent
    checked sum after create → pay → cancel → batch-pay;
  - batch-pay delta regression (the pre-fix stale-total bug);
  - recurring batch/pay net-zero total with exact child amounts;
  - repeated / stale / missing operations leave no partial state.
