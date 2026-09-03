#![allow(deprecated)]
use reporting::{ReportingContract, ReportingContractClient, ReportingError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_addresses_not_configured_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);

    client.init(&admin);

    // Call endpoints before configuring addresses — should return AddressesNotConfigured
    let result = client.try_get_financial_health_report(&owner, &owner, &1000, &100, &200);
    assert_eq!(result, Err(Ok(ReportingError::AddressesNotConfigured)));
}

#[test]
fn test_addresses_configured_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.init(&admin);

    let rs = Address::generate(&env);
    let sg = Address::generate(&env);
    let bp = Address::generate(&env);
    let ins = Address::generate(&env);
    let fw = Address::generate(&env);

    let result = client.try_configure_addresses(&admin, &rs, &sg, &bp, &ins, &fw);
    assert!(result.is_ok());
    let stored = client.get_addresses().unwrap();
    assert_eq!(stored.remittance_split, rs);
}
