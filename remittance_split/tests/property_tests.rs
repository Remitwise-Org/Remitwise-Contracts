#![cfg(test)]

//! Property-based tests for `remittance_split::calculate_split`.
//!
//! These tests use [`proptest`] to exercise randomized inputs and prove that the
//! following invariants hold unconditionally:
//!
//! **P1 – Sum preservation**: for any valid percentage config and any positive
//!   `total_amount`, the four allocated buckets always sum back to `total_amount`
//!   (no value is lost or created by rounding).
//!
//! **P2 – Non-negativity**: every individual bucket is ≥ 0.
//!
//! **P3 – Boundedness**: every individual bucket is ≤ `total_amount`.
//!
//! **P4 – Invalid-amount rejection**: `total_amount ≤ 0` must always be rejected.
//!
//! **P5 – Invalid-percentage rejection**: percentages that do not sum to exactly
//!   10_000 must be rejected at initialization time.
//!
//! **P6 – Determinism**: two identical configs and the same amount always produce
//!   identical allocations.
//!
//! **P7 – Adversarial edge preservation**: extreme values (1, `i128::MAX/100`,
//!   all-in-one-category, etc.) still respect P1–P3.

use proptest::prelude::*;
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Maximum safe total to avoid overflow inside `total * percent / 10_000`.
/// We cap at `i128::MAX / 10_000` to stay in the safe range for all percentages
/// (the worst case is a 10_000 bps / 100% config multiplied before division).
const MAX_SAFE_TOTAL: i128 = i128::MAX / 10_000;

/// Build a fresh environment, register the contract, and return client + owner.
fn setup() -> (Env, RemittanceSplitClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &cid);
    let owner = Address::generate(&env);
    (env, client, owner)
}

/// Register a Stellar asset contract and return its address.
/// Required because `initialize_split` validates the token via `try_symbol` etc.
fn make_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(admin).address()
}

/// Initialize the split config with basis-point percentages (must sum to 10_000).
/// Panics if the contract rejects valid inputs.
fn init_split(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) {
    let token = make_token(env);
    client.initialize_split(owner, &0, &token, &s, &g, &b, &i);
}

/// Set up a trusted orchestrator on the contract and return `(orchestrator, epoch)`.
/// Callers must pass these to every `calculate_split` invocation.
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

// ---------------------------------------------------------------------------
// Proptest strategies
// ---------------------------------------------------------------------------

/// Generates four basis-point percentages (a, b, c, d) where a+b+c+d == 10_000.
///
/// Strategy: draw three values in [0, 100] (whole percent), keep only triples
/// that sum to ≤ 100, then multiply each by 100 to get basis points. d is the
/// remainder × 100. This produces the same statistical distribution as before
/// while satisfying the `== 10_000` invariant.
fn valid_percentages() -> impl Strategy<Value = (u32, u32, u32, u32)> {
    (0u32..=100u32, 0u32..=100u32, 0u32..=100u32).prop_filter_map(
        "reject triples that exceed 100",
        |(a, b, c)| {
            if a + b + c <= 100 {
                // Convert whole-percent to basis points (×100).
                Some((a * 100, b * 100, c * 100, (100 - a - b - c) * 100))
            } else {
                None
            }
        },
    )
}

/// Generates positive `total_amount` values within the overflow-safe range.
fn positive_total() -> impl Strategy<Value = i128> {
    1i128..=MAX_SAFE_TOTAL
}

/// Generates positive totals that span a broad `i128` range.
fn full_range_total() -> impl Strategy<Value = i128> {
    prop_oneof![
        1i128..=100i128,
        Just(i128::MAX / 10_000),
        Just(i128::MAX / 2),
    ]
}

/// Generates non-positive amounts (adversarial: should always be rejected).
fn non_positive_total() -> impl Strategy<Value = i128> {
    prop_oneof![
        Just(0i128),
        Just(-1i128),
        Just(i128::MIN),
        (i128::MIN..0i128),
    ]
}

/// Generates percentage tuples in basis points that do NOT sum to 10_000.
fn invalid_percentages() -> impl Strategy<Value = (u32, u32, u32, u32)> {
    (0u32..=15_000u32, 0u32..=15_000u32, 0u32..=15_000u32, 0u32..=15_000u32).prop_filter_map(
        "keep only tuples whose sum != 10_000",
        |(a, b, c, d)| {
            if a.saturating_add(b).saturating_add(c).saturating_add(d) != 10_000 {
                Some((a, b, c, d))
            } else {
                None
            }
        },
    )
}

// ---------------------------------------------------------------------------
// P1 + P2 + P3 – Core numeric invariants (random percentages, random totals)
// ---------------------------------------------------------------------------

proptest! {
    /// P1/P2/P3: For any valid config and any positive total, the split
    /// preserves the total, keeps each bucket non-negative, and keeps each
    /// bucket ≤ total.
    #[test]
    fn prop_split_invariants_random_config_and_total(
        (sp, sg, sb, si) in valid_percentages(),
        total in positive_total(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let amounts = client.calculate_split(&orch, &epoch, &total);

        let sum: i128 = amounts.iter().sum();

        // P1 – sum preservation
        prop_assert_eq!(
            sum, total,
            "sum ({}) != total ({}) for config {} bp/{} bp/{} bp/{} bp",
            sum, total, sp, sg, sb, si
        );

        for bucket in amounts.iter() {
            // P2 – non-negativity
            prop_assert!(
                bucket >= 0,
                "negative bucket {} for total {} config {} bp",
                bucket, total, sp
            );
            // P3 – boundedness
            prop_assert!(
                bucket <= total,
                "bucket {} exceeds total {} config {} bp",
                bucket, total, sp
            );
        }
    }
}

// ---------------------------------------------------------------------------
// P4 – Full-range conservation regression
// ---------------------------------------------------------------------------

proptest! {
    /// Regression: the split must conserve the entire amount even at the upper
    /// edge of `i128 / 10_000`.
    #[test]
    fn prop_split_conserves_total_across_full_i128_range(
        (sp, sg, sb, si) in valid_percentages(),
        total in full_range_total(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let result = client.try_calculate_split(&orch, &epoch, &total);
        prop_assert!(result.is_ok(), "expected successful split for total {}", total);

        let amounts = result.unwrap().unwrap();
        let sum: i128 = amounts.iter().sum();

        prop_assert_eq!(
            sum, total,
            "split conservation failed for total {} with config {} bp/{} bp/{} bp/{} bp",
            total, sp, sg, sb, si
        );

        for bucket in amounts.iter() {
            prop_assert!(bucket >= 0, "negative bucket for total {}", total);
            prop_assert!(bucket <= total, "bucket exceeds total {}", total);
        }
    }
}

// ---------------------------------------------------------------------------
// P5 – Invalid-amount rejection
// ---------------------------------------------------------------------------

proptest! {
    /// P4: `calculate_split` must reject any `total_amount ≤ 0`.
    #[test]
    fn prop_split_rejects_non_positive_amount(
        amount in non_positive_total(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, 2500, 2500, 2500, 2500);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let result = client.try_calculate_split(&orch, &epoch, &amount);
        prop_assert!(
            result.is_err() || result.unwrap().is_err(),
            "expected error for non-positive amount {}",
            amount
        );
    }
}

// ---------------------------------------------------------------------------
// P6 – Invalid-percentage rejection
// ---------------------------------------------------------------------------

proptest! {
    /// P5: `initialize_split` must reject any percentage tuple that does not
    /// sum to exactly 10_000.
    #[test]
    fn prop_split_rejects_invalid_percentages(
        (a, b, c, d) in invalid_percentages(),
    ) {
        let (env, client, owner) = setup();
        // Use a real SAC token so the rejection is specifically PercentagesDoNotSumTo100.
        let token = make_token(&env);

        let result = client.try_initialize_split(&owner, &0, &token, &a, &b, &c, &d);
        prop_assert!(
            result.is_err() || result.unwrap().is_err(),
            "expected rejection for percentages {}/{}/{}/{} bp (sum={})",
            a, b, c, d,
            a.saturating_add(b).saturating_add(c).saturating_add(d)
        );
    }
}

// ---------------------------------------------------------------------------
// P7 – Determinism
// ---------------------------------------------------------------------------

proptest! {
    /// P6: Two contracts with identical configs produce identical allocations
    /// for the same total.
    #[test]
    fn prop_split_is_deterministic(
        (sp, sg, sb, si) in valid_percentages(),
        total in positive_total(),
    ) {
        // First contract
        let (env1, client1, owner1) = setup();
        init_split(&client1, &env1, &owner1, sp, sg, sb, si);
        let (orch1, epoch1) = setup_orchestrator(&env1, &client1, &owner1);
        let result1 = client1.calculate_split(&orch1, &epoch1, &total);

        // Second contract – same config, same total
        let (env2, client2, owner2) = setup();
        init_split(&client2, &env2, &owner2, sp, sg, sb, si);
        let (orch2, epoch2) = setup_orchestrator(&env2, &client2, &owner2);
        let result2 = client2.calculate_split(&orch2, &epoch2, &total);

        prop_assert_eq!(result1.len(), result2.len());
        for (a, b) in result1.iter().zip(result2.iter()) {
            prop_assert_eq!(
                a, b,
                "non-deterministic result for total={} config={} bp/{} bp/{} bp/{} bp",
                total, sp, sg, sb, si
            );
        }
    }
}

// ---------------------------------------------------------------------------
// P8 – Adversarial edge cases (all boundary inputs still respect P1–P3)
// ---------------------------------------------------------------------------

proptest! {
    /// P7a: All-in-one-category configs (10000/0/0/0 permutations) still
    /// preserve the total and produce exactly one non-zero bucket.
    #[test]
    fn prop_split_single_category_preserves_total(
        // which category gets 100%: 0=spending, 1=savings, 2=bills, 3=insurance
        category in 0usize..4,
        total in positive_total(),
    ) {
        let percs: [(u32, u32, u32, u32); 4] = [
            (10_000, 0, 0, 0),
            (0, 10_000, 0, 0),
            (0, 0, 10_000, 0),
            (0, 0, 0, 10_000),
        ];
        let (sp, sg, sb, si) = percs[category];

        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let amounts = client.calculate_split(&orch, &epoch, &total);

        let sum: i128 = amounts.iter().sum();
        prop_assert_eq!(sum, total, "sum mismatch in single-category split");
        prop_assert_eq!(
            amounts.get(category as u32).unwrap(),
            total,
            "winning bucket should equal total"
        );

        // All other buckets must be 0
        for (i, bucket) in amounts.iter().enumerate() {
            if i != category {
                prop_assert_eq!(bucket, 0i128, "non-winning bucket must be zero");
            }
        }
    }

    /// P7b: Smallest possible total (1 unit) never violates P1–P3 across all
    /// valid percentage configs.
    #[test]
    fn prop_split_unit_total_invariants(
        (sp, sg, sb, si) in valid_percentages(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let amounts = client.calculate_split(&orch, &epoch, &1i128);

        let sum: i128 = amounts.iter().sum();
        prop_assert_eq!(sum, 1i128, "sum must be 1 for unit total");
        for bucket in amounts.iter() {
            prop_assert!(bucket >= 0, "negative bucket for unit total");
            prop_assert!(bucket <= 1, "bucket exceeds unit total");
        }
    }

    /// P7c: Maximum safe total still satisfies P1–P3 across all valid configs.
    #[test]
    fn prop_split_max_safe_total_invariants(
        (sp, sg, sb, si) in valid_percentages(),
    ) {
        let (env, client, owner) = setup();
        init_split(&client, &env, &owner, sp, sg, sb, si);
        let (orch, epoch) = setup_orchestrator(&env, &client, &owner);

        let amounts = client.calculate_split(&orch, &epoch, &MAX_SAFE_TOTAL);

        let sum: i128 = amounts.iter().sum();
        prop_assert_eq!(sum, MAX_SAFE_TOTAL, "sum mismatch at max safe total");
        for bucket in amounts.iter() {
            prop_assert!(bucket >= 0);
            prop_assert!(bucket <= MAX_SAFE_TOTAL);
        }
    }
}
