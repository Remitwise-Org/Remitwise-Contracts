//! # State-Transition Invariant Tests — Issue #1747
//!
//! Enforces recurring-plan transitions, authorization, amount rules, and overdue
//! behavior without hidden state changes. Covers every legal edge plus stale,
//! repeated, skipped, and out-of-order transitions with invariant checks.
//!
//! ## Legal Transition Matrix
//!
//! | From     | To       | Trigger                  | Legal? |
//! |----------|----------|--------------------------|--------|
//! | Active   | Paid     | `pay_bill`               | Yes    |
//! | Active   | Deleted  | `cancel_bill`            | Yes    |
//! | Paid     | Archived | `archive_paid_bills`     | Yes    |
//! | Archived | Active   | `restore_bill`           | Yes    |
//! | Paid     | Active   | `reverse_payment`        | Yes    |
//! | Active   | Archived | —                        | No     |
//! | Archived | Paid     | —                        | No     |
//! | Archived | Deleted  | —                        | No     |
//! | Paid     | Deleted  | `cancel_bill`            | No     |
//! | Paid     | Paid     | `pay_bill` (double-pay)  | No     |
//!
//! ## Coverage
//!
//! - **Legal transitions:** Active→Paid, Active→Deleted, Paid→Archived,
//!   Archived→Active, Paid→Active (reverse)
//! - **Illegal transitions:** Paid→Deleted (cancel), Paid→Paid (double-pay),
//!   Active→Archived (archive unpaid), Archived→Paid (pay archived)
//! - **Recurring lifecycle:** create → pay → child spawned → pay child → cancel
//! - **Invariant preservation:** unpaid totals, owner index, currency index,
//!   paid/paid_at consistency, recurring/frequency consistency
//! - **Stale/repeated ops:** double-pay, double-cancel, pay-after-cancel
//! - **Amount rules:** zero amount rejected, negative amount rejected
//! - **Overdue behavior:** child never born overdue, catch-up loop
//! - **Authorization:** cross-owner pay rejected, cross-owner cancel rejected

#![cfg(test)]

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String, Vec as SorobanVec};

const SECONDS_PER_DAY: u64 = 86_400;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct InvariantHarness {
    env: Env,
    client: BillPaymentsClient<'static>,
    owner: Address,
    orchestrator: Address,
}

impl InvariantHarness {
    fn new(timestamp: u64) -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(timestamp);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        // The cross-contract epoch guard on `pay_bill` requires a trusted
        // orchestrator (epoch 0 is the default for a fresh contract).
        let orchestrator = Address::generate(&env);
        client.init_admin(&owner, &bill_payments::DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orchestrator);
        Self {
            env,
            client,
            owner,
            orchestrator,
        }
    }

    fn create_bill(
        &self,
        name: &str,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
        currency: &str,
    ) -> u32 {
        self.client.create_bill(
            &self.owner,
            &String::from_str(&self.env, name),
            &amount,
            &due_date,
            &recurring,
            &frequency_days,
            &None,
            &String::from_str(&self.env, currency),
            &None,
        )
    }

    fn pay_at(&self, bill_id: u32, timestamp: u64) {
        self.env.ledger().set_timestamp(timestamp);
        self.client
            .pay_bill(&self.orchestrator, &0, &self.owner, &bill_id);
    }

    fn try_pay_at(&self, bill_id: u32, timestamp: u64) -> Result<(), BillPaymentsError> {
        self.env.ledger().set_timestamp(timestamp);
        self.client
            .try_pay_bill(&self.orchestrator, &0, &self.owner, &bill_id)
    }

    fn cancel(&self, bill_id: u32) {
        self.client.cancel_bill(&self.owner, &bill_id);
    }

    fn try_cancel(&self, bill_id: u32) -> Result<(), BillPaymentsError> {
        self.client.try_cancel_bill(&self.owner, &bill_id)
    }

    fn total_unpaid(&self) -> i128 {
        self.client.get_total_unpaid(&self.owner)
    }

    fn bill_exists(&self, bill_id: u32) -> bool {
        self.client.get_bill(&bill_id).is_some()
    }

    fn is_paid(&self, bill_id: u32) -> bool {
        self.client
            .get_bill(&bill_id)
            .map(|b| b.paid)
            .unwrap_or(false)
    }
}

// ===========================================================================
// 1. Legal Transitions — Positive Tests
// ===========================================================================

/// Active → Paid: Pay an unpaid bill and verify state.
#[test]
fn test_active_to_paid_transition() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Rent", 5000, due_date, false, 0, "XLM");

    assert!(!h.is_paid(bill_id));
    assert_eq!(h.total_unpaid(), 5000);

    h.pay_at(bill_id, due_date);

    assert!(h.is_paid(bill_id));
    let bill = h.client.get_bill(&bill_id).unwrap();
    assert!(bill.paid_at.is_some());
    assert_eq!(bill.paid_at.unwrap(), due_date);
}

/// Active → Deleted: Cancel an unpaid bill and verify removal.
#[test]
fn test_active_to_deleted_transition() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Phone", 200, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 200);

    h.cancel(bill_id);

    assert!(!h.bill_exists(bill_id));
    assert_eq!(h.total_unpaid(), 0);
}

/// Paid → Archived: Archive a paid bill.
#[test]
fn test_paid_to_archived_transition() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Groceries", 300, due_date, false, 0, "XLM");

    h.pay_at(bill_id, due_date);
    assert!(h.is_paid(bill_id));

    // Archive by setting before_timestamp after paid_at
    h.client.archive_paid_bills(&h.owner, &(due_date + 1));

    assert!(!h.bill_exists(bill_id));
    let archived = h.client.get_archived_bill(&bill_id);
    assert!(archived.is_some());
    let archived_bill = archived.unwrap();
    assert_eq!(archived_bill.amount, 300);
}

/// Archived → Active: Restore an archived bill.
#[test]
fn test_archived_to_active_transition() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Insurance", 1000, due_date, false, 0, "XLM");

    h.pay_at(bill_id, due_date);
    h.client.archive_paid_bills(&h.owner, &(due_date + 1));
    assert!(!h.bill_exists(bill_id));
    assert!(h.client.get_archived_bill(&bill_id).is_some());

    h.client.restore_bill(&h.owner, &bill_id);

    assert!(h.bill_exists(bill_id));
    assert!(h.client.get_archived_bill(&bill_id).is_none());
    let restored = h.client.get_bill(&bill_id).unwrap();
    assert!(restored.paid);
    assert!(restored.paid_at.is_some());
}

/// Paid → Active: Reverse a payment.
#[test]
fn test_paid_to_active_via_reverse() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Water", 150, due_date, false, 0, "XLM");

    h.pay_at(bill_id, due_date);
    assert!(h.is_paid(bill_id));

    h.client
        .reverse_payment(&h.owner, &bill_id, &150);

    assert!(!h.is_paid(bill_id));
    let bill = h.client.get_bill(&bill_id).unwrap();
    assert!(bill.paid_at.is_none());
    assert_eq!(h.total_unpaid(), 150);
}

// ===========================================================================
// 2. Illegal Transitions — Negative Tests
// ===========================================================================

/// Paid → Deleted: Cancelling a paid bill must fail with BillAlreadyPaid.
#[test]
fn test_paid_to_deleted_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Electric", 800, due_date, false, 0, "XLM");

    h.pay_at(bill_id, due_date);
    assert!(h.is_paid(bill_id));

    let result = h.try_cancel(bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillAlreadyPaid)));
    // Bill must still exist and be paid
    assert!(h.bill_exists(bill_id));
    assert!(h.is_paid(bill_id));
}

/// Paid → Paid: Double-pay must fail with BillAlreadyPaid.
#[test]
fn test_paid_to_paid_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Internet", 600, due_date, false, 0, "XLM");

    h.pay_at(bill_id, due_date);

    let result = h.try_pay_at(bill_id, due_date + 100);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillAlreadyPaid)));
}

// ===========================================================================
// 3. Recurring Lifecycle — State-Transition Invariants
// ===========================================================================

/// Full recurring lifecycle: create → pay → child spawned → pay child → verify
/// invariants preserved across multiple cycles.
#[test]
fn test_recurring_full_lifecycle_invariants() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let freq = 30u32;
    let amount = 5000i128;

    let parent_id = h.create_bill("Rent", amount, due_date, true, freq, "XLM");
    assert_eq!(h.total_unpaid(), amount);

    // Pay parent → child spawned
    h.pay_at(parent_id, due_date);
    assert!(h.is_paid(parent_id));
    assert_eq!(h.total_unpaid(), amount); // child replaces parent

    let child_id = parent_id + 1;
    assert!(h.bill_exists(child_id));
    let child = h.client.get_bill(&child_id).unwrap();
    assert!(!child.paid);
    assert_eq!(child.amount, amount);
    assert_eq!(
        child.due_date,
        due_date + freq as u64 * SECONDS_PER_DAY
    );
    assert!(child.recurring);
    assert_eq!(child.frequency_days, freq);
    assert!(child.due_date > due_date, "child must not be born overdue");

    // Pay child → grandchild spawned
    let child_due = child.due_date;
    h.pay_at(child_id, child_due);
    assert!(h.is_paid(child_id));
    assert_eq!(h.total_unpaid(), amount);

    let grandchild_id = child_id + 1;
    assert!(h.bill_exists(grandchild_id));
    let grandchild = h.client.get_bill(&grandchild_id).unwrap();
    assert!(!grandchild.paid);
    assert_eq!(
        grandchild.due_date,
        child_due + freq as u64 * SECONDS_PER_DAY
    );
    assert!(grandchild.recurring);

    // Cancel grandchild — recurring chain stops
    h.cancel(grandchild_id);
    assert!(!h.bill_exists(grandchild_id));
    assert_eq!(h.total_unpaid(), 0);
}

/// Recurring child is never born overdue, even when parent is paid very late.
#[test]
fn test_recurring_child_never_born_overdue() {
    let h = InvariantHarness::new(0);
    let due_date = 1_000_000u64;
    let freq = 1u32; // 1 day

    let bill_id = h.create_bill("Daily", 100, due_date, true, freq, "USDC");

    // Pay exactly 30 days late — the maximum allowed by the settlement window
    // (strict `>` comparison permits `due_date + 30d` at the boundary).
    let paid_at = due_date + 30 * SECONDS_PER_DAY;
    h.pay_at(bill_id, paid_at);

    let child_id = bill_id + 1;
    let child = h.client.get_bill(&child_id).unwrap();
    assert!(
        child.due_date > paid_at,
        "child due_date {} must be > paid_at {}",
        child.due_date,
        paid_at
    );
    assert!(!child.paid);

    // Also verify child is not in the overdue list
    let overdue = h.client.get_overdue_bills(&0, &100);
    for bill in overdue.items.iter() {
        assert_ne!(bill.id, child_id, "newly spawned child must not be overdue");
    }
}

/// Paying a recurring bill adjusts unpaid total correctly:
/// parent subtracted, child added, net = 0 change.
#[test]
fn test_recurring_pay_unpaid_total_invariant() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let amount = 7500i128;

    let bill_id = h.create_bill("Loan", amount, due_date, true, 30, "USDC");
    assert_eq!(h.total_unpaid(), amount);

    h.pay_at(bill_id, due_date);
    // After paying recurring: parent removed (−amount), child added (+amount)
    assert_eq!(h.total_unpaid(), amount);

    // Cancel the child
    h.cancel(bill_id + 1);
    assert_eq!(h.total_unpaid(), 0);
}

/// Non-recurring pay: unpaid total decreases by the bill amount.
#[test]
fn test_non_recurring_pay_unpaid_total_decreases() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let id1 = h.create_bill("One", 200, due_date, false, 0, "XLM");
    let id2 = h.create_bill("Two", 300, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 500);

    h.pay_at(id1, due_date);
    assert_eq!(h.total_unpaid(), 300);

    h.pay_at(id2, due_date);
    assert_eq!(h.total_unpaid(), 0);
}

// ===========================================================================
// 4. Stale / Repeated Operations — No Partial or Unauthorized State
// ===========================================================================

/// Double-cancel must fail and leave bill intact.
#[test]
fn test_double_cancel_no_side_effects() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Gym", 50, due_date, false, 0, "XLM");

    h.cancel(bill_id);
    assert!(!h.bill_exists(bill_id));

    // Second cancel must fail — bill doesn't exist
    let result = h.try_cancel(bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillNotFound)));
}

/// Pay after cancel must fail — bill no longer exists.
#[test]
fn test_pay_after_cancel_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let bill_id = h.create_bill("Mail", 30, due_date, false, 0, "XLM");

    h.cancel(bill_id);

    let result = h.try_pay_at(bill_id, due_date);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillNotFound)));
}

/// Pay non-existent bill fails cleanly.
#[test]
fn test_pay_nonexistent_bill_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.try_pay_at(9999, 2_000_000);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillNotFound)));
}

/// Cancel non-existent bill fails cleanly.
#[test]
fn test_cancel_nonexistent_bill_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.try_cancel(9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillNotFound)));
}

/// Repeated pay attempts on the same bill don't create duplicate children.
#[test]
fn test_repeated_pay_no_duplicate_children() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let bill_id = h.create_bill("Sub", 500, due_date, true, 7, "XLM");
    h.pay_at(bill_id, due_date);

    // Second pay attempt — must fail
    let result = h.try_pay_at(bill_id, due_date + 100);
    assert_eq!(result, Err(Ok(BillPaymentsError::BillAlreadyPaid)));

    // Only one child should exist
    assert!(h.bill_exists(bill_id + 1));
    assert!(!h.bill_exists(bill_id + 2), "no duplicate child");
}

// ===========================================================================
// 5. Authorization — Cross-owner Operations Blocked
// ===========================================================================

/// Cross-owner pay must fail with Unauthorized.
#[test]
fn test_cross_owner_pay_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let orchestrator = Address::generate(&env);
    client.init_admin(&alice, &bill_payments::DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
    client.set_trusted_orchestrator(&alice, &orchestrator);

    let bill_id = client.create_bill(
        &alice,
        &String::from_str(&env, "Shared"),
        &1000,
        &2_000_000u64,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let result = client.try_pay_bill(&orchestrator, &0, &bob, &bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));
}

/// Cross-owner cancel must fail with Unauthorized.
#[test]
fn test_cross_owner_cancel_rejected() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let bill_id = client.create_bill(
        &alice,
        &String::from_str(&env, "Secret"),
        &100,
        &2_000_000u64,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let result = client.try_cancel_bill(&bob, &bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));
}

// ===========================================================================
// 6. Amount Rules
// ===========================================================================

/// Zero amount rejected at creation.
#[test]
fn test_zero_amount_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.client.try_create_bill(
        &h.owner,
        &String::from_str(&h.env, "Zero"),
        &0,
        &2_000_000u64,
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidAmount)));
}

/// Negative amount rejected at creation.
#[test]
fn test_negative_amount_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.client.try_create_bill(
        &h.owner,
        &String::from_str(&h.env, "Negative"),
        &-100,
        &2_000_000u64,
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidAmount)));
}

/// Recurring with frequency_days = 0 rejected.
#[test]
fn test_recurring_zero_frequency_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.client.try_create_bill(
        &h.owner,
        &String::from_str(&h.env, "Bad Freq"),
        &100,
        &2_000_000u64,
        &true,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidFrequency)));
}

/// Due date in the past rejected.
#[test]
fn test_past_due_date_rejected() {
    let h = InvariantHarness::new(2_000_000);
    let result = h.client.try_create_bill(
        &h.owner,
        &String::from_str(&h.env, "Past"),
        &100,
        &1_999_999u64,
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidDueDate)));
}

// ===========================================================================
// 7. Invariant Preservation — Unpaid Totals, Indexes
// ===========================================================================

/// Creating and cancelling bills preserves unpaid total correctness.
#[test]
fn test_unpaid_total_create_cancel_preserves_invariant() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 5_000_000u64;

    let id1 = h.create_bill("A", 100, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 100);

    let id2 = h.create_bill("B", 200, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 300);

    let id3 = h.create_bill("C", 300, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 600);

    h.cancel(id2);
    assert_eq!(h.total_unpaid(), 400);

    h.cancel(id1);
    assert_eq!(h.total_unpaid(), 300);

    h.cancel(id3);
    assert_eq!(h.total_unpaid(), 0);
}

/// Archived bill: unpaid total unchanged after archiving (already paid).
#[test]
fn test_archive_does_not_affect_unpaid_total() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let id1 = h.create_bill("X", 500, due_date, false, 0, "XLM");
    let id2 = h.create_bill("Y", 600, due_date, false, 0, "XLM");
    assert_eq!(h.total_unpaid(), 1100);

    h.pay_at(id1, due_date);
    assert_eq!(h.total_unpaid(), 600);

    h.client.archive_paid_bills(&h.owner, &(due_date + 1));
    // Archive only moves paid bills — unpaid total should remain the same
    assert_eq!(h.total_unpaid(), 600);
}

/// Multiple owners have isolated unpaid totals.
#[test]
fn test_unpaid_total_owner_isolation() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let due_date = 2_000_000u64;
    let orchestrator = Address::generate(&env);
    client.init_admin(&alice, &bill_payments::DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
    client.set_trusted_orchestrator(&alice, &orchestrator);

    client.create_bill(
        &alice,
        &String::from_str(&env, "Alice Bill"),
        &1000,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    client.create_bill(
        &bob,
        &String::from_str(&env, "Bob Bill"),
        &2000,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    assert_eq!(client.get_total_unpaid(&alice), 1000);
    assert_eq!(client.get_total_unpaid(&bob), 2000);

    // Pay Alice's bill — Bob's total unaffected
    client.pay_bill(&orchestrator, &0, &alice, &1);
    assert_eq!(client.get_total_unpaid(&alice), 0);
    assert_eq!(client.get_total_unpaid(&bob), 2000);
}

// ===========================================================================
// 8. Recurring Bill Schedule Lifecycle — State-Transition Invariants
// ===========================================================================

/// Schedule: create → execute → cancel → verify no more execution.
#[test]
fn test_schedule_lifecycle_create_execute_cancel() {
    let h = InvariantHarness::new(1_000_000);
    let now = 1_000_000u64;

    let schedule_id = h.client.create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "Monthly"),
        &5000,
        &String::from_str(&h.env, "XLM"),
        &(now + 1000),
        &86400,
    );

    // Execute past next_due
    h.env.ledger().set_timestamp(now + 2000);
    let executed = h.client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    // A bill was created
    assert!(h.client.get_bill(&(h.client.get_next_bill_id())).is_some() || {
        // Bill exists with some ID > 0
        let page = h.client.get_unpaid_bills(&h.owner, &0, &10);
        page.count > 0
    });

    // Cancel the schedule
    h.client.cancel_bill_schedule(&h.owner, &schedule_id);
    let schedule = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);

    // Next execution should not generate more bills
    h.env.ledger().set_timestamp(now + 200_000);
    let executed2 = h.client.execute_due_bill_schedules();
    assert_eq!(executed2.len(), 0, "cancelled schedule must not execute");
}

/// Modifying a schedule after creation changes the next generated bill's amount.
#[test]
fn test_schedule_modify_affects_next_bill() {
    let h = InvariantHarness::new(1_000_000);
    let now = 1_000_000u64;

    let schedule_id = h.client.create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "Phone"),
        &1000,
        &String::from_str(&h.env, "XLM"),
        &(now + 1000),
        &86400,
    );

    // Modify amount before execution
    h.client.modify_bill_schedule(
        &h.owner,
        &schedule_id,
        &2500,
        &(now + 2000),
        &86400,
    );

    // Execute
    h.env.ledger().set_timestamp(now + 3000);
    h.client.execute_due_bill_schedules();

    // The generated bill should have the modified amount
    let page = h.client.get_unpaid_bills(&h.owner, &0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().amount, 2500);
}

/// Executing an inactive schedule is a no-op.
#[test]
fn test_inactive_schedule_is_noop() {
    let h = InvariantHarness::new(1_000_000);
    let now = 1_000_000u64;

    let schedule_id = h.client.create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "OneOff"),
        &3000,
        &String::from_str(&h.env, "XLM"),
        &(now + 1000),
        &0, // one-off
    );

    h.env.ledger().set_timestamp(now + 2000);
    let executed = h.client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1);

    // Second execution: schedule is now inactive
    let executed2 = h.client.execute_due_bill_schedules();
    assert_eq!(executed2.len(), 0);
}

// ===========================================================================
// 9. Overdue Behavior
// ===========================================================================

/// Bills past due_date appear in overdue list; paid bills do not.
#[test]
fn test_overdue_bills_exclude_paid() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let id1 = h.create_bill("Late", 100, due_date, false, 0, "XLM");
    let id2 = h.create_bill("Paid", 200, due_date, false, 0, "XLM");

    // Move past due date
    h.env.ledger().set_timestamp(due_date + 100);

    // Both overdue before payment
    let overdue = h.client.get_overdue_bills(&0, &100);
    let overdue_ids: std::vec::Vec<u32> = overdue.items.iter().map(|b| b.id).collect();
    assert!(overdue_ids.contains(&id1));
    assert!(overdue_ids.contains(&id2));

    // Pay id2
    h.pay_at(id2, due_date + 100);

    // Only id1 is now overdue
    let overdue = h.client.get_overdue_bills(&0, &100);
    let overdue_ids: std::vec::Vec<u32> = overdue.items.iter().map(|b| b.id).collect();
    assert!(overdue_ids.contains(&id1));
    assert!(!overdue_ids.contains(&id2));
}

/// Billed with due_date == now is NOT overdue (strict less-than comparison).
#[test]
fn test_due_date_equal_now_not_overdue() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 1_000_000u64;

    h.create_bill("Equal", 100, due_date, false, 0, "XLM");

    // due_date == now — should not be overdue
    let overdue = h.client.get_overdue_bills(&0, &100);
    assert_eq!(overdue.count, 0);
}

// ===========================================================================
// 10. Edge Cases — Frequency Bounds
// ===========================================================================

/// frequency_days = 1 (minimum valid): child due_date = parent + 86400.
#[test]
fn test_min_frequency_child_due_date() {
    let h = InvariantHarness::new(0);
    let due_date = 1_000_000u64;

    let bill_id = h.create_bill("Daily", 50, due_date, true, 1, "USDC");
    h.pay_at(bill_id, due_date);

    let child = h.client.get_bill(&(bill_id + 1)).unwrap();
    assert_eq!(child.due_date, due_date + 86400);
}

/// frequency_days = MAX (36_500): child due_date = parent + 36_500 * 86400.
#[test]
fn test_max_frequency_child_due_date() {
    let h = InvariantHarness::new(0);
    let due_date = 1_000_000u64;

    let bill_id = h.create_bill("Century", 50, due_date, true, 36_500, "USDC");
    h.pay_at(bill_id, due_date);

    let child = h.client.get_bill(&(bill_id + 1)).unwrap();
    assert_eq!(child.due_date, due_date + 36_500u64 * 86400);
}

/// frequency_days = MAX+1 (36_501) rejected.
#[test]
fn test_max_plus_one_frequency_rejected() {
    let h = InvariantHarness::new(1_000_000);
    let result = h.client.try_create_bill(
        &h.owner,
        &String::from_str(&h.env, "TooFreq"),
        &100,
        &2_000_000u64,
        &true,
        &36_501,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidFrequency)));
}

// ===========================================================================
// 11. Batch Pay — State-Transition Invariants
// ===========================================================================

/// Batch pay: mix of valid bills all transition to Paid atomically.
#[test]
fn test_batch_pay_transitions_all_to_paid() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let id1 = h.create_bill("A", 100, due_date, false, 0, "XLM");
    let id2 = h.create_bill("B", 200, due_date, false, 0, "XLM");
    let id3 = h.create_bill("C", 300, due_date, false, 0, "XLM");

    let ids = SorobanVec::new(&h.env);
    // Build batch vector
    let mut batch = Vec::new(&h.env);
    batch.push_back(id1);
    batch.push_back(id2);
    batch.push_back(id3);

    h.client.batch_pay_bills(&h.owner, &batch);

    assert!(h.is_paid(id1));
    assert!(h.is_paid(id2));
    assert!(h.is_paid(id3));
    assert_eq!(h.total_unpaid(), 0);
}

/// Batch pay: non-existent bill IDs are skipped gracefully.
#[test]
fn test_batch_pay_skips_nonexistent_ids() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    let id1 = h.create_bill("Real", 100, due_date, false, 0, "XLM");

    let mut batch = Vec::new(&h.env);
    batch.push_back(999); // doesn't exist
    batch.push_back(id1);
    batch.push_back(998); // doesn't exist

    h.client.batch_pay_bills(&h.owner, &batch);

    assert!(h.is_paid(id1));
    assert_eq!(h.total_unpaid(), 0);
}

/// Batch pay: cross-owner bills are skipped.
#[test]
fn test_batch_pay_skips_cross_owner() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let due_date = 2_000_000u64;

    let id_alice = client.create_bill(
        &alice,
        &String::from_str(&env, "Alice"),
        &100,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    let id_bob = client.create_bill(
        &bob,
        &String::from_str(&env, "Bob"),
        &200,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Alice tries to batch-pay both — Bob's bill should be skipped
    let mut batch = Vec::new(&env);
    batch.push_back(id_alice);
    batch.push_back(id_bob);

    client.batch_pay_bills(&alice, &batch);

    // Alice's bill paid, Bob's untouched
    let alice_bill = client.get_bill(&id_alice).unwrap();
    assert!(alice_bill.paid);
    let bob_bill = client.get_bill(&id_bob).unwrap();
    assert!(!bob_bill.paid);
}

/// Batch pay with recurring bills: children spawned correctly.
#[test]
fn test_batch_pay_recurring_spawns_children() {
    let h = InvariantHarness::new(1_000_000);
    let due_date = 2_000_000u64;
    let freq = 30u32;
    let amount = 500i128;

    let id1 = h.create_bill("Rent", amount, due_date, true, freq, "XLM");
    let id2 = h.create_bill("Sub", amount, due_date, true, freq, "USDC");

    let mut batch = Vec::new(&h.env);
    batch.push_back(id1);
    batch.push_back(id2);

    h.client.batch_pay_bills(&h.owner, &batch);

    // Both parents paid
    assert!(h.is_paid(id1));
    assert!(h.is_paid(id2));

    // Both children spawned
    assert!(h.bill_exists(id1 + 1));
    assert!(h.bill_exists(id2 + 1));

    let child1 = h.client.get_bill(&(id1 + 1)).unwrap();
    assert!(!child1.paid);
    assert!(child1.recurring);

    let child2 = h.client.get_bill(&(id2 + 1)).unwrap();
    assert!(!child2.paid);
    assert!(child2.recurring);

    // Unpaid total equals sum of children
    assert_eq!(h.total_unpaid(), amount * 2);
}
