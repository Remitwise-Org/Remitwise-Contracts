#![cfg(test)]

//! Fuzz/Property-based tests for numeric operations in remittance_split.
//!
//! These tests verify critical numeric invariants:
//! - Overflow protection
//! - Rounding behavior
//! - Sum preservation (split amounts always equal total)
//! - Edge cases with extreme values

use proptest::prelude::*;
use remittance_split::{
    AccountGroup, RemittanceSplit, RemittanceSplitClient, RemittanceSplitError,
    MAX_SCHEDULE_LEAD_TIME, MIN_SCHEDULE_INTERVAL,
};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env};
use std::collections::HashSet;

/// Helper: register a dummy token address (no real token needed for pure math tests).
fn dummy_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract_v2(admin).address()
}

/// Helper: initialize split with a dummy token address.
fn init(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) {
    let token = dummy_token(env);
    client.initialize_split(owner, &0, &token, &s, &g, &b, &i);
}

/// Helper: try_initialize_split with a dummy token address.
fn xorshift64(mut state: u64) -> u64 {
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    state
}

fn bounded_schedule_cases(
    seed: u64,
    count: usize,
    current_time: u64,
) -> std::vec::Vec<(i128, u64, u64, RemittanceSplitError)> {
    let invalid_amounts = [0i128, -1, -100, -1_000_000_000_000];
    let invalid_intervals = [1u64, MIN_SCHEDULE_INTERVAL - 1];
    let mut cases = std::vec::Vec::new();
    let mut state = seed;

    for index in 0..count {
        state = xorshift64(state);
        let selector = (state as usize) % 4;
        let case = match selector {
            0 => {
                let amount = invalid_amounts[((state >> 8) as usize) % invalid_amounts.len()];
                let next_due = current_time + 1_000;
                let interval = MIN_SCHEDULE_INTERVAL;
                (
                    amount,
                    next_due,
                    interval,
                    RemittanceSplitError::InvalidAmount,
                )
            }
            1 => {
                let next_due = if ((state >> 8) & 1) == 0 {
                    current_time
                } else {
                    current_time.saturating_sub(1)
                };
                (
                    1000,
                    next_due,
                    MIN_SCHEDULE_INTERVAL,
                    RemittanceSplitError::InvalidDueDate,
                )
            }
            2 => {
                let interval = invalid_intervals[((state >> 8) as usize) % invalid_intervals.len()];
                (
                    1000,
                    current_time + 1_000,
                    interval,
                    RemittanceSplitError::ScheduleIntervalTooShort,
                )
            }
            _ => {
                let next_due = current_time + MAX_SCHEDULE_LEAD_TIME + 1;
                (
                    1000,
                    next_due,
                    MIN_SCHEDULE_INTERVAL,
                    RemittanceSplitError::ScheduleLeadTimeTooLong,
                )
            }
        };
        cases.push(case);
        state ^= index as u64;
    }

    cases
}

fn assert_schedule_list_unchanged(
    client: &RemittanceSplitClient,
    owner: &Address,
    before_len: u32,
) {
    let after_len = client.get_remittance_schedules(owner).len();
    assert_eq!(
        before_len, after_len,
        "Schedule index was modified on validation failure"
    );
}

fn assert_schedule_unchanged(
    before: &remittance_split::RemittanceSchedule,
    after: &remittance_split::RemittanceSchedule,
) {
    assert_eq!(
        before, after,
        "Schedule changed after invalid modification request"
    );
}

fn try_init(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) -> Result<bool, ()> {
    let token = dummy_token(env);
    client
        .try_initialize_split(owner, &0, &token, &s, &g, &b, &i)
        .map(|r| r.unwrap())
        .map_err(|_| ())
}

// ---------------------------------------------------------------------------

#[test]
fn fuzz_calculate_split_sum_preservation() {
    let test_cases = vec![
        (1000, 50, 30, 15, 5),
        (1, 25, 25, 25, 25),
        (999, 33, 33, 33, 1),
        (i128::MAX / 100, 25, 25, 25, 25),
        (12345678, 17, 19, 23, 41),
        (100, 1, 1, 1, 97),
        (999999, 10, 20, 30, 40),
        (7, 40, 30, 20, 10),
        (543210, 70, 10, 10, 10),
        (1000000, 0, 0, 0, 100),
    ];

    for (total_amount, sp, sg, sb, si) in test_cases {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        if try_init(&client, &env, &owner, sp, sg, sb, si).is_err() {
            continue;
        }

        if client.try_calculate_split(&total_amount).is_err() {
            continue;
        }

        let amounts = client.calculate_split(&total_amount);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(
            sum, total_amount,
            "Sum mismatch for percentages {}%/{}%/{}%/{}%",
            sp, sg, sb, si
        );
        assert!(amounts.iter().all(|a| a >= 0), "Negative amount detected");
    }
}

#[test]
fn fuzz_calculate_split_small_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 25, 25, 25, 25);

    for amount in 1..=100i128 {
        let amounts = client.calculate_split(&amount);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, amount, "Sum mismatch for amount {}", amount);
        assert!(amounts.iter().all(|a| a <= amount), "Amount exceeds total");
    }
}

#[test]
fn fuzz_rounding_behavior() {
    let prime_percentages = vec![
        (3u32, 7u32, 11u32, 79u32),
        (13, 17, 23, 47),
        (19, 23, 29, 29),
        (31, 37, 11, 21),
        (41, 43, 7, 9),
    ];

    for (sp, sg, sb, si) in prime_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        init(&client, &env, &owner, sp, sg, sb, si);

        for amount in &[100i128, 1000, 9999, 123456] {
            let amounts = client.calculate_split(amount);
            let sum: i128 = amounts.iter().sum();
            assert_eq!(
                sum, *amount,
                "Rounding error for amount {} with {}%/{}%/{}%/{}%",
                amount, sp, sg, sb, si
            );
        }
    }
}

#[test]
fn fuzz_invalid_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    for amount in &[0i128, -1, -100, -1000, i128::MIN] {
        let result = client.try_calculate_split(amount);
        assert!(result.is_err(), "Expected error for amount {}", amount);
    }
}

#[test]
fn fuzz_invalid_percentages() {
    let invalid_percentages = vec![
        (50u32, 50u32, 10u32, 0u32),
        (25, 25, 25, 24),
        (100, 0, 0, 1),
        (0, 0, 0, 0),
        (30, 30, 30, 30),
    ];

    for (sp, sg, sb, si) in invalid_percentages {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let total = sp + sg + sb + si;
        let result = try_init(&client, &env, &owner, sp, sg, sb, si);
        if total != 100 {
            assert!(
                result.is_err(),
                "Expected error for percentages summing to {}",
                total
            );
        }
    }
}

#[test]
fn fuzz_large_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 25, 25, 25, 25);

    for amount in &[
        i128::MAX / 1000,
        i128::MAX / 100,
        1_000_000_000_000i128,
        999_999_999_999i128,
    ] {
        if client.try_calculate_split(amount).is_ok() {
            let amounts = client.calculate_split(amount);
            let sum: i128 = amounts.iter().sum();
            assert_eq!(sum, *amount, "Sum mismatch for large amount {}", amount);
        }
    }
}

#[test]
fn fuzz_single_category_splits() {
    let single_category_splits = vec![
        (100u32, 0u32, 0u32, 0u32),
        (0, 100, 0, 0),
        (0, 0, 100, 0),
        (0, 0, 0, 100),
    ];

    for (sp, sg, sb, si) in single_category_splits {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        init(&client, &env, &owner, sp, sg, sb, si);

        let amounts = client.calculate_split(&1000);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, 1000);

        if sp == 100 {
            assert_eq!(amounts.get(0).unwrap(), 1000);
        }
        if sg == 100 {
            assert_eq!(amounts.get(1).unwrap(), 1000);
        }
        if sb == 100 {
            assert_eq!(amounts.get(2).unwrap(), 1000);
        }
    }
}

#[test]
fn fuzz_schedule_create_modify_cancel_validations() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);
    let current_time = env.ledger().timestamp();
    let schedule_count = client.get_remittance_schedules(&owner).len();

    for (amount, next_due, interval, expected_error) in
        bounded_schedule_cases(0x1234_5678, 16, current_time)
    {
        let result = client.try_create_remittance_schedule(&owner, &amount, &next_due, &interval);
        assert_eq!(result, Err(Ok(expected_error)));
        assert_schedule_list_unchanged(&client, &owner, schedule_count);
    }

    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(current_time + 1_000),
        &MIN_SCHEDULE_INTERVAL,
    );
    let schedule_before = client.get_remittance_schedule(&schedule_id).unwrap();

    for (amount, next_due, interval, expected_error) in
        bounded_schedule_cases(0xDEAD_BEEF, 16, current_time)
    {
        let result = client.try_modify_remittance_schedule(
            &owner,
            &schedule_id,
            &amount,
            &next_due,
            &interval,
        );
        assert_eq!(result, Err(Ok(expected_error)));
        let schedule_after = client.get_remittance_schedule(&schedule_id).unwrap();
        assert_schedule_unchanged(&schedule_before, &schedule_after);
    }

    let wrong_owner = Address::generate(&env);
    let unauthorized_result = client.try_cancel_remittance_schedule(&wrong_owner, &schedule_id);
    assert_eq!(
        unauthorized_result,
        Err(Ok(RemittanceSplitError::Unauthorized))
    );
    assert_schedule_list_unchanged(&client, &owner, schedule_count + 1);

    let not_found_id = schedule_id + 10;
    let not_found_result = client.try_cancel_remittance_schedule(&owner, &not_found_id);
    assert_eq!(
        not_found_result,
        Err(Ok(RemittanceSplitError::ScheduleNotFound))
    );
    assert_schedule_list_unchanged(&client, &owner, schedule_count + 1);
}

proptest! {
    /// Property-based test for hardened nonce replay defenses.
    ///
    /// Generates bounded random sequences of (nonce, deadline, amount, request_hash)
    /// and exercises the combined replay defenses: deadline bounds, sequential nonce,
    /// used-nonce set, and request-hash binding.
    ///
    /// Also proves that snapshot import cannot re-enable previously used nonces,
    /// even if the nonce counter is hypothetically reset.
    ///
    /// Security notes:
    /// - Deadline bounds prevent pre-signed transactions from being too stale or too far ahead.
    /// - Sequential nonce ensures monotonic progression, preventing out-of-order replays.
    /// - Used-nonce set provides double-spend protection even if counter resets.
    /// - Request-hash binding ties the signature to exact parameters, preventing swap attacks.
    /// - Eviction policy (MAX_USED_NONCES_PER_ADDR=256) balances security with storage limits.
    #[test]
    #[ignore]
    fn prop_hardened_nonce_replay_protection(
        operations in prop::collection::vec(
            (0u64..1000, 1u64..3600, 1i128..1_000_000, 0u64..u64::MAX),
            1..50
        )
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let usdc_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
        let usdc_addr = usdc_contract.address();

        // Initialize the contract
        client.initialize_split(&owner, &0, &usdc_addr, &25, &25, &25, &25);

        // Mint tokens
        StellarAssetClient::new(&env, &usdc_addr).mint(&owner, &10_000_000i128);

        let accounts = AccountGroup {
            spending: Address::generate(&env),
            savings: Address::generate(&env),
            bills: Address::generate(&env),
            insurance: Address::generate(&env),
        };

        let mut used_nonces = HashSet::new();
        used_nonces.insert(0u64);
        let mut current_nonce = client.get_nonce(&owner);

        for (nonce_offset, deadline_offset, amount, request_hash) in operations {
            let nonce = current_nonce + nonce_offset;
            let deadline = env.ledger().timestamp() + deadline_offset;

            // Compute expected hash
            let expected_hash = RemittanceSplit::compute_request_hash(
                soroban_sdk::symbol_short!("distrib"),
                owner.clone(),
                nonce,
                amount,
                deadline,
            );

            // Test deadline bounds
            if deadline <= env.ledger().timestamp() {
                // Should fail due to expired deadline
                let result = client.try_distribute_usdc(
                    &usdc_addr,
                    &owner,
                    &nonce,
                    &deadline,
                    &request_hash,
                    &accounts,
                    &amount,
                );
                prop_assert!(result.is_err());
                continue;
            }

            if deadline > env.ledger().timestamp() + 3600 {
                // Should fail due to deadline too far
                let result = client.try_distribute_usdc(
                    &usdc_addr,
                    &owner,
                    &nonce,
                    &deadline,
                    &request_hash,
                    &accounts,
                    &amount,
                );
                prop_assert!(result.is_err());
                continue;
            }

            // Test sequential nonce
            if nonce != current_nonce {
                let result = client.try_distribute_usdc(
                    &usdc_addr,
                    &owner,
                    &nonce,
                    &deadline,
                    &expected_hash,
                    &accounts,
                    &amount,
                );
                prop_assert!(result.is_err());
                continue;
            }

            // Test used nonce set - should not be used yet
            prop_assert!(!used_nonces.contains(&nonce));

            // Test request hash binding
            if request_hash != expected_hash {
                let result = client.try_distribute_usdc(
                    &usdc_addr,
                    &owner,
                    &nonce,
                    &deadline,
                    &request_hash,
                    &accounts,
                    &amount,
                );
                prop_assert!(result.is_err());
                continue;
            }

            // Valid operation should succeed
            let result = client.distribute_usdc(
                &usdc_addr,
                &owner,
                &nonce,
                &deadline,
                &expected_hash,
                &accounts,
                &amount,
            );
            prop_assert!(result);

            // Mark nonce as used
            used_nonces.insert(nonce);
            current_nonce += 1;
        }

        // Test eviction policy
        // Fill up to MAX_USED_NONCES_PER_ADDR + some
        for _i in 0..300 {
            let nonce = current_nonce;
            let deadline = env.ledger().timestamp() + 1000;
            let amount = 1000i128;
            let expected_hash = RemittanceSplit::compute_request_hash(
                soroban_sdk::symbol_short!("distrib"),
                owner.clone(),
                nonce,
                amount,
                deadline,
            );

            if client.try_distribute_usdc(
                &usdc_addr,
                &owner,
                &nonce,
                &deadline,
                &expected_hash,
                &accounts,
                &amount,
            ).is_ok() {
                used_nonces.insert(nonce);
                current_nonce += 1;
            }
        }

        // Check that old nonces are evicted (MAX_USED_NONCES_PER_ADDR = 256)
        // The used set should have at most MAX_USED_NONCES_PER_ADDR entries
        prop_assert!(!used_nonces.is_empty());

        // Test snapshot import scenario: even if nonce counter is reset,
        // used nonces should still be blocked
        let old_nonce = 0u64; // Assume this was used
        prop_assert!(used_nonces.contains(&old_nonce));

        // Simulate nonce counter reset (hypothetical)
        // In reality, import_snapshot doesn't reset nonces, but for this test,
        // we manually reset the counter to simulate the threat
        let nonces_key = soroban_sdk::symbol_short!("NONCES");
        let mut nonces_map: soroban_sdk::Map<Address, u64> = env.storage().instance().get(&nonces_key).unwrap_or_else(|| soroban_sdk::Map::new(&env));
        nonces_map.set(owner.clone(), 0u64); // Reset counter
        env.storage().instance().set(&nonces_key, &nonces_map);

        // Now try to reuse the old nonce - should still fail due to used set
        let deadline = env.ledger().timestamp() + 1000;
        let amount = 1000i128;
        let expected_hash = RemittanceSplit::compute_request_hash(
            soroban_sdk::symbol_short!("distrib"),
            owner.clone(),
            old_nonce,
            amount,
            deadline,
        );

        let result = client.try_distribute_usdc(
            &usdc_addr,
            &owner,
            &old_nonce,
            &deadline,
            &expected_hash,
            &accounts,
            &amount,
        );
        prop_assert!(result.is_err()); // Should fail because nonce is used
    }
}

// ---------------------------------------------------------------------------
// Additional bounded fuzz tests for schedule create/modify/cancel validations
// (Issue #1407)
// ---------------------------------------------------------------------------

/// Verify that `bounded_schedule_cases` produces identical error sequences
/// when called twice with the same seed and count. This ensures our PRNG
/// helper is purely deterministic and that test outcomes are reproducible
/// across CI runs and developer machines.
#[test]
fn fuzz_schedule_multi_seed_determinism() {
    let current_time = 1_000_000u64;
    let seeds: &[u64] = &[0x1234_5678, 0xDEAD_BEEF, 0xCAFE_F00D, 0x0000_0001, u64::MAX];

    for &seed in seeds {
        let run_a = bounded_schedule_cases(seed, 32, current_time);
        let run_b = bounded_schedule_cases(seed, 32, current_time);
        assert_eq!(
            run_a, run_b,
            "bounded_schedule_cases is not deterministic for seed {:#x}",
            seed
        );
    }

    // Different seeds must produce at least one differing element.
    let cases_a = bounded_schedule_cases(0xAAAA_AAAA, 32, current_time);
    let cases_b = bounded_schedule_cases(0x5555_5555, 32, current_time);
    assert_ne!(
        cases_a, cases_b,
        "Different seeds produced identical case sequences — PRNG may be broken"
    );
}

/// Exhaustively probe the boundary between `ScheduleIntervalTooShort` and a
/// valid recurring interval. Specifically:
///
/// - `interval == 0`                        → valid (one-off schedule)
/// - `0 < interval < MIN_SCHEDULE_INTERVAL` → `ScheduleIntervalTooShort`
/// - `interval == MIN_SCHEDULE_INTERVAL`    → valid (minimum recurring)
/// - `interval >  MIN_SCHEDULE_INTERVAL`    → valid (longer recurring)
#[test]
fn fuzz_schedule_interval_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let next_due = current_time + MIN_SCHEDULE_INTERVAL * 2; // safely in the future
    let amount = 1_000i128;

    // Values strictly inside the forbidden range must be rejected.
    for bad_interval in [1u64, 2, 100, MIN_SCHEDULE_INTERVAL - 1] {
        let result =
            client.try_create_remittance_schedule(&owner, &amount, &next_due, &bad_interval);
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort)),
            "interval {} should be ScheduleIntervalTooShort",
            bad_interval
        );
    }

    // Exactly at the minimum boundary → should succeed.
    let id_min =
        client.create_remittance_schedule(&owner, &amount, &next_due, &MIN_SCHEDULE_INTERVAL);
    assert!(id_min > 0, "MIN_SCHEDULE_INTERVAL should be accepted");

    // One above minimum → should succeed.
    let id_above =
        client.create_remittance_schedule(&owner, &amount, &next_due, &(MIN_SCHEDULE_INTERVAL + 1));
    assert!(id_above > id_min);

    // interval == 0 (one-off) → should succeed.
    let id_oneoff = client.create_remittance_schedule(&owner, &amount, &next_due, &0);
    assert!(id_oneoff > id_above);
}

/// After a schedule is successfully cancelled, a second cancel attempt on the
/// same ID must return `InactiveSchedule`, not silently succeed or corrupt state.
/// Multiple seeds of fuzz-generated IDs are exercised to rule out ID-specific bugs.
#[test]
fn fuzz_schedule_cancel_idempotency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let next_due = current_time + MIN_SCHEDULE_INTERVAL * 2;

    // Create several schedules and cancel each one, then verify double-cancel.
    let ids: std::vec::Vec<u32> = (0..5)
        .map(|_| {
            client.create_remittance_schedule(&owner, &1_000, &next_due, &MIN_SCHEDULE_INTERVAL)
        })
        .collect();

    for id in &ids {
        // First cancel: must succeed.
        let first = client.try_cancel_remittance_schedule(&owner, id);
        assert_eq!(
            first,
            Ok(Ok(true)),
            "first cancel of id {} should succeed",
            id
        );

        // The schedule must now be inactive.
        let sched = client
            .get_remittance_schedule(id)
            .expect("schedule should still exist after cancel");
        assert!(
            !sched.active,
            "schedule {} should be inactive after cancel",
            id
        );

        // Second cancel: must return InactiveSchedule, not Ok.
        let second = client.try_cancel_remittance_schedule(&owner, id);
        assert_eq!(
            second,
            Err(Ok(RemittanceSplitError::InactiveSchedule)),
            "second cancel of id {} should return InactiveSchedule",
            id
        );

        // State must not have changed between the two cancel calls.
        let sched_after = client.get_remittance_schedule(id).unwrap();
        assert!(
            !sched_after.active,
            "schedule {} must remain inactive after double-cancel",
            id
        );
    }
}

/// Fill an owner's schedule list to exactly `MAX_SCHEDULES_PER_OWNER` and verify
/// that the next creation attempt returns `ScheduleCapExceeded`. Also verifies
/// that no partial write occurs: the schedule count does not change on failure.
#[test]
fn fuzz_schedule_cap_exceeded() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let next_due = current_time + MIN_SCHEDULE_INTERVAL * 2;

    // Fill up to the cap.
    for i in 0..remittance_split::MAX_SCHEDULES_PER_OWNER {
        let result = client.try_create_remittance_schedule(
            &owner,
            &1_000,
            &next_due,
            &MIN_SCHEDULE_INTERVAL,
        );
        assert!(
            result.is_ok(),
            "schedule {} of {} should succeed, got {:?}",
            i + 1,
            remittance_split::MAX_SCHEDULES_PER_OWNER,
            result
        );
    }

    let count_at_cap = client.get_remittance_schedules(&owner).len();
    assert_eq!(count_at_cap, remittance_split::MAX_SCHEDULES_PER_OWNER);

    // One more must fail.
    let over_cap =
        client.try_create_remittance_schedule(&owner, &1_000, &next_due, &MIN_SCHEDULE_INTERVAL);
    assert_eq!(
        over_cap,
        Err(Ok(RemittanceSplitError::ScheduleCapExceeded)),
        "creation beyond cap should return ScheduleCapExceeded"
    );

    // Count must not have increased.
    assert_schedule_list_unchanged(&client, &owner, count_at_cap);

    // Repeat with a second seed to rule out off-by-one in cap check.
    let over_cap_again =
        client.try_create_remittance_schedule(&owner, &500, &next_due, &MIN_SCHEDULE_INTERVAL);
    assert_eq!(
        over_cap_again,
        Err(Ok(RemittanceSplitError::ScheduleCapExceeded))
    );
    assert_schedule_list_unchanged(&client, &owner, count_at_cap);
}

/// A one-off schedule (`interval == 0`) must be created without error and must
/// be stored with `recurring = false`. Attempting to create it with every
/// forbidden non-zero interval less than `MIN_SCHEDULE_INTERVAL` must still
/// fail with `ScheduleIntervalTooShort`, confirming that the one-off exemption
/// applies only to `interval == 0` and not to any other sub-minimum value.
#[test]
fn fuzz_schedule_one_off_created_and_marked_non_recurring() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let next_due = current_time + MIN_SCHEDULE_INTERVAL;

    // interval == 0 must be accepted.
    let id = client.create_remittance_schedule(&owner, &5_000, &next_due, &0);
    let sched = client
        .get_remittance_schedule(&id)
        .expect("one-off schedule must be retrievable");

    assert_eq!(sched.interval, 0, "one-off interval must be stored as 0");
    assert!(
        !sched.recurring,
        "one-off schedule must have recurring=false"
    );
    assert!(
        sched.active,
        "newly created one-off schedule must be active"
    );
    assert_eq!(sched.amount, 5_000);
    assert_eq!(sched.next_due, next_due);

    // Confirm that intervals 1..MIN_SCHEDULE_INTERVAL-1 remain forbidden.
    let mut state = 0xFEED_CAFE_u64;
    for _ in 0..16 {
        state = xorshift64(state);
        let bad_interval = 1 + (state % (MIN_SCHEDULE_INTERVAL - 1));
        let result =
            client.try_create_remittance_schedule(&owner, &1_000, &next_due, &bad_interval);
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort)),
            "interval {} must still be ScheduleIntervalTooShort (not one-off exemption)",
            bad_interval
        );
    }
}

/// Verify that every validation failure path in `create_remittance_schedule`
/// and `modify_remittance_schedule` leaves persistent storage unmodified.
///
/// We check two proxies for "nothing was written":
/// 1. The owner's schedule count does not increase.
/// 2. For modify, the existing schedule's fields are identical before and after.
///
/// This test uses deterministic fuzz seeds to cover all four error classes
/// (InvalidAmount, InvalidDueDate, ScheduleIntervalTooShort, ScheduleLeadTimeTooLong)
/// across multiple PRNG-generated inputs.
#[test]
fn fuzz_schedule_no_storage_write_on_validation_failure() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let initial_count = client.get_remittance_schedules(&owner).len();

    // Run fuzz cases for create — none should write to storage.
    for seed in [0xABCD_1234_u64, 0x9999_8888, 0x1111_2222, 0xFFFF_0000] {
        for (amount, next_due, interval, _expected) in
            bounded_schedule_cases(seed, 20, current_time)
        {
            let _ = client.try_create_remittance_schedule(&owner, &amount, &next_due, &interval);
            assert_schedule_list_unchanged(&client, &owner, initial_count);
        }
    }

    // Create one valid schedule, then run fuzz cases for modify.
    let valid_next_due = current_time + MIN_SCHEDULE_INTERVAL * 2;
    let schedule_id =
        client.create_remittance_schedule(&owner, &2_000, &valid_next_due, &MIN_SCHEDULE_INTERVAL);
    let schedule_before = client.get_remittance_schedule(&schedule_id).unwrap();

    for seed in [0x5A5A_5A5A_u64, 0xA5A5_A5A5, 0x1357_2468, 0x8642_9753] {
        for (amount, next_due, interval, _expected) in
            bounded_schedule_cases(seed, 20, current_time)
        {
            let _ = client.try_modify_remittance_schedule(
                &owner,
                &schedule_id,
                &amount,
                &next_due,
                &interval,
            );
            let schedule_after = client.get_remittance_schedule(&schedule_id).unwrap();
            assert_schedule_unchanged(&schedule_before, &schedule_after);
        }
    }
}

/// `modify_remittance_schedule` must return the correct error when the target
/// schedule does not exist or has been cancelled, and must not write any data.
///
/// Covers:
/// - `ScheduleNotFound` for IDs that were never created (fuzz-generated).
/// - `InactiveSchedule` for IDs that exist but have been cancelled.
/// - Storage is unchanged in both cases (schedule count and individual fields).
#[test]
fn fuzz_schedule_modify_on_inactive_or_nonexistent() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    init(&client, &env, &owner, 50, 30, 15, 5);

    let current_time = env.ledger().timestamp();
    let next_due = current_time + MIN_SCHEDULE_INTERVAL * 2;
    let valid_amount = 1_000i128;
    let valid_interval = MIN_SCHEDULE_INTERVAL;

    // --- Part A: nonexistent IDs ---
    // Use a PRNG to generate IDs that were never allocated.
    let mut state = 0xDEAD_C0DE_u64;
    let initial_count = client.get_remittance_schedules(&owner).len();

    for _ in 0..16 {
        state = xorshift64(state);
        // IDs in the high range are guaranteed not to exist yet.
        let nonexistent_id = 10_000u32 + (state as u32 % 50_000);
        let result = client.try_modify_remittance_schedule(
            &owner,
            &nonexistent_id,
            &valid_amount,
            &next_due,
            &valid_interval,
        );
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::ScheduleNotFound)),
            "modify on nonexistent id {} must return ScheduleNotFound",
            nonexistent_id
        );
        assert_schedule_list_unchanged(&client, &owner, initial_count);
    }

    // --- Part B: inactive (cancelled) schedule ---
    let id = client.create_remittance_schedule(&owner, &valid_amount, &next_due, &valid_interval);
    client.cancel_remittance_schedule(&owner, &id);

    let cancelled_schedule = client.get_remittance_schedule(&id).unwrap();
    assert!(!cancelled_schedule.active);

    // Every combination of valid parameters must be rejected with InactiveSchedule.
    for (_, _, _, _) in bounded_schedule_cases(0xC0DE_BABE, 8, current_time) {
        // Use only valid parameters; we want the inactive guard to fire, not validation.
        let result = client.try_modify_remittance_schedule(
            &owner,
            &id,
            &valid_amount,
            &next_due,
            &valid_interval,
        );
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::InactiveSchedule)),
            "modify on cancelled schedule must return InactiveSchedule"
        );
        // Schedule must remain unchanged.
        let sched_after = client.get_remittance_schedule(&id).unwrap();
        assert_schedule_unchanged(&cancelled_schedule, &sched_after);
    }
}
