# Insurance Per-CoverageType Premium & Coverage Constraints

`TypeConstraints::for_type` in `insurance/src/lib.rs` is the single source of truth for
per-type numeric limits enforced by `create_policy`.  All values are in **stroops**
(1 XLM = 10 000 000 stroops).

## Bounds table

| CoverageType | min_premium | max_premium        | min_coverage | max_coverage          |
|:-------------|------------:|-------------------:|-------------:|----------------------:|
| Health       |           1 |    500_000_000_000 |            1 |   100_000_000_000_000 |
| Life         |           1 |  1_000_000_000_000 |            1 |   500_000_000_000_000 |
| Property     |           1 |  2_000_000_000_000 |            1 | 1_000_000_000_000_000 |
| Auto         |           1 |    750_000_000_000 |            1 |   200_000_000_000_000 |
| Liability    |           1 |    400_000_000_000 |            1 |    50_000_000_000_000 |

## Error mapping

| Condition | Error returned |
|:----------|:---------------|
| `monthly_premium <= 0` or outside `[min_premium, max_premium]` | `InsuranceError::InvalidPremium` |
| `coverage_amount <= 0` or outside `[min_coverage, max_coverage]` | `InsuranceError::InvalidCoverageAmount` |
| `coverage_amount > monthly_premium * 12 * 500` | `InsuranceError::UnsupportedCombination` |

## UnsupportedCombination ratio

After both per-type range checks pass, `create_policy` also rejects policies where the
coverage amount is implausibly large relative to the premium:

```
max_ratio = monthly_premium * 12 * 500   // checked_mul, saturates to i128::MAX on overflow
if coverage_amount > max_ratio → UnsupportedCombination
```

This mirrors an actuarial 500× annual-premium cap and prevents economic abuse of the
micro-insurance product.

## Overflow safety

`checked_mul` is used for the ratio calculation.  If `monthly_premium` is close to
`i128::MAX`, the multiplication saturates to `i128::MAX` rather than wrapping, so the
comparison is always safe.  Any `coverage_amount` that would independently overflow `i128`
is already rejected by the per-type `max_coverage` guard before the ratio check is reached.

## Notes for maintainers

- Changing any bound here is a **breaking change** for integrators.  Bump `CONTRACT_VERSION`
  and update this table in the same commit.
- Tests in `insurance/src/test.rs` (`test_type_constraints_*`) read intent directly from
  `TypeConstraints::for_type` via a mirror struct — do not hard-code magic numbers in tests.
