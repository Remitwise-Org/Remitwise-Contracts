// #![cfg(test)] removed to avoid duplication with lib.rs mod test guard

use super::*;
fn set_time(env: &Env, timestamp: u64) {
    use testutils::set_ledger_time;
    set_ledger_time(env, 1u32, timestamp);
}

use crate::InsuranceError;
use soroban_sdk::IntoVal;
use soroban_sdk::{testutils::Address as AddressTrait, Address, Env, String};

use testutils::{set_ledger_time, setup_test_env};

// Removed local set_time in favor of testutils::set_ledger_time

#[test]
fn test_create_policy_succeeds() {
    setup_test_env!(env, Insurance, client, owner, InsuranceClient);

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "health");

    let policy_id = client.create_policy(
        &owner,
        &name,
        &coverage_type,
        &100,
        &10000, // coverage_amount
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
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "Type"),
        &0,
        &10000,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_create_policy_invalid_coverage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "Type"),
        &100,
        &0,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_pay_premium() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
        &100,
        &10000,
        &None,
    );

    // Initial next_payment_date is ~30 days from creation
    // We'll simulate passage of time is separate, but here we just check it updates
    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_due = initial_policy.next_payment_date;

    // Advance ledger time to simulate paying slightly later
    set_ledger_time(&env, 1, env.ledger().timestamp() + 1000);

    client.pay_premium(&owner, &policy_id);

    let updated_policy = client.get_policy(&policy_id).unwrap();

    // New validation logic: new due date should be current timestamp + 30 days
    // Since we advanced timestamp by 1000, the new due date should be > initial due date
    assert!(updated_policy.next_payment_date > initial_due);
}

#[test]
fn test_pay_premium_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
        &100,
        &10000,
        &None,
    );

    // unauthorized payer — use try_ to check for structured error
    let result = client.try_pay_premium(&other, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

#[test]
fn test_deactivate_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
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
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create 3 policies
    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &String::from_str(&env, "T1"),
        &100,
        &1000,
        &None,
    );
    let p2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "T2"),
        &200,
        &2000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &String::from_str(&env, "T3"),
        &300,
        &3000,
        &None,
    );

    // Deactivate P2
    client.deactivate_policy(&owner, &p2);

    let active = client.get_active_policies(&owner, &0, &10);
    assert_eq!(active.items.len(), 2);

    // Check specific IDs if needed, but length 2 confirms one was filtered
}

#[test]
fn test_get_active_policies_excludes_deactivated() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create policy 1 and policy 2 for the same owner
    let policy_id_1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &String::from_str(&env, "Type 1"),
        &100,
        &1000,
        &None,
    );
    let policy_id_2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "Type 2"),
        &200,
        &2000,
        &None,
    );

    // Deactivate policy 1
    client.deactivate_policy(&owner, &policy_id_1);

    // get_active_policies must return only the still-active policy
    let active = client.get_active_policies(&owner, &0, &DEFAULT_PAGE_LIMIT);
    assert_eq!(
        active.items.len(),
        1,
        "get_active_policies must return exactly one policy"
    );
    let only = active.items.get(0).unwrap();
    assert_eq!(
        only.id, policy_id_2,
        "the returned policy must be the active one (policy_id_2)"
    );
    assert!(only.active, "returned policy must have active == true");
}

#[test]
fn test_get_all_policies_for_owner_pagination() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    env.mock_all_auths();

    // Create 3 policies for owner
    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &String::from_str(&env, "T1"),
        &100,
        &1000,
        &None,
    );
    let p2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "T2"),
        &200,
        &2000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &String::from_str(&env, "T3"),
        &300,
        &3000,
        &None,
    );

    // Create 1 policy for other
    client.create_policy(
        &other,
        &String::from_str(&env, "Other P"),
        &String::from_str(&env, "Type"),
        &500,
        &5000,
        &None,
    );

    // Deactivate P2
    client.deactivate_policy(&owner, &p2);

    // get_all_policies_for_owner should return all 3 for owner
    let page = client.get_all_policies_for_owner(&owner, &0, &10);
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.count, 3);

    // verify p2 is in the list and is inactive
    let mut found_p2 = false;
    for policy in page.items.iter() {
        if policy.id == p2 {
            found_p2 = true;
            assert!(!policy.active);
        }
    }
    assert!(found_p2);
}

#[test]
fn test_get_total_monthly_premium() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &String::from_str(&env, "T1"),
        &100,
        &1000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "T2"),
        &200,
        &2000,
        &None,
    );

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 300);
}

#[test]
fn test_get_total_monthly_premium_zero_policies() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Fresh address with no policies
    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 0);
}

#[test]
fn test_get_total_monthly_premium_one_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create one policy with monthly_premium = 500
    client.create_policy(
        &owner,
        &String::from_str(&env, "Single Policy"),
        &String::from_str(&env, "health"),
        &500,
        &10000,
        &None,
    );

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 500);
}

#[test]
fn test_get_total_monthly_premium_multiple_active_policies() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create three policies with premiums 100, 200, 300
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &String::from_str(&env, "health"),
        &100,
        &1000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 3"),
        &String::from_str(&env, "auto"),
        &300,
        &3000,
        &None,
    );

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 600); // 100 + 200 + 300
}

#[test]
fn test_get_total_monthly_premium_deactivated_policy_excluded() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create two policies with premiums 100 and 200
    let policy1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &String::from_str(&env, "health"),
        &100,
        &1000,
        &None,
    );
    let _policy2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
        &None,
    );

    // Verify total includes both policies initially
    let total_initial = client.get_total_monthly_premium(&owner);
    assert_eq!(total_initial, 300); // 100 + 200

    // Deactivate the first policy
    client.deactivate_policy(&owner, &policy1);

    // Verify total only includes the active policy
    let total_after_deactivation = client.get_total_monthly_premium(&owner);
    assert_eq!(total_after_deactivation, 200); // Only policy 2
}

#[test]
fn test_get_total_monthly_premium_different_owner_isolation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    env.mock_all_auths();

    // Create policies for owner_a
    client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A1"),
        &String::from_str(&env, "health"),
        &100,
        &1000,
        &None,
    );
    client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
        &None,
    );

    // Create policies for owner_b
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "Policy B1"),
        &String::from_str(&env, "emergency"),
        &300,
        &3000,
        &None,
    );

    // Verify owner_a's total only includes their policies
    let total_a = client.get_total_monthly_premium(&owner_a);
    assert_eq!(total_a, 300); // 100 + 200

    // Verify owner_b's total only includes their policies
    let total_b = client.get_total_monthly_premium(&owner_b);
    assert_eq!(total_b, 300); // 300

    // Verify no cross-owner leakage
    assert_ne!(total_a, 0); // owner_a has policies
    assert_ne!(total_b, 0); // owner_b has policies
    assert_eq!(total_a, total_b); // Both have same total but different policies
}

#[test]
fn test_multiple_premium_payments() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "LongTerm"),
        &String::from_str(&env, "Life"),
        &100,
        &10000,
        &None,
    );

    let p1 = client.get_policy(&policy_id).unwrap();
    let first_due = p1.next_payment_date;

    // First payment
    client.pay_premium(&owner, &policy_id);

    // Simulate time passing (still before next due)
    set_ledger_time(&env, 1, env.ledger().timestamp() + 5000);

    // Second payment
    client.pay_premium(&owner, &policy_id);

    let p2 = client.get_policy(&policy_id).unwrap();

    // The logic in contract sets next_payment_date to 'now + 30 days'
    // So paying twice in quick succession just pushes it to 30 days from the SECOND payment
    // It does NOT add 60 days from start. This test verifies that behavior.
    assert!(p2.next_payment_date > first_due);
    assert_eq!(
        p2.next_payment_date,
        env.ledger().timestamp() + (30 * 86400)
    );
}

#[test]
fn test_create_premium_schedule_succeeds() {
    setup_test_env!(env, Insurance, client, owner, InsuranceClient);
    set_ledger_time(&env, 1u32, 1000u64);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_premium_schedule(&schedule_id);
    assert!(schedule.is_some());
    let schedule = schedule.unwrap();
    assert_eq!(schedule.next_due, 3000);
    assert_eq!(schedule.interval, 2592000);
    assert!(schedule.active);
}

#[test]
fn test_modify_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    client.modify_premium_schedule(&owner, &schedule_id, &4000, &2678400);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 4000);
    assert_eq!(schedule.interval, 2678400);
}

#[test]
fn test_cancel_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    client.cancel_premium_schedule(&owner, &schedule_id);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

#[test]
fn test_execute_due_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &0);

    set_ledger_time(&env, 1, 3500);
    let executed = client.execute_due_premium_schedules();

    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, 3500 + 30 * 86400);
}

#[test]
fn test_execute_recurring_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);

    set_ledger_time(&env, 1, 3500);
    client.execute_due_premium_schedules();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.next_due, 3000 + 2592000);
}

#[test]
fn test_execute_missed_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);

    set_time(&env, 3000 + 2592000 * 3 + 100);
    client.execute_due_premium_schedules();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.missed_count, 3);
    assert!(schedule.next_due > 3000 + 2592000 * 3);
}

#[test]
fn test_get_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
        &None,
    );

    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Insurance"),
        &String::from_str(&env, "life"),
        &300,
        &100000,
        &None,
    );

    client.create_premium_schedule(&owner, &policy_id1, &3000, &2592000);
    client.create_premium_schedule(&owner, &policy_id2, &4000, &2592000);

    let schedules = client.get_premium_schedules(&owner);
    assert_eq!(schedules.len(), 2);
}

#[test]
fn test_create_policy_emits_event() {
    use soroban_sdk::testutils::Events;
    // IntoVal import handled globally or locally once
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "health");

    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

    let events = env.events().all();
    assert!(events.len() >= 2);

    let audit_event = events.last().unwrap();

    let expected_topics = vec![
        &env,
        symbol_short!("insure").into_val(&env),
        InsuranceEvent::PolicyCreated.into_val(&env),
    ];

    assert_eq!(audit_event.1, expected_topics);

    let data: (u32, Address) = soroban_sdk::FromVal::from_val(&env, &audit_event.2);
    assert_eq!(data, (policy_id, owner.clone()));
    assert_eq!(audit_event.0, contract_id.clone());
}

#[test]
fn test_pay_premium_emits_event() {
    use soroban_sdk::testutils::Events;
    // IntoVal import handled globally or locally once
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");
    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

    env.mock_all_auths();
    client.pay_premium(&owner, &policy_id);

    let events = env.events().all();
    assert!(events.len() >= 2);

    let audit_event = events.last().unwrap();

    let expected_topics = vec![
        &env,
        symbol_short!("insure").into_val(&env),
        InsuranceEvent::PremiumPaid.into_val(&env),
    ];

    assert_eq!(audit_event.1, expected_topics);

    let data: (u32, Address) = soroban_sdk::FromVal::from_val(&env, &audit_event.2);
    assert_eq!(data, (policy_id, owner.clone()));
    assert_eq!(audit_event.0, contract_id.clone());
}

#[test]
fn test_deactivate_policy_emits_event() {
    use soroban_sdk::testutils::Events;
    // IntoVal import handled globally or locally once
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");
    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);

    env.mock_all_auths();
    client.deactivate_policy(&owner, &policy_id);

    let events = env.events().all();
    assert!(events.len() >= 2);

    let audit_event = events.last().unwrap();

    let expected_topics = vec![
        &env,
        symbol_short!("insure").into_val(&env),
        InsuranceEvent::PolicyDeactivated.into_val(&env),
    ];

    assert_eq!(audit_event.1, expected_topics);

    let data: (u32, Address) = soroban_sdk::FromVal::from_val(&env, &audit_event.2);
    assert_eq!(data, (policy_id, owner.clone()));
    assert_eq!(audit_event.0, contract_id.clone());
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_create_policy_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let _other = Address::generate(&env);

    // Do not mock auth for other, attempt to create policy for owner as other
    // If owner didn't authorize, it panics.
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
        &100,
        &10000,
        &None,
    );
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_pay_premium_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let _other = Address::generate(&env);

    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &owner,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_policy",
            args: (
                &owner,
                String::from_str(&env, "Policy"),
                String::from_str(&env, "Type"),
                100u32,
                10000i128,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
        &100,
        &10000,
        &None,
    );

    // other tries to pay the premium for owner
    client.pay_premium(&owner, &policy_id);
}

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_deactivate_policy_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let _other = Address::generate(&env);

    client.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &owner,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_policy",
            args: (
                &owner,
                String::from_str(&env, "Policy"),
                String::from_str(&env, "Type"),
                100u32,
                10000i128,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &String::from_str(&env, "Type"),
        &100,
        &10000,
        &None,
    );

    // other tries to deactivate the policy for owner
    client.deactivate_policy(&owner, &policy_id);
}

// Required test cases from issue #61// Required test cases from issue #61

#[test]
fn test_create_policy_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Test Policy");
    let coverage_type = String::from_str(&env, "health");
    let monthly_premium = 100;
    let coverage_amount = 10000;

    let policy_id = client.create_policy(
        &owner,
        &name,
        &coverage_type,
        &monthly_premium,
        &coverage_amount,
        &None,
    );

    // Verify returns id
    assert_eq!(policy_id, 1);

    // Verify policy stored correctly
    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.name, name);
    assert_eq!(policy.coverage_type, coverage_type);
    assert_eq!(policy.monthly_premium, monthly_premium);
    assert_eq!(policy.coverage_amount, coverage_amount);
    assert!(policy.active);
}

#[test]
fn test_create_policy_requires_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    // Don't mock auths - this should fail
    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    // Should fail due to missing auth
    assert!(result.is_err());
}

#[test]
fn test_create_policy_negative_premium_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &-1, // negative premium
        &10000,
        &None,
    );

    assert!(result.is_err());
}

#[test]
fn test_create_policy_negative_coverage_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &-1, // negative coverage
        &None,
    );

    assert!(result.is_err());
}

#[test]
fn test_pay_premium_success() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = initial_policy.next_payment_date;

    // Advance time
    set_time(&env, env.ledger().timestamp() + 86400); // +1 day

    let result = client.try_pay_premium(&owner, &policy_id);
    assert!(result.is_ok());

    let updated_policy = client.get_policy(&policy_id).unwrap();

    // next_payment_date should advance ~30 days from current time
    let expected_next_payment = env.ledger().timestamp() + (30 * 86400);
    assert_eq!(updated_policy.next_payment_date, expected_next_payment);
    assert!(updated_policy.next_payment_date > initial_next_payment);
}

#[test]
fn test_pay_premium_unauthorized_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let unauthorized_user = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    // Try to pay premium as unauthorized user
    let result = client.try_pay_premium(&unauthorized_user, &policy_id);
    assert!(result.is_err());
}

    #[test]
    fn test_health_premium_at_maximum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // max_premium = 500_000_000; need coverage ≤ 500M * 12 * 500 = 3T (within 100B limit)
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_000i128,
            &100_000_000_000i128, // max coverage for Health
            &None,
        );
    }

    #[test]
    fn test_health_coverage_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &10_000_000i128, // exactly min_coverage
            &None,
        );
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    // --- Life boundaries ---

    #[test]
    fn test_life_premium_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life Min"),
            &CoverageType::Life,
            &500_000i128,     // min_premium
            &50_000_000i128,  // min_coverage
            &None,
        );
    }

    #[test]
    fn test_liability_premium_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability Min"),
            &CoverageType::Liability,
            &800_000i128,     // min_premium
            &5_000_000i128,   // min_coverage
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 4. create_policy — name validation
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "name cannot be empty")]
    fn test_create_policy_empty_name_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, ""),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "name too long")]
    fn test_create_policy_name_exceeds_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // 65 character name — exceeds MAX_NAME_LEN (64)
        let long_name = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.create_policy(
            &caller,
            &long_name,
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );

    // -----------------------------------------------------------------------
    // 5. create_policy — premium validation failures
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "monthly_premium must be positive")]
    fn test_create_policy_zero_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &0i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium must be positive")]
    fn test_create_policy_negative_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &-1i128,
            &50_000_000i128,
            &None,
        );
    }

    // Create another policy to test unauthorized deactivation
    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
        &None,
    );

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_health_policy_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health max_premium = 500_000_000; supply 500_000_001
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_001i128,
            &10_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_life_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Life min_premium = 500_000; supply 499_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life"),
            &CoverageType::Life,
            &499_999i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_property_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Property min_premium = 2_000_000; supply 1_999_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Property"),
            &CoverageType::Property,
            &1_999_999i128,
            &100_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_auto_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto min_premium = 1_500_000; supply 1_499_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &1_499_999i128,
            &20_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_liability_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability min_premium = 800_000; supply 799_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &799_999i128,
            &5_000_000i128,
            &None,
        );
    }

    // Create policies for owner_a
    let policy_a1 = client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A1"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );
    let policy_a2 = client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
        &None,
    );

    // Create policies for owner_b
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "Policy B1"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
        &None,
    );

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_health_policy_coverage_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health min_coverage = 10_000_000; supply 9_999_999
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &9_999_999i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_health_policy_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health max_coverage = 100_000_000_000; supply 100_000_000_001
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_000i128,
            &100_000_000_001i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_life_policy_coverage_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Life min_coverage = 50_000_000; supply 49_999_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life"),
            &CoverageType::Life,
            &1_000_000i128,
            &49_999_999i128,
            &None,
        );
    }

    // Create multiple active policies
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
        &None,
    );
    let policy3 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 3"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
        &None,
    );

    // -----------------------------------------------------------------------
    // 7. create_policy — ratio guard (unsupported combination)
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "unsupported combination: coverage_amount too high relative to premium")]
    fn test_create_policy_coverage_too_high_for_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // premium = 1_000_000 → annual = 12_000_000 → max_coverage = 6_000_000_000
        // supply coverage = 6_000_000_001 (just over the ratio limit, but within Health's hard max)
        // Need premium high enough so health range isn't hit, but ratio is
        // Health max_coverage = 100_000_000_000
        // Use premium = 1_000_000, coverage = 7_000_000_000 → over ratio (6B), under hard cap (100B)
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &1_000_000i128,
            &7_000_000_000i128,
            &None,
        );
    }

    #[test]
    fn test_create_policy_coverage_exactly_at_ratio_limit_succeeds() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // premium = 1_000_000 → ratio limit = 1M * 12 * 500 = 6_000_000_000
        // Health max_coverage = 100B, so 6B is fine
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &1_000_000i128,
            &6_000_000_000i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 8. External ref validation
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "external_ref length out of range")]
    fn test_create_policy_ext_ref_too_long_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // 129 character external ref — exceeds MAX_EXT_REF_LEN (128)
        let long_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(long_ref),
        );
    }

    #[test]
    fn test_create_policy_ext_ref_at_max_length_succeeds() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Exactly 128 characters
        let max_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(max_ref),
        );
    }

    // -----------------------------------------------------------------------
    // 9. pay_premium — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_pay_premium_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let result = client.pay_premium(&caller, &id, &5_000_000i128);
        assert!(result);
    }

    // Create multiple policies for same owner
    let policy1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
        &None,
    );
    let policy2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Policy"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
        &None,
    );
    let policy3 = client.create_policy(
        &owner,
        &String::from_str(&env, "Emergency Policy"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
        &None,
    );

    // -----------------------------------------------------------------------
    // 10. pay_premium — failure cases
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_pay_premium_nonexistent_policy_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.pay_premium(&caller, &999u32, &5_000_000i128);
    }

    #[test]
    #[should_panic(expected = "amount must equal monthly_premium")]
    fn test_pay_premium_wrong_amount_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.pay_premium(&caller, &id, &4_999_999i128);
    }

    #[test]
    #[should_panic(expected = "policy inactive")]
    fn test_pay_premium_on_inactive_policy_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.deactivate_policy(&owner, &id);
        client.pay_premium(&caller, &id, &5_000_000i128);
    }

    // -----------------------------------------------------------------------
    // 11. deactivate_policy — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_deactivate_policy_success() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let result = client.deactivate_policy(&owner, &id);
        assert!(result);

        let policy = client.get_policy(&id);
        assert!(!policy.active);
    }

    #[test]
    fn test_deactivate_removes_from_active_list() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        assert_eq!(client.get_active_policies().len(), 1);
        client.deactivate_policy(&owner, &id);
        assert_eq!(client.get_active_policies().len(), 0);
    }

    // -----------------------------------------------------------------------
    // 12. deactivate_policy — failure cases
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_deactivate_policy_non_owner_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let non_owner = Address::generate(&env);
        client.deactivate_policy(&non_owner, &id);
    }

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_deactivate_nonexistent_policy_panics() {
        let (_env, client, owner) = setup();
        client.deactivate_policy(&owner, &999u32);
    }

    #[test]
    #[should_panic(expected = "policy already inactive")]
    fn test_deactivate_already_inactive_policy_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.deactivate_policy(&owner, &id);
        // Second deactivation must panic
        client.deactivate_policy(&owner, &id);
    }

    // -----------------------------------------------------------------------
    // 13. set_external_ref
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_success() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let new_ref = String::from_str(&env, "NEW-REF-001");
        client.set_external_ref(&owner, &id, &Some(new_ref));
        let policy = client.get_policy(&id);
        assert!(policy.external_ref.is_some());
    }

    #[test]
    fn test_set_external_ref_clear() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let ext_ref = String::from_str(&env, "INITIAL-REF");
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(ext_ref),
        );
        // Clear the ref
        client.set_external_ref(&owner, &id, &None);
        let policy = client.get_policy(&id);
        assert!(policy.external_ref.is_none());
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Cover"),
        &String::from_str(&env, "life"),
        &200,
        &100000,
        &None,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    #[test]
    #[should_panic(expected = "external_ref length out of range")]
    fn test_set_external_ref_too_long_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let long_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.set_external_ref(&owner, &id, &Some(long_ref));
    }

    // -----------------------------------------------------------------------
    // 14. Queries
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_active_policies_empty_initially() {
        let (_env, client, _owner) = setup();
        assert_eq!(client.get_active_policies().len(), 0);
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Plan"),
        &String::from_str(&env, "health"),
        &150,
        &75000,
        &None,
    );
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    #[test]
    fn test_get_total_monthly_premium_sums_active_only() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id1 = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.create_policy(
            &caller,
            &String::from_str(&env, "Second"),
            &CoverageType::Life,
            &1_000_000i128,
            &60_000_000i128,
            &None,
        );
        assert_eq!(client.get_total_monthly_premium(), 6_000_000i128);
        client.deactivate_policy(&owner, &id1);
        assert_eq!(client.get_total_monthly_premium(), 1_000_000i128);
    }

    #[test]
    fn test_get_total_monthly_premium_zero_when_no_policies() {
        let (_env, client, _owner) = setup();
        assert_eq!(client.get_total_monthly_premium(), 0i128);
    }

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_get_policy_nonexistent_panics() {
        let (_env, client, _owner) = setup();
        client.get_policy(&999u32);
    }

    // -----------------------------------------------------------------------
    // 15. Uninitialized contract guard
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "not initialized")]
    fn test_create_policy_without_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, InsuranceContract);
        let client = InsuranceContractClient::new(&env, &contract_id);
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Test"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Property Plan"),
        &String::from_str(&env, "property"),
        &300,
        &200000,
        &None,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    // -----------------------------------------------------------------------
    // 16. Policy data integrity
    // -----------------------------------------------------------------------

    #[test]
    fn test_policy_fields_stored_correctly() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        env.ledger().set_timestamp(1_700_000_000u64);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "My Health Plan"),
            &CoverageType::Health,
            &10_000_000i128,
            &100_000_000i128,
            &Some(String::from_str(&env, "EXT-001")),
        );
        let policy = client.get_policy(&id);
        assert_eq!(policy.id, 1u32);
        assert_eq!(policy.monthly_premium, 10_000_000i128);
        assert_eq!(policy.coverage_amount, 100_000_000i128);
        assert!(policy.active);
        assert_eq!(policy.last_payment_at, 0u64);
        assert_eq!(policy.created_at, 1_700_000_000u64);
        assert_eq!(
            policy.next_payment_due,
            1_700_000_000u64 + 30 * 24 * 60 * 60
        );
        assert!(policy.external_ref.is_some());
    }

    // -----------------------------------------------------------------------
    // 17. Cross-coverage-type boundary checks
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_property_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Property max_premium = 2_000_000_000; supply 2_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Property"),
            &CoverageType::Property,
            &2_000_000_001i128,
            &100_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_auto_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto max_premium = 750_000_000; supply 750_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &750_000_001i128,
            &20_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_liability_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability max_premium = 400_000_000; supply 400_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &400_000_001i128,
            &5_000_000i128,
            &None,
        );
    }

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Auto Cover"),
        &String::from_str(&env, "auto"),
        &100,
        &50000,
        &None,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &interval);

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_auto_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto max_coverage = 200_000_000_000; supply 200_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &750_000_000i128,
            &200_000_000_001i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_liability_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability max_coverage = 50_000_000_000; supply 50_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &400_000_000i128,
            &50_000_000_001i128,
            &None,
        );
    }
}