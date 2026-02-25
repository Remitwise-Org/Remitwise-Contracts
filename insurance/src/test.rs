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

    let active = client.get_active_policies(&owner, &0, &DEFAULT_PAGE_LIMIT);
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
    let _policy2 = client.create_policy(
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
        symbol_short!("insure").into_val(&env), // Fixed: should be "insure" not "insuranc"
        InsuranceEvent::PolicyDeactivated.into_val(&env),
    ];

    assert_eq!(audit_event.1, expected_topics);

    let data: (u32, Address) = soroban_sdk::FromVal::from_val(&env, &audit_event.2);
    assert_eq!(data, (policy_id, owner.clone()));
    assert_eq!(audit_event.0, contract_id.clone());
}

// Required test cases from issue #61

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
    );

    let initial_policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = initial_policy.next_payment_date;

    // Advance time
    set_time(&env, env.ledger().timestamp() + 86400); // +1 day

    let result = client.pay_premium(&owner, &policy_id);
    assert!(result);

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
    );

    // Try to pay premium as unauthorized user
    let result = client.try_pay_premium(&unauthorized_user, &policy_id);
    assert!(result.is_err());
}

#[test]
fn test_pay_premium_inactive_policy_panics() {
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
    );

    // Deactivate policy first
    client.deactivate_policy(&owner, &policy_id);

    // Try to pay premium on inactive policy
    let result = client.try_pay_premium(&owner, &policy_id);
    assert!(result.is_err());
}

#[test]
fn test_deactivate_policy_owner_only() {
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
    );

    // Owner can deactivate
    let result = client.deactivate_policy(&owner, &policy_id);
    assert!(result);

    let policy = client.get_policy(&policy_id).unwrap();
    assert!(!policy.active);

    // Create another policy to test unauthorized deactivation
    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Test Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
    );

    // Unauthorized user cannot deactivate
    let result = client.try_deactivate_policy(&unauthorized_user, &policy_id2);
    assert!(result.is_err());
}

#[test]
fn test_get_policy_nonexistent() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    // Try to get policy that doesn't exist
    let policy = client.get_policy(&999);
    assert!(policy.is_none());
}

#[test]
fn test_get_active_policies_filters_by_owner_and_active() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    env.mock_all_auths();

    // Create policies for owner_a
    let policy_a1 = client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A1"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
    );
    let policy_a2 = client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
    );

    // Create policies for owner_b
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "Policy B1"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
    );

    // Deactivate one of owner_a's policies
    client.deactivate_policy(&owner_a, &policy_a1);

    // Get active policies for owner_a
    let active_policies_a = client.get_active_policies(&owner_a, &0, &DEFAULT_PAGE_LIMIT);
    assert_eq!(active_policies_a.items.len(), 1);
    let active_policy = active_policies_a.items.get(0).unwrap();
    assert_eq!(active_policy.id, policy_a2);
    assert_eq!(active_policy.owner, owner_a);
    assert!(active_policy.active);

    // Get active policies for owner_b
    let active_policies_b = client.get_active_policies(&owner_b, &0, &DEFAULT_PAGE_LIMIT);
    assert_eq!(active_policies_b.items.len(), 1);
    let active_policy_b = active_policies_b.items.get(0).unwrap();
    assert_eq!(active_policy_b.owner, owner_b);
    assert!(active_policy_b.active);
}

#[test]
fn test_get_total_monthly_premium_comprehensive() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create multiple active policies
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
    );
    let policy3 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 3"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
    );

    // Total should be sum of all active policies' monthly_premium
    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 600); // 100 + 200 + 300

    // Deactivate one policy
    client.deactivate_policy(&owner, &policy3);

    // Total should now exclude the deactivated policy
    let total_after = client.get_total_monthly_premium(&owner);
    assert_eq!(total_after, 300); // 100 + 200
}

#[test]
fn test_multiple_policies_same_owner() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Create multiple policies for same owner
    let policy1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10000,
    );
    let policy2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Policy"),
        &String::from_str(&env, "life"),
        &200,
        &20000,
    );
    let policy3 = client.create_policy(
        &owner,
        &String::from_str(&env, "Emergency Policy"),
        &String::from_str(&env, "emergency"),
        &300,
        &30000,
    );

    // Verify all policies exist and are active
    let p1 = client.get_policy(&policy1).unwrap();
    let p2 = client.get_policy(&policy2).unwrap();
    let p3 = client.get_policy(&policy3).unwrap();

    assert!(p1.active && p2.active && p3.active);
    assert_eq!(p1.owner, owner);
    assert_eq!(p2.owner, owner);
    assert_eq!(p3.owner, owner);

    // Pay premiums for all policies
    set_time(&env, env.ledger().timestamp() + 86400); // +1 day

    let result1 = client.pay_premium(&owner, &policy1);
    let result2 = client.pay_premium(&owner, &policy2);
    let result3 = client.pay_premium(&owner, &policy3);

    assert!(result1 && result2 && result3);

    // Deactivate policies
    let deactivate1 = client.deactivate_policy(&owner, &policy1);
    let deactivate2 = client.deactivate_policy(&owner, &policy2);
    let deactivate3 = client.deactivate_policy(&owner, &policy3);

    assert!(deactivate1 && deactivate2 && deactivate3);

    // Verify all policies are now inactive
    let p1_after = client.get_policy(&policy1).unwrap();
    let p2_after = client.get_policy(&policy2).unwrap();
    let p3_after = client.get_policy(&policy3).unwrap();

    assert!(!p1_after.active && !p2_after.active && !p3_after.active);

    // Verify no active policies remain
    let active_policies = client.get_active_policies(&owner, &0, &DEFAULT_PAGE_LIMIT);
    assert_eq!(active_policies.items.len(), 0);

    // Verify total monthly premium is now 0
    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 0);
}

// ══════════════════════════════════════════════════════════════════════════
// Time & Ledger Drift Resilience Tests (#158)
//
// Assumptions documented here:
//  - execute_due_premium_schedules fires when schedule.next_due <= current_time
//    (inclusive: executes exactly at next_due).
//  - next_payment_date is set to env.ledger().timestamp() + 30 * 86400 at
//    execution time, anchored to actual payment time not original due date.
//  - Stellar ledger timestamps are monotonically increasing in production.
//    After execution next_due advances by the interval, guarding against
//    re-execution even if ledger time were set backward.
// ══════════════════════════════════════════════════════════════════════════

/// Premium schedule must NOT execute one second before next_due.
#[test]
fn test_time_drift_premium_schedule_not_executed_before_next_due() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    let next_due = 5000u64;
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Cover"),
        &String::from_str(&env, "life"),
        &200,
        &100000,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, next_due - 1);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(
        executed.len(),
        0,
        "Premium schedule must not execute one second before next_due"
    );
}

/// Premium schedule must execute exactly at next_due (inclusive boundary).
#[test]
fn test_time_drift_premium_schedule_executes_at_exact_next_due() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    let next_due = 5000u64;
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Plan"),
        &String::from_str(&env, "health"),
        &150,
        &75000,
    );
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, next_due);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(
        executed.len(),
        1,
        "Premium schedule must execute exactly at next_due"
    );
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(
        policy.next_payment_date,
        next_due + 30 * 86400,
        "next_payment_date must be current_time + 30 days"
    );
}

/// next_payment_date is anchored to actual payment time, not original next_due.
/// A late payment pushes next_payment_date further than an on-time payment would.
#[test]
fn test_time_drift_next_payment_date_uses_actual_payment_time() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    let next_due = 5000u64;
    let late_payment_time = next_due + 7 * 86400; // paid 7 days late
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Property Plan"),
        &String::from_str(&env, "property"),
        &300,
        &200000,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &2592000);

    set_time(&env, late_payment_time);
    client.execute_due_premium_schedules();

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(
        policy.next_payment_date,
        late_payment_time + 30 * 86400,
        "next_payment_date must be anchored to actual payment time"
    );
    assert!(
        policy.next_payment_date > next_due + 30 * 86400,
        "Late payment must push next_payment_date beyond on-time payment window"
    );
}

/// After execution next_due advances; a call at a time still before the new
/// next_due must not re-execute. Documents non-monotonic time assumption.
#[test]
fn test_time_drift_no_double_execution_after_schedule_advances() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    let next_due = 5000u64;
    let interval = 2_592_000u64;
    set_time(&env, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Auto Cover"),
        &String::from_str(&env, "auto"),
        &100,
        &50000,
    );
    client.create_premium_schedule(&owner, &policy_id, &next_due, &interval);

    // First execution at next_due
    set_time(&env, next_due);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1);

    // Between old next_due and new next_due: no re-execution
    // NOTE: In production, ledger time is monotonic. This also covers repeated
    //       calls within the same ledger window before the next cycle.
    set_time(&env, next_due + 1000);
    let executed_again = client.execute_due_premium_schedules();
    assert_eq!(
        executed_again.len(),
        0,
        "Schedule must not re-execute before the new next_due"
    );
}
