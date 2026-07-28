//! Settlement Window Guard Tests for `bill_payments`.
//!
//! Locked-in Contract Boundaries:
//! 1. **Creation Guard (`create_bill`)**:
//!    - Inside window (`due_date > now`): Accepted (future due date).
//!    - Exact boundary (`due_date == now`): Accepted (strict `<` comparison permits `due_date == now`).
//!    - Outside window (`due_date < now`): Rejected (`InvalidDueDate (12)`).
//!    - Zero timestamp (`due_date == 0`): Rejected (`InvalidDueDate (12)`).
//!
//! 2. **Overdue Status Guard (`get_overdue_bills` & `get_overdue_bills_by_owner`)**:
//!    - Inside window (`now < due_date`): Excluded (on-time).
//!    - Exact boundary (`now == due_date`): Excluded (strict `<` filter: `due_date == now` is not overdue).
//!    - Outside window (`now > due_date`): Included (overdue).
//!
//! 3. **Recurring Window Advancement Guard (`pay_bill`)**:
//!    - On-time payment: Child due date advances by `frequency_days * 86_400`.
//!    - Delayed payment: Catch-up loop repeatedly advances until `child.due_date > current_time`.
//!    - Exact boundary payment (`now == parent.due_date + period`): Catch-up loop advances child to `now + period`.

use bill_payments::{BillPayments, BillPaymentsClient, Error};
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

const BASE_TIME: u64 = 1_000_000;
const SECONDS_PER_DAY: u64 = 86_400;

fn make_env(timestamp: u64) -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    set_time(&env, timestamp);
    env.budget().reset_unlimited();
    env
}

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
        max_entry_ttl: 3_000_000,
    });
}

fn setup_contract(env: &Env) -> BillPaymentsClient<'_> {
    let id = env.register_contract(None, BillPayments);
    BillPaymentsClient::new(env, &id)
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit Tests — Assertive Names for Window Boundaries
// ─────────────────────────────────────────────────────────────────────────────

/// Creation with `due_date > now` (inside window) succeeds.
#[test]
fn returns_ok_when_creation_due_date_is_strictly_in_future() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let due_date = BASE_TIME + 10_000;
    let res = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Future Invoice"),
        &1_000i128,
        &due_date,
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    assert!(res.is_ok(), "creation inside future window must succeed");
}

/// Creation with `due_date == now` (exact boundary) succeeds because comparison is strict `<`.
#[test]
fn returns_ok_when_creation_due_date_equals_current_ledger_time() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let res = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Exact Boundary Invoice"),
        &1_000i128,
        &BASE_TIME,
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    assert!(res.is_ok(), "creation at exact ledger time boundary must succeed");
}

/// Creation with `due_date < now` (outside window) returns InvalidDueDate.
#[test]
fn returns_err_invalid_due_date_when_creation_due_date_is_in_past() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let res = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Past Invoice"),
        &1_000i128,
        &(BASE_TIME - 1),
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    assert_eq!(
        res,
        Err(Ok(Error::InvalidDueDate)),
        "creation with due_date strictly in past must return InvalidDueDate"
    );
}

/// Creation with `due_date == 0` returns InvalidDueDate.
#[test]
fn returns_err_invalid_due_date_when_creation_due_date_is_zero() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let res = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Zero Invoice"),
        &1_000i128,
        &0u64,
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    assert_eq!(
        res,
        Err(Ok(Error::InvalidDueDate)),
        "creation with zero due_date must return InvalidDueDate"
    );
}

/// Overdue query: `due_date > now` (inside window) is excluded from overdue list.
#[test]
fn returns_zero_overdue_count_when_due_date_is_in_future() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    client.create_bill(
        &owner,
        &String::from_str(&env, "Future Bill"),
        &500i128,
        &(BASE_TIME + 1_000),
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let page = client.get_overdue_bills(&0, &10);
    assert_eq!(page.count, 0, "future bill must not be marked overdue");
}

/// Overdue query: `due_date == now` (exact boundary) is excluded from overdue list.
#[test]
fn returns_zero_overdue_count_when_due_date_equals_current_time() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    client.create_bill(
        &owner,
        &String::from_str(&env, "Boundary Bill"),
        &500i128,
        &BASE_TIME,
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let page = client.get_overdue_bills(&0, &10);
    assert_eq!(
        page.count, 0,
        "due_date == now is on-time (strict < comparison) and must not be marked overdue"
    );
}

/// Overdue query: `due_date < now` (outside window) is included in overdue list.
#[test]
fn returns_overdue_bill_when_ledger_advances_past_due_date() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Overdue Bill"),
        &500i128,
        &BASE_TIME,
        &false,
        &0u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Advance ledger time past due_date
    set_time(&env, BASE_TIME + 1);

    let page = client.get_overdue_bills(&0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().id, bill_id);
}

/// Recurring settlement window advancement: delayed payment catch-up loop guarantees child is born in future.
#[test]
fn advances_recurring_child_due_date_strictly_into_future_on_late_settlement() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    // Create recurring bill due at BASE_TIME + 1 day
    let initial_due = BASE_TIME + SECONDS_PER_DAY;
    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Recurring Bill"),
        &1_000i128,
        &initial_due,
        &true,
        &1u32, // 1 day frequency
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Extremely late payment: advance time by 10 days past initial_due
    let late_payment_time = initial_due + (10 * SECONDS_PER_DAY);
    set_time(&env, late_payment_time);

    // Pay late bill
    client.pay_bill(&owner, &bill_id);

    // Catch-up loop must advance child due date until child.due_date > late_payment_time
    let child_bill = client.get_bill(&2).unwrap();
    assert!(
        child_bill.due_date > late_payment_time,
        "child bill due_date {} must be strictly greater than payment time {}",
        child_bill.due_date,
        late_payment_time
    );
    assert!(!child_bill.paid);
}

/// Recurring exact boundary payment (`now == parent.due_date + period`): catch-up loop advances child to `now + period`.
#[test]
fn advances_recurring_child_due_date_when_settled_at_exact_next_due_boundary() {
    let env = make_env(BASE_TIME);
    let client = setup_contract(&env);
    let owner = Address::generate(&env);

    let period = 30 * SECONDS_PER_DAY;
    let initial_due = BASE_TIME + period;
    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Boundary Recurring"),
        &2_000i128,
        &initial_due,
        &true,
        &30u32,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Payment occurs exactly at initial_due + period (the first expected next_due_date)
    let exact_next_due = initial_due + period;
    set_time(&env, exact_next_due);

    client.pay_bill(&owner, &bill_id);

    let child_bill = client.get_bill(&2).unwrap();
    // Catch-up loop `while next_due_date <= current_time` triggers and advances by another period
    assert_eq!(child_bill.due_date, exact_next_due + period);
    assert!(child_bill.due_date > exact_next_due);
}

// ─────────────────────────────────────────────────────────────────────────────
// Property Test — Pin Settlement Window Creation Boundary Contract
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Property: `create_bill` accepts `due_date` IF AND ONLY IF `due_date >= now && due_date != 0`.
    #[test]
    fn proptest_settlement_window_creation_boundary(
        now in 1_000_000u64..10_000_000u64,
        due_offset in -5_000i64..5_000i64,
    ) {
        let env = make_env(now);
        let client = setup_contract(&env);
        let owner = Address::generate(&env);

        let due_date = if due_offset < 0 {
            now.saturating_sub(due_offset.unsigned_abs())
        } else {
            now.saturating_add(due_offset as u64)
        };

        let res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Prop Bill"),
            &100i128,
            &due_date,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        if due_date == 0 || due_date < now {
            prop_assert_eq!(res, Err(Ok(Error::InvalidDueDate)));
        } else {
            prop_assert!(res.is_ok());
        }
    }
}
