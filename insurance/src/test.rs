#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, String};

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
        &100,      // monthly_premium
        &10000,    // coverage_amount
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
    assert_eq!(p2.next_payment_date, env.ledger().timestamp() + (30 * 86400));
}
