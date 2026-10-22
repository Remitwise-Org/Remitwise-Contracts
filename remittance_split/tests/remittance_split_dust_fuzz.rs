#![cfg(test)]

//! Proptest fuzzing for remittance_split dust conservation.
//!
//! Invariant (dust conservation): for any valid percentage tuple summing to 10_000
//! (basis points) and any positive `total_amount`, the allocations must satisfy:
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
/// With basis-point percentages (up to 10_000), the worst-case multiply is
/// `total * 10_000`, so we cap at `i128::MAX / 10_000`.
const MAX_SAFE_TOTAL: i128 = i128::MAX / 10_000;

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

/// Register a real SAC token. Required because `initialize_split` validates
/// the token via `try_symbol` / `try_name` / `try_decimals`.
fn make_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(admin).address()
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
    let token = make_token(env);
    client.initialize_split(owner, &0, &token, &sp, &sg, &sb, &si);
}

/// Set up a trusted orchestrator and return `(orchestrator, epoch)`.
fn setup_orchestrator(
    env: &Env,
    client: &RemittanceSplitClient,
    owner: &Address,
) -> (Address, u64) {
    let orchestrator = Address::generate(env);
    client.set_trusted_orchestrator(owner, &orchestrator);
    let epoch = client.get_cross_contract_epoch();
    (orchestrator, epoch)
}

/// Generates valid percentages (a,b,c,d) in basis points where a+b+c+d==10_000.
fn valid_percentages() -> impl Strategy<Value = (u32, u32, u32, u32)> {
    // Generate whole-percent triples (0–100) with sum ≤ 100, then
    // multiply by 100 to produce basis-point values summing to 10_000.
    (0u32..=100u32, 0u32..=100u32, 0u32..=100u32).prop_filter_map(
        "sum(a,b,c) <= 100",
        |(a, b, c)| {
            if a + b + c <= 100 {
                Some((a * 100, b * 100, c * 100, (100 - a - b - c) * 100))
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
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let allocs1 = client.calculate_split(&orch, &epoch, &total);
        let sum: i128 = allocs1.iter().sum();
        prop_assert_eq!(
            sum, total,
            "dust conservation violated for total={}; bp={}/{}/{}/{}; allocs={:?}",
            total, sp, sg, sb, si, allocs1
        );

        // Deterministic remainder category: insurance always receives the remainder
        // after flooring spending/savings/bills.
        let spending_expected = total.checked_mul(sp as i128)
            .and_then(|n| n.checked_div(10_000))
            .expect("checked arithmetic should not overflow in test range");
        let savings_expected = total.checked_mul(sg as i128)
            .and_then(|n| n.checked_div(10_000))
            .expect("checked arithmetic should not overflow in test range");
        let bills_expected = total.checked_mul(sb as i128)
            .and_then(|n| n.checked_div(10_000))
            .expect("checked arithmetic should not overflow in test range");
        let insurance_expected = total
            .checked_sub(spending_expected)
            .and_then(|n| n.checked_sub(savings_expected))
            .and_then(|n| n.checked_sub(bills_expected))
            .expect("checked arithmetic should not overflow in test range");

        prop_assert_eq!(
            allocs1.get_unchecked(0), spending_expected,
            "spending bucket mismatch for total={}; bp={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(1), savings_expected,
            "savings bucket mismatch for total={}; bp={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(2), bills_expected,
            "bills bucket mismatch for total={}; bp={}/{}/{}/{}",
            total, sp, sg, sb, si
        );
        prop_assert_eq!(
            allocs1.get_unchecked(3), insurance_expected,
            "insurance bucket must receive the deterministic remainder for total={}; bp={}/{}/{}/{}",
            total, sp, sg, sb, si
        );

        // Determinism stability: calling twice must yield identical allocations.
        let allocs2 = client.calculate_split(&orch, &epoch, &total);
        prop_assert_eq!(allocs1, allocs2, "calculate_split must be deterministic");
    }
}

#[test]
fn dust_edge_cases_are_stable_and_conserve_total() {
    // Edge cases: amount=0 is rejected, but amount=1 must conserve dust.
    // Percentages in basis points (sum = 10_000).
    let cases: Vec<(i128, (u32, u32, u32, u32))> = vec![
        (1, (3300, 3300, 3300, 100)),
        (1, (10_000, 0, 0, 0)),
        (1, (0, 0, 0, 10_000)),
        (MAX_SAFE_TOTAL, (3300, 3300, 3300, 100)),
    ];

    for (total, (sp, sg, sb, si)) in cases {
        let env = Env::default();
        env.mock_all_auths();
        let cid = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &cid);
        let owner = Address::generate(&env);
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let allocs = client.calculate_split(&orch, &epoch, &total);
        let sum: i128 = allocs.iter().sum();
        assert_eq!(sum, total, "dust conservation failed for total={}", total);

        let allocs2 = client.calculate_split(&orch, &epoch, &total);
        assert_eq!(allocs, allocs2, "allocations must be stable across calls");
    }
}
