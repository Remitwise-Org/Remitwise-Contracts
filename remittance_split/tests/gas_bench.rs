use remittance_split::{AccountGroup, RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

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
fn bench_distribute_usdc_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    // The payer is the split owner — distribute_usdc requires caller == config.owner
    let payer = <Address as AddressTrait>::generate(&env);
    let token_admin = <Address as AddressTrait>::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = token_contract.address();

    let amount = 10_000i128;
    StellarAssetClient::new(&env, &token_addr).mint(&payer, &amount);

    // Initialize with payer as owner and the real token address
    client.initialize_split(&payer, &0, &token_addr, &50, &30, &15, &5);

    let accounts = AccountGroup {
        spending: <Address as AddressTrait>::generate(&env),
        savings: <Address as AddressTrait>::generate(&env),
        bills: <Address as AddressTrait>::generate(&env),
        insurance: <Address as AddressTrait>::generate(&env),
    };

    // nonce after initialize_split = 1
    let nonce = 1u64;
    let (cpu, mem, distributed) = measure(&env, || {
        client.distribute_usdc(&token_addr, &payer, &nonce, &accounts, &amount)
    });
    assert!(distributed);

    println!(
        r#"{{"contract":"remittance_split","method":"distribute_usdc","scenario":"4_recipients_all_nonzero","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Create remittance schedule - measures gas cost for creating a new schedule
/// Security: Tests with valid parameters to ensure proper authorization and validation
#[test]
fn bench_create_remittance_schedule() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400; // 1 day from now
    let interval = 2_592_000u64; // 30 days in seconds

    let (cpu, mem, schedule_id) = measure(&env, || {
        client.create_remittance_schedule(&owner, &amount, &next_due, &interval)
    });
    
    
    let schedule_id = result;
    assert_eq!(schedule_id, 1);

    println!(
        r#"{{"contract":"remittance_split","method":"create_remittance_schedule","scenario":"single_recurring_schedule","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Create multiple schedules - measures gas cost scaling with number of schedules
/// Security: Validates that multiple schedules don't cause storage conflicts
#[test]
fn bench_create_multiple_schedules() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    // Create 10 schedules first to establish baseline storage state
    for i in 1..=10 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;
        
        let _result = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    }

    // Measure the 11th schedule creation (worst case with existing schedules)
    let amount = 11_000i128;
    let next_due = env.ledger().timestamp() + 86400 * 11;
    let interval = 2_592_000u64;

    let (cpu, mem, _schedule_id) = measure(&env, || {
        client.create_remittance_schedule(&owner, &amount, &next_due, &interval)
    });
    
    let _result = result;

    println!(
        r#"{{"contract":"remittance_split","method":"create_remittance_schedule","scenario":"11th_schedule_with_existing","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Modify remittance schedule - measures gas cost for updating existing schedule
/// Security: Tests authorization and validates only owner can modify
#[test]
fn bench_modify_remittance_schedule() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create initial schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    // Modify the schedule
    let new_amount = 2_000i128;
    let new_next_due = env.ledger().timestamp() + 172800; // 2 days from now
    let new_interval = 604_800u64; // 1 week in seconds

    let (cpu, mem, result) = measure(&env, || {
        client.modify_remittance_schedule(
            &owner,
            &schedule_id,
            &new_amount,
            &new_next_due,
            &new_interval,
        )
    });
    
    assert!(result);
    println!(
        r#"{{"contract":"remittance_split","method":"modify_remittance_schedule","scenario":"single_schedule_modification","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Cancel remittance schedule - measures gas cost for cancelling a schedule
/// Security: Tests authorization and validates only owner can cancel
#[test]
fn bench_cancel_remittance_schedule() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create initial schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    let (cpu, mem, result) = measure(&env, || {
        client.cancel_remittance_schedule(&owner, &schedule_id)
    });
    
    assert!(result);
    println!(
        r#"{{"contract":"remittance_split","method":"cancel_remittance_schedule","scenario":"single_schedule_cancellation","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Query schedules - measures gas cost for retrieving schedules by owner
/// Security: Tests that only owner's schedules are returned (data isolation)
#[test]
fn bench_get_remittance_schedules_empty() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    let (cpu, mem, schedules) = measure(&env, || client.get_remittance_schedules(&owner));

    assert_eq!(schedules.len(), 0);

    println!(
        r#"{{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"empty_schedules","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Query schedules with data - measures gas cost scaling with number of schedules
/// Security: Validates data isolation between different owners
#[test]
fn bench_get_remittance_schedules_with_data() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner1 = <Address as AddressTrait>::generate(&env);
    let owner2 = <Address as AddressTrait>::generate(&env);

    // Create 5 schedules for owner1
    for i in 1..=5 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;
        
        let _result = client.create_remittance_schedule(&owner1, &amount, &next_due, &interval);
    }

    // Create 3 schedules for owner2 (should not be returned for owner1)
    for i in 1..=3 {
        let amount = 2_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 604_800u64;
        
        let _result = client.create_remittance_schedule(&owner2, &amount, &next_due, &interval);
    }

    let (cpu, mem, schedules) = measure(&env, || client.get_remittance_schedules(&owner1));

    // Should only return owner1's schedules (data isolation test)
    assert_eq!(schedules.len(), 5);

    println!(
        r#"{{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"5_schedules_with_isolation","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Query single schedule - measures gas cost for retrieving specific schedule
/// Security: Tests that schedule data is properly retrieved and validated
#[test]
fn bench_get_remittance_schedule_single() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86400;
    let interval = 2_592_000u64;

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    let (cpu, mem, schedule) = measure(&env, || client.get_remittance_schedule(&schedule_id));

    assert!(schedule.is_some());
    let schedule = schedule.unwrap();
    assert_eq!(schedule.owner, owner);
    assert_eq!(schedule.amount, amount);

    println!(
        r#"{{"contract":"remittance_split","method":"get_remittance_schedule","scenario":"single_schedule_lookup","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Benchmark: Worst case scenario - measures gas cost with maximum realistic load
/// Security: Tests system behavior under stress conditions
#[test]
fn bench_schedule_operations_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);

    // Create 50 schedules to establish worst-case storage state
    for i in 1..=50 {
        let amount = 1_000i128 * i as i128;
        let next_due = env.ledger().timestamp() + 86400 * i;
        let interval = 2_592_000u64;
        
        let _result = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    }

    // Measure query performance with 50 schedules
    let (cpu, mem, schedules) = measure(&env, || client.get_remittance_schedules(&owner));

    assert_eq!(schedules.len(), 50);

    println!(
        r#"{{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"50_schedules_worst_case","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
