#[cfg(test)]
mod tests {
    use crate::*;
    use proptest::prelude::*;
    use remitwise_common::CoverageType;
    use soroban_sdk::testutils::{Address as AddressTrait, Events as _};
    use soroban_sdk::{symbol_short, Env, IntoVal, String, TryFromVal};

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner).unwrap();
    (env, contract_id, owner)
}

fn health_str(env: &Env) -> String { String::from_str(env, "health") }
fn life_str(env: &Env) -> String { String::from_str(env, "life") }
fn property_str(env: &Env) -> String { String::from_str(env, "property") }
fn auto_str(env: &Env) -> String { String::from_str(env, "auto") }
fn liability_str(env: &Env) -> String { String::from_str(env, "liability") }

#[test]
fn test_init_success() {
    let (env, _, owner) = setup();
    let client = InsuranceClient::new(&env, &env.register_contract(None, Insurance));
    client.init(&owner).unwrap();
}

#[test]
fn test_create_policy_success() {
    let (env, _, owner) = setup();
    let client = InsuranceClient::new(&env, &env.register_contract(None, Insurance));
    client.init(&owner).unwrap();
    let caller = Address::generate(&env);
    let id = client.create_policy(&caller, &String::from_str(&env, "P1"), &health_str(&env), &5_000_000i128, &50_000_000i128, &None).unwrap();
    assert_eq!(id, 1);
    let p = client.get_policy(&id).unwrap();
    assert_eq!(p.monthly_premium, 5_000_000);
}

#[test]
fn test_pagination() {
    let (env, _, _) = setup();
    let client = InsuranceClient::new(&env, &env.register_contract(None, Insurance));
    let owner = Address::generate(&env);
    client.init(&Address::generate(&env)).unwrap();
    for i in 0..10 {
        client.create_policy(&owner, &String::from_str(&env, "P"), &health_str(&env), &5_000_000i128, &50_000_000i128, &None).unwrap();
    }
    let page = client.get_active_policies(&owner, &0, &5).unwrap();
    assert_eq!(page.items.len(), 5);
    assert_eq!(page.count, 5);
    assert_eq!(page.next_cursor, 5);
}

#[test]
fn test_total_premium_isolation() {
    let (env, _, _) = setup();
    let client = InsuranceClient::new(&env, &env.register_contract(None, Insurance));
    client.init(&Address::generate(&env)).unwrap();
    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    client.create_policy(&u1, &String::from_str(&env, "P1"), &health_str(&env), &5_000_000i128, &50_000_000i128, &None).unwrap();
    client.create_policy(&u2, &String::from_str(&env, "P2"), &health_str(&env), &6_000_000i128, &50_000_000i128, &None).unwrap();
    assert_eq!(client.get_total_monthly_premium(&u1).unwrap(), 5_000_000);
    assert_eq!(client.get_total_monthly_premium(&u2).unwrap(), 6_000_000);
}

#[test]
fn test_batch_pay() {
    let (env, _, _) = setup();
    let client = InsuranceClient::new(&env, &env.register_contract(None, Insurance));
    client.init(&Address::generate(&env)).unwrap();
    let owner = Address::generate(&env);
    let id1 = client.create_policy(&owner, &String::from_str(&env, "P1"), &health_str(&env), &5_000_000i128, &50_000_000i128, &None).unwrap();
    let id2 = client.create_policy(&owner, &String::from_str(&env, "P2"), &health_str(&env), &5_000_000i128, &50_000_000i128, &None).unwrap();
    let mut ids = Vec::new(&env);
    ids.push_back(id1);
    ids.push_back(id2);
    let count = client.batch_pay_premiums(&owner, &ids).unwrap();
    assert_eq!(count, 2);
}