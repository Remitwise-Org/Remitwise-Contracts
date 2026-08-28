use soroban_sdk::{Env, Address, String};
use reporting::{ReportingClient, ReportingError};

#[test]
fn test_addresses_not_configured_error() {
    let env = Env::default();
    let client = ReportingClient::new(&env, &env.register_contract(None, reporting::Reporting {}));

    // Don't configure addresses
    
    // Test each endpoint that requires addresses
    let result = client.try_submit_report(&Report { /* ... */ });
    assert!(result.is_err());
    
    if let Err(e) = result {
        // Check that it returns the correct error
        // This depends on how your error handling works
        assert!(e.to_string().contains("AddressesNotConfigured"));
    }
}

#[test]
fn test_addresses_configured_success() {
    let env = Env::default();
    let client = ReportingClient::new(&env, &env.register_contract(None, reporting::Reporting {}));
    
    // Configure addresses
    let reporting_contract = env.register_contract(None, reporting::Reporting {});
    let address = Address::random(&env);
    
    // Set up addresses in storage (implementation depends on your contract)
    // ...
    
    // Now endpoints should work
    let result = client.try_submit_report(&Report { /* ... */ });
    assert!(result.is_ok());
}

#[test]
fn test_all_endpoints_fail_closed() {
    let env = Env::default();
    let client = ReportingClient::new(&env, &env.register_contract(None, reporting::Reporting {}));
    
    // List of all endpoints that require addresses
    let endpoints: Vec<fn(&ReportingClient) -> Result<(), ContractError>> = vec![
        // Add all your endpoint functions here
        |c| c.try_submit_report(&Report { /* ... */ }),
        |c| c.try_get_report_data(&DataRequest { /* ... */ }),
        // ... etc
    ];
    
    for endpoint in endpoints {
        let result = endpoint(&client);
        assert!(result.is_err(), "Endpoint should fail when addresses not configured");
    }
}