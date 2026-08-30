#![cfg(test)]

//! Stress tests for arithmetic operations with large i128 values
//!
//! Since Issue #1737, every stored amount is bounded by
//! `remitwise_common::MAX_AMOUNT` (10³⁰) and all aggregation arithmetic is
//! **checked**, never saturating:
//! - Amounts up to `MAX_AMOUNT` are accepted and round-trip exactly.
//! - Amounts above `MAX_AMOUNT` are rejected at the boundary with
//!   `BillPaymentsError::AmountExceedsMax` **before any state change**.
//! - Per-owner totals are exact; overflow is unreachable because each amount
//!   is validated and the owner cap bounds the sum.
//!
//! ## Documented Limitations
//! - Maximum accepted bill amount: `MAX_AMOUNT` (10³⁰ smallest units).
//! - `get_total_unpaid` / `get_total_unpaid_by_currency` use checked
//!   aggregation; the old saturating behavior is removed.

use bill_payments::{
    BillPayments, BillPaymentsClient, BillPaymentsError, DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS,
};
use remitwise_common::MAX_AMOUNT;
use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

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

/// Configure the trusted orchestrator required by the cross-contract epoch
/// guard on `pay_bill`. Returns the orchestrator address to pass to
/// `pay_bill(&orch, &0, ...)` (epoch 0 is the default for a fresh contract).
fn setup_orchestrator(client: &BillPaymentsClient, admin: &Address) -> Address {
    let orch = Address::generate(&client.env);
    client.init_admin(admin, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
    client.set_trusted_orchestrator(admin, &orch);
    orch
}

#[test]
fn test_create_bill_at_max_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // MAX_AMOUNT is the largest legal amount; it must round-trip exactly.
    let large_amount = MAX_AMOUNT;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.amount, large_amount);
    assert!(!bill.paid);
}

#[test]
fn test_create_bill_rejects_amount_above_max() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // One stroop above the cap must be rejected before any state change.
    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Too Big"),
        &(MAX_AMOUNT + 1),
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::AmountExceedsMax)));

    // No partial state: no bill was created and no id was consumed.
    assert!(client.get_bill(&1).is_none());
    assert_eq!(client.get_total_unpaid(&owner), 0);
}

#[test]
fn test_pay_bill_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    let orch = setup_orchestrator(&client, &owner);

    let large_amount = MAX_AMOUNT;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.pay_bill(&orch, &0, &owner, &bill_id);

    let bill = client.get_bill(&bill_id).unwrap();
    assert!(bill.paid);
    assert_eq!(bill.amount, large_amount);
    assert_eq!(client.get_total_unpaid(&owner), 0);
}

#[test]
fn test_recurring_bill_with_large_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    let orch = setup_orchestrator(&client, &owner);

    let large_amount = MAX_AMOUNT;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Recurring"),
        &large_amount,
        &1000000,
        &true,
        &30,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.pay_bill(&orch, &0, &owner, &bill_id);

    // Verify original bill is paid
    let bill = client.get_bill(&bill_id).unwrap();
    assert!(bill.paid);
    assert_eq!(bill.amount, large_amount);

    // Verify next recurring bill was created with same amount (net-zero total)
    let bill2 = client.get_bill(&2).unwrap();
    assert!(!bill2.paid);
    assert_eq!(bill2.amount, large_amount);
    assert_eq!(client.get_total_unpaid(&owner), large_amount);
}

#[test]
fn test_get_total_unpaid_with_two_large_bills() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Two MAX_AMOUNT bills: the total is exactly 2 × 10³⁰ (no saturation).
    let amount = MAX_AMOUNT;

    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill1"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill2"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let total = client.get_total_unpaid(&owner);
    assert_eq!(total, amount + amount);
}

#[test]
fn test_get_total_unpaid_exact_no_saturation() {
    // Regression for the pre-fix saturating aggregation: the total must be
    // the exact sum of validated amounts, never silently clamped.
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let amount = MAX_AMOUNT;

    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill1"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill2"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Exact sum of two MAX_AMOUNT bills — never saturates to i128::MAX.
    let total = client.get_total_unpaid(&owner);
    assert_eq!(total, 2 * MAX_AMOUNT);
    assert!(total < i128::MAX);
}

#[test]
fn test_get_total_unpaid_by_currency_exact() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Two MAX_AMOUNT USDC bills: exact sum, no saturation.
    let amount = MAX_AMOUNT;

    client.create_bill(
        &owner,
        &String::from_str(&env, "USDC Bill 1"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "USDC"),
        &None,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "USDC Bill 2"),
        &amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "USDC"),
        &None,
    );

    let total = client.get_total_unpaid_by_currency(&owner, &String::from_str(&env, "USDC"));
    assert_eq!(total, 2 * MAX_AMOUNT);

    // Create a XLM bill and verify it's not included in USDC total
    env.mock_all_auths();
    client.create_bill(
        &owner,
        &String::from_str(&env, "XLM Bill"),
        &1000,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // USDC total unchanged; XLM total only includes XLM bills.
    let usdc_total = client.get_total_unpaid_by_currency(&owner, &String::from_str(&env, "USDC"));
    assert_eq!(usdc_total, 2 * MAX_AMOUNT);
    let xlm_total = client.get_total_unpaid_by_currency(&owner, &String::from_str(&env, "XLM"));
    assert_eq!(xlm_total, 1000);
}

#[test]
fn test_multiple_large_bills_different_owners() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner1 = <soroban_sdk::Address as AddressTrait>::generate(&env);
    let owner2 = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    let large_amount = MAX_AMOUNT;

    // Each owner can have large bills independently
    client.create_bill(
        &owner1,
        &String::from_str(&env, "Owner1 Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.create_bill(
        &owner2,
        &String::from_str(&env, "Owner2 Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
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
    let orch = setup_orchestrator(&client, &owner);

    let large_amount = MAX_AMOUNT;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Large Bill"),
        &large_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    env.mock_all_auths();
    client.pay_bill(&orch, &0, &owner, &bill_id);

    env.mock_all_auths();
    let before_timestamp: u64 = 2_000_000;
    client.archive_paid_bills(&owner, &before_timestamp);

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

    let amount = MAX_AMOUNT;

    let mut bill_ids = soroban_sdk::Vec::new(&env);

    for i in 0..5 {
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, &format!("Bill{}", i)),
            &amount,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        bill_ids.push_back(bill_id);
        env.mock_all_auths();
    }

    env.mock_all_auths();
    let result = client.batch_pay_bills(&owner, &bill_ids);
    assert!(result.is_ok(), "batch of MAX_AMOUNT bills must succeed exactly");

    // Verify all bills are paid and the total is exactly zero.
    for bill_id in bill_ids.iter() {
        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);
        assert_eq!(bill.amount, amount);
    }
    assert_eq!(client.get_total_unpaid(&owner), 0);
}

#[test]
fn test_edge_case_max_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Test with MAX_AMOUNT — the exact upper boundary.
    let edge_amount = MAX_AMOUNT;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Edge Case"),
        &edge_amount,
        &1000000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
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

    let large_amount = MAX_AMOUNT;

    // Create multiple bills with large amounts
    for i in 0..15 {
        client.create_bill(
            &owner,
            &String::from_str(&env, &format!("Bill{}", i)),
            &large_amount,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
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

#[test]
fn test_recurring_bill_max_frequency() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    let orch = setup_orchestrator(&client, &owner);

    // Use the maximum allowed frequency (36500 days = 100 years)
    let max_freq = 36500;

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Max Freq Bill"),
        &100,
        &1000000,
        &true,
        &max_freq,
        &None, // external_ref
        &String::from_str(&env, "XLM"),
        &None,
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.frequency_days, max_freq);

    // Pay it and verify next bill
    env.mock_all_auths();
    client.pay_bill(&orch, &0, &owner, &bill_id);

    let next_bill = client.get_bill(&2).unwrap();
    let expected_due = 1000000u64 + (max_freq as u64 * 86400);
    assert_eq!(next_bill.due_date, expected_due);
}

#[test]
fn test_recurring_bill_frequency_overflow_protection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();

    // Try to create a bill with a frequency that exceeds MAX_FREQUENCY_DAYS
    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Too High Freq"),
        &100,
        &1000000,
        &true,
        &40000, // Greater than 36500
        &None,  // external_ref
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Should fail with InvalidFrequency
    use bill_payments::Error;
    assert_eq!(result, Err(Ok(Error::InvalidFrequency)));
}

#[test]
fn test_recurring_bill_date_overflow_protection() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    let orch = setup_orchestrator(&client, &owner);

    // Create a bill with a due date very close to u64::MAX
    let near_max_due = u64::MAX - 86400;

    // First, we need to set the ledger time to something before due_date so create_bill succeeds
    set_time(&env, near_max_due - 1000);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Near Max Due"),
        &100,
        &near_max_due,
        &true,
        &30,   // 30 days will definitely overflow if added to near_max_due
        &None, // external_ref
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Paying this should fail due to date overflow (recurring child's due
    // date cannot be represented) — deterministic, no partial state.
    env.mock_all_auths();
    let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);

    use bill_payments::Error;
    assert_eq!(result, Err(Ok(Error::InvalidDueDate)));
}
