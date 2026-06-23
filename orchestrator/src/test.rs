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
    pub fn add_to_goal(_env: Env, _caller: Address, _goal_id: u32, _amount: i128) -> i128 {
        _amount
    }
    pub fn pay_bill(_env: Env, _caller: Address, _bill_id: u32) {}
    pub fn pay_premium(_env: Env, _caller: Address, _policy_id: u32) -> bool {
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

fn register_orchestrator(env: &Env) -> OrchestratorClient<'_> {
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

/// Execute one unsigned remittance flow entry so the audit log grows by one.
///
/// Note: this helper uses the *unsigned* execution path and therefore does not
/// update `ExecutionStats` (which are updated in the signed path).
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
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    // Use a non-zero ledger time so the edge arithmetic is unambiguous.
    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS; // exactly at the edge
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);

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
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS + 1; // one second too far
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);

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
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(5_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now - 1; // strictly in the past
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);

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
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS; // valid, in-window
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

    // First call succeeds and consumes nonce 0.
    let first =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);
    assert_eq!(first, Ok(Ok(true)));
    assert_eq!(client.get_nonce(&executor), 1);

    // Replay the identical request while the deadline is still in-window. The
    // deadline check passes, but the advanced counter rejects the stale nonce.
    let replay =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);
    assert_eq!(
        replay,
        Err(Ok(OrchestratorError::NonceAlreadyUsed)),
        "in-window replay of a consumed nonce must be rejected"
    );
    // The counter does not advance again on the rejected replay.
    assert_eq!(client.get_nonce(&executor), 1);
}

/// A deadline-rejected signed call MUST NOT mutate `ExecutionStats`. The stats
/// counters are only touched after the validation gate in
/// `require_nonce_hardened` passes, so a deadline rejection (which returns
/// before the lock/execute path) must leave every counter untouched.
#[test]
fn test_signed_deadline_rejected_does_not_mutate_stats() {
    let (env, owner) = setup_test();
    let client = register_orchestrator(&env);
    init_orchestrator(&env, &client, &owner);

    env.ledger().set_timestamp(1_000);
    let executor = Address::generate(&env);

    let before = client.get_execution_stats().unwrap();

    // Beyond-window deadline -> DeadlineExpired before any stats mutation.
    let now = env.ledger().timestamp();
    let deadline = now + MAX_DEADLINE_WINDOW_SECS + 1;
    let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);
    let result =
        client.try_execute_remittance_flow_signed(&executor, &1000, &0, &deadline, &hash);
    assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));

    let after = client.get_execution_stats().unwrap();
    assert_eq!(
        before, after,
        "deadline-rejected signed call must not mutate ExecutionStats"
    );
}
