use super::*;
use crate::InsuranceError;
use soroban_sdk::{testutils::Address as AddressTrait, Address, Env, String};

use testutils::{set_ledger_time, setup_test_env};

#[test]
fn test_create_policy_succeeds() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = CoverageType::Health;

    // Health: min premium 1M, min coverage 10M
    let policy_id = client.create_policy(
        &owner,
        &name,
        &coverage_type,
        &1_000_000,  // monthly_premium
        &10_000_000, // coverage_amount
        &None,       // external_ref
    );

    assert_eq!(policy_id, 1);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.monthly_premium, 1_000_000);
    assert_eq!(policy.coverage_amount, 10_000_000);
    assert!(policy.active);
}

#[test]
fn test_create_policy_invalid_premium() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &0,
        &10_000_000,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidPremium)));
}

#[test]
fn test_create_policy_invalid_coverage() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &1_000_000,
        &0,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidCoverage)));
}

#[test]
fn test_pay_premium() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );

    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_due = initial_policy.next_payment_date;

    set_ledger_time(&env, 1, 1000); // 1000 seconds in

    client.pay_premium(&owner, &policy_id);

    let updated_policy = client.get_policy(&policy_id).unwrap();
    assert!(updated_policy.next_payment_date > initial_due);
}

#[test]
fn test_pay_premium_unauthorized() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    let other = Address::generate(&env);
    client.init(&owner);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );

    let result = client.try_pay_premium(&other, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

#[test]
fn test_deactivate_policy() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );

    let success = client.deactivate_policy(&owner, &policy_id);
    assert!(success);

    let policy = client.get_policy(&policy_id).unwrap();
    assert!(!policy.active);
}

#[test]
fn test_get_active_policies() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );
    let p2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );

    client.deactivate_policy(&owner, &p2);

    let active = client.get_active_policies(&owner, &0, &50);
    assert_eq!(active.count, 2);
}

#[test]
fn test_get_total_monthly_premium() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &CoverageType::Health,
        &2_000_000,
        &20_000_000,
        &None,
    );

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 3_000_000);
}

#[test]
fn test_create_premium_schedule_succeeds() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 3000);
    assert_eq!(schedule.interval, 2592000);
    assert!(schedule.active);
}

#[test]
fn test_execute_due_premium_schedules() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.init(&owner);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &1_000_000,
        &10_000_000,
        &None,
    );
    let _schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &0);

    set_ledger_time(&env, 1, 3500);
    let executed = client.execute_due_premium_schedules();

    assert_eq!(executed.len(), 1);

    let updated_policy = client.get_policy(&policy_id).unwrap();
    assert!(updated_policy.next_payment_date > 3500);
}
