#![cfg(test)]

//! Proptest fuzzing for remittance_split dust conservation.
//!
//! Invariant (dust conservation): for any valid percentage tuple summing to 100
//! and any positive `total_amount`, the allocations must satisfy:
//!
//! `spending + savings + bills + insurance == total_amount`
//!
//! Remainder/dust policy (deterministic): the remainder from integer division
//! is **always** assigned to `insurance` (index 3), regardless of the configured
//! `insurance_percent`. This ensures deterministic conservation across all
//! inputs.

use proptest::prelude::*;
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

/// Max safe total to avoid overflow in `total * percent` inside the contract.
const MAX_SAFE_TOTAL: i128 = i128::MAX / 100;

// CI-friendly default: keep the suite bounded.
const CASES: u32 = 800;

fn setup() -> (Env, RemittanceSplitClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &cid);
    let owner = Address::generate(&env);
    (env, client, owner)
}

fn init_split(
    client: &RemittanceSplitClient,
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

/// Generates valid percentages (a,b,c,d) where a+b+c+d==100.
fn valid_percentages() -> impl Strategy<Value = (u32, u32, u32, u32)> {
    // Choose three values in [0,100], keep only those with sum <= 100,
    // and set d as the remainder.
    (0u32..=100u32, 0u32..=100u32, 0u32..=100u32).prop_filter_map(
        "sum(a,b,c) <= 100",
        |(a, b, c)| {
            if a + b + c <= 100 {
                Some((a, b, c, 100 - a - b - c))
            } else {
                None
            }
        },
    )
}

fn positive_total() -> impl Strategy<Value = i128> {
    1i128..=MAX_SAFE_TOTAL
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: CASES,
        .. ProptestConfig::default()
    })]

    /// Prove dust conservation and deterministic dust assignment to insurance.
    #[test]
    fn prop_dust_conservation_and_deterministic_assignment(
        (sp, sg, sb, si) in valid_percentages(),
        total in positive_total(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);

        let allocs1 = client.calculate_split(&total);
        let sum: i128 = allocs1.iter().sum();
        prop_assert_eq!(
            sum, total,
            "dust conservation violated for total={}; percentages={}/{}/{}/{}; allocs={:?}",
            total, sp, sg, sb, si, allocs1
        );

        // Deterministic remainder category: insurance always receives the remainder
        // after flooring spending/savings/bills.
        let spending_expected = total.checked_mul(sp as i128)
            .and_then(|n| n.checked_div(100))
            .expect("checked arithmetic should not overflow in test range");
        let savings_expected = total.checked_mul(sg as i128)
            .and_then(|n| n.checked_div(100))
            .expect("checked arithmetic should not overflow in test range");
        let bills_expected = total.checked_mul(sb as i128)
            .and_then(|n| n.checked_div(100))
            .expect("checked arithmetic should not overflow in test range");
        let insurance_expected = total
            .checked_sub(spending_expected)
            .and_then(|n| n.checked_sub(savings_expected))
            .and_then(|n| n.checked_sub(bills_expected))
            .expect("checked arithmetic should not overflow in test range");

        prop_assert_eq!(
            allocs1.get_unchecked(0), spending_expected,
            "spending bucket mismatch for total={}; percentages={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(1), savings_expected,
            "savings bucket mismatch for total={}; percentages={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(2), bills_expected,
            "bills bucket mismatch for total={}; percentages={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(3), insurance_expected,
            "insurance bucket must receive the deterministic remainder (dust) for total={}; percentages={}/{}/{}/{}",
            total, sp, sg, sb, si
        );

        // Determinism stability: calling twice must yield identical allocations.
        let allocs2 = client.calculate_split(&total);
        prop_assert_eq!(allocs1, allocs2, "calculate_split must be deterministic");
    }
}

#[test]
fn dust_edge_cases_are_stable_and_conserve_total() {
    // Edge cases required by the issue: amount=0 is rejected by contract,
    // but amount=1 must still conserve dust. We also explicitly test a
    // representative near-boundary total.
    let mut cases: Vec<(i128, (u32, u32, u32, u32))> = Vec::new();
    cases.push((1, (33, 33, 33, 1)));
    cases.push((1, (100, 0, 0, 0)));
    cases.push((1, (0, 0, 0, 100)));
    cases.push((MAX_SAFE_TOTAL, (33, 33, 33, 1)));

    for (total, (sp, sg, sb, si)) in cases {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &cid);
        let owner = Address::generate(&env);
        init_split(&client, &env, &owner, sp, sg, sb, si);

        let allocs = client.calculate_split(&total);
        let sum: i128 = allocs.iter().sum();
        assert_eq!(sum, total, "dust conservation failed for total={}", total);

        let allocs2 = client.calculate_split(&total);
        assert_eq!(allocs, allocs2, "allocations must be stable across calls");
    }
}
