#![cfg(test)]

use super::*;
use soroban_sdk::{Address, Env, String, Vec};
use soroban_sdk::testutils::Address as _;
use crate::state::{BillState, check_invariants};

#[test]
fn test_all_legal_transitions() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    
    // Test Active → Paid
    let bill = create_test_bill(&env, &owner, false);
    assert!(BillState::from_bill(&bill, false).can_transition_to(BillState::Paid));
    
    // Test Active → Cancelled
    let bill = create_test_bill(&env, &owner, false);
    assert!(BillState::from_bill(&bill, false).can_transition_to(BillState::Cancelled));
    
    // Test Paid → Archived
    let mut paid_bill = create_test_bill(&env, &owner, false);
    paid_bill.paid = true;
    paid_bill.paid_at = Some(env.ledger().timestamp());
    assert!(BillState::from_bill(&paid_bill, false).can_transition_to(BillState::Archived));
    
    // Test Cancelled → Archived
    assert!(BillState::Cancelled.can_transition_to(BillState::Archived));
    
    // Test Archived → Active (restore)
    assert!(BillState::Archived.can_transition_to(BillState::Active));
}

#[test]
fn test_illegal_transitions_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    
    // Paid → Cancelled (should fail)
    let mut paid_bill = create_test_bill(&env, &owner, false);
    paid_bill.paid = true;
    paid_bill.paid_at = Some(env.ledger().timestamp());
    let result = BillState::validate_transition(&paid_bill, false, BillState::Cancelled, "test");
    assert!(result.is_err());
    
    // Cancelled → Paid (should fail)
    let cancelled_bill = create_test_bill(&env, &owner, false);
    let result = BillState::validate_transition(&cancelled_bill, false, BillState::Paid, "test");
    assert!(result.is_err());
    
    // Archived → Paid (should fail)
    let archived_bill = create_test_bill(&env, &owner, true);
    let result = BillState::validate_transition(&archived_bill, true, BillState::Paid, "test");
    assert!(result.is_err());
}

#[test]
fn test_invariant_paid_without_paid_at() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let mut bill = create_test_bill(&env, &owner, false);
    bill.paid = true;
    bill.paid_at = None;
    assert!(check_invariants(&env, &bill, false).is_err());
}

#[test]
fn test_invariant_unpaid_with_paid_at() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let mut bill = create_test_bill(&env, &owner, false);
    bill.paid = false;
    bill.paid_at = Some(12345);
    assert!(check_invariants(&env, &bill, false).is_err());
}

#[test]
fn test_invariant_recurring_with_zero_frequency() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let mut bill = create_test_bill(&env, &owner, false);
    bill.recurring = true;
    bill.frequency_days = 0;
    assert!(check_invariants(&env, &bill, false).is_err());
}

#[test]
fn test_invariant_zero_amount() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let mut bill = create_test_bill(&env, &owner, false);
    bill.amount = 0;
    assert!(check_invariants(&env, &bill, false).is_err());
}

#[test]
fn test_recurring_bill_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    
    // Create recurring bill
    let mut bill = create_test_bill(&env, &owner, false);
    bill.recurring = true;
    bill.frequency_days = 30;
    
    // Active → Paid (valid)
    assert!(BillState::validate_transition(&bill, false, BillState::Paid, "test").is_ok());
    
    // Pay it
    bill.paid = true;
    bill.paid_at = Some(env.ledger().timestamp());
    
    // Paid → Archive (valid)
    assert!(BillState::validate_transition(&bill, false, BillState::Archived, "test").is_ok());
}

// Helper function
fn create_test_bill(env: &Env, owner: &Address, _archived: bool) -> Bill {
    Bill {
        id: 1,
        owner: owner.clone(),
        name: String::from_str(env, "Test Bill"),
        external_ref: None,
        amount: 1000,
        due_date: env.ledger().timestamp() + 86400,
        recurring: false,
        frequency_days: 0,
        paid: false,
        created_at: env.ledger().timestamp(),
        paid_at: None,
        schedule_id: None,
        tags: Vec::new(env),
        currency: String::from_str(env, "XLM"),
    }
}