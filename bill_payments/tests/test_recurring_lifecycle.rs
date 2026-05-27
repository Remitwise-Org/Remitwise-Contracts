//! # InvalidDueDate Boundary Tests & Recurring Next-Due Generation
//!
//! ## Observed due-date acceptance rule (from lib.rs, line ~530):
//! `if due_date == 0 || due_date < current_time { return Err(InvalidDueDate) }`
//!
//! `due_date >= now` → accepted
//! `due_date < now`  → InvalidDueDate (12)
//! `due_date == 0`   → InvalidDueDate (12)  (special-cased before the comparison)
//!
//! ## Boundary table
//! | due_date value  | expected result          |
//! |-----------------|--------------------------|
//! | now + 1         | Ok                       |
//! | now             | Ok  ← `<` is strict      |
//! | now - 1         | InvalidDueDate(12)       |
//! | 0               | InvalidDueDate(12)       |
//!
//! ## Recurring next-due formula (from lib.rs pay_bill):
//! ```text
//! let mut next_due_date = bill.due_date + frequency_days * SECONDS_PER_DAY;
//! while next_due_date <= current_time {
//!     next_due_date += frequency_days * SECONDS_PER_DAY;
//! }
//! ```
//! Base is `parent_due_date`, NOT `paid_at`.
//! The while-loop advances forward until the child is strictly in the future,
//! so a child bill is NEVER born overdue regardless of how late payment occurs.
//!
//! ## Security assertion:
//! child_due_date > paid_at for all valid inputs (frequency_days >= 1).
//! Guaranteed by the catch-up loop: loop exits only when next_due_date > current_time.

#![cfg(test)]

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Security helper
// ---------------------------------------------------------------------------

/// Assert that `child_due_date` is strictly in the future relative to `paid_at_timestamp`.
/// This is the core security invariant: a recurring child bill must never be born overdue.
fn assert_child_not_overdue(child_due_date: u64, paid_at_timestamp: u64, context: &str) {
    assert!(
        child_due_date > paid_at_timestamp,
        "Security violation in {}: child due_date {} <= paid_at {}. \
         Recurring spawn produced an already-overdue bill.",
        context,
        child_due_date,
        paid_at_timestamp
    );
}

// ---------------------------------------------------------------------------
// Test harness helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Existing lifecycle test (preserved)
// ---------------------------------------------------------------------------

#[test]
fn test_recurring_bill_lifecycle() {
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    env.mock_all_auths();

    let current_time = env.ledger().timestamp();
    let due_date = current_time + 86400;
    let frequency_days = 30u32;

    let bill_id = client.create_bill(
        &user,
        &String::from_str(&env, "Monthly Rent"),
        &10000,
        &due_date,
        &true,
        &frequency_days,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.id, bill_id);
    assert!(bill.recurring);
    assert_eq!(bill.frequency_days, frequency_days);
    assert_eq!(bill.schedule_id, None);

    client.pay_bill(&user, &bill_id);

    let paid_bill = client.get_bill(&bill_id).unwrap();
    assert!(paid_bill.paid);
    let paid_at = paid_bill.paid_at.unwrap();

    let next_bill_id = bill_id + 1;
    let next_bill = client.get_bill(&next_bill_id).unwrap();
    assert_eq!(next_bill.owner, user);
    assert_eq!(next_bill.amount, 10000);
    assert_eq!(next_bill.due_date, due_date + frequency_days as u64 * 86400);
    assert!(next_bill.recurring);
    assert!(!next_bill.paid);

    // Security: child must not be born overdue
    assert_child_not_overdue(next_bill.due_date, paid_at, "test_recurring_bill_lifecycle");

    // Double-pay must fail
    let result = client.try_pay_bill(&user, &bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillAlreadyPaid)));

    assert!(client.get_bill(&(next_bill_id + 1)).is_none());
}

// ---------------------------------------------------------------------------
// create_bill — InvalidDueDate boundary tests
// ---------------------------------------------------------------------------

/// due_date = now + 1  →  Ok (clearly future)
#[test]
fn test_create_bill_due_date_future_accepted() {
    let env = Env::default();
    let now = 1_000_000u64;
    env.ledger().set_timestamp(now);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &(now + 1),
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert!(result.is_ok(), "due_date = now+1 must be accepted");
}

/// due_date = now  →  Ok  (boundary: `<` is strict, so `==` passes)
#[test]
fn test_create_bill_due_date_exactly_now_accepted() {
    let env = Env::default();
    let now = 1_000_000u64;
    env.ledger().set_timestamp(now);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &now,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    // Condition: `due_date < current_time` → now < now is false → accepted
    assert!(
        result.is_ok(),
        "due_date == now must be accepted (strict-less-than boundary)"
    );
}

/// due_date = now - 1  →  InvalidDueDate(12)
#[test]
fn test_create_bill_due_date_one_second_past_rejected() {
    let env = Env::default();
    let now = 1_000_000u64;
    env.ledger().set_timestamp(now);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &(now - 1),
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::InvalidDueDate)),
        "due_date = now-1 must return InvalidDueDate"
    );
}

/// due_date = 0  →  InvalidDueDate(12)  (special-cased in the guard)
#[test]
fn test_create_bill_due_date_zero_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &0u64,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::InvalidDueDate)),
        "due_date = 0 must return InvalidDueDate"
    );
}

/// due_date far in the past  →  InvalidDueDate(12)
#[test]
fn test_create_bill_due_date_far_past_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000); // ~2023
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &946_684_800u64, // year 2000
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidDueDate)));
}

// ---------------------------------------------------------------------------
// create_bill — InvalidFrequency boundary tests
// ---------------------------------------------------------------------------

/// frequency_days = 0 on a recurring bill  →  InvalidFrequency
#[test]
fn test_create_bill_frequency_zero_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &2_000_000u64,
        &true,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidFrequency)));
}

/// frequency_days = MAX_FREQUENCY_DAYS (36_500)  →  Ok
#[test]
fn test_create_bill_frequency_max_accepted() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &2_000_000u64,
        &true,
        &36_500u32, // MAX_FREQUENCY_DAYS
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert!(result.is_ok(), "frequency_days = MAX_FREQUENCY_DAYS must be accepted");
}

/// frequency_days = MAX_FREQUENCY_DAYS + 1 (36_501)  →  InvalidFrequency
#[test]
fn test_create_bill_frequency_over_max_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &2_000_000u64,
        &true,
        &36_501u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidFrequency)));
}

/// frequency_days = 0 on a NON-recurring bill  →  Ok (frequency ignored)
#[test]
fn test_create_bill_frequency_zero_non_recurring_accepted() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bill"),
        &100,
        &2_000_000u64,
        &false, // not recurring
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert!(result.is_ok(), "frequency_days=0 on non-recurring bill must be accepted");
}

// ---------------------------------------------------------------------------
// pay_bill — recurring child due-date formula tests
// ---------------------------------------------------------------------------

/// Child due_date = parent.due_date + frequency_days * 86400 (paid on time)
#[test]
fn test_recurring_child_due_date_formula_on_time_payment() {
    let env = Env::default();
    let now = 500_000u64;
    let due_date = 1_000_000u64;
    let freq = 30u32;
    env.ledger().set_timestamp(now);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Rent"),
        &100,
        &due_date,
        &true,
        &freq,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Pay before due date
    env.ledger().set_timestamp(due_date - 1);
    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    assert_eq!(
        child.due_date,
        due_date + freq as u64 * 86400,
        "child due_date must equal parent.due_date + freq*86400"
    );
    assert!(!child.paid);
    assert_child_not_overdue(child.due_date, paid_at, "on_time_payment");
}

/// Child due_date is independent of paid_at (late payment scenario)
#[test]
fn test_recurring_child_due_date_independent_of_paid_at() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    let freq = 30u32;
    env.ledger().set_timestamp(0);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Rent"),
        &100,
        &due_date,
        &true,
        &freq,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Pay very late — 500 seconds after due_date
    let paid_at_time = due_date + 500;
    env.ledger().set_timestamp(paid_at_time);
    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    // Base formula: due_date + 30*86400 = 1_000_000 + 2_592_000 = 3_592_000
    // paid_at = 1_000_500 < 3_592_000 → no catch-up needed, formula holds
    let expected = due_date + freq as u64 * 86400;
    assert_eq!(child.due_date, expected, "child due_date must use parent.due_date, not paid_at");
    assert_child_not_overdue(child.due_date, paid_at, "late_payment_no_catchup");
}

/// Catch-up loop: when paid so late that parent+1*period is still in the past,
/// the loop advances until child is strictly in the future.
#[test]
fn test_recurring_child_catchup_when_paid_extremely_late() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    let freq = 1u32; // 1 day = 86400 s
    env.ledger().set_timestamp(0);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Daily"),
        &100,
        &due_date,
        &true,
        &freq,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Pay 10 days after due_date — so due_date + 1*86400 would still be in the past
    let paid_at_time = due_date + 10 * 86400 + 1;
    env.ledger().set_timestamp(paid_at_time);
    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    // Child must be strictly in the future (catch-up loop guarantees this)
    assert!(
        child.due_date > paid_at_time,
        "catch-up loop must advance child past current_time; got child.due_date={} paid_at_time={}",
        child.due_date,
        paid_at_time
    );
    assert!(!child.paid);
    assert_child_not_overdue(child.due_date, paid_at, "extremely_late_payment_catchup");
}

/// Multi-cycle: each successive child uses its own due_date as the base.
#[test]
fn test_recurring_multi_cycle_due_dates_chain_correctly() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    let freq = 30u32;
    env.ledger().set_timestamp(0);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let id1 = client.create_bill(
        &owner,
        &String::from_str(&env, "Monthly"),
        &100,
        &due_date,
        &true,
        &freq,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    client.pay_bill(&owner, &id1);
    let id2 = id1 + 1;
    let bill2 = client.get_bill(&id2).unwrap();
    let paid_at1 = client.get_bill(&id1).unwrap().paid_at.unwrap();
    assert_eq!(bill2.due_date, due_date + freq as u64 * 86400);
    assert_child_not_overdue(bill2.due_date, paid_at1, "cycle_1_to_2");

    client.pay_bill(&owner, &id2);
    let id3 = id2 + 1;
    let bill3 = client.get_bill(&id3).unwrap();
    let paid_at2 = client.get_bill(&id2).unwrap().paid_at.unwrap();
    assert_eq!(bill3.due_date, due_date + 2 * freq as u64 * 86400);
    assert_child_not_overdue(bill3.due_date, paid_at2, "cycle_2_to_3");

    client.pay_bill(&owner, &id3);
    let id4 = id3 + 1;
    let bill4 = client.get_bill(&id4).unwrap();
    let paid_at3 = client.get_bill(&id3).unwrap().paid_at.unwrap();
    assert_eq!(bill4.due_date, due_date + 3 * freq as u64 * 86400);
    assert_child_not_overdue(bill4.due_date, paid_at3, "cycle_3_to_4");
}

/// Early payment: child due_date still equals parent.due_date + period
#[test]
fn test_recurring_early_payment_does_not_shift_child_due_date() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    let freq = 30u32;
    // Pay very early — 900_000 seconds before due_date
    let paid_at_time = 100_000u64;
    env.ledger().set_timestamp(paid_at_time);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Rent"),
        &100,
        &due_date,
        &true,
        &freq,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    assert_eq!(
        child.due_date,
        due_date + freq as u64 * 86400,
        "early payment must not shift child due_date"
    );
    assert_child_not_overdue(child.due_date, paid_at, "early_payment");
}

/// frequency_days = 1 (minimum valid): child is exactly 86400 s after parent
#[test]
fn test_recurring_frequency_one_day_child_due_date() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    env.ledger().set_timestamp(0);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Daily"),
        &100,
        &due_date,
        &true,
        &1u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    assert_eq!(child.due_date, due_date + 86400);
    assert_child_not_overdue(child.due_date, paid_at, "frequency_one_day");
}

/// frequency_days = MAX_FREQUENCY_DAYS (36_500): child is 36_500 days after parent
#[test]
fn test_recurring_frequency_max_child_due_date() {
    let env = Env::default();
    let due_date = 1_000_000u64;
    env.ledger().set_timestamp(0);
    env.mock_all_auths();
    let cid = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Century"),
        &100,
        &due_date,
        &true,
        &36_500u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    client.pay_bill(&owner, &bill_id);

    let paid_at = client.get_bill(&bill_id).unwrap().paid_at.unwrap();
    let child = client.get_bill(&(bill_id + 1)).unwrap();

    let expected = due_date + 36_500u64 * 86400;
    assert_eq!(child.due_date, expected);
    assert_child_not_overdue(child.due_date, paid_at, "frequency_max");
}
