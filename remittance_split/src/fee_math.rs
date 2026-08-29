//! Pure fee/allocation arithmetic for `calculate_split`, extracted into its
//! own crate-private module so the percentage math can be reasoned about
//! (and unit-tested) independently of storage reads and event emission.

/// Splits `total_amount` into (spending, savings, bills, insurance) using
/// the given percentages. `spending_pct`, `savings_pct`, and `bills_pct`
/// each take a truncating-integer-division share of `total_amount`;
/// insurance takes whatever remains, so the four amounts always sum back
/// to exactly `total_amount` with no stroop lost to truncation.
///
/// Callers are responsible for validating `total_amount > 0` and that the
/// percentages sum to 10_000 basis points -- this function assumes both already hold.
pub(crate) fn split_amounts(
    total_amount: i128,
    spending_pct: u32,
    savings_pct: u32,
    bills_pct: u32,
) -> Option<(i128, i128, i128, i128)> {
    const BPS_DENOMINATOR: i128 = 10_000;
    let spending = total_amount
        .checked_mul(spending_pct as i128)?
        .checked_div(BPS_DENOMINATOR)?;
    let savings = total_amount
        .checked_mul(savings_pct as i128)?
        .checked_div(BPS_DENOMINATOR)?;
    let bills = total_amount
        .checked_mul(bills_pct as i128)?
        .checked_div(BPS_DENOMINATOR)?;
    let insurance = total_amount
        .checked_sub(spending)?
        .checked_sub(savings)?
        .checked_sub(bills)?;

    Some((spending, savings, bills, insurance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_evenly_divisible_amounts_exactly() {
        assert_eq!(
            split_amounts(1000, 5000, 3000, 1500),
            Some((500, 300, 150, 50))
        );
    }

    #[test]
    fn gives_the_truncation_remainder_to_insurance() {
        // 100 * 33 / 100 = 33 (truncated from 33.0); three shares of 33
        // leave 1 unaccounted for, which must land on insurance rather
        // than being lost.
        let (spending, savings, bills, insurance) = split_amounts(100, 3300, 3300, 3300).unwrap();
        assert_eq!(spending + savings + bills + insurance, 100);
        assert_eq!(insurance, 1);
    }

    #[test]
    fn amounts_always_sum_back_to_the_total() {
        for total in [1i128, 7, 999, 1_000_000, 123_456_789] {
            let (spending, savings, bills, insurance) =
                split_amounts(total, 5000, 3000, 1500).unwrap();
            assert_eq!(spending + savings + bills + insurance, total);
        }
    }

    #[test]
    fn rejects_intermediate_multiplication_overflow() {
        assert_eq!(split_amounts(i128::MAX, 10_000, 0, 0), None);
    }
}
