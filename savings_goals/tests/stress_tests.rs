//! Stress tests for savings_goals storage limits and TTL behavior.
//!
//! Issue #178: Stress Test Storage Limits and TTL
//!
//! Coverage:
//!   - Many goals per user (200+) exercising the instance-storage Map
//!   - Many goals across multiple users, verifying per-owner isolation
//!   - Instance TTL re-bump after a ledger advancement that crosses the threshold
//!   - Batch contribution (batch_add_to_goals) at MAX_BATCH_SIZE (50)
//!   - Performance benchmarks (CPU instructions + memory bytes) for key reads
//!
//! Storage layout (savings_goals):
//!   All goals live in one Map<u32, SavingsGoal> inside instance() storage.
//!   INSTANCE_BUMP_AMOUNT        = 518,400 ledgers (~30 days)
//!   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
//!   MAX_PAGE_LIMIT              = 50
//!   DEFAULT_PAGE_LIMIT          = 20
//!   MAX_BATCH_SIZE              = 50
//!   MAX_AUDIT_ENTRIES           = 100

use savings_goals::{ContributionItem, SavingsGoalContract, SavingsGoalContractClient};
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

/// Create 200 goals for a single user and verify full dataset is accessible
/// via both get_all_goals and cursor-based get_goals pagination.
#[test]
fn stress_200_goals_single_user() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "StressGoal");
    let target_date = 2_000_000_000u64;

    for _ in 0..200 {
        client.create_goal(&owner, &name, &1_000i128, &target_date);
    }

    // Verify via get_all_goals (unbounded)
    let all_goals = client.get_all_goals(&owner);
    assert_eq!(all_goals.len(), 200, "get_all_goals must return all 200 goals");

    // Verify via paginated get_goals (MAX_PAGE_LIMIT = 50 → 4 pages)
    let mut collected = 0u32;
    let mut cursor = 0u32;
    let mut pages = 0u32;
    loop {
        let page = client.get_goals(&owner, &cursor, &50u32);
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

    assert_eq!(collected, 200, "Paginated get_goals must return all 200 goals");
    // get_goals sets next_cursor = last_returned_id; when a page is exactly full the
    // caller receives a non-zero cursor that produces a trailing empty page, so the
    // number of round-trips is pages = ceil(200/50) + 1 trailing = 5.
    assert!(pages >= 4 && pages <= 5, "Expected 4-5 pages for 200 goals at limit 50, got {}", pages);
}

/// Create 200 goals and verify instance TTL stays valid after the instance Map
/// grows to 200 entries.
#[test]
fn stress_instance_ttl_valid_after_200_goals() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLGoal");

    for _ in 0..200 {
        client.create_goal(&owner, &name, &500i128, &2_000_000_000u64);
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

/// Create 20 goals each for 10 different users (200 total) and verify per-owner
/// isolation — one user's goals must not appear in another's query.
#[test]
fn stress_goals_across_10_users() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);

    const N_USERS: usize = 10;
    const GOALS_PER_USER: usize = 20;
    let name = String::from_str(&env, "UserGoal");
    let target_date = 2_000_000_000u64;

    let users: std::vec::Vec<Address> = (0..N_USERS).map(|_| Address::generate(&env)).collect();

    for user in &users {
        for _ in 0..GOALS_PER_USER {
            client.create_goal(user, &name, &1_000i128, &target_date);
        }
    }

    for user in &users {
        let goals = client.get_all_goals(user);
        assert_eq!(
            goals.len() as usize,
            GOALS_PER_USER,
            "Each user must see exactly their own {} goals",
            GOALS_PER_USER
        );
    }
}

// ---------------------------------------------------------------------------
// Stress: TTL re-bump after ledger advancement
// ---------------------------------------------------------------------------

/// Verify the instance TTL is re-bumped to >= INSTANCE_BUMP_AMOUNT (518,400)
/// after the ledger advances far enough to drop TTL below the threshold (17,280).
///
/// Phase 1: create 50 goals at sequence 100 → live_until ≈ 518,500
/// Phase 2: advance to sequence 510,000 → TTL ≈ 8,500 (below threshold)
/// Phase 3: create 1 more goal → extend_ttl fires → TTL re-bumped
#[test]
fn stress_ttl_re_bumped_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLStress");

    // Phase 1: 50 creates
    for _ in 0..50 {
        client.create_goal(&owner, &name, &1_000i128, &2_000_000_000u64);
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

    // Phase 3: create_goal fires extend_ttl → re-bumped
    client.create_goal(&owner, &name, &1_000i128, &2_000_000_000u64);

    let ttl_rebumped = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_rebumped >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after create_goal post-advancement",
        ttl_rebumped
    );
}

/// Verify TTL is also re-bumped by add_to_goal after ledger advancement.
#[test]
fn stress_ttl_re_bumped_by_add_to_goal_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let goal_id = client.create_goal(
        &owner,
        &String::from_str(&env, "AddTTL"),
        &10_000i128,
        &2_000_000_000u64,
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

    // add_to_goal must re-bump TTL
    client.add_to_goal(&owner, &goal_id, &500i128);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after add_to_goal post-advancement",
        ttl
    );
}

// ---------------------------------------------------------------------------
// Stress: batch operations at limit
// ---------------------------------------------------------------------------

/// Verify batch_add_to_goals correctly processes MAX_BATCH_SIZE (50) contributions
/// in a single call, updating all goal balances atomically.
#[test]
fn stress_batch_add_to_goals_at_max_batch_size() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    const BATCH_SIZE: u32 = 50; // MAX_BATCH_SIZE
    let target_date = 2_000_000_000u64;

    // Create exactly 50 goals
    let mut goal_ids = std::vec![];
    for _ in 0..BATCH_SIZE {
        let id = client.create_goal(
            &owner,
            &String::from_str(&env, "BatchGoal"),
            &1_000i128,
            &target_date,
        );
        goal_ids.push(id);
    }

    // Build a soroban_sdk::Vec<ContributionItem>
    let mut contributions = soroban_sdk::Vec::new(&env);
    for &id in &goal_ids {
        contributions.push_back(ContributionItem {
            goal_id: id,
            amount: 100i128,
        });
    }

    let processed = client.batch_add_to_goals(&owner, &contributions);
    assert_eq!(
        processed, BATCH_SIZE,
        "batch_add_to_goals must process all {} contributions",
        BATCH_SIZE
    );

    // Each goal should now have current_amount = 100
    for &id in &goal_ids {
        let goal = client.get_goal(&id).unwrap();
        assert_eq!(
            goal.current_amount, 100,
            "Goal {} must have current_amount = 100 after batch add",
            id
        );
    }
}

/// Verify data persists across repeated ledger advancements (TTL continuously
/// renewed by write operations interspersed between ledger jumps).
#[test]
fn stress_data_persists_across_multiple_ledger_advancements() {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    // High min_persistent_entry_ttl to keep data alive through jumps
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });
    env.budget().reset_unlimited();

    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    // Phase 1: create 30 goals at sequence 100
    for _ in 0..30 {
        client.create_goal(
            &owner,
            &String::from_str(&env, "Phase1"),
            &1_000i128,
            &2_000_000_000u64,
        );
    }
    assert_eq!(client.get_all_goals(&owner).len(), 30);

    // Phase 2: advance to sequence 510,000 and create 20 more
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 510_000,
        timestamp: 1_703_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });
    for _ in 0..20 {
        client.create_goal(
            &owner,
            &String::from_str(&env, "Phase2"),
            &2_000i128,
            &2_100_000_000u64,
        );
    }
    assert_eq!(
        client.get_all_goals(&owner).len(),
        50,
        "Both phases of goals must be present after first ledger jump"
    );

    // Phase 3: advance to sequence 1,020,000 — all 50 goals still accessible
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 1_020_000,
        timestamp: 1_706_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1_100_000,
        max_entry_ttl: 1_200_000,
    });
    let all = client.get_all_goals(&owner);
    assert_eq!(
        all.len(),
        50,
        "All 50 goals must persist across multiple ledger advancements"
    );

    // TTL must still be positive
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(ttl > 0, "Instance TTL must be > 0 after all ledger advancements");
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Measure CPU and memory cost for get_all_goals with 200 goals (unbounded scan).
#[test]
fn bench_get_all_goals_200_goals() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchGoal");
    for _ in 0..200 {
        client.create_goal(&owner, &name, &1_000i128, &1_800_000_000u64);
    }

    let (cpu, mem, goals) = measure(&env, || client.get_all_goals(&owner));
    assert_eq!(goals.len(), 200);

    println!(
        r#"{{"contract":"savings_goals","method":"get_all_goals","scenario":"200_goals_single_owner","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost for get_goals (paginated) — first page of 200.
#[test]
fn bench_get_goals_first_page_of_200() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchPageGoal");
    for _ in 0..200 {
        client.create_goal(&owner, &name, &1_000i128, &1_800_000_000u64);
    }

    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"200_goals_page1_50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost for batch_add_to_goals with 50 contributions.
#[test]
fn bench_batch_add_to_goals_50_contributions() {
    let env = stress_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let mut goal_ids = std::vec![];
    for _ in 0..50 {
        let id = client.create_goal(
            &owner,
            &String::from_str(&env, "BatchBench"),
            &10_000i128,
            &2_000_000_000u64,
        );
        goal_ids.push(id);
    }

    let mut contributions = soroban_sdk::Vec::new(&env);
    for &id in &goal_ids {
        contributions.push_back(ContributionItem {
            goal_id: id,
            amount: 200i128,
        });
    }

    let (cpu, mem, processed) = measure(&env, || client.batch_add_to_goals(&owner, &contributions));
    assert_eq!(processed, 50);

    println!(
        r#"{{"contract":"savings_goals","method":"batch_add_to_goals","scenario":"50_contributions","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
