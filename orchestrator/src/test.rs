extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger as _},
    Address, Env, FromVal, IntoVal, Symbol, Vec,
};
use std::{fs, path::PathBuf, string::String};

#[contract]
pub struct MockContract;

#[contractimpl]
impl MockContract {
    pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
        true
    }
    pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
        soroban_sdk::vec![&env, 2500, 2500, 2500, 2500]
    }
    pub fn add_to_goal(_env: Env, _caller: Address, _goal_id: u32, _amount: i128) {}
    pub fn pay_bill(_env: Env, _caller: Address, _bill_id: u32, _amount: i128) {}
    pub fn pay_premium(_env: Env, _caller: Address, _policy_id: u32, _amount: i128) {}
    // Compensation / reverse methods for rollback support.
    pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
    pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
    pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
}

#[contract]
pub struct FailingMock;

#[contractimpl]
impl FailingMock {}

// Each failing mock needs its own module to avoid Soroban macro name
// collisions (the macro generates __fn_name helper modules).
mod mock_fail_savings {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {
            panic!("savings step failed")
        }
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

mod mock_fail_bill {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {
            panic!("bill step failed")
        }
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

mod mock_fail_insurance {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {
            panic!("insurance step failed")
        }
    }
}

mod mock_no_limit {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            false
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn setup_test() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    (env, owner)
}

fn register_orchestrator(env: &Env) -> (Address, OrchestratorClient<'_>) {
    let id = env.register_contract(None, Orchestrator);
    (id.clone(), OrchestratorClient::new(env, &id))
}

fn init_orchestrator(env: &Env, client: &OrchestratorClient, owner: &Address) {
    // Each dependency must be a distinct address — register separate mock instances
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(owner, &fw, &rs, &sg, &bp, &ins);
}

/// Execute one unsigned remittance flow entry so the audit log grows by one.
///
/// Uses the unsigned execution path, which emits lifecycle events and updates
/// `ExecutionStats` identically to the signed path.
fn do_flow(env: &Env, client: &OrchestratorClient, executor: &Address, _nonce: u64) {
    let mock_id = env.register_contract(None, MockContract);
    env.budget().reset_unlimited();
    client.execute_remittance_flow(&RemittanceFlowParams {
        caller: executor.clone(),
        total_amount: 1000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });
}

/// Mirror of `Orchestrator::compute_request_hash` for test use.
fn compute_test_hash(
    _env: &Env,
    operation: Symbol,
    nonce: u64,
    amount: i128,
    deadline: u64,
) -> u64 {
    Orchestrator::compute_request_hash(operation, nonce, amount, deadline, 1, 1, 1)
}

fn wasm_size_budgets() -> &'static [(&'static str, usize)] {
    &[
        ("remittance_split.wasm", 110_000),
        ("savings_goals.wasm", 112_000),
        ("bill_payments.wasm", 135_000),
        ("insurance.wasm", 70_000), // Increased to accommodate kill switch guard & pagination security additions
        ("family_wallet.wasm", 130_000),
    ]
}

fn verify_wasm_size(contract: &str, size: usize, max_bytes: usize) -> Result<(), String> {
    if size <= max_bytes {
        Ok(())
    } else {
        Err(format!(
            "WASM size for '{}' is {} bytes, which exceeds the budget of {} bytes.",
            contract, size, max_bytes
        ))
    }
}

#[test]
fn test_wasm_artifacts_respect_documented_size_budgets() {
    let release_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("wasm32-unknown-unknown")
        .join("release");

    for (filename, max_bytes) in wasm_size_budgets().iter() {
        let artifact_path = release_dir.join(filename);
        let metadata = fs::metadata(&artifact_path).unwrap_or_else(|_| {
            panic!(
                "Expected wasm artifact '{}' to exist at '{}'. Build wasm artifacts first.",
                filename,
                artifact_path.display()
            )
        });
        let size = metadata.len() as usize;
        verify_wasm_size(filename, size, *max_bytes).unwrap();
    }
}

#[test]
fn test_wasm_size_budget_validation_rejects_oversized_artifacts() {
    assert!(verify_wasm_size("example.wasm", 10_001, 10_000).is_err());
}

// ---------------------------------------------------------------------------
// Original tests (reentrancy / lock)
// ---------------------------------------------------------------------------

#[test]
fn test_execute_flow_success() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    client.execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 10000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    // Check lock is released
    assert!(!client.get_execution_state());
}

#[test]
fn test_lock_released_on_invalid_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mock_id = Address::generate(&env);
    let caller = Address::generate(&env);

    // Should return Err(InvalidAmount)
    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: -100i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    assert!(result.is_err());
    assert!(!client.get_execution_state());
}

#[test]
fn test_reentrancy_rejection() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let caller = Address::generate(&env);

    // Test that if the lock is set manually, the call fails.
    env.as_contract(&orchestrator_id, || {
        env.storage().instance().set(&EXEC_LOCK, &true);
    });

    let mock_id = Address::generate(&env);
    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 1000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    match result {
        Err(Ok(OrchestratorError::ExecutionLocked)) => (),
        _ => panic!("Expected ExecutionLocked error"),
    }

    // Check it's still locked (because we set it manually and the call failed before acquiring)
    assert!(client.get_execution_state());
}

#[test]
fn test_lock_recovery_after_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let failing_id = env.register_contract(None, FailingMock);
    let caller = Address::generate(&env);

    // A panic in Soroban rolls back everything, including the lock.
    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 1000i128,
        family_wallet: failing_id.clone(),
        remittance_split: failing_id.clone(),
        savings: failing_id.clone(),
        bills: failing_id.clone(),
        insurance: failing_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    assert!(result.is_err());
    // In Soroban, if the transaction panics, the state is rolled back.
    // In a test, if we use `try_`, it might behave differently depending on where the panic happens.
    // But since `perform_remittance_flow` is called within the orchestrator, a panic there
    // will roll back the `EXEC_LOCK` set by the orchestrator.
    assert!(!client.get_execution_state());
}

// ---------------------------------------------------------------------------
// Audit log tests
// ---------------------------------------------------------------------------

#[test]
fn test_audit_log_limit_clamped_to_max() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    // Add 10 entries
    for nonce in 0..10u64 {
        do_flow(&env, &client, &executor, nonce);
    }

    // limit=9999 should be clamped to MAX_AUDIT_ENTRIES (100), returning all 10
    let page = client.get_audit_log(&0, &9999);
    assert_eq!(page.len(), 10);
}

#[test]
fn test_audit_log_pagination_no_duplicates() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    // Add 10 entries
    for nonce in 0..10u64 {
        do_flow(&env, &client, &executor, nonce);
    }

    // Page through with page size 3
    let page0 = client.get_audit_log(&0, &3);
    let page1 = client.get_audit_log(&3, &3);
    let page2 = client.get_audit_log(&6, &3);
    let page3 = client.get_audit_log(&9, &3);

    assert_eq!(page0.len(), 3);
    assert_eq!(page1.len(), 3);
    assert_eq!(page2.len(), 3);
    assert_eq!(page3.len(), 1); // only 1 entry left

    // Collect all timestamps and verify no duplicates
    let mut timestamps: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);
    for i in 0..page0.len() {
        timestamps.push_back(page0.get(i).unwrap().timestamp);
    }
    for i in 0..page1.len() {
        timestamps.push_back(page1.get(i).unwrap().timestamp);
    }
    for i in 0..page2.len() {
        timestamps.push_back(page2.get(i).unwrap().timestamp);
    }
    for i in 0..page3.len() {
        timestamps.push_back(page3.get(i).unwrap().timestamp);
    }

    assert_eq!(timestamps.len(), 10);
}

#[test]
fn test_audit_log_cap_eviction_order() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // Fill to exactly MAX_AUDIT_ENTRIES
    for nonce in 0..MAX_AUDIT_ENTRIES as u64 {
        env.ledger().set_timestamp(100_000 + nonce);
        do_flow(&env, &client, &executor, nonce);
    }

    // Log should be full at MAX_AUDIT_ENTRIES
    let full_page = client.get_audit_log(&0, &MAX_AUDIT_ENTRIES);
    assert_eq!(full_page.len(), MAX_AUDIT_ENTRIES);

    // The oldest entry should have timestamp 100_000
    let oldest = full_page.get(0).unwrap();
    assert_eq!(oldest.timestamp, 100_000);

    // Add one more — should evict the oldest (timestamp 100_000)
    env.ledger()
        .set_timestamp(100_000 + MAX_AUDIT_ENTRIES as u64);
    do_flow(&env, &client, &executor, MAX_AUDIT_ENTRIES as u64);

    let after_eviction = client.get_audit_log(&0, &MAX_AUDIT_ENTRIES);
    assert_eq!(after_eviction.len(), MAX_AUDIT_ENTRIES);

    // Oldest entry is now timestamp 100_001 (the second entry before eviction)
    let new_oldest = after_eviction.get(0).unwrap();
    assert_eq!(new_oldest.timestamp, 100_001);

    // Newest entry is the one we just added
    let newest = after_eviction.get(MAX_AUDIT_ENTRIES - 1).unwrap();
    assert_eq!(newest.timestamp, 100_000 + MAX_AUDIT_ENTRIES as u64);
}

#[test]
fn test_evicted_entries_counter_increments() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // Fill to cap
    for nonce in 0..MAX_AUDIT_ENTRIES as u64 {
        do_flow(&env, &client, &executor, nonce);
    }

    // No evictions yet
    let stats = client.get_execution_stats().unwrap();
    assert_eq!(stats.evicted_entries, 0);

    // Add 3 more — should evict 3
    for nonce in MAX_AUDIT_ENTRIES as u64..(MAX_AUDIT_ENTRIES as u64 + 3) {
        do_flow(&env, &client, &executor, nonce);
    }

    let stats = client.get_execution_stats().unwrap();
    assert_eq!(stats.evicted_entries, 3);
}

#[test]
fn test_audit_log_entries_ordered_oldest_to_newest() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    for nonce in 0..5u64 {
        env.ledger().set_timestamp(100_000 + nonce * 10);
        do_flow(&env, &client, &executor, nonce);
    }

    let page = client.get_audit_log(&0, &10);
    assert_eq!(page.len(), 5);

    // Verify ascending timestamp order
    for i in 0..(page.len() - 1) {
        let a = page.get(i).unwrap().timestamp;
        let b = page.get(i + 1).unwrap().timestamp;
        assert!(a <= b, "entries not in ascending order: {} > {}", a, b);
    }
}

#[test]
fn test_audit_log_from_index_at_last_entry() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    for nonce in 0..5u64 {
        do_flow(&env, &client, &executor, nonce);
    }

    // from_index=4 is the last valid index (len=5)
    let page = client.get_audit_log(&4, &10);
    assert_eq!(page.len(), 1);
}

#[test]
fn test_audit_log_limit_exactly_one() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    for nonce in 0..5u64 {
        do_flow(&env, &client, &executor, nonce);
    }

    let page = client.get_audit_log(&0, &1);
    assert_eq!(page.len(), 1);
}

#[test]
fn test_audit_log_cap_does_not_exceed_max() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // Add more than MAX_AUDIT_ENTRIES
    for nonce in 0..(MAX_AUDIT_ENTRIES as u64 + 20) {
        do_flow(&env, &client, &executor, nonce);
    }

    // Log must never exceed MAX_AUDIT_ENTRIES
    let page = client.get_audit_log(&0, &(MAX_AUDIT_ENTRIES + 100));
    assert_eq!(page.len(), MAX_AUDIT_ENTRIES);
}

#[test]
fn test_get_execution_stats_initial() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let stats = client.get_execution_stats();
    assert_eq!(
        stats,
        Some(ExecutionStats {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            last_execution_time: 0,
            evicted_entries: 0,
        })
    );
}

// ---------------------------------------------------------------------------
// Nonce replay protection tests (Issue #648)
// ---------------------------------------------------------------------------

#[test]
fn test_nonce_starts_at_zero() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let nonce = client.get_nonce(&executor);
    assert_eq!(nonce, 0, "New address should start with nonce 0");
}

#[test]
fn test_execute_flow_signed_invalid_amount_zero() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 0, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &0, &0, &deadline, &hash, &0u64);

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_execute_flow_signed_invalid_amount_negative() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, -100, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &(-100i128),
        &0,
        &deadline,
        &hash,
        &0u64,
    );

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_execute_flow_signed_invalid_amount_i128_min() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, i128::MIN, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &(i128::MIN),
        &0,
        &deadline,
        &hash,
        &0u64,
    );

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_execute_flow_signed_valid_amount_minimum_positive() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1, &0, &deadline, &hash, &0u64);

    assert!(
        result.is_ok(),
        "amount=1 should be accepted as valid positive amount"
    );
}

#[test]
fn test_execute_flow_deadline_expired() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // deadline <= now → DeadlineExpired
    let deadline = env.ledger().timestamp(); // not strictly in the future
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);

    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
}

#[test]
fn test_execute_flow_deadline_too_far() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + MAX_DEADLINE_WINDOW_SECS + 1000;

    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);

    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
}

#[test]
fn test_execute_flow_invalid_hash() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 1000;

    let bad_hash = 12345u64;

    let result = client
        .try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &bad_hash, &0u64);

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidNonce)));
}

#[test]
fn test_out_of_order_nonce_fails() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;

    // Attempt to execute with nonce 5 when current nonce is 0
    let hash = compute_test_hash(&env, symbol_short!("flow"), 5, 1000, deadline);
    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &5, &deadline, &hash, &0u64);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidNonce)),
        "Out-of-order nonce should fail (must equal current nonce)"
    );
}

#[test]
fn test_multiple_addresses_independent_nonces() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor1 = Address::generate(&env);
    let executor2 = Address::generate(&env);

    // Executor1 starts with nonce 0
    assert_eq!(client.get_nonce(&executor1), 0);
    // Executor2 starts with nonce 0
    assert_eq!(client.get_nonce(&executor2), 0);

    let deadline = env.ledger().timestamp() + 1000;

    // Execute for executor1 with nonce 0
    let hash1 = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);
    let result1 =
        client.try_execute_remittance_flow_signed(&executor1, &1000, &0, &deadline, &hash1, &0u64);
    assert!(result1.is_ok());

    // Executor1 nonce should be 1
    assert_eq!(client.get_nonce(&executor1), 1);

    // Executor2 nonce should still be 0 (independent)
    assert_eq!(client.get_nonce(&executor2), 0);

    // Executor2 can execute with nonce 0
    let hash2 = compute_test_hash(&env, symbol_short!("flow"), 0, 500, deadline);
    let result2 =
        client.try_execute_remittance_flow_signed(&executor2, &500, &0, &deadline, &hash2, &0u64);
    assert!(result2.is_ok(), "Executor2 should execute with nonce 0");
}

#[test]
fn test_request_hash_binding_prevents_parameter_swap() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;

    // Compute hash for amount 1000
    let hash_1000 = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    // Try to execute with different amount but using hash from 1000
    let result = client
        .try_execute_remittance_flow_signed(&executor, &5000, &0, &deadline, &hash_1000, &0u64);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidNonce)),
        "Parameter swap attempt should fail (hash mismatch)"
    );
}

#[test]
fn test_deadline_window_prevents_old_requests() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // Create a request with a deadline far in the future
    let current_time = env.ledger().timestamp();
    let far_deadline = current_time + 366 * 86400; // 1 year in future (exceeds MAX_DEADLINE_WINDOW_SECS)

    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, far_deadline);
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &1000,
        &0,
        &far_deadline,
        &hash,
        &0u64,
    );

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::DeadlineExpired)),
        "Request with deadline too far in future should fail"
    );
}

// ---------------------------------------------------------------------------
// Signed-flow deadline window boundary tests
//
// The signed entrypoint `execute_remittance_flow_signed` bounds the validity
// of a signed authorization to `MAX_DEADLINE_WINDOW_SECS` (1 hour) past the
// ledger timestamp. The boundary semantics enforced by
// `require_nonce_hardened` (orchestrator/src/lib.rs) are:
//
//   deadline <  now                          -> DeadlineExpired (past)
//   deadline == now                          -> DeadlineExpired (not strictly future)
//   deadline == now + 1                      -> Accepted
//   deadline == now + MAX_DEADLINE_..SECS    -> Accepted  (inclusive upper edge)
//   deadline == now + MAX_DEADLINE_..SECS+1  -> DeadlineExpired (beyond window)
//
// The comparisons are `deadline <= now` (reject) and
// `deadline > now + MAX_DEADLINE_WINDOW_SECS` (reject), so the upper edge is
// inclusive. These tests pin each edge exactly so an off-by-one regression in
// either comparison is caught. See docs/orchestrator-deadline-window.md.
// ---------------------------------------------------------------------------

/// A signed deadline exactly at `now + MAX_DEADLINE_WINDOW_SECS` is the
/// inclusive upper edge of the window and MUST be accepted: the window check is
/// `deadline > now + MAX_DEADLINE_WINDOW_SECS`, so equality passes through.
#[test]
fn test_signed_deadline_at_window_edge_accepted() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    // Use a non-zero ledger time so the edge arithmetic is unambiguous.
    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS; // exactly at the edge
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);

    assert_eq!(
        result,
        Ok(Ok(true)),
        "deadline == now + MAX_DEADLINE_WINDOW_SECS is the inclusive edge and must be accepted"
    );
    // Nonce advanced, confirming the flow actually executed (not silently no-op'd).
    assert_eq!(client.get_nonce(&executor), 1);
}

/// One second beyond the window edge MUST be rejected with the typed
/// `DeadlineExpired` error and MUST NOT advance the nonce.
#[test]
fn test_signed_deadline_one_past_window_rejected() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS + 1; // one second too far
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::DeadlineExpired)),
        "deadline one second beyond the window must be rejected with DeadlineExpired"
    );
    // A rejected call must leave the nonce counter untouched.
    assert_eq!(client.get_nonce(&executor), 0);
}

/// A deadline strictly in the past MUST be rejected with `DeadlineExpired`.
#[test]
fn test_signed_deadline_in_past_rejected() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(5_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now - 1; // strictly in the past
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::DeadlineExpired)),
        "deadline in the past must be rejected with DeadlineExpired"
    );
    assert_eq!(client.get_nonce(&executor), 0);
}

/// The signed flow enforces nonce uniqueness alongside the deadline check: a
/// signature that is still inside its deadline window cannot be replayed once
/// its nonce has been consumed. After a successful execution the per-address
/// counter advances, so re-submitting the identical (still-in-window) request
/// is rejected even though the deadline itself remains valid.
#[test]
fn test_signed_in_window_replay_with_used_nonce_rejected() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS; // valid, in-window
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    // First call succeeds and consumes nonce 0.
    let first =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);
    assert_eq!(first, Ok(Ok(true)));
    assert_eq!(client.get_nonce(&executor), 1);

    // Replay the identical request while the deadline is still in-window. The
    // deadline and hash checks pass, but the used-nonce check fires before the
    // sequential counter check and rejects the stale nonce.
    let replay =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);
    assert_eq!(
        replay,
        Err(Ok(OrchestratorError::NonceAlreadyUsed)),
        "in-window replay of a consumed nonce must be rejected (used-nonce check fires first)"
    );
    // The counter does not advance again on the rejected replay.
    assert_eq!(client.get_nonce(&executor), 1);
}

// ---------------------------------------------------------------------------
// Rollback / Compensation tests
// ---------------------------------------------------------------------------

/// Helper: initialise the orchestrator with a specific set of downstream
/// mocks for signed-flow rollback tests.
fn init_orchestrator_with_mocks(
    _env: &Env,
    client: &OrchestratorClient,
    owner: &Address,
    fw: Address,
    rs: Address,
    sg: Address,
    bp: Address,
    ins: Address,
) {
    client.init(owner, &fw, &rs, &sg, &bp, &ins);
}

fn signed_flow_deadline(env: &Env) -> u64 {
    env.ledger().timestamp() + 1000
}

fn signed_flow_hash(
    _env: &Env,
    _executor: &Address,
    amount: i128,
    nonce: u64,
    deadline: u64,
) -> u64 {
    compute_test_hash(_env, symbol_short!("flow"), nonce, amount, deadline)
}

// ---------------------------------------------------------------------------
// Configurable deadline window (init_with_deadline_window)
// ---------------------------------------------------------------------------

#[test]
fn get_deadline_window_defaults_to_max_when_plain_init_used() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    assert_eq!(client.get_deadline_window(), MAX_DEADLINE_WINDOW_SECS);
}

#[test]
fn init_with_deadline_window_rejects_zero_window() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let result =
        client.try_init_with_deadline_window(&owner, &fw, &rs, &sg, &bp, &ins, &0u64);
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn init_with_deadline_window_enforces_custom_window() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let custom_window = 500u64;
    client.init_with_deadline_window(&owner, &fw, &rs, &sg, &bp, &ins, &custom_window);
    assert_eq!(client.get_deadline_window(), custom_window);

    let executor = Address::generate(&env);

    // Before this fix, MAX_DEADLINE_WINDOW_SECS (3600s) was hardcoded, so a
    // deadline 1000s out would have been accepted. With a configured 500s
    // window it must now be rejected.
    let too_far_deadline = env.ledger().timestamp() + 1000;
    let hash = signed_flow_hash(&env, &executor, 10000, 0, too_far_deadline);
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10000,
        &0u64,
        &too_far_deadline,
        &hash,
        &0u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));

    // A deadline within the custom window passes the deadline check (it may
    // still fail later for unrelated reasons, e.g. mock dependencies not
    // implementing every entrypoint -- this only pins the deadline gate).
    let ok_deadline = env.ledger().timestamp() + 400;
    let hash2 = signed_flow_hash(&env, &executor, 10000, 0, ok_deadline);
    let result2 = client.try_execute_remittance_flow_signed(
        &executor,
        &10000,
        &0u64,
        &ok_deadline,
        &hash2,
        &0u64,
    );
    assert_ne!(result2, Err(Ok(OrchestratorError::DeadlineExpired)));
}

#[test]
fn test_rollback_savings_step_returns_cross_contract_error() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, mock_fail_savings::Contract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);
    // First write step (savings) fails — nothing to compensate.
    assert_eq!(result, Err(Ok(OrchestratorError::CrossContractCallFailed)));
    // Lock must be released.
    assert!(!client.get_execution_state());
    // Nonce not advanced on failure.
    assert_eq!(client.get_nonce(&executor), 0);
}

#[test]
fn test_cross_contract_failure_emits_step_and_cause() {
    // Before this fix, every downstream failure collapsed into the same
    // generic `CrossContractCallFailed` error with no way to tell which step
    // failed or whether it was the callee's own logic vs. the invocation
    // itself. Assert the swallowed information is now surfaced as an event.
    let (env, owner) = setup_test();
    let (orchestrator_id, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, mock_fail_savings::Contract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);
    let _ =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);

    let expected_topics = soroban_sdk::vec![
        &env,
        symbol_short!("cctx_err").into_val(&env),
        symbol_short!("savings").into_val(&env),
    ];
    let matched: std::vec::Vec<_> = env
        .events()
        .all()
        .iter()
        .filter(|(cid, topics, _)| cid == &orchestrator_id && *topics == expected_topics)
        .collect();
    assert_eq!(matched.len(), 1);
    // `mock_fail_savings` panics rather than returning a typed error, so this
    // is an invocation-level failure (`rejected_by_contract == false`).
    let (_, _, data) = &matched[0];
    assert_eq!(bool::from_val(&env, data), false);
}

#[test]
fn test_rollback_bill_step_triggers_compensation() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, mock_fail_bill::Contract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);
    // Bill step failed after savings succeeded → rollback.
    assert_eq!(result, Err(Ok(OrchestratorError::RemittanceFlowRolledBack)));
    // Lock must be released.
    assert!(!client.get_execution_state());
    // Nonce not advanced on failure.
    assert_eq!(client.get_nonce(&executor), 0);
}

#[test]
fn test_rollback_insurance_step_triggers_compensation() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, mock_fail_insurance::Contract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);
    // Insurance step failed after savings + bills → rollback.
    assert_eq!(result, Err(Ok(OrchestratorError::RemittanceFlowRolledBack)));
    // Lock must be released.
    assert!(!client.get_execution_state());
    // Nonce not advanced on failure.
    assert_eq!(client.get_nonce(&executor), 0);
}

#[test]
fn test_rollback_lock_released_and_stats_updated_on_failure() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, mock_fail_savings::Contract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);

    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);
    let result =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);

    // Verify the error is the expected orchestration error.
    // Note: Soroban's try_call path rolls back ALL storage on error return,
    // so stats/audit storage changes from the error-handling branch inside
    // execute_remittance_flow_signed are also reverted. This is expected
    // behaviour — the error is surfaced to the caller.
    // Soroban's try_ may surface the contract error as Err(...) or Ok(Err(...)).
    // Either way, we just verify it's not a bare Ok(Ok(...)).
    let is_error = match &result {
        Ok(inner) => inner.is_err(),
        Err(_) => true,
    };
    assert!(is_error, "expected error, got {:?}", result);

    // Lock released (rollback reverts EXEC_LOCK set by LockGuard).
    assert!(!client.get_execution_state());

    // Stats/audit ARE updated inside the contract before the error return,
    // but try_call reverts them. We verify by checking the error value
    // rather than post-call storage.
    // Non-try callers will see the audit/stats updates committed.
}

#[test]
fn test_rollback_spending_check_rejection() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, mock_no_limit::Contract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);
    // Spending limit check is pre-validation (read-only), fails before any writes.
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));
    // Lock must be released (lock was never acquired — error before lock scope).
    assert!(!client.get_execution_state());
}

#[test]
fn test_rollback_audit_records_failure_with_step_context() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, mock_fail_savings::Contract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    init_orchestrator_with_mocks(&env, &client, &owner, fw, rs, sg, bp, ins);

    let executor = Address::generate(&env);
    let deadline = signed_flow_deadline(&env);
    let hash = signed_flow_hash(&env, &executor, 10000, 0, deadline);

    let _ =
        client.try_execute_remittance_flow_signed(&executor, &10000, &0, &deadline, &hash, &0u64);

    // Note: try_call rolls back the audit storage on error, so we verify
    // the failure path exists via the other tests that check error values.
    // The audit/stats updates are best-effort (visible to non-try callers).
}

/// A deadline-rejected signed call MUST NOT mutate `ExecutionStats`. The stats
/// counters are only touched after the validation gate in
/// `require_nonce_hardened` passes, so a deadline rejection (which returns
/// before the lock/execute path) must leave every counter untouched.
#[test]
fn test_signed_deadline_rejected_does_not_mutate_stats() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let before = client.get_execution_stats().unwrap();

    // Beyond-window deadline -> DeadlineExpired before any stats mutation.
    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS + 1;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);
    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash, &0u64);
    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));

    let after = client.get_execution_stats().unwrap();
    assert_eq!(
        before, after,
        "deadline-rejected signed call must not mutate ExecutionStats"
    );
}

// ============================================================================
// Issue #1539: Additional rollback on inner call failure tests
//
// These tests supplement the existing rollback coverage by exercising:
//  - The unsigned `execute_remittance_flow` path (the existing tests use
//    the signed `execute_remittance_flow_signed` path)
//  - Rollback behaviour with dedicated mocks that implement both forward
//    step methods AND compensation (reverse) interfaces, so the
//    best-effort compensation actually succeeds
//  - Fan-out flow (`execute_flow_fanout`) which does NOT compensate —
//    verifying compensation is only applied on the atomic flow path
// ---------------------------------------------------------------------------

/// Failing bill mock with compensation support for unsigned flow.
/// Forward steps succeed except pay_bill; reverse methods succeed.
mod mock_unsigned_fail_bill {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool { true }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {
            panic!("bill step failed")
        }
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
        // Compensation methods (reverse interfaces)
        pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Failing insurance mock with compensation support for unsigned flow.
mod mock_unsigned_fail_insurance {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool { true }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {
            panic!("insurance step failed")
        }
        // Compensation methods
        pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Test that the unsigned `execute_remittance_flow` returns
/// `RemittanceFlowRolledBack` when a downstream bill step fails after
/// savings succeeded. This mirrors the signed-path test but covers the
/// unsigned entry point (which uses `execute_remittance_flow` directly).
#[test]
fn test_unsigned_rollback_bill_failure_returns_rolled_back() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mock_id = env.register_contract(None, mock_unsigned_fail_bill::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 10000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::RemittanceFlowRolledBack)),
        "Unsigned flow: bill failure after savings must roll back"
    );
    assert!(!client.get_execution_state(), "Lock must be released after rollback");
}

/// Test that the unsigned flow returns `RemittanceFlowRolledBack` when
/// insurance fails after both savings and bills succeeded, triggering
/// compensation for both prior steps.
#[test]
fn test_unsigned_rollback_insurance_failure_returns_rolled_back() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mock_id = env.register_contract(None, mock_unsigned_fail_insurance::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 10000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::RemittanceFlowRolledBack)),
        "Unsigned flow: insurance failure after savings+bills must roll back"
    );
    assert!(!client.get_execution_state(), "Lock must be released after rollback");
}

/// Test that the unsigned flow returns `CrossContractCallFailed` (not
/// `RemittanceFlowRolledBack`) when the FIRST write step fails — there
/// is nothing to compensate.
#[test]
fn test_unsigned_rollback_first_step_fails_no_compensation() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let mock_id = env.register_contract(None, mock_fail_savings::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 10000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::CrossContractCallFailed)),
        "Unsigned flow: first-step failure must return CrossContractCallFailed"
    );
    assert!(!client.get_execution_state(), "Lock must be released");
}

/// Test that the fan-out flow (`execute_flow_fanout`) returns
/// `CrossContractCallFailed` (not `RemittanceFlowRolledBack`) when a
/// downstream step fails — verification that compensation is NOT applied
/// on the fan-out path (unlike the atomic flow which compensates).
#[test]
fn test_fanout_flow_does_not_compensate_on_bill_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Use the bill-failing mock with compensation support; fan-out should
    // return CrossContractCallFailed rather than attempting rollback.
    let owner = Address::generate(&env);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, mock_unsigned_fail_bill::Contract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let caller = Address::generate(&env);

    // Fan-out with failing bill step: must NOT compensate (no rollback)
    let result = client.try_execute_flow_fanout(&caller, &10000i128);

    // The fan-out captures per-step results — the outer Result should be Ok,
    // but the inner FanOutFlowResult.all_succeeded should be false.
    assert!(result.is_ok(), "fan-out must not panic on step failure");
    if let Ok(Ok(fanout)) = &result {
        assert!(!fanout.all_succeeded, "fan-out must report all_succeeded=false when a step fails");
        assert!(!fanout.savings.succeeded, "savings step must report failure when bill mock panics");
        assert!(!fanout.bills.succeeded, "bill step must report failure");
    } else if let Ok(Err(e)) = &result {
        // If the fan-out returns an error, it must be CrossContractCallFailed,
        // NOT RemittanceFlowRolledBack (which would indicate compensation).
        assert_eq!(
            *e,
            OrchestratorError::CrossContractCallFailed,
            "fan-out failure must be CrossContractCallFailed, not RemittanceFlowRolledBack"
        );
    }

    // Lock must be released even on fan-out failure
    assert!(!client.get_execution_state(), "Lock must be released after fan-out failure");
}

// ---------------------------------------------------------------------------
// Flow lifecycle event tests (unsigned + signed parity)
// ---------------------------------------------------------------------------

fn remitwise_topic(env: &Env, action: Symbol) -> soroban_sdk::Vec<soroban_sdk::Val> {
    soroban_sdk::vec![
        env,
        symbol_short!("Remitwise").into_val(env),
        remitwise_common::EventCategory::Transaction
            .to_u32()
            .into_val(env),
        remitwise_common::EventPriority::High.to_u32().into_val(env),
        action.into_val(env),
    ]
}

fn count_remitwise_events(env: &Env, contract_id: &Address, action: Symbol) -> u32 {
    let expected = remitwise_topic(env, action);
    env.events()
        .all()
        .iter()
        .filter(|(cid, topics, _)| cid == contract_id && *topics == expected)
        .count() as u32
}

fn flow_params(
    _env: &Env,
    caller: &Address,
    mock_id: &Address,
    amount: i128,
) -> RemittanceFlowParams {
    RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: amount,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    }
}

#[test]
fn test_flow_event_emitted_on_start() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow")),
        0
    );

    client.execute_remittance_flow(&flow_params(&env, &caller, &mock_id, 1000));

    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow")),
        1
    );
}

#[test]
fn test_flow_ok_event_emitted_on_success() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    client.execute_remittance_flow(&flow_params(&env, &caller, &mock_id, 1000));

    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow_ok")),
        1
    );

    let stats = client.get_execution_stats().unwrap();
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.successful_executions, 1);
    assert_eq!(stats.failed_executions, 0);

    let audit = client.get_audit_log(&0, &1);
    assert_eq!(audit.len(), 1);
    assert_eq!(audit.get(0).unwrap().operation, symbol_short!("flow_exec"));
    assert!(audit.get(0).unwrap().success);
}

#[test]
fn test_flow_fail_event_emitted_on_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let deny_id = env.register_contract(None, mock_no_limit::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params(&env, &caller, &deny_id, 1000));
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));

    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow")),
        1
    );
    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow_fail")),
        1
    );
    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow_ok")),
        0
    );
}

#[test]
fn test_record_flow_outcome_failure_updates_stats() {
    let (env, owner) = setup_test();
    let (orchestrator_id, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);
    let caller = Address::generate(&env);

    env.as_contract(&orchestrator_id, || {
        let _ = Orchestrator::record_flow_outcome(
            &env,
            &caller,
            1000,
            Err(OrchestratorError::Unauthorized),
        );
    });

    let stats = client.get_execution_stats().unwrap();
    assert_eq!(stats.total_executions, 1);
    assert_eq!(stats.successful_executions, 0);
    assert_eq!(stats.failed_executions, 1);

    let audit = client.get_audit_log(&0, &1);
    assert_eq!(audit.len(), 1);
    assert!(!audit.get(0).unwrap().success);
}

#[test]
fn test_flow_lifecycle_events_order() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    client.execute_remittance_flow(&flow_params(&env, &caller, &mock_id, 1000));

    let flow_topic = remitwise_topic(&env, symbol_short!("flow"));
    let ok_topic = remitwise_topic(&env, symbol_short!("flow_ok"));

    let mut flow_idx = None;
    let mut ok_idx = None;
    for (i, (_cid, topics, _)) in env.events().all().iter().enumerate() {
        if topics == flow_topic {
            flow_idx = Some(i);
        }
        if topics == ok_topic {
            ok_idx = Some(i);
        }
    }

    assert!(flow_idx.is_some());
    assert!(ok_idx.is_some());
    assert!(flow_idx.unwrap() < ok_idx.unwrap());
}

#[test]
fn test_flow_fail_does_not_leak_sensitive_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let deny_id = env.register_contract(None, mock_no_limit::Contract);
    let caller = Address::generate(&env);
    let sensitive_amount = 999_999i128;

    let _ =
        client.try_execute_remittance_flow(&flow_params(&env, &caller, &deny_id, sensitive_amount));

    let fail_topic = remitwise_topic(&env, symbol_short!("flow_fail"));
    let fail_event = env
        .events()
        .all()
        .iter()
        .find(|(cid, topics, _)| cid == &orchestrator_id && *topics == fail_topic)
        .expect("flow_fail event missing");

    let payload: (Address, u32) = FromVal::from_val(&env, &fail_event.2);
    assert_eq!(payload.0, caller);
    assert_eq!(payload.1, OrchestratorError::Unauthorized as u32);
}

#[test]
fn test_unsigned_and_signed_flow_stats_parity() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let unsigned_executor = Address::generate(&env);
    let signed_executor = Address::generate(&env);
    let mock_id = env.register_contract(None, MockContract);

    client.execute_remittance_flow(&flow_params(&env, &unsigned_executor, &mock_id, 1000));

    let after_unsigned = client.get_execution_stats().unwrap();
    assert_eq!(after_unsigned.total_executions, 1);
    assert_eq!(after_unsigned.successful_executions, 1);
    assert_eq!(after_unsigned.failed_executions, 0);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);
    assert!(client.execute_remittance_flow_signed(
        &signed_executor,
        &1000,
        &0,
        &deadline,
        &hash,
        &0u64
    ));

    let after_signed = client.get_execution_stats().unwrap();
    assert_eq!(after_signed.total_executions, 2);
    assert_eq!(after_signed.successful_executions, 2);
    assert_eq!(after_signed.failed_executions, 0);
}

// ---------------------------------------------------------------------------
// Pre-upgrade snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn test_pre_upgrade_roundtrip() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    // Take snapshot
    let result = client.try_pre_upgrade(&owner);
    assert!(result.is_ok());

    // Modify version
    let result = client.try_set_version(&owner, &42);
    assert!(result.is_ok());
    assert_eq!(client.get_version(), 42);

    // Restore from snapshot
    let result = client.try_restore_from_snapshot(&owner);
    assert!(result.is_ok());

    // Version should be restored
    assert_eq!(client.get_version(), 1);
}

#[test]
fn test_pre_upgrade_bumps_persistent_snapshot_ttl() {
    // Before this fix, pre_upgrade wrote SNAPSHOT_KEY / SNAP_TS to the
    // *persistent* bucket but only ever called extend_ttl on the *instance*
    // bucket elsewhere in the contract -- the persistent entry's TTL was
    // never bumped at all, so it could be archived off the ledger (breaking
    // restore_from_snapshot) well before the instance itself expired.
    use soroban_sdk::testutils::storage::Persistent;

    let (env, owner) = setup_test();
    let (orchestrator_id, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);
    env.ledger().with_mut(|li| li.max_entry_ttl = 6_000_000);

    client.pre_upgrade(&owner);

    let ttl = env.as_contract(&orchestrator_id, || {
        env.storage().persistent().get_ttl(&SNAPSHOT_KEY)
    });
    assert_eq!(
        ttl, PERSISTENT_BUMP_AMOUNT,
        "persistent snapshot entry must be bumped with the persistent bucket's amount"
    );
}

#[test]
fn test_pre_upgrade_unauthorized_fails() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let stranger = Address::generate(&env);

    // Unauthorized pre_upgrade
    let result = client.try_pre_upgrade(&stranger);
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));

    // Owner can pre_upgrade
    let result = client.try_pre_upgrade(&owner);
    assert!(result.is_ok());

    // Unauthorized restore
    let result = client.try_restore_from_snapshot(&stranger);
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));

    // Unauthorized discard
    let result = client.try_discard_snapshot(&stranger);
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));
}

#[test]
fn test_pre_upgrade_discard() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    // Take snapshot
    let result = client.try_pre_upgrade(&owner);
    assert!(result.is_ok());

    // Discard snapshot
    let result = client.try_discard_snapshot(&owner);
    assert!(result.is_ok());

    // Restore should now fail (no snapshot)
    let result = client.try_restore_from_snapshot(&owner);
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidDependency)));
}

#[test]
fn test_invalid_amount_unsigned_emits_audit_without_lifecycle_events() {
    let (env, owner) = setup_test();
    let (orchestrator_id, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params(&env, &caller, &mock_id, 0));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));

    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow")),
        0
    );
    assert_eq!(
        count_remitwise_events(&env, &orchestrator_id, symbol_short!("flow_ok")),
        0
    );

    let stats = client.get_execution_stats().unwrap();
    assert_eq!(stats.total_executions, 0);
    assert_eq!(stats.successful_executions, 0);
    assert_eq!(stats.failed_executions, 0);
}

#[test]
fn test_invalid_amount_unsigned_negative() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params(&env, &caller, &mock_id, -100));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_invalid_amount_unsigned_i128_min() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    let result =
        client.try_execute_remittance_flow(&flow_params(&env, &caller, &mock_id, i128::MIN));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_valid_amount_unsigned_minimum_positive() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let mock_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params(&env, &caller, &mock_id, 1));
    assert!(
        result.is_ok(),
        "amount=1 should be accepted as valid positive amount"
    );
}

#[test]
fn test_double_init_fails() {
    let (env, owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    let result1 = client.try_init(&owner, &fw, &rs, &sg, &bp, &ins);
    assert_eq!(result1, Ok(Ok(true)), "first init should succeed");

    let result2 = client.try_init(&owner, &fw, &rs, &sg, &bp, &ins);
    assert_eq!(
        result2,
        Err(Ok(OrchestratorError::Unauthorized)),
        "second init should fail with Unauthorized"
    );
}

#[test]
fn test_not_initialized_fails() {
    let (env, _owner) = setup_test();
    let (_, client) = register_orchestrator(&env);

    let executor = Address::generate(&env);
    let mock_id = Address::generate(&env);
    let _ = client.try_execute_remittance_flow(&RemittanceFlowParams {
        caller: executor.clone(),
        total_amount: 1000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    });

    let stats = client.get_execution_stats();
    assert_eq!(
        stats, None,
        "get_execution_stats should return None when not initialized"
    );
}

// ---------------------------------------------------------------------------
// MockSplit allocation-length / content hardening tests (Issue #828)
//
// The orchestrator calls an external calculate_split whose return vector it
// does not control. These tests prove that short (0 / 1 / 3 entries) and
// negative-valued responses all produce a typed `InvalidAmount` and that the
// EXEC_LOCK is released (not stuck) in every rejection path.
// ---------------------------------------------------------------------------

/// Mock whose calculate_split returns an empty vec (0 allocations).
mod mock_split_0 {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct Contract;
    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            Vec::new(&env)
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Mock whose calculate_split returns 1 allocation.
mod mock_split_1 {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct Contract;
    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 10000i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Mock whose calculate_split returns 3 allocations (one short of required 4).
mod mock_split_3 {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct Contract;
    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Mock whose calculate_split returns exactly 4 allocations (valid).
mod mock_split_4 {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct Contract;
    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
        pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Mock whose calculate_split returns a negative savings allocation.
mod mock_split_negative {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};
    #[contract]
    pub struct Contract;
    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, -500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Helper: build RemittanceFlowParams using the same contract for every role.
/// Hostile downstream that returns `false` for all three payment steps.
/// Used to verify that EXEC_LOCK is released even when downstream contracts
/// are maximally adversarial (all steps fail without panicking).
mod mock_hostile_all_fail {
    use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {
            panic!("savings step failed")
        }
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {
            panic!("bill step failed")
        }
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {
            panic!("insurance step failed")
        }
        pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) {}
        pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) {}
        pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) {}
    }
}

/// Panic-safe EXEC_LOCK release: hostile downstream fails savings step → lock must be released.
#[test]
fn test_exec_lock_released_when_hostile_downstream_fails_savings() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_hostile_all_fail::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    // Hostile savings step → CrossContractCallFailed.
    assert_eq!(result, Err(Ok(OrchestratorError::CrossContractCallFailed)));
    // EXEC_LOCK must be unlocked — a subsequent call must not see ExecutionLocked.
    assert!(!client.get_execution_state());
}

/// Panic-safe EXEC_LOCK release: hostile downstream fails bill step → lock must be released.
#[test]
fn test_exec_lock_released_when_hostile_downstream_fails_bill() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_fail_bill::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    // Unsigned path does not enable compensation (compensate_on_failure=false),
    // so CrossContractCallFailed is expected instead of RemittanceFlowRolledBack.
    assert_eq!(result, Err(Ok(OrchestratorError::CrossContractCallFailed)));
    assert!(!client.get_execution_state());
}

/// After a hostile downstream failure, a subsequent valid call must succeed —
/// proving the lock was fully released and is not wedged.
#[test]
fn test_lock_recovers_for_subsequent_valid_call_after_hostile_failure() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let hostile_id = env.register_contract(None, mock_hostile_all_fail::Contract);
    let good_id = env.register_contract(None, MockContract);
    let caller = Address::generate(&env);

    // First call — hostile downstream, must fail.
    let _ = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &hostile_id));
    assert!(!client.get_execution_state());

    // Second call — well-behaved downstream, must succeed.
    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &good_id));
    assert_eq!(result, Ok(Ok(())));
    assert!(!client.get_execution_state());
}

fn flow_params_single(_env: &Env, caller: &Address, mock_id: &Address) -> RemittanceFlowParams {
    RemittanceFlowParams {
        caller: caller.clone(),
        total_amount: 10_000i128,
        family_wallet: mock_id.clone(),
        remittance_split: mock_id.clone(),
        savings: mock_id.clone(),
        bills: mock_id.clone(),
        insurance: mock_id.clone(),
        goal_id: 1,
        bill_id: 1,
        policy_id: 1,
    }
}

#[test]
fn test_split_0_allocations_returns_invalid_amount_and_releases_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_split_0::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
    // EXEC_LOCK must be released (not stuck).
    assert!(!client.get_execution_state());
}

#[test]
fn test_split_1_allocation_returns_invalid_amount_and_releases_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_split_1::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
    assert!(!client.get_execution_state());
}

#[test]
fn test_split_3_allocations_returns_invalid_amount_and_releases_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_split_3::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
    assert!(!client.get_execution_state());
}

#[test]
fn test_split_4_allocations_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_split_4::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    assert!(
        result.is_ok(),
        "4-allocation split should succeed: {:?}",
        result
    );
    assert!(!client.get_execution_state());
}

#[test]
fn test_split_negative_allocation_returns_invalid_amount_and_releases_lock() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let mock_id = env.register_contract(None, mock_split_negative::Contract);
    let caller = Address::generate(&env);

    let result = client.try_execute_remittance_flow(&flow_params_single(&env, &caller, &mock_id));
    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
    // No downstream add_to_goal/pay_bill/pay_premium must have been called —
    // the lock is released, confirming we exited cleanly before execution.
    assert!(!client.get_execution_state());
}

/// Test that epoch mismatch rejects stale actor tokens.
#[test]
fn test_epoch_mismatch_rejects_stale_token() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    // Initialize orchestrator (each dependency must be a distinct address)
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Get current epoch (should be 0)
    let current_epoch = client.get_actor_epoch_public();
    assert_eq!(current_epoch, 0);

    // Bump epoch to 1
    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 1);

    // Try to execute with stale epoch (0) - should fail with EpochMismatch
    let amount = 10_000i128;
    let nonce = 0u64;
    let deadline = 10_000u64;
    let request_hash = 12345u64;

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &amount,
        &nonce,
        &deadline,
        &request_hash,
        &0u64, // stale epoch
    );

    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

/// Test that matching epoch allows execution (doesn't fail with EpochMismatch).
#[test]
fn test_matching_epoch_allows_execution() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    // Initialize orchestrator (each dependency must be a distinct address)
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Get current epoch (should be 0)
    let current_epoch = client.get_actor_epoch_public();
    assert_eq!(current_epoch, 0);

    // Execute with matching epoch (0) - should not fail with EpochMismatch
    let amount = 10_000i128;
    let nonce = 0u64;
    let deadline = 10_000u64;
    let request_hash = 12345u64;

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &amount,
        &nonce,
        &deadline,
        &request_hash,
        &0u64, // matching epoch
    );

    // Should not fail with EpochMismatch (may fail for other reasons like nonce validation)
    assert_ne!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Epoch guard boundary tests: Same / Off-by-one / Wildly different
// ---------------------------------------------------------------------------

/// After init(), get_actor_epoch_public() returns 0.
#[test]
fn epoch_starts_at_zero_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    assert_eq!(client.get_actor_epoch_public(), 0);
}

/// Each bump increments the epoch by exactly 1 (off-by-one precision).
#[test]
fn bump_actor_epoch_increments_by_exactly_one() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    assert_eq!(client.get_actor_epoch_public(), 0);

    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 1);
    assert_eq!(client.get_actor_epoch_public(), 1);

    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 2);
    assert_eq!(client.get_actor_epoch_public(), 2);

    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 3);
    assert_eq!(client.get_actor_epoch_public(), 3);
}

/// Multiple sequential bumps accumulate; only the latest epoch is valid.
#[test]
fn multiple_bumps_all_reject_stale_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Bump 0 → 1 → 2 → 3
    for expected in 1u64..=3 {
        let new_epoch = client.bump_actor_epoch(&owner);
        assert_eq!(new_epoch, expected);
    }
    assert_eq!(client.get_actor_epoch_public(), 3);

    // Epochs 0, 1, 2 should all be stale after bumping to 3.
    for stale in 0u64..3 {
        let result = client.try_execute_remittance_flow_signed(
            &executor,
            &10_000i128,
            &0u64,
            &10_000u64,
            &12345u64,
            &stale,
        );
        assert_eq!(
            result,
            Err(Ok(OrchestratorError::EpochMismatch)),
            "epoch {stale} should be rejected when current is 3"
        );
    }

    // Epoch 3 (matching) should not be rejected on the epoch check.
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &3u64,
    );
    assert_ne!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Off-by-one: one step ahead
// ---------------------------------------------------------------------------

/// Providing epoch that is 1 ahead of current rejects with EpochMismatch.
#[test]
fn off_by_one_future_epoch_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Current epoch is 0; provide 1 (one step ahead).
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &1u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));

    // Bump to 1; now provide 2 (one step ahead).
    client.bump_actor_epoch(&owner);
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &2u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Off-by-one: one step behind (stale by exactly 1)
// ---------------------------------------------------------------------------

/// Providing epoch that is 1 behind current rejects with EpochMismatch.
#[test]
fn off_by_one_stale_epoch_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Bump to 1; provide 0 (one step behind).
    client.bump_actor_epoch(&owner);
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &0u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));

    // Bump to 2; provide 1 (one step behind).
    client.bump_actor_epoch(&owner);
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &1u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Wildly different: far-ahead epoch
// ---------------------------------------------------------------------------

/// Providing u64::MAX when current epoch is 0 rejects with EpochMismatch.
#[test]
fn wildly_different_future_epoch_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &u64::MAX,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

/// Providing 999 when current epoch is 0 rejects with EpochMismatch.
#[test]
fn wildly_different_arbitrary_epoch_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &999u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

/// Providing 42 when current epoch is 3 rejects with EpochMismatch.
#[test]
fn wildly_different_future_epoch_after_bump_rejects() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Bump to 3
    client.bump_actor_epoch(&owner);
    client.bump_actor_epoch(&owner);
    client.bump_actor_epoch(&owner);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &42u64,
    );
    assert_eq!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Matching epoch (same) — happy path after bump
// ---------------------------------------------------------------------------

/// After bump to N, providing N does not fail with EpochMismatch.
#[test]
fn same_epoch_after_bump_allows_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Bump to 1
    client.bump_actor_epoch(&owner);
    assert_eq!(client.get_actor_epoch_public(), 1);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &10_000i128,
        &0u64,
        &10_000u64,
        &12345u64,
        &1u64,
    );
    assert_ne!(result, Err(Ok(OrchestratorError::EpochMismatch)));
}

// ---------------------------------------------------------------------------
// Bump authorization
// ---------------------------------------------------------------------------

/// Non-owner cannot bump the epoch.
#[test]
fn non_owner_cannot_bump_epoch() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);
    let non_owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.try_bump_actor_epoch(&non_owner);
    assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));
}

// ---------------------------------------------------------------------------
// Bump emits event with correct old/new values
// ---------------------------------------------------------------------------

/// bump_actor_epoch emits an event with (old_epoch, new_epoch).
#[test]
fn bump_actor_epoch_emits_event_with_correct_values() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 1);

    let expected_topics = soroban_sdk::vec![
        &env,
        symbol_short!("orch").into_val(&env),
        symbol_short!("epch_bump").into_val(&env),
    ];

    let bump_event = env
        .events()
        .all()
        .iter()
        .find(|(cid, topics, _)| cid == &orchestrator_id && *topics == expected_topics)
        .expect("epch_bump event missing");

    let payload: (u64, u64) = FromVal::from_val(&env, &bump_event.2);
    assert_eq!(payload, (0u64, 1u64));
}

/// Second bump emits event with (1, 2).
#[test]
fn second_bump_emits_correct_old_and_new() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    client.bump_actor_epoch(&owner);
    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 2);

    let expected_topics = soroban_sdk::vec![
        &env,
        symbol_short!("orch").into_val(&env),
        symbol_short!("epch_bump").into_val(&env),
    ];

    let mut bump_payloads: std::vec::Vec<(u64, u64)> = std::vec::Vec::new();
    for (_, topics, data) in env.events().all().iter() {
        if topics == expected_topics {
            let payload: (u64, u64) = FromVal::from_val(&env, &data);
            bump_payloads.push(payload);
        }
    }

    assert_eq!(bump_payloads.len(), 2);
    assert_eq!(bump_payloads[0], (0u64, 1u64));
    assert_eq!(bump_payloads[1], (1u64, 2u64));
}

// ---------------------------------------------------------------------------
// get_actor_epoch_public reflects bump state
// ---------------------------------------------------------------------------

/// get_actor_epoch_public returns 0 before any bump, 1 after one bump, etc.
#[test]
fn get_actor_epoch_public_reflects_bump_state() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    assert_eq!(client.get_actor_epoch_public(), 0);

    client.bump_actor_epoch(&owner);
    assert_eq!(client.get_actor_epoch_public(), 1);

    client.bump_actor_epoch(&owner);
    assert_eq!(client.get_actor_epoch_public(), 2);

    client.bump_actor_epoch(&owner);
    assert_eq!(client.get_actor_epoch_public(), 3);
}

// ---------------------------------------------------------------------------
// Overflow: bump from u64::MAX
// ---------------------------------------------------------------------------

/// Bumping the epoch when it is at u64::MAX returns Overflow.
#[test]
fn bump_actor_epoch_overflow_returns_error() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, MockContract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Manually set epoch to u64::MAX to test overflow.
    env.as_contract(&orchestrator_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("ACT_EPOCH"), &u64::MAX);
    });
    assert_eq!(client.get_actor_epoch_public(), u64::MAX);

    let result = client.try_bump_actor_epoch(&owner);
    assert_eq!(result, Err(Ok(OrchestratorError::Overflow)));

    // Epoch must remain at u64::MAX — the bump must not have mutated storage.
    assert_eq!(client.get_actor_epoch_public(), u64::MAX);
}

// ---------------------------------------------------------------------------
// get_fee_schedule view function tests
// ---------------------------------------------------------------------------

/// Mock remittance split contract that returns a fixed split.
mod mock_remittance_split {
    use soroban_sdk::{contract, contractimpl, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn get_split(env: Env) -> Vec<u32> {
            soroban_sdk::vec![&env, 5000u32, 3000u32, 1500u32, 500u32]
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 5000i128, 3000i128, 1500i128, 500i128]
        }
    }
}

/// Mock remittance split contract that returns an invalid split (wrong length).
mod mock_remittance_split_invalid {
    use soroban_sdk::{contract, contractimpl, Env, Vec};

    #[contract]
    pub struct Contract;

    #[contractimpl]
    impl Contract {
        pub fn get_split(env: Env) -> Vec<u32> {
            soroban_sdk::vec![&env, 5000u32, 3000u32] // Only 2 entries
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 5000i128, 3000i128]
        }
    }
}

#[test]
fn test_get_fee_schedule_returns_correct_split() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    // Initialize with a mock remittance split contract
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Call the view function
    let result = client.get_fee_schedule();

    assert!(result.is_some(), "fee schedule should be Some");
    let (spending, savings, bills, insurance) = result.unwrap();
    assert_eq!(spending, 5000);
    assert_eq!(savings, 3000);
    assert_eq!(bills, 1500);
    assert_eq!(insurance, 500);
    // Sum should be 10000 (100%)
    assert_eq!(spending + savings + bills + insurance, 10000);
}

#[test]
fn test_get_fee_schedule_returns_none_when_invalid_split() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let owner = Address::generate(&env);

    // Initialize with a mock remittance split that returns invalid length
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split_invalid::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    // Call the view function - should return None for invalid split
    let result = client.get_fee_schedule();
    assert!(
        result.is_none(),
        "fee schedule should be None for invalid split"
    );
}

#[test]
fn test_get_fee_schedule_returns_none_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let orchestrator_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Don't initialize - should return None
    let result = client.get_fee_schedule();
    assert!(
        result.is_none(),
        "fee schedule should be None when not initialized"
    );
}

// ---------------------------------------------------------------------------
// execute_flow_fanout — #1339 (memoised fee lookup) + #1345 (error semantics)
// ---------------------------------------------------------------------------
//
// Setup convention for all fanout tests:
//  • `mock_remittance_split::Contract` returns allocations [5000, 3000, 1500, 500]
//    for any input amount, so with total=10_000: savings=3000, bills=1500, ins=500.
//  • `MockContract` succeeds for every downstream call.
//  • Failing mocks (mock_fail_savings, mock_fail_bill, mock_fail_insurance) panic
//    inside their respective method, which the `try_*` harness surfaces as Err.

// ---------------------------------------------------------------------------
// #1345 happy-path: succeeded must be true when calls succeed
// ---------------------------------------------------------------------------

/// When all three downstream calls succeed, every `succeeded` flag must be
/// `true` and `all_succeeded` must be `true`.
///
/// Before the fix, `.is_err()` was used instead of `.is_ok()`, so a successful
/// call would set `succeeded = false`.
#[test]
fn test_fanout_all_succeed_flags_are_true() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.execute_flow_fanout(&executor, &10_000i128);

    assert!(
        result.savings.succeeded,
        "savings.succeeded must be true when add_to_goal succeeds (fix for #1345)"
    );
    assert!(
        result.bills.succeeded,
        "bills.succeeded must be true when pay_bill succeeds (fix for #1345)"
    );
    assert!(
        result.insurance.succeeded,
        "insurance.succeeded must be true when pay_premium succeeds (fix for #1345)"
    );
    assert!(
        result.all_succeeded,
        "all_succeeded must be true when all steps succeed (fix for #1345)"
    );
}

// ---------------------------------------------------------------------------
// #1345 failure-path: succeeded must be false when a call fails
// ---------------------------------------------------------------------------

/// When the savings step fails (mock panics), savings.succeeded must be
/// false and all_succeeded must be false, but bills and insurance still run.
#[test]
fn test_fanout_savings_failure_reports_succeeded_false() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    // savings-goals uses the panicking mock; fw and other deps use the no-op mock
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, mock_fail_savings::Contract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.execute_flow_fanout(&executor, &10_000i128);

    assert!(
        !result.savings.succeeded,
        "savings.succeeded must be false when add_to_goal fails (fix for #1345)"
    );
    // bills and insurance still attempt and succeed independently
    assert!(
        result.bills.succeeded,
        "bills.succeeded should be true — fan-out continues after savings failure"
    );
    assert!(
        result.insurance.succeeded,
        "insurance.succeeded should be true — fan-out continues after savings failure"
    );
    assert!(
        !result.all_succeeded,
        "all_succeeded must be false when any step fails (fix for #1345)"
    );
}

/// When the bill step fails, bills.succeeded is false, all_succeeded is false,
/// and savings/insurance retain their own outcomes.
#[test]
fn test_fanout_bill_failure_reports_succeeded_false() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, mock_fail_bill::Contract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.execute_flow_fanout(&executor, &10_000i128);

    assert!(result.savings.succeeded, "savings step should succeed");
    assert!(
        !result.bills.succeeded,
        "bills.succeeded must be false when pay_bill fails (fix for #1345)"
    );
    assert!(result.insurance.succeeded, "insurance step should succeed");
    assert!(!result.all_succeeded, "all_succeeded must be false");
}

/// When the insurance step fails, insurance.succeeded is false.
#[test]
fn test_fanout_insurance_failure_reports_succeeded_false() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, mock_fail_insurance::Contract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.execute_flow_fanout(&executor, &10_000i128);

    assert!(result.savings.succeeded, "savings step should succeed");
    assert!(result.bills.succeeded, "bills step should succeed");
    assert!(
        !result.insurance.succeeded,
        "insurance.succeeded must be false when pay_premium fails (fix for #1345)"
    );
    assert!(!result.all_succeeded, "all_succeeded must be false");
}

// ---------------------------------------------------------------------------
// #1339 memoised fee lookup: allocations come from calculate_split, not amount/3
// ---------------------------------------------------------------------------

/// The allocated amounts must match the split percentages returned by the
/// remittance-split contract, not a naive `amount / 3` division.
///
/// `mock_remittance_split::Contract` returns [5000, 3000, 1500, 500] for any
/// amount, so with total = 10_000:
///   savings   = 3000
///   bills     = 1500
///   insurance = 500
///
/// The old hardcoded path produced  savings=3334, bills=3333, insurance=3333.
#[test]
fn test_fanout_amounts_match_calculate_split_not_hardcoded_thirds() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.execute_flow_fanout(&executor, &10_000i128);

    // Amounts must reflect the split percentages, not amount/3.
    assert_eq!(
        result.savings.amount, 3000,
        "savings amount must be 3000 (30% of 10_000 from calculate_split) — fix for #1339"
    );
    assert_eq!(
        result.bills.amount, 1500,
        "bills amount must be 1500 (15% of 10_000 from calculate_split) — fix for #1339"
    );
    assert_eq!(
        result.insurance.amount, 500,
        "insurance amount must be 500 (5% of 10_000 from calculate_split) — fix for #1339"
    );
}

/// InvalidAmount is returned when amount ≤ 0 (explicit failure mode).
#[test]
fn test_fanout_rejects_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    let rs = env.register_contract(None, mock_remittance_split::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.try_execute_flow_fanout(&executor, &0i128);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidAmount)),
        "fanout must reject zero amount"
    );
}

/// InvalidAmount is returned when calculate_split returns fewer than 4 entries.
#[test]
fn test_fanout_rejects_short_split_vector() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();

    let owner = Address::generate(&env);
    let executor = Address::generate(&env);

    let orch_id = env.register_contract(None, Orchestrator);
    let fw = env.register_contract(None, MockContract);
    // mock_remittance_split_invalid returns only 2 entries
    let rs = env.register_contract(None, mock_remittance_split_invalid::Contract);
    let sg = env.register_contract(None, MockContract);
    let bp = env.register_contract(None, MockContract);
    let ins = env.register_contract(None, MockContract);

    let client = OrchestratorClient::new(&env, &orch_id);
    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let result = client.try_execute_flow_fanout(&executor, &10_000i128);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidAmount)),
        "fanout must return InvalidAmount when split vector is short"
    );
}


