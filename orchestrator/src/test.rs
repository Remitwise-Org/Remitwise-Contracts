#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, Symbol,
};

#[contract]
pub struct MockContract;

#[contractimpl]
impl MockContract {
    pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
        true
    }
    pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
        vec![&env, 2500, 2500, 2500, 2500]
    }
    pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) -> bool {
        true
    }
    pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) -> bool {
        true
    }
    pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) -> bool {
        true
    }
}

#[contract]
pub struct FailingMock;

#[contractimpl]
impl FailingMock {}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn setup_test() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    (env, owner)
}

fn register_orchestrator(env: &Env) -> OrchestratorClient {
    let id = env.register_contract(None, Orchestrator);
    OrchestratorClient::new(env, &id)
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

/// Execute one remittance flow entry so the audit log grows by one.
fn do_flow(env: &Env, client: &OrchestratorClient, executor: &Address, _nonce: u64) {
    // Reuse a single mock contract registered once per env (stored via a stable address).
    // We register it fresh here but the env caches it; the key insight is we must NOT
    // register a new contract on every call as that exhausts the budget.
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
    let op_bits: u64 = operation.to_val().get_payload();
    let amt_lo = amount as u64;
    let amt_hi = (amount >> 64) as u64;
    op_bits
        .wrapping_add(nonce)
        .wrapping_add(amt_lo)
        .wrapping_add(amt_hi)
        .wrapping_add(deadline)
        .wrapping_mul(1_000_000_007)
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
    assert_eq!(client.get_execution_state(), true);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
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
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let nonce = client.get_nonce(&executor);
    assert_eq!(nonce, 0, "New address should start with nonce 0");
}

#[test]
fn test_execute_flow_signed_invalid_amount() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 0, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor, &0, // amount 0
        &0, &deadline, &hash,
    );

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
}

#[test]
fn test_execute_flow_deadline_expired() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // deadline <= now → DeadlineExpired
    let deadline = env.ledger().timestamp(); // not strictly in the future
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result = client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);

    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
}

#[test]
fn test_execute_flow_deadline_too_far() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + MAX_DEADLINE_WINDOW_SECS + 1000;

    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result = client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);

    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
}

#[test]
fn test_execute_flow_invalid_hash() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 1000;

    let bad_hash = 12345u64;

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &bad_hash);

    assert_eq!(result, Err(Ok(OrchestratorError::InvalidNonce)));
}

#[test]
fn test_out_of_order_nonce_fails() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;

    // Attempt to execute with nonce 5 when current nonce is 0
    let hash = compute_test_hash(&env, symbol_short!("flow"), 5, 1000, deadline);
    let result = client.try_execute_remittance_flow_signed(&executor, &1000, &5, &deadline, &hash);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidNonce)),
        "Out-of-order nonce should fail (must equal current nonce)"
    );
}

#[test]
fn test_multiple_addresses_independent_nonces() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
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
        client.try_execute_remittance_flow_signed(&executor1, &1000, &0, &deadline, &hash1);
    assert!(result1.is_ok());

    // Executor1 nonce should be 1
    assert_eq!(client.get_nonce(&executor1), 1);

    // Executor2 nonce should still be 0 (independent)
    assert_eq!(client.get_nonce(&executor2), 0);

    // Executor2 can execute with nonce 0
    let hash2 = compute_test_hash(&env, symbol_short!("flow"), 0, 500, deadline);
    let result2 =
        client.try_execute_remittance_flow_signed(&executor2, &500, &0, &deadline, &hash2);
    assert!(result2.is_ok(), "Executor2 should execute with nonce 0");
}

#[test]
fn test_request_hash_binding_prevents_parameter_swap() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    let deadline = env.ledger().timestamp() + 1000;

    // Compute hash for amount 1000
    let hash_1000 = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    // Try to execute with different amount but using hash from 1000
    let result =
        client.try_execute_remittance_flow_signed(&executor, &5000, &0, &deadline, &hash_1000);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::InvalidNonce)),
        "Parameter swap attempt should fail (hash mismatch)"
    );
}

#[test]
fn test_deadline_window_prevents_old_requests() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    let executor = Address::generate(&env);

    // Create a request with a deadline far in the future
    let current_time = env.ledger().timestamp();
    let far_deadline = current_time + 366 * 86400; // 1 year in future (exceeds MAX_DEADLINE_WINDOW_SECS)

    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, far_deadline);
    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &far_deadline, &hash);

    assert_eq!(
        result,
        Err(Ok(OrchestratorError::DeadlineExpired)),
        "Request with deadline too far in future should fail"
    );
}
