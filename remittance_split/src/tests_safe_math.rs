// Issue #887 — safe-math wrapper tests for `remittance_split`.
//
// # Coverage
//
// `calculate_split_amounts` (private) / `calculate_split` (public entry-point)
// ─────────────────────────────────────────────────────────────────────────────
// Inside `calculate_split_amounts` every allocation step calls:
//   `total_amount.checked_mul(pct).and_then(|n| n.checked_div(100))`
//
// A `None` from any of those steps maps to `RemittanceSplitError::Overflow`,
// NOT a panic.  The safe range is `1..=i128::MAX/100`.
//
// Tests verify:
//   • Happy path — typical and boundary-safe inputs return Ok splits.
//   • checked_mul overflow — total_amount > i128::MAX/100 returns Overflow error.
//   • Zero / negative amount — returns InvalidAmount error (pre-checked).
//   • checked_sub underflow — impossible via public API because remainder is
//     always computed as `total - sum(others)` over non-negative allocations,
//     but the path is exercised implicitly by the overflow tests.
//   • Determinism — same inputs always produce the same split (no side-effects).
//
// All tests are deterministic.  No std:: in contract paths; `extern crate std`
// only inside the test module for format macros in error messages.

#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as AddressTrait, Address, Env};

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Maximum safe total for any percentage ≤ 100:
/// `total * 100 ≤ i128::MAX`  ⟺  `total ≤ i128::MAX / 100`.
const MAX_SAFE_TOTAL: i128 = i128::MAX / 100;

fn new_client(env: &Env) -> (RemittanceSplitClient<'_>, Address) {
    env.mock_all_auths();
    let cid = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &cid);
    let owner = Address::generate(env);
    (client, owner)
}

fn init(
    client: &RemittanceSplitClient<'_>,
    env: &Env,
    owner: &Address,
    sp: u32,
    sg: u32,
    sb: u32,
    si: u32,
) {
    let token = Address::generate(env);
    client.initialize_split(owner, &0, &token, &sp, &sg, &sb, &si);
}

// ─── happy path ──────────────────────────────────────────────────────────────

/// Standard split (50/25/15/10) on a typical amount returns the expected buckets.
#[test]
fn checked_mul_returns_correct_split_for_typical_amount() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 50, 25, 15, 10);

    let amounts = client.calculate_split(&1_000i128);

    assert_eq!(amounts.get(0).unwrap(), 500i128); // spending 50%
    assert_eq!(amounts.get(1).unwrap(), 250i128); // savings 25%
    assert_eq!(amounts.get(2).unwrap(), 150i128); // bills 15%
    assert_eq!(amounts.get(3).unwrap(), 100i128); // insurance 10%
}

/// The minimum valid amount (1 unit) must not panic and must conserve totals.
#[test]
fn checked_mul_returns_no_error_for_minimum_valid_amount() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 25, 25, 25, 25);

    let amounts = client.calculate_split(&1i128);
    let sum: i128 = amounts.iter().sum();
    assert_eq!(sum, 1i128, "total conservation must hold for unit amount");
}

/// The minimum valid amount must allocate the entire transfer to insurance when
/// all floor-based buckets round down to zero.
#[test]
fn checked_mul_preserves_total_at_minimum_valid_amount_boundary() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 33, 33, 33, 1);

    let amounts = client.calculate_split(&1i128);
    let sum: i128 = amounts.iter().sum();

    assert_eq!(sum, 1i128, "total conservation must hold at the minimum transfer boundary");
    assert_eq!(amounts.get(0).unwrap(), 0i128, "spending should round down to zero");
    assert_eq!(amounts.get(1).unwrap(), 0i128, "savings should round down to zero");
    assert_eq!(amounts.get(2).unwrap(), 0i128, "bills should round down to zero");
    assert_eq!(amounts.get(3).unwrap(), 1i128, "insurance must receive the lone remainder");
}

/// One unit above the minimum valid amount must still conserve the total without
/// losing or creating dust.
#[test]
fn checked_mul_preserves_total_one_above_minimum_valid_amount_boundary() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 33, 33, 33, 1);

    let amounts = client.calculate_split(&2i128);
    let sum: i128 = amounts.iter().sum();

    assert_eq!(sum, 2i128, "total conservation must hold one unit above the minimum transfer boundary");
    assert_eq!(amounts.get(0).unwrap(), 0i128, "spending should round down to zero");
    assert_eq!(amounts.get(1).unwrap(), 0i128, "savings should round down to zero");
    assert_eq!(amounts.get(2).unwrap(), 0i128, "bills should round down to zero");
    assert_eq!(amounts.get(3).unwrap(), 2i128, "insurance must receive the full remainder");
}

/// The largest safe amount (i128::MAX / 100) must succeed and conserve total.
#[test]
fn checked_mul_succeeds_at_maximum_safe_total_boundary() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 30, 30, 20, 20);

    let amounts = client.calculate_split(&MAX_SAFE_TOTAL);
    let sum: i128 = amounts.iter().sum();
    assert_eq!(
        sum, MAX_SAFE_TOTAL,
        "total conservation must hold at i128::MAX/100"
    );
}

/// All-in-one-category config (100/0/0/0) still conserves the total.
#[test]
fn checked_mul_conserves_total_for_single_category_config() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 100, 0, 0, 0);

    let total = 999_999i128;
    let amounts = client.calculate_split(&total);
    let sum: i128 = amounts.iter().sum();
    assert_eq!(sum, total);
    assert_eq!(amounts.get(0).unwrap(), total); // 100% to spending
    assert_eq!(amounts.get(1).unwrap(), 0i128);
    assert_eq!(amounts.get(2).unwrap(), 0i128);
    assert_eq!(amounts.get(3).unwrap(), 0i128);
}

// ─── checked_mul overflow → RemittanceSplitError::Overflow (not a panic) ────

/// total_amount one step above i128::MAX/100 causes checked_mul to return None
/// which must map to RemittanceSplitError::Overflow, not a panic.
#[test]
fn checked_mul_returns_error_on_overflow() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    // Any non-zero pct ≥ 1 will overflow when total > i128::MAX/pct.
    // 50% means the binding multiplier is 50; i128::MAX/50 + 1 overflows.
    init(&client, &env, &owner, 50, 25, 15, 10);

    let overflow_total = i128::MAX / 50 + 1;
    let result = client.try_calculate_split(&overflow_total);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Overflow)),
        "checked_mul overflow must produce RemittanceSplitError::Overflow, not a panic"
    );
}

/// i128::MAX itself (much larger than any safe range) returns Overflow.
#[test]
fn checked_mul_returns_error_on_i128_max_input() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 25, 25, 25, 25);

    let result = client.try_calculate_split(&i128::MAX);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Overflow)),
        "i128::MAX must return Overflow error, not a panic"
    );
}

// ─── zero / negative amount → RemittanceSplitError::InvalidAmount ────────────

/// zero total_amount is rejected before any arithmetic is attempted.
#[test]
fn checked_mul_returns_error_on_zero_amount() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 25, 25, 25, 25);

    let result = client.try_calculate_split(&0i128);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::InvalidAmount)),
        "zero amount must return InvalidAmount, not Overflow"
    );
}

/// negative total_amount is rejected before any arithmetic is attempted.
#[test]
fn checked_mul_returns_error_on_negative_amount() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 25, 25, 25, 25);

    let result = client.try_calculate_split(&-1i128);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::InvalidAmount)),
        "negative amount must return InvalidAmount, not Overflow"
    );
}

/// i128::MIN as total_amount is rejected before any arithmetic is attempted.
#[test]
fn checked_mul_returns_error_on_i128_min_amount() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 25, 25, 25, 25);

    let result = client.try_calculate_split(&i128::MIN);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::InvalidAmount)),
        "i128::MIN must return InvalidAmount, not a panic"
    );
}

// ─── determinism ─────────────────────────────────────────────────────────────

/// Calling calculate_split twice with the same inputs must yield identical results.
#[test]
fn checked_mul_is_deterministic_for_identical_inputs() {
    let env = Env::default();
    let (client, owner) = new_client(&env);
    init(&client, &env, &owner, 40, 30, 20, 10);

    let total = 100_000i128;
    let first = client.calculate_split(&total);
    let second = client.calculate_split(&total);

    assert_eq!(first, second, "calculate_split must be deterministic");
}
