//! Stress tests for insurance storage limits and TTL behavior.

use insurance::{Insurance, InsuranceClient};
use remitwise_common::CoverageType;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

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

/// Create 200 policies for a single user and verify full dataset is returned
/// by get_active_policies (returns all active policies).
#[test]
fn stress_200_policies_single_user() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "StressPolicy");
    let coverage_type = CoverageType::Health;

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
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

    assert_eq!(
        collected, 200,
        "Pagination must return all 200 active policies"
    );
    // get_active_policies sets next_cursor = last_returned_id; when a page is exactly
    // full the caller receives a non-zero cursor that produces a trailing empty page,
    // so the round-trip count is pages = ceil(200/50) + 1 trailing = 5.
    assert!(
        (4..=5).contains(&pages),
        "Expected 4-5 pages for 200 policies at limit 50, got {}",
        pages
    );
}

/// Contract test for PolicyPage semantics: stable ID ordering, cursor progression,
/// inactive exclusion, and duplicate-free pagination across pages.
#[test]
fn contract_policy_page_ordering_and_cursor_correctness() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "ContractPolicy");
    let coverage_type = String::from_str(&env, "health");

    let mut created_ids = std::vec::Vec::new();
    for _ in 0..6 {
        let id = client.create_policy(&owner, &name, &coverage_type, &120i128, &12_000i128);
        created_ids.push(id);
    }

    // Make dataset mixed active/inactive.
    client.deactivate_policy(&owner, &created_ids[1]); // id #2
    client.deactivate_policy(&owner, &created_ids[4]); // id #5

    let expected_active_ids = std::vec![
        created_ids[0],
        created_ids[2],
        created_ids[3],
        created_ids[5],
    ];

    let mut cursor = 0u32;
    let mut seen_ids = std::vec::Vec::new();
    let mut ended = false;

    loop {
        let page = client.get_active_policies(&owner, &cursor, &2u32);
        assert!(page.count <= 2, "page count must obey the requested limit");

        for policy in page.items.iter() {
            assert!(policy.active, "inactive policy must never be returned");
            assert_eq!(policy.owner, owner, "page must be owner-scoped");
            seen_ids.push(policy.id);
        }

        if page.next_cursor == 0 {
            ended = true;
            break;
        }

        assert!(page.count > 0, "non-terminal pages must contain at least one item");
        let last_index = page.count - 1;
        let last_policy_id = page.items.get(last_index).unwrap().id;
        assert_eq!(
            page.next_cursor, last_policy_id,
            "next_cursor must equal the last returned policy id"
        );
        assert!(
            page.next_cursor > cursor,
            "next_cursor must advance monotonically between pages"
        );
        cursor = page.next_cursor;
    }

    assert!(ended, "paging must terminate with next_cursor == 0");
    assert_eq!(
        seen_ids, expected_active_ids,
        "active policies must be ordered canonically by ascending policy id"
    );
    assert!(
        !seen_ids.contains(&created_ids[1]) && !seen_ids.contains(&created_ids[4]),
        "inactive policies must be excluded from all pages"
    );

    let mut deduped = seen_ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        seen_ids.len(),
        "policy IDs must not duplicate across paginated responses"
    );
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
    let coverage_type = CoverageType::Life;

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &50i128, &5_000i128, &None);
    }

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must remain >= INSTANCE_BUMP_AMOUNT (518,400) after 200 creates",
        ttl
    );
}

/// Create 20 policies each for 10 different users (200 total) and verify
/// per-owner isolation.
#[test]
fn stress_policies_across_10_users() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    const N_USERS: usize = 10;
    const POLICIES_PER_USER: u32 = 20;
    const PREMIUM_PER_POLICY: i128 = 150;
    let name = String::from_str(&env, "UserPolicy");
    let coverage_type = CoverageType::Health;

    let users: std::vec::Vec<Address> = (0..N_USERS).map(|_| Address::generate(&env)).collect();

    for user in &users {
        for _ in 0..POLICIES_PER_USER {
            client.create_policy(
                user,
                &name,
                &CoverageType::Health,
                &PREMIUM_PER_POLICY,
                &50_000i128,
                &None,
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

        let page = client.get_active_policies(user, &0u32, &50u32);
        let active = page.items;
        assert_eq!(
            active.len(),
            POLICIES_PER_USER,
            "Each user must see exactly their own {} policies",
            POLICIES_PER_USER
        );
    }
}

/// Verify the instance TTL is re-bumped after ledger advancement.
#[test]
fn stress_ttl_re_bumped_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLStress");
    let coverage_type = CoverageType::Health;

    // Phase 1: 50 creates
    for _ in 0..50 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
    }

    let ttl_batch1 = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_batch1 >= 518_400,
        "TTL ({}) must be >= 518,400 after first batch of creates",
        ttl_batch1
    );

    // Phase 2: advance ledger so TTL drops below threshold
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
    client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);

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
        &CoverageType::Health,
        &200i128,
        &20_000i128,
        &None,
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
    client.pay_premium(&owner, &policy_id);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after pay_premium post-advancement",
        ttl
    );
}

/// Create 50 policies and pay all premiums in a single batch.
#[test]
fn stress_batch_pay_premiums_at_max_batch_size() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    const BATCH_SIZE: u32 = 50;
    let name = String::from_str(&env, "BatchPolicy");
    let coverage_type = CoverageType::Health;

    let mut policy_ids = std::vec![];
    for _ in 0..BATCH_SIZE {
        let id = client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
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

/// Create 200 policies and deactivate 100, verify only 100 remain active.
#[test]
fn stress_deactivate_half_of_200_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "DeactPolicy");
    let coverage_type = CoverageType::Life;

    let mut all_ids = std::vec![];
    for _ in 0..200 {
        let id = client.create_policy(&owner, &name, &coverage_type, &80i128, &8_000i128, &None);
        all_ids.push(id);
    }

    // Deactivate even-indexed policies
    for (i, &id) in all_ids.iter().enumerate() {
        if i % 2 == 1 {
            client.deactivate_policy(&owner, &id);
        }
    }

    // Count all active policies via pagination.
    let mut collected = 0u32;
    let mut cursor = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &50u32);
        collected += page.count;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    assert_eq!(
        collected, 100,
        "After deactivating 100 of 200 policies, only 100 must remain active"
    );

    let remaining_premium = client.get_total_monthly_premium(&owner);
    assert_eq!(
        remaining_premium,
        100 * 80i128,
        "Monthly premium must reflect only the 100 still-active policies"
    );
}

/// Measure CPU and memory cost for get_active_policies with 200 policies.
#[test]
fn bench_get_active_policies_200_policies() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchPolicy");
    let coverage_type = CoverageType::Health;

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
    }

    let (cpu, mem, active) = measure(&env, || client.get_active_policies(&owner, &0u32, &50u32));
    assert_eq!(active.items.len(), 50, "Must return first page (limit 50)");

    println!(
        r#"{{"contract":"insurance","method":"get_active_policies","scenario":"200_policies","cpu":{},"mem":{}}}"#,
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
    let coverage_type = CoverageType::Health;

    for _ in 0..200 {
        client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
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
    let coverage_type = CoverageType::Health;

    let mut policy_ids = std::vec![];
    for _ in 0..50 {
        let id = client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
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

#[test]
fn stress_batch_pay_mixed_states() {
    let env = stress_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "MixedBatch");
    let coverage_type = CoverageType::Health;

    let mut policy_ids = std::vec![];
    for i in 0..50 {
        if i % 2 == 0 {
            // Valid policy
            let id =
                client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
            policy_ids.push(id);
        } else {
            // Invalid policy: deactivated
            let id =
                client.create_policy(&owner, &name, &coverage_type, &100i128, &10_000i128, &None);
            client.deactivate_policy(&owner, &id);
            policy_ids.push(id);
        }
    }

    let mut ids_vec = soroban_sdk::Vec::new(&env);
    for &id in &policy_ids {
        ids_vec.push_back(id);
    }

    let (cpu, mem, count) = measure(&env, || client.batch_pay_premiums(&owner, &ids_vec));
    assert_eq!(count, 25, "Exactly 25 policies should be paid");

    println!(
        r#"{{"contract":"insurance","method":"batch_pay_premiums","scenario":"50_policies_mixed","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
