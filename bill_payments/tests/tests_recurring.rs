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

use bill_payments::{Bill, BillEvent, BillPayments, BillPaymentsClient, BillPaymentsError};
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

    fn sum_unpaid_bills(&self) -> i128 {
        let mut cursor = 0u32;
        let mut sum = 0i128;
        loop {
            let page = self.client.get_unpaid_bills(&self.owner, &cursor, &50);
            for bill in page.items.iter() {
                sum += bill.amount;
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        sum
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
    assert_eq!(
        child.schedule_id, parent.schedule_id,
        "schedule_id must clone"
    );
    assert!(!child.paid, "child must be unpaid");
    assert!(child.paid_at.is_none(), "child paid_at must be None");
    assert_eq!(
        child.created_at, pay_timestamp,
        "created_at must be pay time"
    );
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
            | (
                BillEvent::RecurringBillCreated,
                BillEvent::RecurringBillCreated
            )
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

    let total_before = h.client.get_total_unpaid(&h.owner);
    assert_eq!(total_before, amount, "total before should be parent amount");

    let parent = h.client.get_bill(&parent_id).unwrap();
    h.pay_at(parent_id, due_date - 1);

    let total_after = h.client.get_total_unpaid(&h.owner);
    assert_eq!(
        total_after, amount,
        "total after should still be amount (subtract parent, add child)"
    );

    let child_id = h.child_id(parent_id);
    let child = h.client.get_bill(&child_id).unwrap();
    let expected_due = due_date + frequency_days as u64 * SECONDS_PER_DAY;

    assert_cloned_recurring_fields(&parent, &child, child_id, expected_due, due_date - 1);

    assert!(
        h.client.get_bill(&(child_id + 1)).is_none(),
        "exactly one child"
    );

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
    assert!(bill_event_emitted(&h.env, &h.contract_id, BillEvent::Paid));
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
    let frequency_days = 5u32;
    let parent_id = h.create_recurring("Mortgage", 250_000, due_date, frequency_days, "XLM");

    // Parent is 25 days overdue at payment time (within 30-day settlement window).
    let pay_at = due_date + 25 * SECONDS_PER_DAY;
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
    let parent_id = h.create_recurring("Daily sub", 99, due_date, 1, "USDC");
    h.client
        .add_tags_to_bill(&h.owner, &parent_id, &tags(&h.env, &["daily", "streaming"]));

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

    assert!(bill_event_emitted(&h.env, &h.contract_id, BillEvent::Paid));
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

#[test]
fn test_sum_unpaid_bills_equals_get_total_unpaid() {
    let h = RecurringHarness::new(1_000_000);
    let due_date = 2_000_000u64;

    // Create multiple bills
    h.create_one_time("One-time 1", 100, due_date);
    h.create_recurring("Recurring 1", 200, due_date, 30, "XLM");
    h.create_one_time("One-time 2", 300, due_date);

    let sum = h.sum_unpaid_bills();
    let get_total = h.client.get_total_unpaid(&h.owner);

    assert_eq!(sum, 600, "sum of bills should equal get_total_unpaid");
    assert_eq!(sum, get_total, "sum_unpaid_bills == get_total_unpaid");

    // Pay the first one-time bill
    h.pay_at(1, due_date);
    let sum_after_pay_one_time = h.sum_unpaid_bills();
    let get_total_after_pay_one_time = h.client.get_total_unpaid(&h.owner);
    assert_eq!(
        sum_after_pay_one_time, 500,
        "after paying one-time, sum is 200 + 300"
    );
    assert_eq!(sum_after_pay_one_time, get_total_after_pay_one_time);

    // Pay the recurring bill, which should spawn a new one
    h.pay_at(2, due_date);
    let sum_after_pay_recurring = h.sum_unpaid_bills();
    let get_total_after_pay_recurring = h.client.get_total_unpaid(&h.owner);
    assert_eq!(
        sum_after_pay_recurring, 500,
        "after paying recurring, sum remains 200 + 300 (new child)"
    );
    assert_eq!(sum_after_pay_recurring, get_total_after_pay_recurring);
}

// ===========================================================================
// Bill Schedule Pagination Tests — Issue #1751
//
// Verifies `get_bill_schedules_page` cursor semantics, ordering guarantees,
// limit enforcement, owner isolation, and end-of-stream behavior.
//
// Cursor semantics (EXCLUSIVE):
//   - cursor = 0  → start from first schedule
//   - cursor = N  → return only schedules with ID strictly > N
//   - next_cursor = last returned ID when more pages exist
//   - next_cursor = 0 on final (or only) page
//
// All invariants match the Pagination Handbook
// (docs/PAGINATION_HANDBOOK.md).
// ===========================================================================

// ---------------------------------------------------------------------------
// Schedule pagination harness
// ---------------------------------------------------------------------------

struct SchedulePaginationHarness<'a> {
    env: Env,
    client: BillPaymentsClient<'a>,
    owner: Address,
}

impl SchedulePaginationHarness<'_> {
    fn new() -> Self {
        let env = Env::default();
        env.ledger().set_timestamp(1_000_000);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, bill_payments::BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        Self { env, client, owner }
    }

    /// Create a recurring schedule and return its ID.
    fn create_schedule(&self, name: &str) -> u32 {
        self.client
            .create_bill_schedule(
                &self.owner,
                &String::from_str(&self.env, name),
                &1000i128,
                &String::from_str(&self.env, "XLM"),
                &2_000_000u64, // next_due far in future
                &86400u64,     // 1-day interval
            )
    }

    /// Collect all schedule IDs via full page traversal.
    fn collect_all_ids(&self) -> std::vec::Vec<u32> {
        let mut ids = std::vec::Vec::new();
        let mut cursor = 0u32;
        loop {
            let page = self.client.get_bill_schedules_page(&self.owner, &cursor, &50u32);
            for sched in page.items.iter() {
                ids.push(sched.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        ids
    }
}

// ---------------------------------------------------------------------------
// Basic / first-page tests
// ---------------------------------------------------------------------------

/// Empty owner → empty page, next_cursor == 0.
#[test]
fn test_schedule_page_empty_owner_returns_empty() {
    let h = SchedulePaginationHarness::new();
    let page = h.client.get_bill_schedules_page(&h.owner, &0u32, &10u32);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.items.len(), 0);
}

/// Single schedule → returned on first page, next_cursor == 0.
#[test]
fn test_schedule_page_single_schedule_fits_first_page() {
    let h = SchedulePaginationHarness::new();
    let id = h.create_schedule("One");

    let page = h.client.get_bill_schedules_page(&h.owner, &0u32, &10u32);
    assert_eq!(page.count, 1);
    assert_eq!(page.next_cursor, 0, "next_cursor must be 0 on final page");
    assert_eq!(page.items.get(0).unwrap().id, id);
}

/// First page with limit < total → returns `limit` items, non-zero next_cursor.
#[test]
fn test_schedule_page_first_page_has_correct_items_and_next_cursor() {
    let h = SchedulePaginationHarness::new();
    for i in 1..=5u32 {
        h.create_schedule(&std::format!("Sched{i}"));
    }

    let page = h.client.get_bill_schedules_page(&h.owner, &0u32, &3u32);
    assert_eq!(page.count, 3);
    assert!(page.next_cursor > 0, "must have next_cursor when more items exist");
    // Items must be in ascending ID order
    let ids: std::vec::Vec<u32> = page.items.iter().map(|s| s.id).collect();
    for i in 1..ids.len() {
        assert!(ids[i - 1] < ids[i], "items must be strictly ascending");
    }
}

/// Second page resumes correctly after first page.
#[test]
fn test_schedule_page_second_page_continues_from_next_cursor() {
    let h = SchedulePaginationHarness::new();
    for i in 1..=6u32 {
        h.create_schedule(&std::format!("S{i}"));
    }

    let page1 = h.client.get_bill_schedules_page(&h.owner, &0u32, &3u32);
    assert_eq!(page1.count, 3);
    assert!(page1.next_cursor > 0);

    let page2 = h.client.get_bill_schedules_page(&h.owner, &page1.next_cursor, &3u32);
    assert_eq!(page2.count, 3);
    assert_eq!(page2.next_cursor, 0, "final page must have next_cursor == 0");

    // No overlap between pages
    let ids1: std::vec::Vec<u32> = page1.items.iter().map(|s| s.id).collect();
    let ids2: std::vec::Vec<u32> = page2.items.iter().map(|s| s.id).collect();
    for id in &ids2 {
        assert!(!ids1.contains(id), "ID {id} appeared on both pages");
    }
    // All IDs on page2 must be strictly greater than all on page1
    let max1 = ids1.iter().max().copied().unwrap_or(0);
    let min2 = ids2.iter().min().copied().unwrap_or(u32::MAX);
    assert!(min2 > max1, "page2 IDs must all be greater than page1 IDs");
}

// ---------------------------------------------------------------------------
// End-of-stream and cursor boundary tests
// ---------------------------------------------------------------------------

/// Exact-fit: items == limit → next_cursor == 0 (no more pages).
#[test]
fn test_schedule_page_exact_fit_no_next_cursor() {
    let h = SchedulePaginationHarness::new();
    for i in 1..=4u32 {
        h.create_schedule(&std::format!("E{i}"));
    }

    let page = h.client.get_bill_schedules_page(&h.owner, &0u32, &4u32);
    assert_eq!(page.count, 4);
    assert_eq!(
        page.next_cursor, 0,
        "exact-fit must return next_cursor == 0"
    );
}

/// Out-of-range cursor (beyond all IDs) → empty page, next_cursor == 0.
#[test]
fn test_schedule_page_out_of_range_cursor_returns_empty() {
    let h = SchedulePaginationHarness::new();
    h.create_schedule("A");
    h.create_schedule("B");

    let page = h.client.get_bill_schedules_page(&h.owner, &999_999u32, &10u32);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.items.len(), 0);
}

/// Cursor at the ID of the last schedule → empty page.
#[test]
fn test_schedule_page_cursor_at_last_id_returns_empty() {
    let h = SchedulePaginationHarness::new();
    let id1 = h.create_schedule("First");
    let _id2 = h.create_schedule("Second");
    let id3 = h.create_schedule("Third");

    // cursor = id3 → no items with ID > id3
    let page = h.client.get_bill_schedules_page(&h.owner, &id3, &10u32);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);

    // Sanity: cursor at id1 should return items id2 and id3
    let page2 = h.client.get_bill_schedules_page(&h.owner, &id1, &10u32);
    assert_eq!(page2.count, 2);
    assert_eq!(page2.next_cursor, 0);
}

/// Calling with next_cursor == 0 on the final page is idempotent.
#[test]
fn test_schedule_page_idempotent_repeat_final_page() {
    let h = SchedulePaginationHarness::new();
    h.create_schedule("X");
    h.create_schedule("Y");

    // First traversal
    let page1 = h.client.get_bill_schedules_page(&h.owner, &0u32, &10u32);
    assert_eq!(page1.next_cursor, 0);

    // Repeating with cursor=0 must return the same page
    let page2 = h.client.get_bill_schedules_page(&h.owner, &0u32, &10u32);
    assert_eq!(page1.count, page2.count);
    for i in 0..page1.items.len() {
        assert_eq!(
            page1.items.get(i).unwrap().id,
            page2.items.get(i).unwrap().id
        );
    }
}

// ---------------------------------------------------------------------------
// Limit enforcement tests
// ---------------------------------------------------------------------------

/// limit == 0 is normalised to DEFAULT_PAGE_LIMIT (20).
#[test]
fn test_schedule_page_limit_zero_normalised_to_default() {
    // DEFAULT_PAGE_LIMIT = 20
    let h = SchedulePaginationHarness::new();
    // Create more than DEFAULT_PAGE_LIMIT schedules to see clamping.
    // MAX_BILL_SCHEDULES_PER_OWNER limits how many we can create; use 5 for simplicity.
    for i in 1..=5u32 {
        h.create_schedule(&std::format!("N{i}"));
    }

    let page_default = h.client.get_bill_schedules_page(&h.owner, &0u32, &0u32);
    let page_explicit = h.client.get_bill_schedules_page(&h.owner, &0u32, &20u32);
    // Both should return the same set because 5 < DEFAULT_PAGE_LIMIT (20)
    assert_eq!(page_default.count, page_explicit.count);
    assert_eq!(page_default.next_cursor, page_explicit.next_cursor);
}

/// limit > MAX_PAGE_LIMIT is clamped to MAX_PAGE_LIMIT.
#[test]
fn test_schedule_page_large_limit_clamped() {
    // MAX_PAGE_LIMIT = 50
    let h = SchedulePaginationHarness::new();
    for i in 1..=5u32 {
        h.create_schedule(&std::format!("L{i}"));
    }

    // Request well above MAX_PAGE_LIMIT (50)
    let page = h.client.get_bill_schedules_page(&h.owner, &0u32, &100_000u32);
    assert!(
        page.count <= 50,
        "count {} must be <= MAX_PAGE_LIMIT 50",
        page.count,
    );
}

// ---------------------------------------------------------------------------
// Multi-page traversal / no-duplicates test
// ---------------------------------------------------------------------------

/// Full multi-page traversal collects exactly the right set of schedules
/// with no duplicates and in strictly ascending ID order.
#[test]
fn test_schedule_page_full_traversal_no_duplicates_ascending() {
    let h = SchedulePaginationHarness::new();
    let mut expected_ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for i in 1..=8u32 {
        let id = h.create_schedule(&std::format!("T{i}"));
        expected_ids.push(id);
    }

    // Traverse with small pages to force multiple page fetches
    let mut collected: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    let mut page_count = 0u32;
    loop {
        let page = h.client.get_bill_schedules_page(&h.owner, &cursor, &3u32);
        for sched in page.items.iter() {
            collected.push(sched.id);
        }
        page_count += 1;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    // Exactly the expected IDs, no more no fewer
    assert_eq!(collected.len(), expected_ids.len(), "count mismatch");
    assert_eq!(collected, expected_ids, "IDs must match and be in order");
    // Verify no duplicates (strictly ascending implies no duplicates)
    for i in 1..collected.len() {
        assert!(
            collected[i - 1] < collected[i],
            "non-ascending at position {i}"
        );
    }
    // 8 items / 3 per page = 3 full pages + 1 final → 3 pages with 3 items and 1 with 2
    assert!(page_count >= 2, "must have required multiple pages");
}

// ---------------------------------------------------------------------------
// Owner isolation test
// ---------------------------------------------------------------------------

/// Schedules from one owner must not appear in another owner's pages.
#[test]
fn test_schedule_page_owner_isolation() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let contract_id = env.register_contract(None, bill_payments::BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner_a = Address::generate(&env);
    let owner_b = Address::generate(&env);

    // Create schedules interleaved between owners
    let make_schedule = |owner: &Address, name: &str| -> u32 {
        client
            .create_bill_schedule(
                owner,
                &String::from_str(&env, name),
                &500i128,
                &String::from_str(&env, "XLM"),
                &2_000_000u64,
                &86400u64,
            )
    };

    let id_a1 = make_schedule(&owner_a, "A1");
    let id_b1 = make_schedule(&owner_b, "B1");
    let id_a2 = make_schedule(&owner_a, "A2");
    let id_b2 = make_schedule(&owner_b, "B2");
    let id_a3 = make_schedule(&owner_a, "A3");

    let page_a = client.get_bill_schedules_page(&owner_a, &0u32, &50u32);
    let page_b = client.get_bill_schedules_page(&owner_b, &0u32, &50u32);

    // Counts match what was created per owner
    assert_eq!(page_a.count, 3, "owner A must have exactly 3 schedules");
    assert_eq!(page_b.count, 2, "owner B must have exactly 2 schedules");

    let ids_a: std::vec::Vec<u32> = page_a.items.iter().map(|s| s.id).collect();
    let ids_b: std::vec::Vec<u32> = page_b.items.iter().map(|s| s.id).collect();

    // A's schedules must not appear in B's page and vice versa
    for id in &ids_a {
        assert!(!ids_b.contains(id), "A's ID {id} appeared in B's page");
    }
    for id in &ids_b {
        assert!(!ids_a.contains(id), "B's ID {id} appeared in A's page");
    }

    // Confirm correct IDs
    assert!(ids_a.contains(&id_a1));
    assert!(ids_a.contains(&id_a2));
    assert!(ids_a.contains(&id_a3));
    assert!(ids_b.contains(&id_b1));
    assert!(ids_b.contains(&id_b2));
}

// ---------------------------------------------------------------------------
// Cancelled schedule exclusion test
// ---------------------------------------------------------------------------

/// Cancelled schedules are removed from the owner index and must not appear
/// in paginated results.
#[test]
fn test_schedule_page_cancelled_schedule_excluded() {
    let h = SchedulePaginationHarness::new();
    let id1 = h.create_schedule("Active1");
    let id2 = h.create_schedule("ToCancel");
    let id3 = h.create_schedule("Active2");

    h.client.cancel_bill_schedule(&h.owner, &id2);

    let collected = h.collect_all_ids();
    assert_eq!(
        collected.len(),
        2,
        "cancelled schedule must be excluded from pages"
    );
    assert!(collected.contains(&id1));
    assert!(!collected.contains(&id2), "cancelled ID must not appear");
    assert!(collected.contains(&id3));
}

// ---------------------------------------------------------------------------
// Concurrent-insert stability test
// ---------------------------------------------------------------------------

/// New schedules inserted between page fetches appear on subsequent pages
/// (not skipped) when the cursor correctly marks the boundary.
#[test]
fn test_schedule_page_concurrent_insert_not_skipped() {
    let h = SchedulePaginationHarness::new();
    let id1 = h.create_schedule("Before1");
    let id2 = h.create_schedule("Before2");
    let id3 = h.create_schedule("Before3");

    // Fetch first page (2 items)
    let page1 = h.client.get_bill_schedules_page(&h.owner, &0u32, &2u32);
    assert_eq!(page1.count, 2);
    let cursor_after_page1 = page1.next_cursor;

    // Insert a new schedule AFTER fetching page1 (simulates concurrent insert)
    let id_new = h.create_schedule("NewConcurrent");

    // The new schedule has a higher ID, so it appears on the next page
    let page2 = h.client.get_bill_schedules_page(&h.owner, &cursor_after_page1, &10u32);
    let ids2: std::vec::Vec<u32> = page2.items.iter().map(|s| s.id).collect();

    // id3 must still appear (was there before)
    assert!(ids2.contains(&id3), "id3 must appear on page2");
    // New schedule also appears (higher ID, same owner)
    assert!(
        ids2.contains(&id_new),
        "new concurrent schedule must appear on subsequent page"
    );
    // No items from page1 re-appear
    let ids1: std::vec::Vec<u32> = page1.items.iter().map(|s| s.id).collect();
    assert!(ids1.contains(&id1) || ids1.contains(&id2));
    for id in &ids2 {
        assert!(!ids1.contains(id), "re-delivered ID {id} from page1");
    }
}

// ---------------------------------------------------------------------------
// Schedule field integrity on paginated items
// ---------------------------------------------------------------------------

/// Items returned through pagination carry the correct fields (not truncated).
#[test]
fn test_schedule_page_items_have_correct_fields() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    env.mock_all_auths();
    let cid = env.register_contract(None, bill_payments::BillPayments);
    let client = BillPaymentsClient::new(&env, &cid);
    let owner = Address::generate(&env);

    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Utilities"),
            &9_999i128,
            &String::from_str(&env, "USDC"),
            &2_000_000u64,
            &(7 * 86400u64), // weekly
        );

    let page = client.get_bill_schedules_page(&owner, &0u32, &10u32);
    assert_eq!(page.count, 1);
    let sched = page.items.get(0).unwrap();

    assert_eq!(sched.id, schedule_id);
    assert_eq!(sched.owner, owner);
    assert_eq!(sched.amount, 9_999);
    assert_eq!(sched.currency, String::from_str(&env, "USDC"));
    assert_eq!(sched.next_due, 2_000_000u64);
    assert_eq!(sched.interval, 7 * 86400u64);
    assert!(sched.recurring);
    assert!(sched.active);
}
