//! Stress tests for insurance storage limits and TTL behavior.
//!
//! Issue #178: Stress Test Storage Limits and TTL
//!
//! Coverage:
//!   - Many policies per user (200+) exercising the instance-storage Map
//!   - Many policies across multiple users, verifying per-owner isolation
//!   - Instance TTL re-bump after a ledger advancement that crosses the threshold
//!   - Batch premium payment at MAX_BATCH_SIZE (50)
//!   - Performance benchmarks (CPU instructions + memory bytes) for key reads
//!
//! Storage layout (insurance):
//!   All policies live in one Map<u32, InsurancePolicy> inside instance() storage.
//!   INSTANCE_BUMP_AMOUNT        = 518,400 ledgers (~30 days)
//!   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
//!   MAX_PAGE_LIMIT              = 50
//!   DEFAULT_PAGE_LIMIT          = 20
//!   MAX_BATCH_SIZE              = 50

use insurance::{Insurance, InsuranceClient};
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stress_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    env.budget().reset_unlimited();
    env
}

fn measure<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    let result = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, result)
}

// ---------------------------------------------------------------------------
// Stress: many entities per user
// ---------------------------------------------------------------------------

/// Create 200 policies for a single user and verify full dataset is accessible
/// via cursor-based get_active_policies pagination (MAX_PAGE_LIMIT = 50).
#[test]
fn stress_200_policies_single_user() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "StressPolicy");
    let coverage_type = String::from_str(&env, "health");

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
    }

    // Verify aggregate monthly premium
    let total_premium = client.get_total_monthly_premium(&owner);
    assert_eq!(
        total_premium,
        200 * 100i128,
        "get_total_monthly_premium must sum premiums across all 200 policies"
    );

    // Exhaust all pages (MAX_PAGE_LIMIT = 50 → 4 pages)
    let mut collected = 0u32;
    let mut cursor = 0u32;
    let mut pages = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &50u32);
        assert!(
            page.count <= 50,
            "Page count {} exceeds MAX_PAGE_LIMIT 50",
            page.count
        );
        collected += page.count;
        pages += 1;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected, 200, "Pagination must return all 200 active policies");
    // get_active_policies sets next_cursor = last_returned_id; when a page is exactly
    // full the caller receives a non-zero cursor that produces a trailing empty page,
    // so the round-trip count is pages = ceil(200/50) + 1 trailing = 5.
    assert!(pages >= 4 && pages <= 5, "Expected 4-5 pages for 200 policies at limit 50, got {}", pages);
}

/// Create 200 policies and verify instance TTL remains valid after the instance
/// Map grows to 200 entries.
#[test]
fn stress_instance_ttl_valid_after_200_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLPolicy");
    let coverage_type = String::from_str(&env, "life");

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &50i128, &5_000i128);
    }

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must remain >= INSTANCE_BUMP_AMOUNT (518,400) after 200 creates",
        ttl
    );
}

// ---------------------------------------------------------------------------
// Stress: many users
// ---------------------------------------------------------------------------

/// Create 20 policies each for 10 different users (200 total) and verify
/// per-owner isolation — each user sees only their own policies and premiums.
#[test]
fn stress_policies_across_10_users() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    const N_USERS: usize = 10;
    const POLICIES_PER_USER: u32 = 20;
    const PREMIUM_PER_POLICY: i128 = 150;
    let name = String::from_str(&env, "UserPolicy");
    let coverage_type = String::from_str(&env, "health");

    let users: std::vec::Vec<Address> = (0..N_USERS).map(|_| Address::generate(&env)).collect();

    for user in &users {
        for _ in 0..POLICIES_PER_USER {
            client.create_policy(
                user,
                &name,
                &coverage_type,
                &PREMIUM_PER_POLICY,
                &50_000i128,
            );
        }
    }

    for user in &users {
        let total = client.get_total_monthly_premium(user);
        assert_eq!(
            total,
            POLICIES_PER_USER as i128 * PREMIUM_PER_POLICY,
            "Each user's total premium must reflect only their own policies"
        );

        // Verify paginated count
        let mut seen = 0u32;
        let mut cursor = 0u32;
        loop {
            let page = client.get_active_policies(user, &cursor, &50u32);
            seen += page.count;
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        assert_eq!(
            seen, POLICIES_PER_USER,
            "Each user must see exactly their own {} policies",
            POLICIES_PER_USER
        );
    }
}

// ---------------------------------------------------------------------------
// Stress: TTL re-bump after ledger advancement
// ---------------------------------------------------------------------------

/// Verify the instance TTL is re-bumped to >= INSTANCE_BUMP_AMOUNT (518,400)
/// after the ledger advances far enough to drop TTL below the threshold (17,280).
///
/// Phase 1: create 50 policies at sequence 100 → live_until ≈ 518,500
/// Phase 2: advance to sequence 510,000 → TTL ≈ 8,500 (below 17,280 threshold)
/// Phase 3: create 1 more policy → extend_ttl fires → TTL re-bumped
#[test]
fn stress_ttl_re_bumped_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLStress");
    let coverage_type = String::from_str(&env, "health");

    // Phase 1: 50 creates
    for _ in 0..50 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
    }

    let ttl_batch1 = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_batch1 >= 518_400,
        "TTL ({}) must be >= 518,400 after first batch of creates",
        ttl_batch1
    );

    // Phase 2: advance ledger so TTL drops below threshold
    // live_until ≈ 518,500; at seq 510,000 → TTL ≈ 8,500 < 17,280
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 510_000,
        timestamp: 1_705_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });

    let ttl_degraded = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_degraded < 17_280,
        "TTL ({}) must have degraded below threshold 17,280 after ledger jump",
        ttl_degraded
    );

    // Phase 3: create_policy fires extend_ttl → re-bumped
    client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);

    let ttl_rebumped = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_rebumped >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after create_policy post-advancement",
        ttl_rebumped
    );
}

/// Verify TTL is also re-bumped by pay_premium after ledger advancement.
#[test]
fn stress_ttl_re_bumped_by_pay_premium_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "PayTTL"),
        &String::from_str(&env, "health"),
        &200i128,
        &20_000i128,
    );

    // Advance ledger so TTL drops below threshold
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 510_000,
        timestamp: 1_705_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });

    // pay_premium must re-bump TTL
    let paid = client.pay_premium(&owner, &policy_id);
    assert!(paid, "pay_premium must succeed");

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after pay_premium post-advancement",
        ttl
    );
}

// ---------------------------------------------------------------------------
// Stress: batch operations at limit
// ---------------------------------------------------------------------------

/// Create 50 policies and pay all premiums in a single batch_pay_premiums call
/// (MAX_BATCH_SIZE = 50). Verify count returned and each policy has been updated.
#[test]
fn stress_batch_pay_premiums_at_max_batch_size() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    const BATCH_SIZE: u32 = 50; // MAX_BATCH_SIZE
    let name = String::from_str(&env, "BatchPolicy");
    let coverage_type = String::from_str(&env, "health");

    let mut policy_ids = std::vec![];
    for _ in 0..BATCH_SIZE {
        let id = client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
        policy_ids.push(id);
    }

    let mut ids_vec = soroban_sdk::Vec::new(&env);
    for &id in &policy_ids {
        ids_vec.push_back(id);
    }

    let paid_count = client.batch_pay_premiums(&owner, &ids_vec);
    assert_eq!(
        paid_count, BATCH_SIZE,
        "batch_pay_premiums must process all {} policies",
        BATCH_SIZE
    );

    // Verify each policy still has an active status and its next_payment_date is
    // set to current_time + 30 days. Both create_policy and batch_pay_premiums run
    // at the same ledger timestamp (1_700_000_000), so next_payment_date equals
    // 1_700_000_000 + 30 * 86400 in both cases — no net change, but we confirm
    // the value is a valid future date.
    let expected_next = 1_700_000_000u64 + (30 * 86400);
    for &id in &policy_ids {
        let policy = client.get_policy(&id).unwrap();
        assert!(
            policy.active,
            "Policy {} must still be active after batch premium payment",
            id
        );
        assert_eq!(
            policy.next_payment_date, expected_next,
            "Policy {} next_payment_date must equal current_time + 30 days after batch pay",
            id
        );
    }
}

/// Create 200 policies and deactivate 100 of them, then verify that
/// get_active_policies only returns the remaining 100 active ones.
#[test]
fn stress_deactivate_half_of_200_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "DeactPolicy");
    let coverage_type = String::from_str(&env, "life");

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &80i128, &8_000i128);
    }

    // Deactivate even-numbered policies (IDs 2, 4, 6, …, 200)
    for id in (2u32..=200).step_by(2) {
        client.deactivate_policy(&owner, &id);
    }

    // get_active_policies must return only the 100 remaining active ones
    let mut active_count = 0u32;
    let mut cursor = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &50u32);
        active_count += page.count;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(
        active_count, 100,
        "After deactivating 100 of 200 policies, only 100 must be returned by get_active_policies"
    );

    // Verify monthly premium dropped by exactly half: 100 deactivated × 80 = 8000 less
    let remaining_premium = client.get_total_monthly_premium(&owner);
    assert_eq!(
        remaining_premium,
        100 * 80i128,
        "Monthly premium must reflect only the 100 still-active policies"
    );
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Measure CPU and memory cost for get_active_policies — first page of 200.
#[test]
fn bench_get_active_policies_first_page_of_200() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchPolicy");
    let coverage_type = String::from_str(&env, "health");

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
    }

    let (cpu, mem, page) = measure(&env, || client.get_active_policies(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50, "First page must return 50 policies");

    println!(
        r#"{{"contract":"insurance","method":"get_active_policies","scenario":"200_policies_page1_50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost for get_total_monthly_premium with 200 active policies.
#[test]
fn bench_get_total_monthly_premium_200_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "PremBench");
    let coverage_type = String::from_str(&env, "health");

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
    }

    let expected = 200i128 * 100;
    let (cpu, mem, total) = measure(&env, || client.get_total_monthly_premium(&owner));
    assert_eq!(total, expected);

    println!(
        r#"{{"contract":"insurance","method":"get_total_monthly_premium","scenario":"200_active_policies","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost for batch_pay_premiums with 50 policies.
#[test]
fn bench_batch_pay_premiums_50_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BatchBench");
    let coverage_type = String::from_str(&env, "health");

    let mut policy_ids = std::vec![];
    for _ in 0..50 {
        let id = client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128);
        policy_ids.push(id);
    }

    let mut ids_vec = soroban_sdk::Vec::new(&env);
    for &id in &policy_ids {
        ids_vec.push_back(id);
    }

    let (cpu, mem, count) = measure(&env, || client.batch_pay_premiums(&owner, &ids_vec));
    assert_eq!(count, 50);

    println!(
        r#"{{"contract":"insurance","method":"batch_pay_premiums","scenario":"50_policies","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
