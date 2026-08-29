//! Exact-integer amount rules shared across contracts.
//!
//! All monetary amounts in this workspace are **exact integers** expressed in
//! the asset's smallest unit ("stroops": `1 XLM = 10_000_000` stroops, see
//! [`crate::tokens`]). There is no floating-point or fixed-point fractional
//! representation anywhere in storage or scheduling math: an amount is
//! **rejected** rather than rounded, truncated, or silently clamped. This
//! module centralises the rules that every amount boundary must enforce so
//! the behaviour is identical and reviewable across contracts.
//!
//! The two rules enforced by [`validate_amount`]:
//!
//! 1. **Sign** — amounts must be strictly positive (`>= MIN_AMOUNT`).
//! 2. **Scale / magnitude** — amounts must not exceed [`MAX_AMOUNT`].
//!
//! Callers must run this check **before any state mutation** at every
//! boundary (bill/schedule creation, modification, schedule execution,
//! payment), so a rejected value can never leave partial state behind.

/// Upper bound (inclusive) for any single stored amount, in smallest units.
///
/// `10^30` — one sextillion stroops (≈ `10^23` XLM) — is a round power of ten
/// chosen so that every aggregation over validated amounts is provably free of
/// `i128` overflow:
///
/// | Aggregation | Worst case | Headroom vs `i128::MAX` (~1.7 × 10³⁸) |
/// |---|---|---|
/// | Per-owner unpaid total | `MAX_BILLS_PER_OWNER (1000) × 10³⁰ = 10³³` | ~1.7 × 10⁵ × |
/// | Single batch delta | `MAX_BATCH_SIZE (50) × 10³⁰ = 5 × 10³¹` | ~3.4 × 10⁶ × |
///
/// A value above this bound is rejected at the boundary by
/// [`validate_amount`] instead of being allowed to threaten downstream
/// aggregation arithmetic.
pub const MAX_AMOUNT: i128 = 1_000_000_000_000_000_000_000_000_000_000;

/// Smallest accepted amount (inclusive): one unit of the asset's smallest
/// denomination. Amounts are exact integers, so `1` is the minimum
/// representable positive value; `0` and negatives are rejected as
/// non-positive.
pub const MIN_AMOUNT: i128 = 1;

/// Typed error distinguishing *why* an amount was rejected, so callers can
/// map it to their contract-specific `#[contracterror]` without losing the
/// category.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AmountValidationError {
    /// `amount < MIN_AMOUNT` — zero or negative.
    NonPositive,
    /// `amount > MAX_AMOUNT`.
    ExceedsMaximum,
}

/// Validates an amount against the shared exact-integer rules.
///
/// # Rules
/// 1. **Sign** — `amount >= MIN_AMOUNT`: zero and negative amounts are
///    rejected with [`AmountValidationError::NonPositive`].
/// 2. **Scale / magnitude** — `amount <= MAX_AMOUNT`: values above the bound
///    are rejected with [`AmountValidationError::ExceedsMaximum`] rather than
///    accepted and allowed to threaten downstream aggregation overflow.
///
/// # Errors
/// Returns [`AmountValidationError::NonPositive`] for `amount <= 0` and
/// [`AmountValidationError::ExceedsMaximum`] for `amount > MAX_AMOUNT`.
pub fn validate_amount(amount: i128) -> Result<(), AmountValidationError> {
    if amount < MIN_AMOUNT {
        Err(AmountValidationError::NonPositive)
    } else if amount > MAX_AMOUNT {
        Err(AmountValidationError::ExceedsMaximum)
    } else {
        Ok(())
    }
}

/// Error returned when amount arithmetic (addition or subtraction) would
/// overflow `i128`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct AmountOverflowError;

/// Checked addition of two amounts.
///
/// Returns [`AmountOverflowError`] on overflow instead of wrapping or
/// saturating, so callers can reject the operation **before committing any
/// state**. Silent saturation is deliberately avoided: it truncates balances
/// in a way that can survive review.
pub fn checked_add_amount(a: i128, b: i128) -> Result<i128, AmountOverflowError> {
    a.checked_add(b).ok_or(AmountOverflowError)
}

/// Checked subtraction of `b` from `a`.
///
/// Returns [`AmountOverflowError`] on underflow instead of wrapping or
/// saturating. Used when reducing an owner's unpaid total by a bill amount.
pub fn checked_sub_amount(a: i128, b: i128) -> Result<i128, AmountOverflowError> {
    a.checked_sub(b).ok_or(AmountOverflowError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Property test: `validate_amount` accepts exactly the range
    /// `[MIN_AMOUNT, MAX_AMOUNT]` and rejects everything outside it, for a
    /// wide sweep of sign/magnitude combinations including near-overflow
    /// values. This pins the boundary rule against independent range checks.
    proptest! {
        #[test]
        fn validate_amount_matches_range_invariant(
            amount in -1_000_000i128..=2_000_000_000_000_000_000_000_000_000_000i128,
        ) {
            let expected = amount >= MIN_AMOUNT && amount <= MAX_AMOUNT;
            assert_eq!(
                validate_amount(amount).is_ok(),
                expected,
                "validate_amount({amount}) must match the range invariant"
            );
        }
    }

    #[test]
    fn rejects_zero_and_negative() {
        assert_eq!(validate_amount(0), Err(AmountValidationError::NonPositive));
        assert_eq!(
            validate_amount(-1),
            Err(AmountValidationError::NonPositive)
        );
        assert_eq!(
            validate_amount(i128::MIN),
            Err(AmountValidationError::NonPositive)
        );
    }

    #[test]
    fn accepts_minimum_positive_amount() {
        assert_eq!(validate_amount(MIN_AMOUNT), Ok(()));
        assert_eq!(validate_amount(1), Ok(()));
    }

    #[test]
    fn accepts_maximum_amount() {
        assert_eq!(validate_amount(MAX_AMOUNT), Ok(()));
    }

    #[test]
    fn rejects_amounts_above_maximum() {
        assert_eq!(
            validate_amount(MAX_AMOUNT + 1),
            Err(AmountValidationError::ExceedsMaximum)
        );
        assert_eq!(
            validate_amount(i128::MAX),
            Err(AmountValidationError::ExceedsMaximum)
        );
    }

    #[test]
    fn checked_add_is_exact() {
        assert_eq!(checked_add_amount(MAX_AMOUNT, MAX_AMOUNT), Ok(2 * MAX_AMOUNT));
        assert_eq!(
            checked_add_amount(i128::MAX, 1),
            Err(AmountOverflowError)
        );
    }

    #[test]
    fn checked_sub_is_exact() {
        assert_eq!(
            checked_sub_amount(2 * MAX_AMOUNT, MAX_AMOUNT),
            Ok(MAX_AMOUNT)
        );
        assert_eq!(checked_sub_amount(0, 1), Err(AmountOverflowError));
    }
}
