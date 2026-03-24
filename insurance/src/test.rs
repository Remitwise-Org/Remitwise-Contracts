#![cfg(test)]

use super::*;
use crate::InsuranceError;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger, LedgerInfo},
    Address, Env, String,
};

fn set_time(env: &Env, timestamp: u64) {
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
}

fn make_env() -> Env {
    let env = Env::default();
    set_time(&env, 1_000_000);
    env
}

#[test]
fn test_create_policy_succeeds() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );
    assert_eq!(policy_id, 1);
    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.monthly_premium, 100);
    assert_eq!(policy.coverage_amount, 10000);
    assert!(policy.active);
}

#[test]
fn test_create_policy_invalid_premium() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "health"),
        &0,
        &10000,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_create_policy_invalid_coverage() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "health"),
        &100,
        &0,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_pay_premium() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_due = initial_policy.next_payment_date;

    set_time(&env, 1_001_000);
    client.pay_premium(&owner, &policy_id);

    let updated_policy = client.get_policy(&policy_id).unwrap();
    assert!(updated_policy.next_payment_date > initial_due);
}

#[test]
fn test_pay_premium_unauthorized() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    let result = client.try_pay_premium(&other, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

#[test]
fn test_deactivate_policy() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    let success = client.deactivate_policy(&owner, &policy_id);
    assert!(success);
    let policy = client.get_policy(&policy_id).unwrap();
    assert!(!policy.active);
}

#[test]
fn test_get_active_policies() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let p1 = client.create_policy(&owner, &String::from_str(&env, "P1"), &String::from_str(&env, "health"), &100, &1000, &None);
    let p2 = client.create_policy(&owner, &String::from_str(&env, "P2"), &String::from_str(&env, "life"), &200, &2000, &None);
    client.create_policy(&owner, &String::from_str(&env, "P3"), &String::from_str(&env, "auto"), &300, &3000, &None);

    client.deactivate_policy(&owner, &p2);

    let page = client.get_active_policies(&owner, &0, &10);
    assert_eq!(page.count, 2);
    for p in page.items.iter() {
        assert!(p.active);
        assert_ne!(p.id, p2);
    }
    let _ = p1;
}

#[test]
fn test_get_active_policies_excludes_deactivated() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id_1 = client.create_policy(&owner, &String::from_str(&env, "Policy 1"), &String::from_str(&env, "health"), &100, &1000, &None);
    let policy_id_2 = client.create_policy(&owner, &String::from_str(&env, "Policy 2"), &String::from_str(&env, "life"), &200, &2000, &None);

    client.deactivate_policy(&owner, &policy_id_1);

    let active = client.get_active_policies(&owner, &0, &10);
    assert_eq!(active.count, 1);
    let only = active.items.get(0).unwrap();
    assert_eq!(only.id, policy_id_2);
    assert!(only.active);
}

#[test]
fn test_get_total_monthly_premium() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    client.create_policy(&owner, &String::from_str(&env, "P1"), &String::from_str(&env, "health"), &100, &1000, &None);
    client.create_policy(&owner, &String::from_str(&env, "P2"), &String::from_str(&env, "life"), &200, &2000, &None);

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 300);
}

#[test]
fn test_get_total_monthly_premium_zero_policies() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 0);
}

#[test]
fn test_get_total_monthly_premium_deactivated_excluded() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy1 = client.create_policy(&owner, &String::from_str(&env, "P1"), &String::from_str(&env, "health"), &100, &1000, &None);
    client.create_policy(&owner, &String::from_str(&env, "P2"), &String::from_str(&env, "life"), &200, &2000, &None);

    assert_eq!(client.get_total_monthly_premium(&owner), 300);
    client.deactivate_policy(&owner, &policy1);
    assert_eq!(client.get_total_monthly_premium(&owner), 200);
}

#[test]
fn test_get_policy_nonexistent() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    env.mock_all_auths();

    let policy = client.get_policy(&999);
    assert!(policy.is_none());
}

#[test]
fn test_pay_premium_inactive_policy() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "P"), &String::from_str(&env, "health"), &100, &10000, &None);
    client.deactivate_policy(&owner, &policy_id);

    let result = client.try_pay_premium(&owner, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyInactive)));
}

#[test]
fn test_deactivate_policy_unauthorized() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "P"), &String::from_str(&env, "health"), &100, &10000, &None);
    let result = client.try_deactivate_policy(&other, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

#[test]
fn test_multiple_policies_same_owner() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let p1 = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &100, &10000, &None);
    let p2 = client.create_policy(&owner, &String::from_str(&env, "Life"), &String::from_str(&env, "life"), &200, &20000, &None);
    let p3 = client.create_policy(&owner, &String::from_str(&env, "Auto"), &String::from_str(&env, "auto"), &300, &30000, &None);

    assert_eq!(client.get_total_monthly_premium(&owner), 600);

    client.deactivate_policy(&owner, &p1);
    client.deactivate_policy(&owner, &p2);
    client.deactivate_policy(&owner, &p3);

    let active = client.get_active_policies(&owner, &0, &10);
    assert_eq!(active.count, 0);
    assert_eq!(client.get_total_monthly_premium(&owner), 0);
}

#[test]
fn test_create_premium_schedule_succeeds() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let next_due = 2_000_000u64; // future relative to make_env's 1_000_000
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, next_due);
    assert_eq!(schedule.interval, 2592000);
    assert!(schedule.active);
}

#[test]
fn test_modify_premium_schedule() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &2_000_000u64, &2592000);
    client.modify_premium_schedule(&owner, &schedule_id, &3_000_000u64, &2678400);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 3_000_000u64);
    assert_eq!(schedule.interval, 2678400);
}

#[test]
fn test_cancel_premium_schedule() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &2_000_000u64, &2592000);
    client.cancel_premium_schedule(&owner, &schedule_id);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

#[test]
fn test_execute_due_premium_schedules() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let next_due = 2_000_000u64;
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &0);

    set_time(&env, next_due + 500);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, next_due + 500 + 30 * 86400);
}

#[test]
fn test_execute_recurring_premium_schedule() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let next_due = 2_000_000u64;
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, next_due + 500);
    client.execute_due_premium_schedules();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.next_due, next_due + 2592000);
}

#[test]
fn test_get_premium_schedules() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let policy_id1 = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &500, &50000, &None);
    let policy_id2 = client.create_policy(&owner, &String::from_str(&env, "Life"), &String::from_str(&env, "life"), &300, &100000, &None);

    client.create_premium_schedule(&owner, &policy_id1, &2_000_000u64, &2592000);
    client.create_premium_schedule(&owner, &policy_id2, &3_000_000u64, &2592000);

    let schedules = client.get_premium_schedules(&owner);
    assert_eq!(schedules.len(), 2);
}

// ══════════════════════════════════════════════════════════════════════════
// Time & Ledger Drift Resilience Tests
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_time_drift_premium_schedule_not_executed_before_next_due() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let next_due = 2_000_000u64; // future relative to make_env's 1_000_000
    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Life"), &String::from_str(&env, "life"), &200, &100000, &None);
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, next_due - 1);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 0, "Must not execute one second before next_due");
}

#[test]
fn test_time_drift_premium_schedule_executes_at_exact_next_due() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let next_due = 2_000_000u64;
    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Health"), &String::from_str(&env, "health"), &150, &75000, &None);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, next_due);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1, "Must execute exactly at next_due");
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, next_due + 30 * 86400);
}

#[test]
fn test_time_drift_next_payment_date_uses_actual_payment_time() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let next_due = 2_000_000u64;
    let late_payment_time = next_due + 7 * 86400;
    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Property"), &String::from_str(&env, "property"), &300, &200000, &None);
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, late_payment_time);
    client.execute_due_premium_schedules();

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, late_payment_time + 30 * 86400);
    assert!(policy.next_payment_date > next_due + 30 * 86400);
}

#[test]
fn test_time_drift_no_double_execution_after_schedule_advances() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let next_due = 2_000_000u64;
    let interval = 2_592_000u64;
    let policy_id = client.create_policy(&owner, &String::from_str(&env, "Auto"), &String::from_str(&env, "auto"), &100, &50000, &None);
    client.create_premium_schedule(&owner, &policy_id, &next_due, &interval);

    set_time(&env, next_due);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1);

    set_time(&env, next_due + 1000);
    let executed_again = client.execute_due_premium_schedules();
    assert_eq!(executed_again.len(), 0, "Must not re-execute before new next_due");
}
