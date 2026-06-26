// Issue #887 — safe-math wrapper tests for `reporting`.
//
// # Coverage
//
// `safe_percent(numerator, denominator, scale) -> i128`
// -------------------------------------------------------
// This is the only custom arithmetic helper defined in the `reporting` crate.
// It computes `(numerator * scale) / denominator` using `checked_mul` so that
// very large numerators never panic — they saturate to ±scale instead.
//
// The tests below cover:
//   • Happy path — normal in-range inputs produce the exact result.
//   • Zero denominator — must return 0 (safe default, no divide-by-zero).
//   • Negative denominator — must return 0 (same safe guard).
//   • Overflow saturation — numerator × scale overflows i128; positive result
//     saturates to +scale, negative to -scale.
//   • Scale = 0 — result is always 0 regardless of other values.
//   • Exact boundary — numerator = i128::MAX / scale produces exact result.
//
// All tests are deterministic (no Date::now() / random inputs).
// No std:: calls — all assertions use core primitives.

#![cfg(test)]

extern crate std;

use crate::safe_percent;

// ─── happy path ─────────────────────────────────────────────────────────────

/// 50 out of 100 at scale 100 → 50.
#[test]
fn safe_percent_returns_correct_result_for_normal_inputs() {
    assert_eq!(safe_percent(50, 100, 100), 50);
}

/// Fraction that rounds down (33/100 * 100 = 33, remainder discarded).
#[test]
fn safe_percent_truncates_towards_zero_for_non_divisible_inputs() {
    assert_eq!(safe_percent(1, 3, 100), 33);
}

/// 100% case: numerator == denominator at scale 100 → 100.
#[test]
fn safe_percent_returns_scale_when_numerator_equals_denominator() {
    assert_eq!(safe_percent(7, 7, 100), 100);
}

/// Negative numerator: -50 / 100 * 100 → -50.
#[test]
fn safe_percent_handles_negative_numerator_correctly() {
    assert_eq!(safe_percent(-50, 100, 100), -50);
}

/// Scale other than 100 (basis-points style: scale = 10_000).
#[test]
fn safe_percent_works_with_custom_scale() {
    // 25 / 100 * 10_000 = 2_500
    assert_eq!(safe_percent(25, 100, 10_000), 2_500);
}

/// numerator = 0 → result always 0.
#[test]
fn safe_percent_returns_zero_when_numerator_is_zero() {
    assert_eq!(safe_percent(0, 100, 100), 0);
}

// ─── zero / negative denominator (safe default) ─────────────────────────────

/// denominator = 0 must return 0 (no divide-by-zero panic).
#[test]
fn safe_percent_returns_zero_on_zero_denominator() {
    assert_eq!(safe_percent(50, 0, 100), 0);
}

/// denominator = -1 must return 0 (guards ≤ 0 branch).
#[test]
fn safe_percent_returns_zero_on_negative_denominator() {
    assert_eq!(safe_percent(50, -1, 100), 0);
}

/// i128::MIN denominator still returns 0, not a panic.
#[test]
fn safe_percent_returns_zero_on_i128_min_denominator() {
    assert_eq!(safe_percent(1, i128::MIN, 100), 0);
}

// ─── scale = 0 ───────────────────────────────────────────────────────────────

/// scale = 0 → result is always 0 (0 / anything = 0).
#[test]
fn safe_percent_returns_zero_when_scale_is_zero() {
    assert_eq!(safe_percent(1, 1, 0), 0);
}

// ─── overflow saturation (the key no-panic guarantee) ───────────────────────

/// When `numerator * scale` overflows i128 and numerator > 0, `safe_percent`
/// must return `+scale` instead of panicking or wrapping.
#[test]
fn safe_percent_returns_positive_scale_on_overflow_with_positive_numerator() {
    // i128::MAX * 100 definitely overflows i128.
    let result = safe_percent(i128::MAX, 1, 100);
    // Contract: saturate to +scale (not a panic, not a wrap).
    assert_eq!(result, 100, "overflow with positive numerator must saturate to +scale");
}

/// When `numerator * scale` overflows and numerator < 0, result must be `-scale`.
#[test]
fn safe_percent_returns_negative_scale_on_overflow_with_negative_numerator() {
    // i128::MIN * 100 overflows (most-negative × positive).
    let result = safe_percent(i128::MIN, 1, 100);
    assert_eq!(result, -100, "overflow with negative numerator must saturate to -scale");
}

/// Large but non-overflowing numerator: (i128::MAX / 100) * 100 fits in i128.
#[test]
fn safe_percent_handles_largest_non_overflowing_numerator_exactly() {
    let n = i128::MAX / 100;
    // n * 100 = i128::MAX rounded down, denominator = i128::MAX / 100
    // → result is 100 (== scale), because n / n * 100 / 100 = 100.
    let result = safe_percent(n, n, 100);
    assert_eq!(result, 100);
}

/// One step past the safe range triggers the overflow path.
/// (i128::MAX / 100 + 1) * 100 overflows → saturates to scale.
#[test]
fn safe_percent_saturates_one_step_past_non_overflow_boundary() {
    let n = i128::MAX / 100 + 1;
    let result = safe_percent(n, 1, 100);
    assert_eq!(result, 100, "one step past safe range must saturate to scale");
}
