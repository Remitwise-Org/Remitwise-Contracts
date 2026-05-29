#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, TryFromVal,
};

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

fn setup_split(
    env: &Env,
    spending: u32,
    savings: u32,
    bills: u32,
    insurance: u32,
) -> (
    RemittanceSplitClient<'_>,
    Address,
    Address,
    StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    set_time(env, 1_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(env, &token_addr);

    client.initialize_split(
        &owner,
        &0,
        &token_addr,
        &spending,
        &savings,
        &bills,
        &insurance,
    );

    (client, owner, token_addr, stellar_client)
}

fn sample_accounts(env: &Env) -> AccountGroup {
    AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    }
}

#[test]
fn test_distribution_completed_event() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 40, 30, 20, 10);
    let accounts = sample_accounts(&env);

    let total_amount = 1_000i128;
    stellar_client.mint(&owner, &total_amount);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        total_amount,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &total_amount,
    );

    let events = env.events().all();
    let last_event = events.last();
    assert!(last_event.is_some(), "no events emitted");

    if let Some(event_tuple) = last_event {
        let (_, topics, data) = event_tuple;
        assert_eq!(topics.len(), 4);

        let event_result: Result<DistributionCompletedEvent, _> = DistributionCompletedEvent::try_from_val(&env, &data);
        assert!(event_result.is_ok(), "failed to decode distribution event");

        if let Ok(event) = event_result {
            assert_eq!(event.from, owner);
            assert_eq!(event.total_amount, total_amount);
            assert_eq!(event.spending_amount, 400);
            assert_eq!(event.savings_amount, 300);
            assert_eq!(event.bills_amount, 200);
            assert_eq!(event.insurance_amount, 100);
            assert_eq!(event.timestamp, env.ledger().timestamp());
        }
    }
}

#[test]
fn test_distribution_event_topic_correctness() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 50, 0, 0);
    let accounts = sample_accounts(&env);

    stellar_client.mint(&owner, &100);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        100,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &100,
    );

    let events = env.events().all();
    let dist_comp_event = events
        .iter()
        .find(|event| event.1.len() == 4);

    assert!(dist_comp_event.is_some(), "distribution completed event not found");

    if let Some(event) = dist_comp_event {
        assert_eq!(event.1.len(), 4);
    }
}

#[test]
fn test_request_hash_deterministic() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let hash1 = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        7,
        1_000,
        2_000,
    );
    let hash2 =
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 7, 1_000, 2_000);

    assert_eq!(hash1, hash2);
}

#[test]
fn test_request_hash_changes_with_parameters() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let base = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        0,
        1_000,
        2_000,
    );

    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            1,
            1_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            0,
            2_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 0, 1_000, 3_000)
    );
}

#[test]
fn test_distribute_usdc_signed_success() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 30, 15, 5);
    let accounts = sample_accounts(&env);
    let token = TokenClient::new(&env, &token_addr);

    stellar_client.mint(&owner, &1_000);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: accounts.clone(),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.distribute_usdc_signed(&request, &hash);
    assert!(result);
    assert_eq!(token.balance(&accounts.spending), 500);
    assert_eq!(token.balance(&accounts.savings), 300);
    assert_eq!(token.balance(&accounts.bills), 150);
    assert_eq!(token.balance(&accounts.insurance), 50);
    assert_eq!(client.get_nonce(&owner), 2);
}

#[test]
fn test_distribute_usdc_signed_deadline_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() - 1,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

#[test]
fn test_distribute_usdc_signed_hash_mismatch() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let wrong_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount + 1,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
}

// ============================================================================
// Execute Due Remittance Schedules Tests
// ============================================================================
// These tests verify the idempotent executor for remittance schedules.
// Key security properties: due/not-due partitioning, idempotency on repeated
// calls, InactiveSchedule skipping, and correct next_due advancement.
// ============================================================================

#[test]
fn test_execute_due_remittance_schedules_basic() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a one-shot schedule due at time 3000
    let schedule_id = client.create_remittance_schedule(&owner, &1_000, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Advance time past due date
    set_time(&env, 3_500);

    // Execute due schedules
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify schedule is now inactive (one-off)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(!schedule.active);
        assert_eq!(schedule.last_executed, Some(3_500));
    }
}

#[test]
fn test_execute_recurring_remittance_schedule() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a recurring schedule: 1000 amount, due at 3000, every 86400 seconds
    let schedule_id = client.create_remittance_schedule(&owner, &1_000, &3_000, &86_400);
    assert_eq!(schedule_id, Ok(1));

    // Advance time past first due date
    set_time(&env, 3_500);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify next_due was advanced by interval
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.next_due, 3_000 + 86_400);
        assert_eq!(schedule.last_executed, Some(3_500));
        assert_eq!(schedule.missed_count, 0);
    }
}

#[test]
fn test_execute_missed_remittance_schedules() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a recurring schedule
    let schedule_id = client.create_remittance_schedule(&owner, &500, &3_000, &86_400);
    assert_eq!(schedule_id, Ok(1));

    // Advance time far past multiple intervals: 3000 + 86400*3 + 100
    set_time(&env, 3_000 + 86_400 * 3 + 100);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    // Verify missed_count is 3 (the three intervals that were skipped)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert_eq!(schedule.missed_count, 3);
        assert!(schedule.next_due > 3_000 + 86_400 * 3);
        assert_eq!(schedule.last_executed, Some(3_000 + 86_400 * 3 + 100));
    }
}

#[test]
fn test_execute_idempotent_oneshot() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create one-shot schedule
    let schedule_id = client.create_remittance_schedule(&owner, &750, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Advance time past due
    set_time(&env, 3_500);

    // First execution
    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);
    let first_id = first.get(0);
    assert!(first_id.is_some());
    if let Some(id) = first_id {
        assert_eq!(id, 1);
    }

    // Second execution at same timestamp must be idempotent (no-op)
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0, "Second call must be a no-op");

    // Verify schedule remains inactive
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(!schedule.active);
        assert_eq!(schedule.last_executed, Some(3_500));
    }
}

#[test]
fn test_execute_idempotent_recurring() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create recurring schedule
    let schedule_id = client.create_remittance_schedule(&owner, &300, &3_000, &86_400);
    assert_eq!(schedule_id, Ok(1));

    set_time(&env, 3_500);

    // First execution
    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);

    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    let first_next_due = if let Some(schedule) = schedule_result {
        schedule.next_due
    } else {
        panic!("Schedule not found");
    };

    // Second execution at same timestamp must not re-execute
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0);

    // Verify next_due unchanged (idempotent advancement)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert_eq!(schedule.next_due, first_next_due);
    }
}

#[test]
fn test_execute_skips_inactive_schedules() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule and cancel it
    let schedule_id = client.create_remittance_schedule(&owner, &200, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));
    
    client.cancel_remittance_schedule(&owner, &1);

    // Advance past due time
    set_time(&env, 3_500);

    // Execute should skip inactive schedule
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_skips_not_yet_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule due at 3000
    let schedule_id = client.create_remittance_schedule(&owner, &400, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Advance time but stay before due date
    set_time(&env, 2_500);

    // Execute should not execute (not yet due)
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);

    // Verify schedule unchanged
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.last_executed, None);
    }
}

#[test]
fn test_execute_exactly_equal_next_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &600, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Advance exactly to next_due (edge case: == not just >)
    set_time(&env, 3_000);

    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1, "Should execute when time == next_due");
}

#[test]
fn test_execute_empty_schedule_set() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // No schedules created; just advance time
    set_time(&env, 5_000);

    // Execute on empty set should return empty Vec
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_all_inactive_set() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create and cancel multiple schedules
    for i in 1..=3 {
        let id = client.create_remittance_schedule(&owner, &100 * i as i128, &(3_000 + i as u64 * 1000), &0);
        assert!(id.is_ok());
        client.cancel_remittance_schedule(&owner, &(i as u32));
    }

    set_time(&env, 6_000);

    // Execute should return empty (all inactive)
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_paused_contract_returns_empty() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &500, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Pause contract
    let result = client.try_pause(&owner);
    assert!(result.is_ok());

    set_time(&env, 3_500);

    // Execute should return empty when paused
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);

    // Verify schedule was NOT executed (unchanged)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.last_executed, None);
    }
}

#[test]
fn test_execute_mixed_due_not_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule 1: due at 2000 (one-off)
    let id1 = client.create_remittance_schedule(&owner, &100, &2_000, &0);
    assert_eq!(id1, Ok(1));

    // Create schedule 2: due at 4000 (one-off)
    let id2 = client.create_remittance_schedule(&owner, &200, &4_000, &0);
    assert_eq!(id2, Ok(2));

    // Advance to time 3000 (only schedule 1 is due)
    set_time(&env, 3_000);

    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify only schedule 1 is inactive
    let schedule1_result = client.get_remittance_schedule(&1);
    assert!(schedule1_result.is_some());
    if let Some(schedule1) = schedule1_result {
        assert!(!schedule1.active);
    }

    let schedule2_result = client.get_remittance_schedule(&2);
    assert!(schedule2_result.is_some());
    if let Some(schedule2) = schedule2_result {
        assert!(schedule2.active);
    }
}

// ============================================================================
// Pause-Path Coverage Tests (Issue #612)
// ============================================================================
// Verify that each state-mutating entrypoint refuses to mutate while paused

#[test]
fn test_update_split_rejected_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Pause contract
    client.pause(&owner).unwrap();

    // Attempt to update split while paused — should fail with Unauthorized
    let nonce = client.get_split_nonce(&owner);
    let result = client.update_split(&owner, &nonce, &40, &35, &15, &10);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        RemittanceSplitError::Unauthorized,
        "update_split should reject when paused"
    );

    // Verify config unchanged
    let config = client.get_split_config().unwrap();
    assert_eq!(config.spending_percent, 50);
}

#[test]
fn test_create_remittance_schedule_rejected_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Pause contract
    client.pause(&owner).unwrap();

    // Attempt to create schedule while paused — should fail with Unauthorized
    let result = client.create_remittance_schedule(&owner, &100, &3_000, &0);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        RemittanceSplitError::Unauthorized,
        "create_remittance_schedule should reject when paused"
    );
}

#[test]
fn test_modify_remittance_schedule_rejected_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule first (while not paused)
    let schedule_id = client.create_remittance_schedule(&owner, &200, &3_000, &0);
    assert!(schedule_id.is_ok());

    // Pause contract
    client.pause(&owner).unwrap();

    // Attempt to modify schedule while paused — should fail with Unauthorized
    let result = client.modify_remittance_schedule(&owner, &1, &250, &4_000, &0);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        RemittanceSplitError::Unauthorized,
        "modify_remittance_schedule should reject when paused"
    );

    // Verify schedule unchanged
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert_eq!(schedule.amount, 200);
}

#[test]
fn test_cancel_remittance_schedule_rejected_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule first (while not paused)
    let schedule_id = client.create_remittance_schedule(&owner, &300, &3_000, &0);
    assert!(schedule_id.is_ok());

    // Pause contract
    client.pause(&owner).unwrap();

    // Attempt to cancel schedule while paused — should fail with Unauthorized
    let result = client.cancel_remittance_schedule(&owner, &1);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        RemittanceSplitError::Unauthorized,
        "cancel_remittance_schedule should reject when paused"
    );

    // Verify schedule still active
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(schedule.active);
}

#[test]
fn test_import_snapshot_rejected_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Export initial snapshot
    let snapshot = client.export_snapshot(&owner).unwrap();

    // Pause contract
    client.pause(&owner).unwrap();

    // Attempt to import snapshot while paused — should fail with Unauthorized
    let nonce = client.get_split_nonce(&owner);
    let result = client.import_snapshot(&owner, &nonce, &snapshot);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        RemittanceSplitError::Unauthorized,
        "import_snapshot should reject when paused"
    );
}

#[test]
fn test_getters_remain_callable_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &500, &3_000, &0);
    assert_eq!(schedule_id, Ok(1));

    // Pause contract
    client.pause(&owner).unwrap();

    // All read-only operations should succeed while paused
    assert!(client.is_paused().unwrap());
    assert!(client.get_split_config().is_ok());
    assert!(client.get_split().is_ok());
    assert!(client.get_remittance_schedule(&1).is_ok());
    assert!(client.export_snapshot(&owner).is_ok());
    assert_eq!(client.execute_due_remittance_schedules().len(), 0);

    // Verify nonce is still retrievable
    let nonce = client.get_split_nonce(&owner);
    assert!(nonce > 0);
}

#[test]
fn test_paused_then_unpaused_resumes_normally() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Verify we can mutate before pause
    let nonce1 = client.get_split_nonce(&owner);
    assert!(client.update_split(&owner, &nonce1, &45, &30, &15, &10).is_ok());

    // Pause
    client.pause(&owner).unwrap();
    assert!(client.is_paused().unwrap());

    // Verify mutation is blocked while paused
    let nonce2 = client.get_split_nonce(&owner);
    let result = client.update_split(&owner, &nonce2, &40, &35, &15, &10);
    assert!(result.is_err());

    // Unpause
    client.unpause(&owner).unwrap();
    assert!(!client.is_paused().unwrap());

    // Verify mutation works again
    let nonce3 = client.get_split_nonce(&owner);
    let result = client.update_split(&owner, &nonce3, &40, &35, &15, &10);
    assert!(result.is_ok());

    // Verify final state
    let config = client.get_split_config().unwrap();
    assert_eq!(config.spending_percent, 40);
}

// Helper function to invoke execute_due_remittance_schedules via client
// (Note: You may need to add this to the RemittanceSplitClient or call directly)
pub fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

// ============================================================================
// Set Pause Admin Tests
// ============================================================================

#[test]
fn test_set_pause_admin_by_owner() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let new_pause_admin = Address::generate(&env);
    
    // Owner can set pause admin
    let result = client.try_set_pause_admin(&owner, &new_pause_admin);
    assert!(result.is_ok(), "Owner should be able to set pause admin");

    // Verify storage mutation
    assert_eq!(client.get_pause_admin_public(), Some(new_pause_admin.clone()));

    // Verify event emission
    let events = env.events().all();
    let adm_xfr_event = events
        .iter()
        .find(|event| {
            let topics = &event.1;
            topics.len() == 2 && topics.get(1) == Some(&symbol_short!("adm_xfr"))
        });
    assert!(adm_xfr_event.is_some(), "adm_xfr event should be emitted");

    if let Some(event) = adm_xfr_event {
        let (_, _, data) = event;
        let parse_result = data.try_into_val::<(Option<Address>, Address)>(&env);
        assert!(parse_result.is_ok(), "Event data should be parseable as (Option<Address>, Address)");
        if let Ok((old_admin, new_admin)) = parse_result {
            assert_eq!(old_admin, None); // No previous pause admin
            assert_eq!(new_admin, new_pause_admin);
        }
    }
}

#[test]
fn test_set_pause_admin_unauthorized_caller() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let unauthorized_caller = Address::generate(&env);
    let new_pause_admin = Address::generate(&env);

    // Record initial state
    let initial_pause_admin = client.get_pause_admin_public();

    // Unauthorized caller should be rejected
    let result = client.try_set_pause_admin(&unauthorized_caller, &new_pause_admin);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));

    // Verify no storage mutation occurred
    assert_eq!(client.get_pause_admin_public(), initial_pause_admin);
}

#[test]
fn test_set_pause_admin_self_transfer() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let pause_admin = Address::generate(&env);
    let result = client.try_set_pause_admin(&owner, &pause_admin);
assert!(result.is_ok());

    let initial_pause_admin = client.get_pause_admin_public();

    // Self-transfer should be idempotent (allowed but no change)
    let result = client.try_set_pause_admin(&owner, &pause_admin);
assert!(result.is_ok());

    // Verify storage unchanged (idempotent)
    assert_eq!(client.get_pause_admin_public(), initial_pause_admin);
}

#[test]
fn test_set_pause_admin_double_transfer() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let pause_admin1 = Address::generate(&env);
    let pause_admin2 = Address::generate(&env);

    // First transfer
    let result = client.try_set_pause_admin(&owner, &pause_admin1);
    assert!(result.is_ok());
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin1.clone()));

    // Second transfer
    let result = client.try_set_pause_admin(&owner, &pause_admin2);
    assert!(result.is_ok());
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin2.clone()));

    // Verify two events were emitted
    let events = env.events().all();
    let adm_xfr_events: Vec<_> = events
        .iter()
        .filter(|event| {
            let topics = &event.1;
            topics.len() == 2 && topics.get(1) == Some(&symbol_short!("adm_xfr"))
        })
        .collect();

    assert_eq!(adm_xfr_events.len(), 2);
}

#[test]
fn test_set_pause_admin_when_paused() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let pause_admin = Address::generate(&env);
    let result = client.try_set_pause_admin(&owner, &pause_admin);
assert!(result.is_ok());

    // Pause the contract
    let result = client.try_pause(&pause_admin);
    assert!(result.is_ok());
    assert!(client.is_paused());

    let new_pause_admin = Address::generate(&env);

    // Transfer should fail when contract is paused
    let result = client.try_set_pause_admin(&owner, &new_pause_admin);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));

    // Verify no storage mutation
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin));
}

// ============================================================================
// Set Upgrade Admin Tests
// ============================================================================

#[test]
fn test_set_upgrade_admin_by_owner_initial() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let new_upgrade_admin = Address::generate(&env);

    // Owner can set initial upgrade admin
    let result = client.try_set_upgrade_admin(&owner, &new_upgrade_admin);
    assert!(result.is_ok());

    // Verify storage mutation
    assert_eq!(client.get_upgrade_admin_public(), Some(new_upgrade_admin.clone()));

    // Verify event emission
    let events = env.events().all();
    let adm_xfr_event = events
        .iter()
        .find(|event| {
            let topics = &event.1;
            topics.len() == 2 && topics.get(1) == Some(&symbol_short!("adm_xfr"))
        })
        ;
    assert!(adm_xfr_event.is_some(), "adm_xfr event should be emitted");

    if let Some(event) = adm_xfr_event {
        let (_, _, data) = event;
        let parse_result = data.try_into_val::<(Option<Address>, Address)>(&env);
        assert!(parse_result.is_ok(), "Event data should be parseable");
        if let Ok((old_admin, new_admin)) = parse_result {
            assert_eq!(old_admin, None); // No previous upgrade admin
            assert_eq!(new_admin, new_upgrade_admin);
        }
    }
}

#[test]
fn test_set_upgrade_admin_by_current_admin() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let upgrade_admin1 = Address::generate(&env);
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin1);
    assert!(result.is_ok());

    let upgrade_admin2 = Address::generate(&env);

    // Current admin can transfer to new admin
    let result = client.try_set_upgrade_admin(&upgrade_admin1, &upgrade_admin2);
    assert!(result.is_ok());

    // Verify storage mutation
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin2.clone()));

    // Verify event emission
    let events = env.events().all();
    let adm_xfr_events: Vec<_> = events
        .iter()
        .filter(|event| {
            let topics = &event.1;
            topics.len() == 2 && topics.get(1) == Some(&symbol_short!("adm_xfr"))
        })
        .collect();

    assert_eq!(adm_xfr_events.len(), 2);

    // Check the second event (transfer from admin1 to admin2)
    let (_, _, data) = &adm_xfr_events[1];
    let parse_result = data.try_into_val::<(Option<Address>, Address)>(&env);
    assert!(parse_result.is_ok(), "Event data should be parseable");
    if let Ok((old_admin, new_admin)) = parse_result {
        assert_eq!(old_admin, Some(upgrade_admin1));
        assert_eq!(new_admin, upgrade_admin2);
    }
}

#[test]
fn test_set_upgrade_admin_unauthorized_caller() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    // Test 1: Unauthorized caller when no admin is set
    let unauthorized_caller = Address::generate(&env);
    let new_upgrade_admin = Address::generate(&env);

    let result = client.try_set_upgrade_admin(&unauthorized_caller, &new_upgrade_admin);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
    assert_eq!(client.get_upgrade_admin_public(), None);

    // Test 2: Set an admin first
    let upgrade_admin = Address::generate(&env);
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin);
assert!(result.is_ok());

    // Test 3: Unauthorized caller when admin is set
    let result = client.try_set_upgrade_admin(&unauthorized_caller, &new_upgrade_admin);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin));
}

#[test]
fn test_set_upgrade_admin_self_transfer() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let upgrade_admin = Address::generate(&env);
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin);
assert!(result.is_ok());

    let initial_upgrade_admin = client.get_upgrade_admin_public();

    // Self-transfer should be idempotent (allowed but no change)
    let result = client.try_set_upgrade_admin(&upgrade_admin, &upgrade_admin);
    assert!(result.is_ok());

    // Verify storage unchanged (idempotent)
    assert_eq!(client.get_upgrade_admin_public(), initial_upgrade_admin);
}

#[test]
fn test_set_upgrade_admin_double_transfer() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let upgrade_admin1 = Address::generate(&env);
    let upgrade_admin2 = Address::generate(&env);

    // First transfer by owner
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin1);
    assert!(result.is_ok());
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin1.clone()));

    // Second transfer by admin1
    let result = client.try_set_upgrade_admin(&upgrade_admin1, &upgrade_admin2);
    assert!(result.is_ok());
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin2));

    // Verify two admin transfer events were emitted
    let events = env.events().all();
    let adm_xfr_events: Vec<_> = events
        .iter()
        .filter(|event| {
            let topics = &event.1;
            topics.len() == 2 && topics.get(1) == Some(&symbol_short!("adm_xfr"))
        })
        .collect();

    assert_eq!(adm_xfr_events.len(), 2);

    // Check the first event (transfer from owner to admin1)
    let (_, _, data) = &adm_xfr_events[0];
    let parse_result = data.try_into_val::<(Option<Address>, Address)>(&env);
    assert!(parse_result.is_ok(), "Event data should be parseable");
    if let Ok((old_admin, new_admin)) = parse_result {
        assert_eq!(old_admin, None); // No previous upgrade admin
        assert_eq!(new_admin, upgrade_admin1);
    }

    // Check the second event (transfer from admin1 to admin2)
    let (_, _, data) = &adm_xfr_events[1];
    let parse_result = data.try_into_val::<(Option<Address>, Address)>(&env);
    assert!(parse_result.is_ok(), "Event data should be parseable");
    if let Ok((old_admin, new_admin)) = parse_result {
        assert_eq!(old_admin, Some(upgrade_admin1));
        assert_eq!(new_admin, upgrade_admin2);
    }
}

#[test]
fn test_set_upgrade_admin_owner_cannot_override_after_initial_set() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let upgrade_admin1 = Address::generate(&env);
    let upgrade_admin2 = Address::generate(&env);

    // Owner sets initial admin
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin1);
    assert!(result.is_ok());
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin1.clone()));

    // Owner should NOT be able to override once admin is set
    let result = client.try_set_upgrade_admin(&owner, &upgrade_admin2);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));

    // Verify no storage mutation
    assert_eq!(client.get_upgrade_admin_public(), Some(upgrade_admin1));
}
