# Insurance Numeric Bounds

This document explains the numeric bounds enforced by the `insurance` contract to avoid invalid policies and to guarantee overflow-safe aggregation of monthly premiums.

Summary
- Per-policy values must be strictly positive and bounded by compile-time constants.
- Bounds are chosen so that summing up to `MAX_POLICIES_PER_OWNER` policies cannot overflow an i128 accumulator used by `get_total_monthly_premium`.

Constants
- `MAX_POLICIES_PER_OWNER` — existing contract constant controlling the maximum number of active policies an owner may hold.
- `MAX_MONTHLY_PREMIUM` — maximum allowed monthly premium for a single policy. Calculated as `i128::MAX / MAX_POLICIES_PER_OWNER` to ensure that summing `MAX_POLICIES_PER_OWNER` premiums cannot overflow an `i128` accumulator.
- `MAX_COVERAGE_AMOUNT` — maximum allowed coverage amount for a single policy. Chosen similarly to keep per-policy coverage in a reasonable, bounded range.

Rationale
- Smart contracts run in a restricted environment with fixed-size integer arithmetic. To avoid panics and unexpected failures when aggregating many policies, the contract enforces conservative per-policy upper bounds so that simple summation of up to the allowed number of policies cannot exceed the accumulator type's limit.
- Enforcing strictly positive values avoids nonsensical policies with zero or negative premium/coverage.

Contract behavior
- `create_policy(...)` validates `monthly_premium` and `coverage_amount` and returns typed errors on violation:
  - `InsuranceError::MonthlyPremiumTooLow` when `monthly_premium <= 0`.
  - `InsuranceError::CoverageAmountTooLow` when `coverage_amount <= 0`.
  - `InsuranceError::MonthlyPremiumTooHigh` when `monthly_premium > MAX_MONTHLY_PREMIUM`.
  - `InsuranceError::CoverageAmountTooHigh` when `coverage_amount > MAX_COVERAGE_AMOUNT`.
- When validations pass, the policy is persisted, owner indexes/stats are updated, and events are emitted.

Examples
- If `MAX_POLICIES_PER_OWNER = 50` and `i128::MAX` is the accumulator limit, then `MAX_MONTHLY_PREMIUM = i128::MAX / 50` ensures adding up to 50 policies' premiums will not overflow.

Testing
- Unit and contract tests were added to assert:
  - Non-positive premiums/coverage are rejected.
  - Values greater than the per-policy max are rejected.
  - `MAX_MONTHLY_PREMIUM` and `MAX_COVERAGE_AMOUNT` values are accepted.
  - Summing `MAX_POLICIES_PER_OWNER` policies with `MAX_MONTHLY_PREMIUM` premiums is overflow-safe and equals the expected total.

Notes for maintainers
- If `MAX_POLICIES_PER_OWNER` changes, recompute `MAX_MONTHLY_PREMIUM` accordingly to preserve overflow safety.
- Keep the `InsuranceError` variants documented and stable to avoid breaking clients.

References
- See `insurance/src/lib.rs` for the constants and the `create_policy` validation implementation.
- See `insurance/tests/caps_and_stats_tests.rs` for the corresponding tests.
