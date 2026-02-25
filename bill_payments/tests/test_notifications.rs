#![cfg(test)]

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, testutils::Events, Address, Env, Symbol, TryFromVal};

#[test]
fn test_notification_flow() {
    let e = Env::default();

    // Register the contract
    let contract_id = e.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&e, &contract_id);

    // Setup: Create a User
    let user = Address::generate(&e);

    // Mock authorization so 'require_auth' passes
    e.mock_all_auths();

    // Create Bill
    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&e, "Electricity"),
        &1000,
        &1234567890,
        &false,
        &0,
        &soroban_sdk::String::from_str(&e, "XLM"),
    );

    // VERIFY: Get Events
    let all_events = e.events().all();
    assert!(!all_events.is_empty(), "No events were emitted!");

    let last_event = all_events.last().unwrap();
    let topics = &last_event.1;

    // Convert 'Val' back to Rust types
    let namespace: Symbol = Symbol::try_from_val(&e, &topics.get(0).unwrap()).unwrap();
    let category: u32 = u32::try_from_val(&e, &topics.get(1).unwrap()).unwrap();
    let action: Symbol = Symbol::try_from_val(&e, &topics.get(3).unwrap()).unwrap();

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(category, 1u32); // Category: State (1)
    assert_eq!(action, symbol_short!("created"));

    std::println!("✅ Creation Event Verified");

    // CALL: Pay Bill
    client.pay_bill(&user, &bill_id);

    // VERIFY: Check for Payment Event
    let new_events = e.events().all();
    let pay_event = new_events.last().unwrap();
    let pay_topics = &pay_event.1;

    let pay_category: u32 = u32::try_from_val(&e, &pay_topics.get(1).unwrap()).unwrap();
    let pay_priority: u32 = u32::try_from_val(&e, &pay_topics.get(2).unwrap()).unwrap();
    let pay_action: Symbol = Symbol::try_from_val(&e, &pay_topics.get(3).unwrap()).unwrap();

    assert_eq!(pay_category, 0u32); // Category: Transaction (0)
    assert_eq!(pay_priority, 2u32); // Priority: High (2)
    assert_eq!(pay_action, symbol_short!("paid"));

    std::println!("✅ Payment Event Verified");
}
