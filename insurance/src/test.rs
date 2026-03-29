#![cfg(test)]

use crate::{Insurance, InsuranceClient, InsuranceError};
use remitwise_common::CoverageType;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    (env, contract_id)
}

#[test]
fn test_initialize_then_create_policy() {
    let (env, contract_id) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    assert_eq!(client.try_initialize(&owner), Ok(Ok(())));

    let policy_id = client
        .try_create_policy(
            &owner,
            &String::from_str(&env, "Health"),
            &CoverageType::Health,
            &200i128,
            &50_000i128,
            &None,
        )
        .unwrap()
        .unwrap();

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert!(policy.active);
}

#[test]
fn test_create_policy_without_initialize_fails() {
    let (env, contract_id) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let res = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Health"),
        &CoverageType::Health,
        &200i128,
        &50_000i128,
        &None,
    );

    assert!(matches!(res, Err(Ok(InsuranceError::NotInitialized)) | Err(Ok(InsuranceError::Unauthorized))));
}

#[test]
fn test_pay_premium_updates_next_payment_date() {
    let (env, contract_id) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.try_initialize(&owner).unwrap().unwrap();
    let policy_id = client
        .try_create_policy(
            &owner,
            &String::from_str(&env, "Health"),
            &CoverageType::Health,
            &200i128,
            &50_000i128,
            &None,
        )
        .unwrap()
        .unwrap();

    let before = client.get_policy(&policy_id).unwrap().next_payment_date;
    let ok = client.try_pay_premium(&owner, &policy_id);
    assert_eq!(ok, Ok(Ok(())));
    let after = client.get_policy(&policy_id).unwrap().next_payment_date;
    assert!(after > before);
}

#[test]
fn test_deactivate_policy_excludes_from_active_policies() {
    let (env, contract_id) = setup();
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.try_initialize(&owner).unwrap().unwrap();
    let p1 = client
        .try_create_policy(
            &owner,
            &String::from_str(&env, "P1"),
            &CoverageType::Health,
            &100i128,
            &10_000i128,
            &None,
        )
        .unwrap()
        .unwrap();
    let p2 = client
        .try_create_policy(
            &owner,
            &String::from_str(&env, "P2"),
            &CoverageType::Life,
            &200i128,
            &20_000i128,
            &None,
        )
        .unwrap()
        .unwrap();

    assert_eq!(client.try_deactivate_policy(&owner, &p2), Ok(Ok(true)));

    let active = client.get_active_policies(&owner, &0u32, &50u32);
    assert_eq!(active.count, 1);
    assert_eq!(active.items.get(0).unwrap().id, p1);
}
