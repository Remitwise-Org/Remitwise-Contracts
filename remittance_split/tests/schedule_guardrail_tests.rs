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
