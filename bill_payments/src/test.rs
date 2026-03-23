#![cfg(test)]

use crate::{BillPayments, BillPaymentsClient, Error};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let owner = Address::generate(&env);
    (env, owner, contract_id)
}

fn try_create(
    client: &BillPaymentsClient<'_>,
    owner: &Address,
    name: &String,
    amount: i128,
    due_date: u64,
    recurring: bool,
    frequency_days: u32,
    currency: &String,
) -> Result<u32, Error> {
    let result = client
        .try_create_bill(
            owner,
            name,
            &amount,
            &due_date,
            &recurring,
            &frequency_days,
            &None,
            currency,
        );
    match result {
        Ok(Ok(id)) => Ok(id),
        Ok(Err(_)) => panic!("unexpected conversion error from client"),
        Err(Ok(err)) => Err(err),
        Err(Err(_)) => panic!("unexpected invoke error from host"),
    }
}

#[test]
fn create_bill_rejects_zero_amount() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Power"),
        0,
        env.ledger().timestamp() + 300,
        false,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidAmount));
}

#[test]
fn create_bill_rejects_negative_amount() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Water"),
        -1,
        env.ledger().timestamp() + 300,
        false,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidAmount));
}

#[test]
fn create_bill_rejects_zero_due_date() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Internet"),
        100,
        0,
        false,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidDueDate));
}

#[test]
fn create_bill_rejects_due_date_equal_to_now() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let now = env.ledger().timestamp();
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Rent"),
        100,
        now,
        false,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidDueDate));
}

#[test]
fn create_bill_rejects_due_date_in_past() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    env.ledger().set_timestamp(10_000);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "School Fees"),
        100,
        9_999,
        false,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidDueDate));
}

#[test]
fn create_bill_rejects_recurring_with_zero_frequency() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Subscription"),
        100,
        env.ledger().timestamp() + 500,
        true,
        0,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidFrequency));
}

#[test]
fn create_bill_rejects_non_recurring_with_frequency() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "One-Time Tax"),
        200,
        env.ledger().timestamp() + 600,
        false,
        30,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::InvalidRecurrenceCombination));
}

#[test]
fn create_bill_rejects_recurrence_overflow() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let result = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Edge Overflow"),
        200,
        u64::MAX - 1,
        true,
        1,
        &String::from_str(&env, "XLM"),
    );
    assert_eq!(result, Err(Error::FrequencyOverflow));
}

#[test]
fn create_bill_accepts_valid_non_recurring_inputs() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let id = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Valid One-Time"),
        300,
        env.ledger().timestamp() + 700,
        false,
        0,
        &String::from_str(&env, "XLM"),
    )
    .expect("valid input should create bill");
    assert_eq!(id, 1);
}

#[test]
fn create_bill_accepts_valid_recurring_inputs() {
    let (env, owner, contract_id) = setup();
    let client = BillPaymentsClient::new(&env, &contract_id);
    let id = try_create(
        &client,
        &owner,
        &String::from_str(&env, "Valid Recurring"),
        300,
        env.ledger().timestamp() + 700,
        true,
        30,
        &String::from_str(&env, "XLM"),
    )
    .expect("valid recurring input should create bill");
    assert_eq!(id, 1);
}
