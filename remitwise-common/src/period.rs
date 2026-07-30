//! Shared period-key validation helpers.
//!
//! Period keys identify the accounting period to which an entity belongs.
//! Combining entities with different keys can cause state from one period to
//! be interpreted as state from another, so callers must validate keys before
//! performing a multi-entity operation.
//!
//! This module also hosts the [`verify_period_active`] defence-in-depth
//! helper for issue #1234, which rejects writes into periods that are
//! either still in the future (`period_start > now`) or that have already
//! been sealed into the caller's archive storage (`is_archived == true`).

use soroban_sdk::contracterror;

/// Errors returned by period-key validation.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeriodKeyError {
    /// The entities belong to different periods.
    MismatchedPeriodKey = 1,
    /// The supplied period is not currently active. Either its start
    /// timestamp is still in the future (the period has not opened yet), or
    /// the caller has already moved the period to its archive storage.
    ///
    /// Writes that would touch a future or archived period must be rejected
    /// to keep the active/archived partition clean. See
    /// [`verify_period_active`] for the full threat model.
    PeriodNotActive = 2,
}

/// Require two entities to belong to the same period.
pub fn require_matching_period_key(
    a_pk: u64,
    b_pk: u64,
) -> Result<(), PeriodKeyError> {
    if a_pk == b_pk {
        Ok(())
    } else {
        Err(PeriodKeyError::MismatchedPeriodKey)
    }
}

/// Defence-in-depth period-active guard (closes #1234).
///
/// Rejects writes into periods that are either:
///
/// 1. **Future** — the period's start timestamp has not yet been reached, or
/// 2. **Archived** — the caller has already moved the period to its archive
///    storage (typically by an admin archive/archive-cleanup entrypoint).
///
/// # Threat model
///
/// Without this guard, a buggy or compromised caller could:
///
/// - **Pre-load future periods.** Write health-score reports, schedule
///   entries, or bills tagged to a future period. In particular for
///   scoring/analytics, an attacker could game a "best month ever" report
///   by populating a near-future bucket with self-serving data before the
///   period actually opens.
/// - **Resurrect archived periods.** Mutate state under a `(user, pk)`
///   composite key whose period has already been sealed and moved to
///   archive storage. This breaks the invariant that the archive map is
///   immutable once sealed, voids any dashboards that read the archive,
///   and re-exposes funds or scoring that were intentionally finalised.
///
/// Both failure modes are rejected up-front at any write entry point that
/// opts into this check, so the blast radius is the entry point that
/// forgets to call it.
///
/// # Why this is a pure helper
///
/// This function is **stateless**: it does not touch contract storage.
/// The caller supplies `is_archived` based on its own archive tracking
/// (e.g. the archive map in `reporting`, `bill_payments`, `family_wallet`,
/// or any future contract that adopts period-keyed storage). Keeping the
/// helper pure lets it be reused without coupling every caller to a
/// single canonical archive storage location, and makes it trivially
/// unit-testable.
///
/// # Arguments
/// * `period_start`  — the timestamp at which the period started (e.g. the
///                      Unix-second start boundary of a `pk = YYYYMM`, the
///                      day-start of a `pk = YYYYMMDD`, or the Unix-second
///                      boundary itself).
/// * `now`           — the current ledger timestamp
///                      (`env.ledger().timestamp()`).
/// * `is_archived`   — `true` iff the caller has already moved the
///                      period to its archive storage; `false` otherwise.
///
/// # Errors
/// * [`PeriodKeyError::PeriodNotActive`] when `is_archived == true` or
///   `period_start > now` (the period is still in the future).
///
/// # Recommended call-site pattern
///
/// ```ignore
/// pub fn store_current_period_report(env: Env, user: Address, pk: u64, period_start: u64) {
///     let now = env.ledger().timestamp();
///     let archived = is_period_in_archive_map(&env, &user, pk);
///     verify_period_active(period_start, now, archived).unwrap_or_else(|_| {
///         soroban_sdk::panic_with_error!(&env, MyContractError::PeriodNotActive)
///     });
///     // ... proceed with the write
/// }
/// ```
pub fn verify_period_active(
    period_start: u64,
    now: u64,
    is_archived: bool,
) -> Result<(), PeriodKeyError> {
    if is_archived {
        return Err(PeriodKeyError::PeriodNotActive);
    }
    if period_start > now {
        return Err(PeriodKeyError::PeriodNotActive);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{require_matching_period_key, verify_period_active, PeriodKeyError};

    // ── require_matching_period_key: happy path ───────────────────────────

    #[test]
    fn accepts_entities_from_the_same_period() {
        assert_eq!(require_matching_period_key(202401, 202401), Ok(()));
    }

    /// Zero is a valid period key; two entities both at zero belong to
    /// the same (epoch-zero) period and must be accepted.
    #[test]
    fn accepts_matching_period_key_zero() {
        assert_eq!(require_matching_period_key(0, 0), Ok(()));
    }

    /// Two entities keyed to `u64::MAX` both belong to the same period.
    #[test]
    fn accepts_matching_period_key_u64_max() {
        assert_eq!(require_matching_period_key(u64::MAX, u64::MAX), Ok(()));
    }

    /// Arbitrary symmetric mid-range value is accepted.
    #[test]
    fn accepts_matching_period_key_mid_range() {
        let pk = 1_704_067_200u64; // 2024-01-01T00:00:00Z as a raw timestamp key
        assert_eq!(require_matching_period_key(pk, pk), Ok(()));
    }

    /// The comparison is symmetric: (a, b) and (b, a) with the same value
    /// both accept.
    #[test]
    fn accepts_matching_period_key_is_symmetric() {
        let pk = 202406u64;
        assert_eq!(require_matching_period_key(pk, pk), Ok(()));
    }

    // ── require_matching_period_key: sad path ─────────────────────────────

    #[test]
    fn rejects_entities_from_different_periods() {
        let result = require_matching_period_key(202401, 202402);
        assert_eq!(result, Err(PeriodKeyError::MismatchedPeriodKey));
    }

    /// One-off adjacency: period keys that differ by exactly 1 are different
    /// periods and must be rejected. This pins the strict `==` contract
    /// against an off-by-one relaxation.
    #[test]
    fn rejects_period_keys_differing_by_one() {
        assert_eq!(
            require_matching_period_key(202401, 202402),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
        // Reverse direction too (argument order must not matter for rejection).
        assert_eq!(
            require_matching_period_key(202402, 202401),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
    }

    /// Zero vs one — smallest possible non-zero mismatch.
    #[test]
    fn rejects_zero_vs_one_period_key() {
        assert_eq!(
            require_matching_period_key(0, 1),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
        assert_eq!(
            require_matching_period_key(1, 0),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
    }

    /// `u64::MAX` vs `u64::MAX - 1`: upper-boundary one-off.
    #[test]
    fn rejects_u64_max_vs_max_minus_one() {
        assert_eq!(
            require_matching_period_key(u64::MAX, u64::MAX - 1),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
    }

    /// Wildly different keys: representative months one year apart.
    #[test]
    fn rejects_period_keys_one_year_apart() {
        assert_eq!(
            require_matching_period_key(202301, 202401),
            Err(PeriodKeyError::MismatchedPeriodKey),
        );
    }

    // ── require_matching_period_key: error discriminant stability ─────────

    /// `MismatchedPeriodKey` is pinned at discriminant 1 for ABI stability.
    /// Changing the discriminant would break any contract that maps this
    /// error over XDR to a caller.
    #[test]
    fn mismatched_period_key_discriminant_is_stable_at_1() {
        let err = PeriodKeyError::MismatchedPeriodKey;
        assert_eq!(err as u32, 1);
    }

    // ── require_matching_period_key: proptest ─────────────────────────────

    // Proptest requires std (allocator + thread-local RNG). The outer crate
    // is `#![no_std]` for WASM builds, but `#[cfg(test)]` compilation always
    // links std (via `[dev-dependencies]`), so the `extern crate std` below
    // is needed to bring std into scope for proptest's macros.
    #[cfg(test)]
    mod proptest_period_key {
        extern crate std;
        use super::{require_matching_period_key, PeriodKeyError};
        use proptest::prelude::*;

        proptest! {
            /// For any arbitrary `u64` period key, matching both sides always
            /// succeeds.  This property must hold for all 2^64 possible keys.
            #[test]
            fn matching_keys_always_accept(pk in any::<u64>()) {
                prop_assert_eq!(require_matching_period_key(pk, pk), Ok(()));
            }

            /// For any two distinct `u64` period keys, the comparison always
            /// rejects. Distinct is guaranteed by filtering equal pairs.
            #[test]
            fn distinct_keys_always_reject(
                a in any::<u64>(),
                b in any::<u64>(),
            ) {
                prop_assume!(a != b);
                prop_assert_eq!(
                    require_matching_period_key(a, b),
                    Err(PeriodKeyError::MismatchedPeriodKey),
                );
            }

            /// Commutativity: (a, b) and (b, a) always produce the same
            /// result. There is no ordered/directed semantics — only equality.
            #[test]
            fn comparison_is_commutative(a in any::<u64>(), b in any::<u64>()) {
                let fwd = require_matching_period_key(a, b);
                let rev = require_matching_period_key(b, a);
                prop_assert_eq!(fwd, rev);
            }
        }
    }

    // ─── verify_period_active: success paths ─────────────────────────────────

    const PK_START: u64 = 1_704_067_200; // 2024-01-01T00:00:00Z

    /// Period that started well before `now` and is not archived is active.
    #[test]
    fn accepts_past_period_that_is_not_archived() {
        assert_eq!(
            verify_period_active(PK_START, PK_START + 86_400, false),
            Ok(()),
        );
    }

    /// Period that opens exactly at `now` is active (strict `>` test,
    /// inclusive boundary).
    #[test]
    fn accepts_period_at_its_start_boundary() {
        assert_eq!(
            verify_period_active(PK_START, PK_START, false),
            Ok(()),
        );
    }

    /// Even far into the next year, a non-archived period whose start has
    /// already passed remains active.
    #[test]
    fn accepts_old_period_long_after_its_start() {
        assert_eq!(
            verify_period_active(PK_START, PK_START + 365 * 86_400, false),
            Ok(()),
        );
    }

    // ─── verify_period_active: future period (negative test) ─────────────────

    /// A period whose start is strictly later than `now` is rejected.
    /// This is the headline #1234 negative test — without the
    /// `verify_period_active` guard, this write would have been accepted
    /// and pre-load the future bucket.
    #[test]
    fn rejects_future_period() {
        let result = verify_period_active(PK_START, PK_START - 1, false);
        assert_eq!(result, Err(PeriodKeyError::PeriodNotActive));
    }

    /// A period one full day in the future is also rejected (regression
    /// pin against off-by-one errors in `period_start > now`).
    #[test]
    fn rejects_period_one_day_in_future() {
        let now = PK_START;
        let result = verify_period_active(PK_START + 86_400, now, false);
        assert_eq!(result, Err(PeriodKeyError::PeriodNotActive));
    }

    /// Period one second in the future is still rejected. Pins the
    /// `period_start > now` strict-inequality contract at the smallest
    /// feasible boundary.
    #[test]
    fn rejects_period_one_second_in_future() {
        let now = PK_START;
        let result = verify_period_active(now + 1, now, false);
        assert_eq!(result, Err(PeriodKeyError::PeriodNotActive));
    }

    // ─── verify_period_active: archived period (negative test) ───────────────

    /// A period that the caller has already archived is rejected even
    /// though it is still within its open window. This is the second
    /// negative test for #1234: without `verify_period_active`, the
    /// resurrected write would silently mutate the archive map.
    #[test]
    fn rejects_archived_period_in_its_window() {
        assert_eq!(
            verify_period_active(PK_START, PK_START + 86_400, true),
            Err(PeriodKeyError::PeriodNotActive),
        );
    }

    /// A period that is both flagged as archived AND still in the future
    /// (relative to its caller-supplied `now`) is rejected. Both
    /// independent failure modes reach the same verdict, so the test
    /// pins both at once.
    #[test]
    fn rejects_archived_future_period() {
        let result = verify_period_active(PK_START + 86_400, PK_START, true);
        assert_eq!(result, Err(PeriodKeyError::PeriodNotActive));
    }

    // ─── verify_period_active: u64 boundary ─────────────────────────────────

    /// A period that opened one second before `u64::MAX` and is still
    /// inside its window is accepted. Pins the positive-side upper
    /// boundary (no overflow, no off-by-one rejection).
    #[test]
    fn accepts_period_within_u64_max_window() {
        let now = u64::MAX;
        // period_start = u64::MAX - 1, now = u64::MAX ⇒ period opened 1
        // second ago at the very top of the range, still in window.
        assert_eq!(
            verify_period_active(u64::MAX - 1, now, false),
            Ok(()),
        );
    }

    /// Boundary: when `period_start` is one second past `now`, the period
    /// is still classified as future and must be rejected even at the
    /// `u64::MAX` upper boundary. Strict `>` test ⇒ future ⇒ reject.
    #[test]
    fn rejects_one_second_before_u64_max_period() {
        let now = u64::MAX - 1;
        // period_start = u64::MAX is the very last timestamp; now is one
        // second before. Strict `> now` means the period is still future.
        assert_eq!(
            verify_period_active(u64::MAX, now, false),
            Err(PeriodKeyError::PeriodNotActive),
        );
    }
}
