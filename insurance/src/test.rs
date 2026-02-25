#![cfg(test)]

use super::*;
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
        max_entry_ttl: 100000,
    });
}

#[test]
fn test_create_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");

    let policy_id = client.create_policy(
        &owner,
        &name,
        &coverage_type,
        &100,   // monthly_premium
        &10000, // coverage_amount
    );

    assert_eq!(policy_id, 1);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.monthly_premium, 100);
    assert_eq!(policy.coverage_amount, 10000);
    assert!(policy.active);
}

#[test]
#[should_panic(expected = "Monthly premium must be positive")]
fn test_create_policy_invalid_premium() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "Type"),
        &0,
        &10000,
    );
}

#[test]
#[should_panic(expected = "Coverage amount must be positive")]
fn test_create_policy_invalid_coverage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &String::from_str(&env, "Type"),
        &100,
        &0,
    );
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
    );

    // Initial next_payment_date is ~30 days from creation
    // We'll simulate passage of time is separate, but here we just check it updates
    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_due = initial_policy.next_payment_date;

    // Advance ledger time to simulate paying slightly later
    let mut ledger_info = env.ledger().get();
    ledger_info.timestamp += 1000;
    env.ledger().set(ledger_info);

    let success = client.pay_premium(&owner, &policy_id);
    assert!(success);

    let updated_policy = client.get_policy(&policy_id).unwrap();

    // New validation logic: new due date should be current timestamp + 30 days
    // Since we advanced timestamp by 1000, the new due date should be > initial due date
    assert!(updated_policy.next_payment_date > initial_due);
}

#[test]
#[should_panic(expected = "Only the policy owner can pay premiums")]
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
    );

    // unauthorized payer
    client.pay_premium(&other, &policy_id);
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
    );
    let p2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "T2"),
        &200,
        &2000,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &String::from_str(&env, "T3"),
        &300,
        &3000,
    );

    // Deactivate P2
    client.deactivate_policy(&owner, &p2);

    let active = client.get_active_policies(&owner);
    assert_eq!(active.len(), 2);

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
    );
    let policy_id_2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "Type 2"),
        &200,
        &2000,
    );

    // Deactivate policy 1
    client.deactivate_policy(&owner, &policy_id_1);

    // get_active_policies must return only the still-active policy
    let active = client.get_active_policies(&owner);
    assert_eq!(active.len(), 1, "get_active_policies must return exactly one policy");
    let only = active.get(0).unwrap();
    assert_eq!(only.id, policy_id_2, "the returned policy must be the active one (policy_id_2)");
    assert!(only.active, "returned policy must have active == true");
    // Deactivated policy_id_1 is not in the list (implied by len() == 1 and only.id == policy_id_2)
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
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &String::from_str(&env, "T2"),
        &200,
        &2000,
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
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 3"),
        &String::from_str(&env, "emergency"),
        &300,
        &3000,
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
    );
    let policy2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
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
    );
    client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A2"),
        &String::from_str(&env, "life"),
        &200,
        &2000,
    );

    // Create policies for owner_b
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "Policy B1"),
        &String::from_str(&env, "emergency"),
        &300,
        &3000,
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
    );

    let p1 = client.get_policy(&policy_id).unwrap();
    let first_due = p1.next_payment_date;

    // First payment
    client.pay_premium(&owner, &policy_id);

    // Simulate time passing (still before next due)
    let mut ledger = env.ledger().get();
    ledger.timestamp += 5000;
    env.ledger().set(ledger);

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
fn test_create_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
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
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
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
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
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
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &0);

    set_time(&env, 3500);
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
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
    );

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);

    set_time(&env, 3500);
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
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
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
    set_time(&env, 1000);

    let policy_id1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &String::from_str(&env, "health"),
        &500,
        &50000,
    );

    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Insurance"),
        &String::from_str(&env, "life"),
        &300,
        &100000,
    );

    client.create_premium_schedule(&owner, &policy_id1, &3000, &2592000);
    client.create_premium_schedule(&owner, &policy_id2, &4000, &2592000);

    let schedules = client.get_premium_schedules(&owner);
    assert_eq!(schedules.len(), 2);
}

#[test]
fn test_create_policy_emits_event() {
    use soroban_sdk::testutils::Events;
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");

    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000);

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
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");
    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000);

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
    use soroban_sdk::{symbol_short, vec, IntoVal};

    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = String::from_str(&env, "Health");
    let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000);

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

// ──────────────────────────────────────────────────────────────────────────
// Test: pay_premium after deactivate_policy (#104)
// ──────────────────────────────────────────────────────────────────────────

/// After deactivating a policy, `pay_premium` must be rejected with
/// `PolicyInactive`. The policy must remain inactive and no state
/// change should occur from the failed call.
#[test]
fn test_pay_premium_after_deactivate() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // 1. Create a policy
    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Plan"),
        &String::from_str(&env, "health"),
        &150,
        &50000,
    );

    // Sanity: policy should be active after creation
    let policy_before = client.get_policy(&policy_id).unwrap();
    assert!(policy_before.active);

    // 2. Deactivate the policy
    let deactivated = client.deactivate_policy(&owner, &policy_id);
    assert!(deactivated);

    // Confirm it is now inactive
    let policy_after_deactivate = client.get_policy(&policy_id).unwrap();
    assert!(!policy_after_deactivate.active);

    // Capture next_payment_date before the failed pay attempt
    let next_payment_before = policy_after_deactivate.next_payment_date;

    // 3. Attempt to pay premium — must fail with PolicyInactive
    let result = client.try_pay_premium(&owner, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyInactive)));

    // 4. Verify no state change occurred from the failed call
    let policy_after_failed_pay = client.get_policy(&policy_id).unwrap();
    assert!(!policy_after_failed_pay.active);
    assert_eq!(
        policy_after_failed_pay.next_payment_date,
        next_payment_before
    );
}
