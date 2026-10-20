use reporting::{ContractAddresses, ReportingContract, ReportingContractClient, ReportingError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_addresses_not_configured_error() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);

    // Don't configure addresses
    let user = Address::generate(&env);
    let result = client.try_get_savings_report(&user, &0, &100);
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

    assert_eq!(client.get_addresses(), None);

    let addrs = ContractAddresses {
        remittance_split: Address::generate(&env),
        savings_goals: Address::generate(&env),
        bill_payments: Address::generate(&env),
        insurance: Address::generate(&env),
        family_wallet: Address::generate(&env),
    };

    let result = client.try_configure_addresses(
        &admin,
        &addrs.remittance_split,
        &addrs.savings_goals,
        &addrs.bill_payments,
        &addrs.insurance,
        &addrs.family_wallet,
    );
    assert_eq!(result, Ok(Ok(())));
    assert_eq!(client.get_addresses(), Some(addrs));
}

#[test]
fn test_all_endpoints_fail_closed() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.init(&admin);
    let user = Address::generate(&env);

    assert_eq!(
        client.try_check_dependencies(&admin),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
    assert_eq!(
        client.try_get_savings_report(&user, &0, &100),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
    assert_eq!(
        client.try_get_bill_compliance_report(&user, &0, &100),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
    assert_eq!(
        client.try_get_insurance_report(&user, &0, &100),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
    assert_eq!(
        client.try_get_financial_health_report(&user, &0, &100),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
    assert_eq!(
        client.try_generate_financial_health_report(&user, &0, &100),
        Err(Ok(ReportingError::AddressesNotConfigured))
    );
}
