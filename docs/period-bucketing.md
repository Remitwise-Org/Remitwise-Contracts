# Period Bucketing: Stable Day/Week/Month Keys for `Timestamp`

**Audience:** Contributor (engineers building, extending, reviewing, or testing logic that depends on stable calendar bucketing of Unix timestamps in Remitwise contracts).

---

## 1. Purpose

Many cross-cutting features — off-chain reporting rollups, indexer cursor keys, batch-aggregation grouping, recurring-schedule backfill — need to map a Unix timestamp to a **stable, reproducible bucket identifier**. The naïve answer is "use `timestamp / 86_400` for days," but that pattern is then re-implemented inconsistently across contracts, drifts between Day / Week / Month, and gets the calendar arithmetic wrong around leap years and month boundaries.

`Timestamp::to_period_key` in [`remitwise-common`](../remitwise-common/src/lib.rs) centralises this into one helper that:

- returns the same key on-chain, off-chain, and across the indexer pipeline;
- never panics for any `u64` input;
- documents its bounds so callers do not need to re-think boundary cases;
- uses one source of truth for the bucket-length constants (`SECONDS_PER_DAY`, `SECONDS_PER_WEEK`).

---

## 2. Definitions

The function is generic over a [`PeriodKind`](../remitwise-common/src/lib.rs) selector:

| `PeriodKind` | Output | Formula (UTC, proleptic Gregorian) |
| :--- | :--- | :--- |
| `Day`   | epoch-day index   (`u64`) | `timestamp / SECONDS_PER_DAY` |
| `Week`  | epoch-week index  (`u64`) | `timestamp / SECONDS_PER_WEEK` |
| `Month` | YYYYMM integer    (`u64`) | Howard Hinnant's `civil_from_days` — see [§3](#3-algorithm-month-bucket) |

All three are **TZ-naive UTC** and **leap-second agnostic** — they treat the Unix-second timeline as a flat count without inserting or removing the historical 25 leap seconds.

---

## 3. Algorithm — Month Bucket

The Month branch implements Howard Hinnant's `civil_from_days` algorithm against the proleptic Gregorian calendar in UTC. The constant `719_468` aligns day `0` (1970-01-01, Unix epoch) with Hinnant's reference epoch (0000-03-01).

```text
days      = timestamp / SECONDS_PER_DAY
z         = days + 719468
era       = (z >= 0 ? z : z - 146096) / 146097
doe       = z - era * 146097                          // [0, 146096]
yoe       = (doe - doe/1460 + doe/36524 - doe/146096) / 365   // [0, 399]
y         = yoe + era * 400
doy       = doe - (365*yoe + yoe/4 - yoe/100)
mp        = (5*doy + 2) / 153
month     = (mp + 2) % 12 + 1
year      = y + (mp + 2) / 12
result    = (year as u64) * 100 + (month as u64)     // YYYYMM
```

The Hinnant pre-epoch branch (`if z >= 0 … else z - 146_096`) is **dead code** for the `u64` API because `timestamp / SECONDS_PER_DAY` is always `≥ 0`. See [§5 Edge Cases](#5-edge-cases) for the rationale.

Reference: <https://howardhinnant.github.io/date_algorithms.html#civil_from_days>

---

## 4. Public API

```rust
pub enum PeriodKind {
    Day,
    Week,
    Month,
}

pub const SECONDS_PER_DAY: u64 = 86_400;
pub const SECONDS_PER_WEEK: u64 = 86_400 * 7;

impl Timestamp {
    pub fn to_period_key(timestamp: u64, period: PeriodKind) -> u64 { /* see lib.rs */ }
}
```

The function is `#[inline(always)]`, carries a function-scoped `#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]` for the two necessary `as` casts, and never returns a `Result` / never panics.

---

## 5. Edge Cases

| Case | Behaviour |
| :--- | :--- |
| `timestamp = 0` (1970-01-01 00:00:00 UTC)            | Day `0`, Week `0`, Month `197001` |
| `timestamp ∈ [0, 86_399]`                              | All map to Day `0` |
| `timestamp = 86_400` (1970-01-02 00:00:00 UTC)         | Day `1` |
| `timestamp = 604_800` (1970-01-08 00:00:00 UTC)        | Week `1` |
| Pre-1970 timestamp (any value < 86_400)               | Day `0` / Week `0` / Month `197001`. The `u64` API cannot represent pre-1970 instants. |
| `timestamp = u64::MAX`                                 | Day `u64::MAX / 86_400`, Week `u64::MAX / 604_800`, Month YYYYMM with month ∈ `1..=12`. **No panic.** |
| Leap seconds                                          | Ignored — the function treats every UTC second as a flat count. |
| Leap year (e.g. 2020-02-29)                            | Day/Week bucket as normal; Month returns `202002`, the same as 2020-02-28 (intentional: a single YYYYMM bucket spans the whole month). |
| Far-future overflow                                   | `(year as u64) * 100 + (month as u64)` overflows `u64` for year ≳ `1.8 × 10¹⁷`. Soroban ledger timestamps will not reach this in practice; the function does not panic, but the result wraps silently past that horizon. |

---

## 6. Concrete Contributor Example

```rust
use remitwise_common::{PeriodKind, Timestamp, SECONDS_PER_DAY, SECONDS_PER_WEEK};

// Bucket a Soroban ledger timestamp into day / week / month keys.
fn classify(now: u64) -> (u64, u64, u64) {
    (
        Timestamp::to_period_key(now, PeriodKind::Day),
        Timestamp::to_period_key(now, PeriodKind::Week),
        Timestamp::to_period_key(now, PeriodKind::Month),
    )
}

#[test]
fn classify_epoch() {
    let (d, w, m) = classify(0);
    assert_eq!(d, 0);
    assert_eq!(w, 0);
    assert_eq!(m, 197_001);
}

#[test]
fn classify_does_not_panic_at_u64_max() {
    let (d, w, m) = classify(u64::MAX);
    assert_eq!(d, u64::MAX / SECONDS_PER_DAY);
    assert_eq!(w, u64::MAX / SECONDS_PER_WEEK);
    let month = m % 100;
    assert!(month >= 1 && month <= 12);
}
```

---

## 7. Cross-references

- [`remitwise-common/README.md`](../remitwise-common/README.md) — crate-level overview of the helper, including the `"New in 2026.7"` banner.
- [`docs/PERIOD_INVARIANTS.md`](PERIOD_INVARIANTS.md) — broader invariants for time-bound mechanics across all contracts (ledger-time authority, period-boundary rules, overflow safety).
- [`docs/PERIOD_LIFECYCLE.md`](PERIOD_LIFECYCLE.md) — recurring-schedule lifecycle and catch-up semantics, which often key on `Timestamp::to_period_key(..., Month)`.
- [`docs/SETTLEMENT_WINDOWS.md`](SETTLEMENT_WINDOWS.md) — invoice settlement-window rules; bucketing is a downstream consumer pattern.
- [`docs/INDEXING.md`](INDEXING.md) — indexer cursor / backfill patterns that may use Day/Week keys for stable cursors.

---

## 8. Out of Scope / Follow-ups

> **TODO (follow-up issue):** Add `PeriodKind::Quarter` and `PeriodKind::Year` variants. These would derive from the Month bucket (`Quarter = Month / 3`, `Year = Month / 100`) and would only need new public-API symbols + tests, not new calendar arithmetic.

> **TODO (follow-up issue):** If `PeriodKind` ever crosses a contract ABI (e.g. via a public contract entry point), add `#[contracttype]` + a `from_u32` round-trip + an encoding-stability test mirroring `remitwise-common/src/lib.rs`'s `encoding_stability_tests` module.

> **TODO (follow-up issue):** Wire `Timestamp::to_period_key` into downstream consumers that already key off `timestamp / 86_400` ad-hoc — for example, the reporting crate's `(user, period_key)` composite storage key (`STORAGE_LAYOUT.md` §`REPORTS`).