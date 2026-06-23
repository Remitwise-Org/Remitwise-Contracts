//! Tests for `get_insurance_report`'s `coverage_to_premium_ratio` computation
//! and its active-only aggregation.
//!
//! The reporting contract computes, from the active insurance policies:
//!
//! ```text
//! annual_premium             = monthly_premium * 12          (saturating)
//! coverage_to_premium_ratio  = (total_coverage * 100) / annual_premium
//! ```
//!
//! where the ratio uses checked math and yields `0` when the denominator
//! (`annual_premium`) is `<= 0` — so it never divides by zero, even when the
//! aggregated premium is `0`.
//!
//! These tests drive the reporting contract against a lightweight insurance
//! stub that implements the cross-contract interface reporting expects. The
//! interface now matches the real insurance contract exactly:
//! `get_active_policies -> PolicyPage { items: Vec<u32> (ids), .. }` plus
//! `get_policy(id) -> Option<InsurancePolicy>` and `get_total_monthly_premium`.
//!
//! The previously-latent cross-contract type mismatch is now FIXED: reporting
//! pages through policy IDs from `get_active_policies` and resolves each full
//! policy (with its `coverage_amount`) via `get_policy`, mirroring the real
//! `insurance` contract whose `get_active_policies` returns ids only.

use reporting::{
    CoverageType, InsurancePolicy, PolicyPage, ReportingContract, ReportingContractClient,
};
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, String, Vec,
};

// ─────────────────────────────────────────────────────────────────────────────
// Insurance stub implementing reporting's expected interface
// ─────────────────────────────────────────────────────────────────────────────

#[contract]
pub struct InsuranceStub;

#[contractimpl]
impl InsuranceStub {
    /// Seed the full policy set (active and inactive). The query methods below
    /// expose only the active subset, mirroring the real contract.
    pub fn seed(env: Env, policies: Vec<InsurancePolicy>) {
        env.storage()
            .instance()
            .set(&symbol_short!("POLICIES"), &policies);
    }

    pub fn get_active_policies(env: Env, _owner: Address, _cursor: u32, _limit: u32) -> PolicyPage {
        let all: Vec<InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Vec::new(&env));
        let mut items = Vec::new(&env);
        for p in all.iter() {
            if p.active {
                items.push_back(p.id);
            }
        }
        let count = items.len();
        PolicyPage {
            items,
            next_cursor: 0,
            count,
        }
    }

    pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy> {
        let all: Vec<InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Vec::new(&env));
        for p in all.iter() {
            if p.id == policy_id {
                return Some(p);
            }
        }
        None
    }

    pub fn get_total_monthly_premium(env: Env, _owner: Address) -> i128 {
        let all: Vec<InsurancePolicy> = env
            .storage()
            .instance()
            .get(&symbol_short!("POLICIES"))
            .unwrap_or_else(|| Vec::new(&env));
        let mut total = 0i128;
        for p in all.iter() {
            if p.active {
                total = total.saturating_add(p.monthly_premium);
            }
        }
        total
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn policy(
    env: &Env,
    id: u32,
    owner: &Address,
    monthly_premium: i128,
    coverage_amount: i128,
    active: bool,
) -> InsurancePolicy {
    InsurancePolicy {
        id,
        owner: owner.clone(),
        name: String::from_str(env, "P"),
        coverage_type: CoverageType::Health,
        monthly_premium,
        coverage_amount,
        external_ref: None,
        active,
        created_at: 0,
        last_payment_at: 0,
        next_payment_date: 0,
    }
}

/// Register reporting + a seeded insurance stub, wire them up, and return the
/// reporting contract id plus the user address to query.
fn setup(env: &Env, policies: Vec<InsurancePolicy>) -> (Address, Address) {
    let admin = Address::generate(env);
    let user = Address::generate(env);

    let stub_id = env.register_contract(None, InsuranceStub);
    InsuranceStubClient::new(env, &stub_id).seed(&policies);

    let reporting_id = env.register_contract(None, ReportingContract);
    let rc = ReportingContractClient::new(env, &reporting_id);
    rc.init(&admin);
    rc.configure_addresses(
        &admin,
        &Address::generate(env), // remittance_split (unused by this report)
        &Address::generate(env), // savings_goals    (unused)
        &Address::generate(env), // bill_payments    (unused)
        &stub_id,                // insurance
        &Address::generate(env), // family_wallet    (unused)
    );

    (reporting_id, user)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ratio_and_annual_premium_are_computed_with_checked_math() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);

    let mut policies = Vec::new(&env);
    policies.push_back(policy(&env, 1, &owner, 3_000_000, 360_000_000, true));
    policies.push_back(policy(&env, 2, &owner, 2_000_000, 240_000_000, true));
    // monthly = 5_000_000 ; annual = 60_000_000 ; total_coverage = 600_000_000
    // ratio = (600_000_000 * 100) / 60_000_000 = 1000

    let (reporting_id, user) = setup(&env, policies);
    let rc = ReportingContractClient::new(&env, &reporting_id);
    let caller = Address::generate(&env);

    let report = rc.get_insurance_report(&caller, &user, &0u64, &100u64);

    assert_eq!(report.active_policies, 2);
    assert_eq!(report.total_coverage, 600_000_000);
    assert_eq!(report.monthly_premium, 5_000_000);
    assert_eq!(report.annual_premium, 60_000_000);
    assert_eq!(
        report.annual_premium,
        report.monthly_premium * 12,
        "annual premium must be exactly monthly * 12"
    );
    assert_eq!(report.coverage_to_premium_ratio, 1000);
}

#[test]
fn zero_active_policies_yields_zero_ratio_without_dividing_by_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let (reporting_id, user) = setup(&env, Vec::new(&env));
    let rc = ReportingContractClient::new(&env, &reporting_id);
    let caller = Address::generate(&env);

    // Must not panic (no divide-by-zero) and must report zeroes.
    let report = rc.get_insurance_report(&caller, &user, &0u64, &100u64);

    assert_eq!(report.active_policies, 0);
    assert_eq!(report.total_coverage, 0);
    assert_eq!(report.monthly_premium, 0);
    assert_eq!(report.annual_premium, 0);
    assert_eq!(report.coverage_to_premium_ratio, 0);
}

/// Coverage present but aggregated premium is zero: the denominator is `0`, so
/// the checked ratio math must return `0` rather than dividing by zero.
#[test]
fn zero_premium_with_coverage_does_not_divide_by_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);

    let mut policies = Vec::new(&env);
    policies.push_back(policy(&env, 1, &owner, 0, 500_000_000, true));

    let (reporting_id, user) = setup(&env, policies);
    let rc = ReportingContractClient::new(&env, &reporting_id);
    let caller = Address::generate(&env);

    let report = rc.get_insurance_report(&caller, &user, &0u64, &100u64);

    assert_eq!(report.active_policies, 1);
    assert_eq!(report.total_coverage, 500_000_000);
    assert_eq!(report.annual_premium, 0);
    assert_eq!(
        report.coverage_to_premium_ratio, 0,
        "ratio must be 0 when annual premium is 0 (no divide-by-zero)"
    );
}

/// Deactivated policies must be excluded from every aggregate.
#[test]
fn aggregation_is_active_only() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);

    let mut policies = Vec::new(&env);
    policies.push_back(policy(&env, 1, &owner, 5_000_000, 500_000_000, true));
    policies.push_back(policy(&env, 2, &owner, 9_000_000, 900_000_000, false)); // inactive

    let (reporting_id, user) = setup(&env, policies);
    let rc = ReportingContractClient::new(&env, &reporting_id);
    let caller = Address::generate(&env);

    let report = rc.get_insurance_report(&caller, &user, &0u64, &100u64);

    assert_eq!(
        report.active_policies, 1,
        "inactive policy must not be counted"
    );
    assert_eq!(
        report.total_coverage, 500_000_000,
        "inactive coverage must be excluded"
    );
    assert_eq!(
        report.monthly_premium, 5_000_000,
        "inactive premium must be excluded"
    );
    assert_eq!(report.annual_premium, 60_000_000);
    // ratio = (500_000_000 * 100) / 60_000_000 = 833
    assert_eq!(report.coverage_to_premium_ratio, 833);
}
