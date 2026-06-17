//! Boundary tests for the `create_policy` coverage-to-premium ratio guard.
//!
//! The on-create guard in `insurance::create_policy` enforces:
//!
//! ```text
//! coverage_amount <= monthly_premium * 12 * 500
//! ```
//!
//! (i.e. `coverage_amount <= monthly_premium * 6000`). The multiplication is
//! `checked` and saturates to `i128::MAX` on overflow, so the comparison never
//! panics from arithmetic itself.
//!
//! The guard runs *after* the per-`CoverageType` premium/coverage min-max
//! bounds, so a value can satisfy one constraint while violating the other.
//! These tests pin both the exact boundary and that interaction.
//!
//! NOTE (documented behavior, not asserted as desired): the contract currently
//! rejects an over-ratio combination by `panic!`-ing with a string, *not* by
//! returning the typed `InsuranceError::UnsupportedCombination`. The typed
//! error variant exists but is unused on this path. The rejection tests
//! therefore assert that the call errors (`try_*` returns `Err`) rather than
//! matching a specific error code.

use insurance::{Insurance, InsuranceClient};
use remitwise_common::CoverageType;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

/// `monthly_premium * 12 * 500` collapses to `monthly_premium * RATIO`.
const RATIO: i128 = 12 * 500;

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let admin = Address::generate(&env);
    InsuranceClient::new(&env, &contract_id).init(&admin);
    let caller = Address::generate(&env);
    (env, contract_id, caller)
}

/// Attempt to create a policy on a fresh contract; returns `true` if accepted.
fn try_create(ct: CoverageType, premium: i128, coverage: i128) -> bool {
    let (env, contract_id, caller) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let name = String::from_str(&env, "P");
    client
        .try_create_policy(&caller, &name, &ct, &premium, &coverage)
        .is_ok()
}

#[test]
fn ratio_boundary_exact_is_accepted() {
    let (env, contract_id, caller) = setup();
    let client = InsuranceClient::new(&env, &contract_id);

    let premium = 1_000_000i128;
    let coverage = premium * RATIO; // 6_000_000_000, well within Health max_coverage

    let id = client.create_policy(
        &caller,
        &String::from_str(&env, "P"),
        &CoverageType::Health,
        &premium,
        &coverage,
    );
    assert_eq!(id, 1);

    let p = client.get_policy(&id).expect("policy should exist");
    assert_eq!(p.coverage_amount, coverage);
    assert_eq!(p.monthly_premium, premium);
}

#[test]
fn ratio_boundary_plus_one_is_rejected() {
    let (env, contract_id, caller) = setup();
    let client = InsuranceClient::new(&env, &contract_id);

    let premium = 1_000_000i128;
    let coverage = premium * RATIO + 1; // exactly one stroop over the cap

    let res = client.try_create_policy(
        &caller,
        &String::from_str(&env, "P"),
        &CoverageType::Health,
        &premium,
        &coverage,
    );
    assert!(
        res.is_err(),
        "coverage one stroop above premium*12*500 must be rejected"
    );
}

/// The exact ratio boundary must hold for every coverage type, with `+1`
/// rejected. `premium = 10_000_000` keeps `premium * RATIO = 6e10` comfortably
/// inside every type's `max_coverage`, so the ratio guard is the binding limit.
#[test]
fn ratio_boundary_exact_vs_plus_one_all_types() {
    let premium = 10_000_000i128;
    let boundary = premium * RATIO;

    for ct in [
        CoverageType::Health,
        CoverageType::Life,
        CoverageType::Property,
        CoverageType::Auto,
        CoverageType::Liability,
    ] {
        assert!(
            try_create(ct, premium, boundary),
            "exact ratio boundary must be accepted for this coverage type"
        );
        assert!(
            !try_create(ct, premium, boundary + 1),
            "ratio boundary + 1 must be rejected for this coverage type"
        );
    }
}

/// A combination can pass the ratio guard yet fail the per-type coverage bound.
///
/// Liability: `max_premium = 4e11`, `max_coverage = 5e13`. With `premium = 4e11`
/// the ratio cap is `premium * 6000 = 2.4e15`, so `coverage = 6e13` is *under*
/// the ratio cap but *over* the type's `max_coverage`. It must be rejected.
#[test]
fn ratio_passes_but_type_coverage_bound_fails() {
    let premium = 400_000_000_000i128; // Liability max_premium
    let coverage = 60_000_000_000_000i128; // > max_coverage (5e13), < premium*6000 (2.4e15)

    assert!(
        coverage < premium * RATIO,
        "precondition: coverage is within the ratio cap"
    );
    assert!(
        !try_create(CoverageType::Liability, premium, coverage),
        "coverage over the type max_coverage must be rejected even though the ratio passes"
    );
}

/// The mirror case: a combination can satisfy the per-type bounds yet fail the
/// ratio guard.
///
/// Health: `min_premium = 1`, coverage range `1..=1e14`. With `premium = 1` the
/// ratio cap is `6000`, so `coverage = 1_000_000` is a valid type-coverage value
/// but far exceeds the ratio cap. It must be rejected.
#[test]
fn type_bounds_pass_but_ratio_fails() {
    let premium = 1i128; // Health min_premium, valid for the type
    let coverage = 1_000_000i128; // within Health coverage bounds, but >> premium*6000 (6000)

    assert!(
        coverage > premium * RATIO,
        "precondition: coverage exceeds the ratio cap"
    );
    assert!(
        !try_create(CoverageType::Health, premium, coverage),
        "coverage over the ratio cap must be rejected even though type bounds pass"
    );
}
