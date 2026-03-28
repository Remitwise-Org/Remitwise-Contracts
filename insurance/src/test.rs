use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let owner = Address::generate(&env);
    (env, contract_id, owner)
}

#[test]
fn test_create_policy_succeeds() {
    let (env, contract_id, owner) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Policy"),
        &String::from_str(&env, "health"),
        &100,
        &10_000,
        &None,
    );

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.name, String::from_str(&env, "Health Policy"));
    assert_eq!(policy.coverage_type, String::from_str(&env, "health"));
    assert!(policy.active);
}

#[test]
fn test_create_policy_rejects_zero_amounts() {
    let (env, contract_id, owner) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Invalid"),
        &String::from_str(&env, "health"),
        &0,
        &10_000,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_pay_premium_updates_next_payment_date() {
    let (env, contract_id, owner) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    env.ledger().set_timestamp(1_000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Monthly"),
        &String::from_str(&env, "health"),
        &100,
        &10_000,
        &None,
    );

    env.ledger().set_timestamp(2_000);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, 2_000 + (30 * 86400));
}

#[test]
fn test_get_active_policies_filters_by_owner() {
    let (env, contract_id, owner_a) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let owner_b = Address::generate(&env);

    client.create_policy(
        &owner_a,
        &String::from_str(&env, "A1"),
        &String::from_str(&env, "health"),
        &100,
        &10_000,
        &None,
    );
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "B1"),
        &String::from_str(&env, "life"),
        &200,
        &20_000,
        &None,
    );

    let page = client.get_active_policies(&owner_a, &0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().owner, owner_a);
}
