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
    let last_event = events.last().expect("no events emitted");
    let (_, topics, data) = last_event;

    assert_eq!(topics.len(), 4);

    let event: DistributionCompletedEvent = DistributionCompletedEvent::try_from_val(&env, &data)
        .expect("failed to decode distribution event");

    assert_eq!(event.from, owner);
    assert_eq!(event.total_amount, total_amount);
    assert_eq!(event.spending_amount, 400);
    assert_eq!(event.savings_amount, 300);
    assert_eq!(event.bills_amount, 200);
    assert_eq!(event.insurance_amount, 100);
    assert_eq!(event.timestamp, env.ledger().timestamp());
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
        .find(|event| event.1.len() == 4)
        .expect("distribution completed event not found");

    assert_eq!(dist_comp_event.1.len(), 4);
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
    assert_eq!(executed.get(0).unwrap(), 1);

    // Verify schedule is now inactive (one-off)
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(!schedule.active);
    assert_eq!(schedule.last_executed, Some(3_500));
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
    assert_eq!(executed.get(0).unwrap(), 1);

    // Verify next_due was advanced by interval
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.next_due, 3_000 + 86_400);
    assert_eq!(schedule.last_executed, Some(3_500));
    assert_eq!(schedule.missed_count, 0);
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
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert_eq!(schedule.missed_count, 3);
    assert!(schedule.next_due > 3_000 + 86_400 * 3);
    assert_eq!(schedule.last_executed, Some(3_000 + 86_400 * 3 + 100));
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
    assert_eq!(first.get(0).unwrap(), 1);

    // Second execution at same timestamp must be idempotent (no-op)
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0, "Second call must be a no-op");

    // Verify schedule remains inactive
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(!schedule.active);
    assert_eq!(schedule.last_executed, Some(3_500));
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

    let first_next_due = client.get_remittance_schedule(&1).unwrap().next_due;

    // Second execution at same timestamp must not re-execute
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0);

    // Verify next_due unchanged (idempotent advancement)
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert_eq!(schedule.next_due, first_next_due);
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
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.last_executed, None);
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
    client.pause(&owner).unwrap();

    set_time(&env, 3_500);

    // Execute should return empty when paused
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);

    // Verify schedule was NOT executed (unchanged)
    let schedule = client.get_remittance_schedule(&1).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.last_executed, None);
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
    assert_eq!(executed.get(0).unwrap(), 1);

    // Verify only schedule 1 is inactive
    assert!(!client.get_remittance_schedule(&1).unwrap().active);
    assert!(client.get_remittance_schedule(&2).unwrap().active);
}

// Helper function to invoke execute_due_remittance_schedules via client
// (Note: You may need to add this to the RemittanceSplitClient or call directly)
pub fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}
