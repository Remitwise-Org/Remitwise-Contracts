//! Premium-fee arithmetic. A loyalty/volume discount must be applied to
//! the *full* fee before the result is capped -- not the other way
//! around. Capping first and then discounting the already-capped amount
//! shortchanges exactly the customers a discount is meant to reward: the
//! bigger their discount, the less of it survives being computed off a
//! number that was already clamped down.

pub(crate) const BPS_DENOMINATOR: i128 = 10_000;

/// Applies a `discount_bps` (basis points, e.g. `500` = 5%) discount to
/// `base_fee`, then caps the discounted result at `fee_cap`.
///
/// Order matters: discount first, then cap. Capping first would compute
/// the discount off an already-reduced number, silently giving the
/// customer less benefit than the configured discount promises whenever
/// the cap binds.
pub(crate) fn apply_discount_then_cap(base_fee: i128, discount_bps: u32, fee_cap: i128) -> i128 {
    let discount = (base_fee * discount_bps as i128) / BPS_DENOMINATOR;
    let discounted = base_fee - discount;
    discounted.min(fee_cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discount_is_computed_off_the_full_fee_not_the_capped_fee() {
        // 10% off a 1000 base fee is 900, which is under the 920 cap, so
        // the cap doesn't bind and the correct result is 900.
        //
        // The bug this guards against: applying the cap first would give
        // min(1000, 920) = 920, then discount 10% off *that* = 828 -- a
        // worse deal for the customer than the discount promises.
        let correct = apply_discount_then_cap(1000, 1_000, 920);
        let wrong_cap_first_result = 828;

        assert_eq!(correct, 900);
        assert_ne!(correct, wrong_cap_first_result);
    }

    #[test]
    fn cap_still_binds_when_the_discount_isnt_enough() {
        // 5% off 1000 is 950, which exceeds the 900 cap, so the cap
        // correctly takes over.
        assert_eq!(apply_discount_then_cap(1000, 500, 900), 900);
    }

    #[test]
    fn zero_discount_still_respects_the_cap() {
        assert_eq!(apply_discount_then_cap(1000, 0, 800), 800);
    }

    #[test]
    fn a_cap_higher_than_the_discounted_fee_never_binds() {
        assert_eq!(apply_discount_then_cap(1000, 500, 1_000_000), 950);
    }
}
