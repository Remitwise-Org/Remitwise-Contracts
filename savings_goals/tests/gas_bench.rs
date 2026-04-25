use savings_goals::{
    ContributionItem, SavingsGoalContract, SavingsGoalContractClient, MAX_PAGE_LIMIT,
};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String, Vec};

fn bench_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    let mut budget = env.budget();
    budget.reset_unlimited();
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

#[test]
fn bench_get_all_goals_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "BenchGoal");
    for _ in 0..100 {
        client.create_goal(&owner, &name, &1_000i128, &1_800_000u64);
    }

    let (cpu, mem, goals) = measure(&env, || client.get_all_goals(&owner));
    assert_eq!(goals.len(), 100);

    println!(
        r#"{{"contract":"savings_goals","method":"get_all_goals","scenario":"100_goals_single_owner","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_batch_add_to_goals_max() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "BatchGoal");
    let mut contributions = Vec::new(&env);

    // Create 50 goals and prepare contributions
    for _ in 0..50 {
        let goal_id = client.create_goal(&owner, &name, &10_000i128, &1_800_000u64);
        contributions.push_back(ContributionItem {
            goal_id,
            amount: 100,
        });
    }

    let (cpu, mem, count) = measure(&env, || client.batch_add_to_goals(&owner, &contributions));
    assert_eq!(count, 50);

    println!(
        r#"{{"contract":"savings_goals","method":"batch_add_to_goals","scenario":"50_items","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_execute_due_savings_schedules() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "ScheduleGoal");
    let goal_id = client.create_goal(&owner, &name, &100_000i128, &1_800_000u64);

    // Create 50 schedules
    let current_time = 1_700_000_000;
    let next_due = current_time + 10;
    for _ in 0..50 {
        client.create_savings_schedule(&owner, &goal_id, &100i128, &next_due, &86400u64);
    }

    // Advance time so schedules are due
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 2,
        timestamp: current_time + 100,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });

    let (cpu, mem, executed) = measure(&env, || client.execute_due_savings_schedules());
    assert_eq!(executed.len(), 50);

    println!(
        r#"{{"contract":"savings_goals","method":"execute_due_savings_schedules","scenario":"50_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_create_savings_schedule() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    let name = String::from_str(&env, "ScheduleGoal");
    let goal_id = client.create_goal(&owner, &name, &10_000i128, &1_800_000u64);

    let current_time = 1_700_000_000;
    let next_due = current_time + 10;

    let (cpu, mem, _) = measure(&env, || {
        client.create_savings_schedule(&owner, &goal_id, &100i128, &next_due, &86400u64)
    });

    println!(
        r#"{{"contract":"savings_goals","method":"create_savings_schedule","scenario":"single_schedule","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

// ---------------------------------------------------------------------------
// Paginated goal listing benchmarks
//
// `get_goals` loads the full goals map from instance storage on every call,
// so both first-page and last-page costs grow with the total number of goals
// owned by the caller. These benchmarks document that scaling behaviour and
// flag regressions if the pagination index or storage layout changes.
//
// Archive queries are not benchmarked here because the savings_goals contract
// does not yet expose an archive function. When archival is added a separate
// bench_get_archived_goals_page_* suite should be added alongside it.
// ---------------------------------------------------------------------------

fn setup_goals(client: &SavingsGoalContractClient, owner: &Address, n: u32) {
    let name = String::from_str(&client.env, "G");
    for _ in 0..n {
        client.create_goal(owner, &name, &10_000i128, &1_800_000u64);
    }
}

/// First-page fetch with 50 goals in the store (no cursor scan overhead).
#[test]
fn bench_get_goals_page_first_n50() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 50);

    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &0, &MAX_PAGE_LIMIT));
    assert_eq!(page.count, MAX_PAGE_LIMIT);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"first_page_n50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// First-page fetch with 200 goals in the store.
#[test]
fn bench_get_goals_page_first_n200() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 200);

    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &0, &MAX_PAGE_LIMIT));
    assert_eq!(page.count, MAX_PAGE_LIMIT);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"first_page_n200","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// First-page fetch with 1000 goals in the store.
#[test]
fn bench_get_goals_page_first_n1000() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 1000);

    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &0, &MAX_PAGE_LIMIT));
    assert_eq!(page.count, MAX_PAGE_LIMIT);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"first_page_n1000","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Last-page fetch with 50 goals — worst case for the linear cursor scan.
#[test]
fn bench_get_goals_page_last_n50() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 50);

    // Walk to the last page using the cursor returned by each prior page.
    let mut cursor = 0u32;
    let mut last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    while last_page.next_cursor != 0 {
        cursor = last_page.next_cursor;
        last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    }
    // Measure cost of fetching that final page.
    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT));
    assert!(page.count > 0);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"last_page_n50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Last-page fetch with 200 goals.
#[test]
fn bench_get_goals_page_last_n200() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 200);

    let mut cursor = 0u32;
    let mut last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    while last_page.next_cursor != 0 {
        cursor = last_page.next_cursor;
        last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    }
    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT));
    assert!(page.count > 0);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"last_page_n200","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Last-page fetch with 1000 goals — maximum-scale worst case.
#[test]
fn bench_get_goals_page_last_n1000() {
    let env = bench_env();
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &contract_id);
    let owner = <Address as AddressTrait>::generate(&env);

    setup_goals(&client, &owner, 1000);

    let mut cursor = 0u32;
    let mut last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    while last_page.next_cursor != 0 {
        cursor = last_page.next_cursor;
        last_page = client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT);
    }
    let (cpu, mem, page) = measure(&env, || client.get_goals(&owner, &cursor, &MAX_PAGE_LIMIT));
    assert!(page.count > 0);

    println!(
        r#"{{"contract":"savings_goals","method":"get_goals","scenario":"last_page_n1000","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
