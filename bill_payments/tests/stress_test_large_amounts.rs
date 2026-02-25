#![cfg(test)]

//! Stress tests for arithmetic operations with very large i128 values
//!
//! These tests verify that the bill_payments contract handles extreme values correctly:
//! - Values near i128::MAX/2 to avoid overflow in additions
//! - Proper error handling for overflow conditions
//! - No unexpected panics or wrap-around behavior
//!
//! ## Documented Limitations
//! - Maximum safe bill amount: i128::MAX/2 (to allow for safe addition operations)
//! - get_total_unpaid uses checked_add internally via += operator
//! - No explicit caps are imposed by the contract, but overflow will panic

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
use soroban_sdk::{Env, String};

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
fn test_create_bill_near_max_i128() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with i128::MAX / 2 - a very large but safe value
    let large_amount = i128::MAX / 2;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.amount, large_amount);
    assert!(!bill.paid);
}

#[test]
fn test_pay_bill_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 2;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
    );

    env.mock_all_auths();
    client.pay_bill(&owner, &bill_id);

    let bill = client.get_bill(&bill_id).unwrap();
    assert!(bill.paid);
    assert_eq!(bill.amount, large_amount);
}

#[test]
fn test_recurring_bill_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 2;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Recurring"),
        &large_amount,
        &1000000,
        &true,
        &30,
    );

    env.mock_all_auths();
    client.pay_bill(&owner, &bill_id);

    // Verify original bill is paid
    let bill = client.get_bill(&bill_id).unwrap();
    assert!(bill.paid);
    assert_eq!(bill.amount, large_amount);

    // Verify next recurring bill was created with same amount
    let bill2 = client.get_bill(&2).unwrap();
    assert!(!bill2.paid);
    assert_eq!(bill2.amount, large_amount);
}

#[test]
fn test_get_total_unpaid_with_two_large_bills() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Create two bills, each at i128::MAX / 4 to safely add them
    let amount = i128::MAX / 4;

    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill1"),
        &amount,
        &1000000,
        &false,
        &0,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill2"),
        &amount,
        &1000000,
        &false,
        &0,
    );

    let total = client.get_total_unpaid(&owner);
    assert_eq!(total, amount + amount);
}

#[test]
#[should_panic(expected = "overflow")]
fn test_get_total_unpaid_overflow_panics() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Create two bills that will overflow when added
    let amount = i128::MAX / 2 + 1000;

    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill1"),
        &amount,
        &1000000,
        &false,
        &0,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill2"),
        &amount,
        &1000000,
        &false,
        &0,
    );

    // This should panic due to overflow
    client.get_total_unpaid(&owner);
}

#[test]
fn test_multiple_large_bills_different_owners() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner1 = <soroban_sdk::Address as AddressTrait>::generate(&env);
    let owner2 = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 2;

    // Each owner can have large bills independently
    client.create_bill(
        &owner1,
        &String::from_str(&env, "Owner1 Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner2,
        &String::from_str(&env, "Owner2 Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
    );

    let total1 = client.get_total_unpaid(&owner1);
    let total2 = client.get_total_unpaid(&owner2);

    assert_eq!(total1, large_amount);
    assert_eq!(total2, large_amount);
}

#[test]
fn test_archive_large_amount_bill() {
    let env = Env::default();
    set_time(&env, 1000000);

    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 2;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
    );

    env.mock_all_auths();
    client.pay_bill(&owner, &bill_id);

    env.mock_all_auths();
    client.archive_paid_bills(&owner);

    let archived = client.get_archived_bill(&bill_id).unwrap();
    assert_eq!(archived.amount, large_amount);
}

#[test]
fn test_batch_pay_large_bills() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let amount = i128::MAX / 10; // Safe for multiple bills

    let mut bill_ids = soroban_sdk::Vec::new(&env);

    for i in 0..5 {
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, &format!("Bill{}", i)),
            &amount,
            &1000000,
            &false,
            &0,
        );
        bill_ids.push_back(bill_id);
        env.mock_all_auths();
    }

    env.mock_all_auths();
    let paid_count = client.batch_pay_bills(&owner, &bill_ids);

    assert_eq!(paid_count, 5);

    // Verify all bills are paid
    for bill_id in bill_ids.iter() {
        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);
        assert_eq!(bill.amount, amount);
    }
}

#[test]
fn test_overdue_bills_with_large_amounts() {
    let env = Env::default();
    set_time(&env, 2_000_000);

    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 2;

    client.create_bill(
        &owner,
        &String::from_str(&env, "Overdue Large"),
        &large_amount,
        &1000000, // Past due
        &false,
        &0,
    );

    let page = client.get_overdue_bills(&0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().amount, large_amount);
}

#[test]
fn test_edge_case_i128_max_minus_one() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with i128::MAX - 1
    let edge_amount = i128::MAX - 1;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Edge Case"),
        &edge_amount,
        &1000000,
        &false,
        &0,
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.amount, edge_amount);
}

#[test]
fn test_pagination_with_large_amounts() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = i128::MAX / 100;

    // Create multiple bills with large amounts
    for i in 0..15 {
        client.create_bill(
            &owner,
            &String::from_str(&env, &format!("Bill{}", i)),
            &large_amount,
            &1000000,
            &false,
            &0,
        );
        env.mock_all_auths();
    }

    // Test pagination
    let page1 = client.get_unpaid_bills(&owner, &0, &10);
    assert_eq!(page1.count, 10);
    assert!(page1.next_cursor > 0);

    let page2 = client.get_unpaid_bills(&owner, &page1.next_cursor, &10);
    assert_eq!(page2.count, 5);

    // Verify all amounts are correct
    for bill in page1.items.iter() {
        assert_eq!(bill.amount, large_amount);
    }
    for bill in page2.items.iter() {
        assert_eq!(bill.amount, large_amount);
    }
}
