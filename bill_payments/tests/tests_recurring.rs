//! Recurring child-bill generation tests for `pay_bill`.
//!
//! ## Cloning policy (`bill_payments/src/lib.rs`)
//!
//! When a recurring bill (`recurring == true`, valid `frequency_days`) is paid, exactly one
//! child bill is spawned with:
//!
//! - **Cloned:** `owner`, `name`, `amount`, `currency`, `tags`, `recurring` (`true`),
//!   `frequency_days`, `schedule_id`
//! - **Fresh:** `id` (`NEXT_ID + 1`), `paid == false`, `created_at == pay timestamp`,
//!   `paid_at == None`, `external_ref == None` (avoids uniqueness conflicts)
//!
//! ## Due-date advancement policy
//!
//! ```text
//! period = frequency_days * 86_400
//! next_due_date = parent.due_date + period
//! while next_due_date <= current_time {
//!     next_due_date += period
//! }
//! ```
//!
//! The base is **`parent.due_date`**, not `paid_at`. The catch-up loop guarantees the child
//! is never born with `due_date <= current_time`, so it cannot appear in `get_overdue_bills`
//! immediately after generation.
//!
//! Non-recurring bills spawn **no** child on payment.

#![cfg(test)]

use bill_payments::{
    Bill, BillEvent, BillPayments, BillPaymentsClient, BillPaymentsError,
};
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::{Address, Env, IntoVal, String, TryFromVal, Val, Vec as SorobanVec};

const SECONDS_PER_DAY: u64 = 86_400;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct RecurringHarness<'a> {
    env: Env,
    client: BillPaymentsClient<'a>,
    owner: Address,
    contract_id: Address,
}

impl RecurringHarness<'_> {
    fn new(timestamp: u64) -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(timestamp);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        Self {
            env,
            client,
            owner,
            contract_id,
        }
    }

    fn create_recurring(
        &self,
        name: &str,
        amount: i128,
        due_date: u64,
        frequency_days: u32,
        currency: &str,
    ) -> u32 {
        self.client.create_bill(
            &self.owner,
            &String::from_str(&self.env, name),
            &amount,
            &due_date,
            &true,
            &frequency_days,
            &None,
            &String::from_str(&self.env, currency),
            &None,
        )
    }

    fn create_one_time(&self, name: &str, amount: i128, due_date: u64) -> u32 {
        self.client.create_bill(
            &self.owner,
            &String::from_str(&self.env, name),
            &amount,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&self.env, "XLM"),
            &None,
        )
    }

    fn pay_at(&self, bill_id: u32, timestamp: u64) {
        self.env.ledger().set_timestamp(timestamp);
        self.client.pay_bill(&self.owner, &bill_id);
    }

    fn child_id(&self, parent_id: u32) -> u32 {
        parent_id + 1
    }
}

fn tags(env: &Env, values: &[&str]) -> SorobanVec<String> {
    let mut v = SorobanVec::new(env);
    for value in values {
        v.push_back(String::from_str(env, value));
    }
    v
}

fn assert_cloned_recurring_fields(
    parent: &Bill,
    child: &Bill,
    expected_child_id: u32,
    expected_due_date: u64,
    pay_timestamp: u64,
) {
    assert_eq!(child.id, expected_child_id, "child must get a fresh id");
    assert_eq!(child.owner, parent.owner, "owner must clone");
    assert_eq!(child.name, parent.name, "name must clone");
    assert_eq!(child.amount, parent.amount, "amount must clone");
    assert_eq!(child.currency, parent.currency, "currency must clone");
    assert!(child.recurring, "recurring flag must stay true");
    assert_eq!(
        child.frequency_days, parent.frequency_days,
        "frequency_days must clone"
    );
    assert_eq!(child.tags, parent.tags, "tags must clone");
    assert_eq!(child.schedule_id, parent.schedule_id, "schedule_id must clone");
    assert!(!child.paid, "child must be unpaid");
    assert!(child.paid_at.is_none(), "child paid_at must be None");
    assert_eq!(child.created_at, pay_timestamp, "created_at must be pay time");
    assert!(
        child.external_ref.is_none(),
        "external_ref must not clone (uniqueness policy)"
    );
    assert_eq!(
        child.due_date, expected_due_date,
        "due_date must follow frequency_days advancement policy"
    );
    assert!(
        child.due_date > pay_timestamp,
        "child must not be born overdue (due_date > pay timestamp)"
    );
}

fn bill_event_matches(env: &Env, val: &Val, expected: &BillEvent) -> bool {
    let Ok(decoded) = BillEvent::try_from_val(env, val) else {
        return false;
    };
    matches!(
        (&decoded, expected),
        (BillEvent::Paid, BillEvent::Paid)
            | (BillEvent::RecurringBillCreated, BillEvent::RecurringBillCreated)
            | (BillEvent::ScheduleExecuted, BillEvent::ScheduleExecuted)
    )
}

fn bill_event_emitted(env: &Env, contract_id: &Address, expected: BillEvent) -> bool {
    for (cid, topics, _data) in env.events().all() {
        if cid != *contract_id {
            continue;
        }
        if topics.len() < 2 {
            continue;
        }
        if bill_event_matches(env, &topics.get(1).unwrap(), &expected) {
            return true;
        }
    }
    false
}

fn count_contract_bill_events(env: &Env, contract_id: &Address) -> u32 {
    let mut count = 0u32;
    for (cid, topics, _data) in env.events().all() {
        if cid != *contract_id || topics.len() < 2 {
            continue;
        }
        if BillEvent::try_from_val(env, &topics.get(1).unwrap()).is_ok() {
            count += 1;
        }
    }
    count
}

fn child_in_overdue_list(client: &BillPaymentsClient, child_id: u32) -> bool {
    let page = client.get_overdue_bills(&0, &100);
    page.items.iter().any(|bill| bill.id == child_id)
}

// ---------------------------------------------------------------------------
// Field cloning and spawn count
// ---------------------------------------------------------------------------

#[test]
fn test_recurring_pay_spawns_one_child_with_all_cloned_fields() {
    let h = RecurringHarness::new(100_000);
    let due_date = 500_000u64;
    let frequency_days = 30u32;
    let amount = 12_345i128;

    let parent_id = h.create_recurring("Utilities", amount, due_date, frequency_days, "USDC");
    h.client.add_tags_to_bill(
        &h.owner,
        &parent_id,
        &tags(&h.env, &["monthly", "essential"]),
    );

    let parent = h.client.get_bill(&parent_id).unwrap();
    h.pay_at(parent_id, due_date - 1);

    let child_id = h.child_id(parent_id);
    let child = h.client.get_bill(&child_id).unwrap();
    let expected_due = due_date + frequency_days as u64 * SECONDS_PER_DAY;

    assert_cloned_recurring_fields(
        &parent,
        &child,
        child_id,
        expected_due,
        due_date - 1,
    );

    assert!(h.client.get_bill(&(child_id + 1)).is_none(), "exactly one child");

    let unpaid = h.client.get_unpaid_bills(&h.owner, &0, &10);
    assert_eq!(unpaid.count, 1, "only the spawned child remains unpaid");
    assert_eq!(unpaid.items.get(0).unwrap().id, child_id);
}

#[test]
fn test_non_recurring_pay_spawns_no_child() {
    let h = RecurringHarness::new(200_000);
    let due_date = 400_000u64;

    let bill_id = h.create_one_time("One-off", 500, due_date);
    let events_before = count_contract_bill_events(&h.env, &h.contract_id);

    h.pay_at(bill_id, due_date);

    assert!(h.client.get_bill(&(bill_id + 1)).is_none());
    assert_eq!(
        count_contract_bill_events(&h.env, &h.contract_id),
        events_before + 1,
        "only BillEvent::Paid must be emitted for non-recurring pay"
    );
    assert!(bill_event_emitted(
        &h.env,
        &h.contract_id,
        BillEvent::Paid
    ));
    assert!(!bill_event_emitted(
        &h.env,
        &h.contract_id,
        BillEvent::RecurringBillCreated
    ));

    let unpaid = h.client.get_unpaid_bills(&h.owner, &0, &10);
    assert_eq!(unpaid.count, 0, "no unpaid bills after one-time payment");
}

// ---------------------------------------------------------------------------
// Due-date advancement and overdue safety
// ---------------------------------------------------------------------------

#[test]
fn test_recurring_long_overdue_child_due_date_not_in_past() {
    let h = RecurringHarness::new(0);
    let due_date = 1_000_000u64;
    let frequency_days = 30u32;
    let parent_id = h.create_recurring("Mortgage", 250_000, due_date, frequency_days, "XLM");

    // Parent is ~4 months overdue at payment time.
    let pay_at = due_date + 120 * SECONDS_PER_DAY;
    h.pay_at(parent_id, pay_at);

    let child_id = h.child_id(parent_id);
    let child = h.client.get_bill(&child_id).unwrap();

    assert!(
        child.due_date > pay_at,
        "catch-up must advance child beyond current ledger time; got {} vs pay_at {}",
        child.due_date,
        pay_at
    );
    assert!(
        !child_in_overdue_list(&h.client, child_id),
        "newly spawned child must not appear in get_overdue_bills"
    );

    let period = frequency_days as u64 * SECONDS_PER_DAY;
    let mut expected = due_date + period;
    while expected <= pay_at {
        expected += period;
    }
    assert_eq!(child.due_date, expected);
}

#[test]
fn test_recurring_frequency_one_day_tags_preserved() {
    let h = RecurringHarness::new(0);
    let due_date = 2_000_000u64;
    let parent_id = h.create_recurring("Daily sub", 99, due_date, 1, "NGN");
    h.client.add_tags_to_bill(&h.owner, &parent_id, &tags(&h.env, &["daily", "streaming"]));

    // Pay one second before due date so child lands at due_date + 1 day without catch-up.
    h.pay_at(parent_id, due_date - 1);

    let parent = h.client.get_bill(&parent_id).unwrap();
    let child = h.client.get_bill(&h.child_id(parent_id)).unwrap();

    assert_cloned_recurring_fields(
        &parent,
        &child,
        h.child_id(parent_id),
        due_date + SECONDS_PER_DAY,
        due_date - 1,
    );
    assert_eq!(child.tags.len(), 2);
}

// ---------------------------------------------------------------------------
// Events and InvalidDueDate boundaries
// ---------------------------------------------------------------------------

#[test]
fn test_recurring_pay_emits_paid_and_recurring_bill_created_events() {
    let h = RecurringHarness::new(0);
    let due_date = 900_000u64;
    let parent_id = h.create_recurring("Subscription", 1_000, due_date, 7, "XLM");

    h.pay_at(parent_id, due_date);

    assert!(bill_event_emitted(
        &h.env,
        &h.contract_id,
        BillEvent::Paid
    ));
    assert!(bill_event_emitted(
        &h.env,
        &h.contract_id,
        BillEvent::RecurringBillCreated
    ));
    assert!(!bill_event_emitted(
        &h.env,
        &h.contract_id,
        BillEvent::ScheduleExecuted
    ));
}

#[test]
fn test_bill_event_schedule_executed_variant_serializes() {
    let env = Env::default();
    let variant = BillEvent::ScheduleExecuted;
    let val: Val = variant.into_val(&env);
    BillEvent::try_from_val(&env, &val).expect("ScheduleExecuted must round-trip on wire");
}

#[test]
fn test_create_bill_invalid_due_date_boundaries() {
    let h = RecurringHarness::new(1_000_000);
    let owner = h.owner.clone();

    let ok_future = h.client.try_create_bill(
        &owner,
        &String::from_str(&h.env, "Future"),
        &100,
        &(1_000_001),
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert!(ok_future.is_ok(), "due_date > now must be accepted");

    let ok_now = h.client.try_create_bill(
        &owner,
        &String::from_str(&h.env, "Now"),
        &100,
        &1_000_000,
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert!(ok_now.is_ok(), "due_date == now must be accepted");

    let past = h.client.try_create_bill(
        &owner,
        &String::from_str(&h.env, "Past"),
        &100,
        &(1_000_000 - 1),
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(past, Err(Ok(BillPaymentsError::InvalidDueDate)));

    let zero = h.client.try_create_bill(
        &owner,
        &String::from_str(&h.env, "Zero"),
        &100,
        &0u64,
        &false,
        &0,
        &None,
        &String::from_str(&h.env, "XLM"),
        &None,
    );
    assert_eq!(zero, Err(Ok(BillPaymentsError::InvalidDueDate)));
}
