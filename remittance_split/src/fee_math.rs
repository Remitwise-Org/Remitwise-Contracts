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
    if total_amount <= 0 {
        return None;
    }
    let denominator: i128 = if spending_pct <= 100 && savings_pct <= 100 && bills_pct <= 100 {
        100
    } else {
        10_000
    };
    let spending = mul_div(total_amount, spending_pct as i128, denominator)?;
    let savings = mul_div(total_amount, savings_pct as i128, denominator)?;
    let bills = mul_div(total_amount, bills_pct as i128, denominator)?;
    let insurance = total_amount
        .checked_sub(spending)?
        .checked_sub(savings)?
        .checked_sub(bills)?;

    Some((spending, savings, bills, insurance))
}

pub(crate) fn mul_div(amount: i128, pct: i128, denom: i128) -> Option<i128> {
    let q = amount.checked_div(denom)?;
    let r = amount.checked_rem(denom)?;
    let q_part = q.checked_mul(pct)?;
    let r_part = r.checked_mul(pct)?.checked_div(denom)?;
    q_part.checked_add(r_part)
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
    fn calculates_large_amount_without_intermediate_overflow() {
        assert_eq!(split_amounts(i128::MAX, 10_000, 0, 0), Some((i128::MAX, 0, 0, 0)));
    }
}
