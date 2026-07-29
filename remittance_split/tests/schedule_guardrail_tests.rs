#![cfg(test)]

use remittance_split::{
    ExportSnapshot, RemittanceSchedule, RemittanceSplit, RemittanceSplitClient,
    RemittanceSplitError, SplitConfig, MAX_SCHEDULE_LEAD_TIME, MIN_SCHEDULE_INTERVAL,
};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

fn dummy_token(env: &Env) -> Address {
    Address::generate(env)
}

fn init(
    client: &RemittanceSplitClient,
    env: &Env,
    owner: &Address,
    s: u32,
    g: u32,
    b: u32,
    i: u32,
) {
    let token = dummy_token(env);
    client.initialize_split(owner, &0, &token, &s, &g, &b, &i);
}

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    init(&client, &env, &owner, 50, 30, 15, 5);
    (env, contract_id, owner)
}

fn checksum(version: u32, config: &SplitConfig, schedules: &Vec<RemittanceSchedule>) -> u64 {
    (version as u64)
        .wrapping_add(config.spending_percent as u64)
        .wrapping_add(config.savings_percent as u64)
        .wrapping_add(config.bills_percent as u64)
        .wrapping_add(config.insurance_percent as u64)
        .wrapping_add(schedules.len() as u64)
        .wrapping_mul(31)
}

fn snapshot_with_schedule(
    env: &Env,
    owner: &Address,
    schedule: RemittanceSchedule,
) -> ExportSnapshot {
    let config = SplitConfig {
        owner: owner.clone(),
        spending_percent: 50,
        savings_percent: 30,
        bills_percent: 15,
        insurance_percent: 5,
        timestamp: env.ledger().timestamp(),
        initialized: true,
        usdc_contract: dummy_token(env),
    };
    let mut schedules = Vec::new(env);
    schedules.push_back(schedule);
    let checksum = checksum(2, &config, &schedules);
    ExportSnapshot {
        schema_version: 2,
        checksum,
        config,
        schedules,
        exported_at: env.ledger().timestamp(),
    }
}

#[test]
fn test_create_schedule_one_off_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let schedule_id = client.create_remittance_schedule(&owner, &1000, &next_due, &0);
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.interval, 0);
    assert!(!schedule.recurring);
}

#[test]
fn test_create_schedule_at_min_interval_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let schedule_id =
        client.create_remittance_schedule(&owner, &1000, &next_due, &MIN_SCHEDULE_INTERVAL);
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.interval, MIN_SCHEDULE_INTERVAL);
    assert!(schedule.recurring);
}

#[test]
fn test_create_schedule_below_min_interval_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let result = client.try_create_remittance_schedule(
        &owner,
        &1000,
        &next_due,
        &(MIN_SCHEDULE_INTERVAL - 1),
    );
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort))
    );
}

#[test]
fn test_create_schedule_above_min_interval_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let schedule_id =
        client.create_remittance_schedule(&owner, &1000, &next_due, &(MIN_SCHEDULE_INTERVAL * 2));
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.interval, MIN_SCHEDULE_INTERVAL * 2);
    assert!(schedule.recurring);
}

#[test]
fn test_modify_schedule_below_min_interval_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(env.ledger().timestamp() + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    let result = client.try_modify_remittance_schedule(
        &owner,
        &schedule_id,
        &1000,
        &(env.ledger().timestamp() + 2),
        &(MIN_SCHEDULE_INTERVAL - 1),
    );
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort))
    );
}

#[test]
fn test_modify_schedule_to_one_off_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(env.ledger().timestamp() + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    client.modify_remittance_schedule(
        &owner,
        &schedule_id,
        &1000,
        &(env.ledger().timestamp() + 2),
        &0,
    );
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.interval, 0);
    assert!(!schedule.recurring);
}

#[test]
fn test_create_schedule_at_max_lead_time_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME;
    let schedule_id =
        client.create_remittance_schedule(&owner, &1000, &next_due, &MIN_SCHEDULE_INTERVAL);
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, next_due);
}

#[test]
fn test_create_schedule_beyond_max_lead_time_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME + 1;
    let result =
        client.try_create_remittance_schedule(&owner, &1000, &next_due, &MIN_SCHEDULE_INTERVAL);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleLeadTimeTooLong))
    );
}

#[test]
fn test_modify_schedule_beyond_max_lead_time_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(env.ledger().timestamp() + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    let result = client.try_modify_remittance_schedule(
        &owner,
        &schedule_id,
        &1000,
        &(env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleLeadTimeTooLong))
    );
}

#[test]
fn test_import_snapshot_rejects_short_interval_schedule() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + 1,
            interval: MIN_SCHEDULE_INTERVAL - 1,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    let result = client.try_import_snapshot(&owner, &1, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort))
    );
}

#[test]
fn test_import_snapshot_rejects_far_future_schedule() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME + 1,
            interval: MIN_SCHEDULE_INTERVAL,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    let result = client.try_import_snapshot(&owner, &1, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleLeadTimeTooLong))
    );
}

#[test]
fn test_import_snapshot_allows_inactive_schedule_with_short_interval() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + 1,
            interval: 1,
            recurring: true,
            active: false,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    assert!(client.import_snapshot(&owner, &1, &snapshot));
}

#[test]
fn test_create_schedule_interval_one_second_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let result = client.try_create_remittance_schedule(&owner, &1000, &next_due, &1);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort))
    );
}

// ---------------------------------------------------------------------------
// Exact MIN/MAX edge boundary tests
// ---------------------------------------------------------------------------

/// Constant value pin: MIN_SCHEDULE_INTERVAL must be exactly 3 600 seconds (1 hour).
/// Any change to this constant is a breaking protocol change and must be deliberate.
#[test]
fn test_min_schedule_interval_constant_value() {
    assert_eq!(
        MIN_SCHEDULE_INTERVAL, 3_600,
        "MIN_SCHEDULE_INTERVAL must be exactly 3600 seconds (1 hour)"
    );
}

/// Constant value pin: MAX_SCHEDULE_LEAD_TIME must be exactly 31 536 000 seconds (365 days).
/// Any change to this constant is a breaking protocol change and must be deliberate.
#[test]
fn test_max_schedule_lead_time_constant_value() {
    assert_eq!(
        MAX_SCHEDULE_LEAD_TIME,
        365 * 24 * 3_600,
        "MAX_SCHEDULE_LEAD_TIME must be exactly 365 * 24 * 3600 seconds (1 year)"
    );
}

/// Exact MIN edge on modify: interval == MIN_SCHEDULE_INTERVAL must be accepted.
#[test]
fn test_modify_schedule_at_min_interval_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(env.ledger().timestamp() + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    // Modify to the same exact MIN — must succeed.
    client.modify_remittance_schedule(
        &owner,
        &schedule_id,
        &2000,
        &(env.ledger().timestamp() + 2),
        &MIN_SCHEDULE_INTERVAL,
    );
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.interval, MIN_SCHEDULE_INTERVAL);
    assert!(schedule.recurring);
}

/// Exact MAX edge on modify: next_due == now + MAX_SCHEDULE_LEAD_TIME must be accepted.
#[test]
fn test_modify_schedule_at_max_lead_time_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let schedule_id = client.create_remittance_schedule(
        &owner,
        &1000,
        &(env.ledger().timestamp() + 1),
        &MIN_SCHEDULE_INTERVAL,
    );
    let max_due = env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME;
    client.modify_remittance_schedule(
        &owner,
        &schedule_id,
        &1000,
        &max_due,
        &MIN_SCHEDULE_INTERVAL,
    );
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, max_due);
}

/// Exact MIN edge on import: active schedule with interval == MIN_SCHEDULE_INTERVAL must be accepted.
#[test]
fn test_import_snapshot_at_min_interval_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + 1,
            interval: MIN_SCHEDULE_INTERVAL,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    assert!(
        client.import_snapshot(&owner, &1, &snapshot),
        "import with interval == MIN_SCHEDULE_INTERVAL must succeed"
    );
}

/// Exact MAX edge on import: active schedule with next_due == now + MAX_SCHEDULE_LEAD_TIME must be accepted.
#[test]
fn test_import_snapshot_at_max_lead_time_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME,
            interval: MIN_SCHEDULE_INTERVAL,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    assert!(
        client.import_snapshot(&owner, &1, &snapshot),
        "import with next_due == now + MAX_SCHEDULE_LEAD_TIME must succeed"
    );
}

/// MIN-2 is also rejected (not just MIN-1), confirming the boundary is a hard floor.
#[test]
fn test_create_schedule_interval_min_minus_2_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let result = client.try_create_remittance_schedule(
        &owner,
        &1000,
        &next_due,
        &(MIN_SCHEDULE_INTERVAL - 2),
    );
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort)),
        "interval == MIN-2 must also be rejected"
    );
}

/// MAX+2 is also rejected (not just MAX+1), confirming the boundary is a hard ceiling.
#[test]
fn test_create_schedule_lead_time_max_plus_2_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME + 2;
    let result =
        client.try_create_remittance_schedule(&owner, &1000, &next_due, &MIN_SCHEDULE_INTERVAL);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ScheduleLeadTimeTooLong)),
        "next_due == now + MAX+2 must also be rejected"
    );
}

/// next_due == now + 1 is the minimum valid lead time (just one second in the future).
#[test]
fn test_create_schedule_next_due_one_second_ahead_allowed() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let next_due = env.ledger().timestamp() + 1;
    let schedule_id =
        client.create_remittance_schedule(&owner, &1000, &next_due, &MIN_SCHEDULE_INTERVAL);
    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, next_due);
}

/// Exact MIN-1 on import (active) is rejected — mirrors the create path.
#[test]
fn test_import_snapshot_interval_min_minus_1_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + 1,
            interval: MIN_SCHEDULE_INTERVAL - 1,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    assert_eq!(
        client.try_import_snapshot(&owner, &1, &snapshot),
        Err(Ok(RemittanceSplitError::ScheduleIntervalTooShort)),
        "import with interval == MIN-1 must be rejected"
    );
}

/// Exact MAX+1 on import (active) is rejected — mirrors the create path.
#[test]
fn test_import_snapshot_lead_time_max_plus_1_rejected() {
    let (env, contract_id, owner) = setup();
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let snapshot = snapshot_with_schedule(
        &env,
        &owner,
        RemittanceSchedule {
            id: 1,
            owner: owner.clone(),
            amount: 1000,
            next_due: env.ledger().timestamp() + MAX_SCHEDULE_LEAD_TIME + 1,
            interval: MIN_SCHEDULE_INTERVAL,
            recurring: true,
            active: true,
            created_at: env.ledger().timestamp(),
            last_executed: None,
            missed_count: 0,
        },
    );
    assert_eq!(
        client.try_import_snapshot(&owner, &1, &snapshot),
        Err(Ok(RemittanceSplitError::ScheduleLeadTimeTooLong)),
        "import with next_due == now + MAX+1 must be rejected"
    );
}
