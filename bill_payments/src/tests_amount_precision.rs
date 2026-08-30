#![cfg(test)]

extern crate std;

//! Regression coverage for Issue #1737 — bill scheduling and execution:
//! amount precision and overflow.
//!
//! The invariant under test (enforced at every boundary by the shared
//! `remitwise_common::amount` rules and checked arithmetic in
//! `execute_due_bill_schedules` / `batch_pay_bills` / `pay_bill`):
//!
//! 1. Amounts are exact integers in `[MIN_AMOUNT, MAX_AMOUNT]` — zero,
//!    negative, and oversized values are rejected **before any state change**.
//! 2. Arithmetic (next_due advancement, id counters, unpaid totals) is
//!    **checked** — overflow is rejected deterministically (typed error or
//!    revert) instead of silently saturating/truncating.
//! 3. Rejected, stale, and repeated operations leave **no partial state**.
//! 4. The unpaid-total cache always equals an independent recomputation of
//!    the sum of unpaid bill amounts (the "oracle" below).
//!
//! Values deliberately covered: zero, minimum (1), maximum (MAX_AMOUNT),
//! near-overflow (MAX_AMOUNT + 1, i128::MAX), fractional-scale amounts (any
//! i128 digit pattern must round-trip exactly — there is no decimal
//! arithmetic to round), and conversion boundaries (interval → frequency_days,
//! seconds-per-day multiplication).

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError, BillSchedule};
use remitwise_common::{MAX_AMOUNT, MIN_AMOUNT};
use soroban_sdk::{
    symbol_short,
    testutils::EnvTestConfig,
    Address, Env, Map, String, Vec,
};
use testutils::{generate_test_address, set_ledger_time};

const SECONDS_PER_DAY: u64 = 86_400;

fn setup() -> (Env, BillPaymentsClient<'static>, Address) {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = generate_test_address(&env);
    (env, client, owner)
}

/// Configure the trusted orchestrator required by the cross-contract epoch
/// guard on `pay_bill`. Returns the orchestrator address to pass to
/// `pay_bill(&orch, &0, ...)` (epoch 0 is the default for a fresh contract).
fn setup_orchestrator(client: &BillPaymentsClient, admin: &Address) -> Address {
    let orch = Address::generate(&client.env);
    client.init_admin(admin, &bill_payments::DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
    client.set_trusted_orchestrator(admin, &orch);
    orch
}

fn create_owner_bill(
    client: &BillPaymentsClient,
    owner: &Address,
    name: &str,
    amount: i128,
    due_date: u64,
) -> u32 {
    client.create_bill(
        owner,
        &String::from_str(&client.env, name),
        &amount,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&client.env, "XLM"),
        &None,
    )
}

fn create_schedule(
    client: &BillPaymentsClient,
    owner: &Address,
    amount: i128,
    next_due: u64,
    interval: u64,
) -> u32 {
    client.create_bill_schedule(
        owner,
        &String::from_str(&client.env, "Rent"),
        &amount,
        &String::from_str(&client.env, "XLM"),
        &next_due,
        &interval,
    )
}

// ---------------------------------------------------------------------------
// 1. Sign and scale rejection at every boundary (zero, negative, oversized)
// ---------------------------------------------------------------------------

#[test]
fn test_create_bill_rejects_zero_negative_and_oversized() {
    let (env, client, owner) = setup();

    for (amount, expected) in [
        (0i128, BillPaymentsError::InvalidAmount),
        (-1i128, BillPaymentsError::InvalidAmount),
        (i128::MIN, BillPaymentsError::InvalidAmount),
        (MAX_AMOUNT + 1, BillPaymentsError::AmountExceedsMax),
        (i128::MAX, BillPaymentsError::AmountExceedsMax),
    ] {
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Bad"),
            &amount,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            result,
            Err(Ok(expected)),
            "amount {amount} must be rejected before any state change"
        );
    }

    // No partial state: no bill was created and no id was consumed.
    assert!(client.get_bill(&1).is_none(), "no bill may exist");
    assert_eq!(client.get_total_unpaid(&owner), 0, "unpaid total must be 0");

    // A valid bill still gets id 1 — rejected attempts must not consume ids.
    let id = create_owner_bill(&client, &owner, "Valid", 100, 1_000_000);
    assert_eq!(id, 1, "rejected attempts must not consume bill ids");
}

#[test]
fn test_create_bill_accepts_min_and_max_amounts() {
    let (env, client, owner) = setup();

    let id_min = create_owner_bill(&client, &owner, "Min", MIN_AMOUNT, 1_000_000);
    let id_max = create_owner_bill(&client, &owner, "Max", MAX_AMOUNT, 1_001_000);

    assert_eq!(client.get_bill(&id_min).unwrap().amount, MIN_AMOUNT);
    assert_eq!(client.get_bill(&id_max).unwrap().amount, MAX_AMOUNT);
    assert_eq!(
        client.get_total_unpaid(&owner),
        MIN_AMOUNT + MAX_AMOUNT,
        "unpaid total must be the exact sum of the two validated amounts"
    );
}

#[test]
fn test_create_bill_schedule_rejects_invalid_amounts() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    for (amount, expected) in [
        (0i128, BillPaymentsError::InvalidAmount),
        (-5i128, BillPaymentsError::InvalidAmount),
        (MAX_AMOUNT + 1, BillPaymentsError::AmountExceedsMax),
        (i128::MAX, BillPaymentsError::AmountExceedsMax),
    ] {
        let result = client.try_create_bill_schedule(
            &owner,
            &String::from_str(&env, "Bad"),
            &amount,
            &String::from_str(&env, "XLM"),
            &(now + 1_000),
            &86_400,
        );
        assert_eq!(
            result,
            Err(Ok(expected)),
            "schedule amount {amount} must be rejected before any state change"
        );
    }

    // No partial state: no schedule stored, no schedule id consumed.
    assert_eq!(
        client.get_bill_schedules(&owner).len(),
        0,
        "no schedule may exist after rejected attempts"
    );
    let id = create_schedule(&client, &owner, 100, now + 1_000, 86_400);
    assert_eq!(id, 1, "rejected attempts must not consume schedule ids");
}

#[test]
fn test_modify_bill_schedule_rejects_invalid_amounts_no_partial_state() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let schedule_id = create_schedule(&client, &owner, 1_000, now + 1_000, 86_400);

    for (amount, expected) in [
        (0i128, BillPaymentsError::InvalidAmount),
        (-1i128, BillPaymentsError::InvalidAmount),
        (MAX_AMOUNT + 1, BillPaymentsError::AmountExceedsMax),
    ] {
        let result = client.try_modify_bill_schedule(
            &owner,
            &schedule_id,
            &amount,
            &(now + 2_000),
            &86_400,
        );
        assert_eq!(result, Err(Ok(expected)));
    }

    // The rejected modifications must leave the stored schedule untouched.
    let schedule = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.amount, 1_000, "amount must be unchanged");
    assert_eq!(schedule.next_due, now + 1_000, "next_due must be unchanged");
    assert_eq!(schedule.interval, 86_400, "interval must be unchanged");
}

// ---------------------------------------------------------------------------
// 2. Schedule interval cap (prevents un-executable schedules)
// ---------------------------------------------------------------------------

#[test]
fn test_schedule_interval_cap_rejects_oversized_interval() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let max_interval = bill_payments::MAX_SCHEDULE_INTERVAL;

    // Boundary: exactly MAX_SCHEDULE_INTERVAL is accepted.
    let id = create_schedule(&client, &owner, 100, now + 1_000, max_interval);
    assert_eq!(id, 1);

    // One second above the cap is rejected at creation and modification.
    let result = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "TooLong"),
        &100,
        &String::from_str(&env, "XLM"),
        &(now + 1_000),
        &(max_interval + 1),
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleIntervalTooLong)));

    let result = client.try_modify_bill_schedule(
        &owner,
        &id,
        &100,
        &(now + 2_000),
        &(max_interval + 1),
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleIntervalTooLong)));

    // Rejected modification left the schedule unchanged.
    assert_eq!(client.get_bill_schedule(&id).unwrap().interval, max_interval);
}

// ---------------------------------------------------------------------------
// 3. Conversion boundaries: interval → frequency_days and seconds-per-day
// ---------------------------------------------------------------------------

#[test]
fn test_interval_to_frequency_days_conversion_boundaries() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    // Minimum interval (1 hour) → frequency_days = 1.
    create_schedule(&client, &owner, 100, now + 1_000, 3_600);
    // Maximum interval (100 years) → frequency_days = MAX_FREQUENCY_DAYS.
    create_schedule(
        &client,
        &owner,
        100,
        now + 1_000,
        bill_payments::MAX_SCHEDULE_INTERVAL,
    );

    set_ledger_time(&env, 1, now + 2_000);
    client.execute_due_bill_schedules();

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 2, "both schedules must generate a bill");
    assert_eq!(bills.get(0).unwrap().frequency_days, 1);
    assert_eq!(
        bills.get(1).unwrap().frequency_days,
        bill_payments::MAX_FREQUENCY_DAYS,
        "max interval must convert exactly to MAX_FREQUENCY_DAYS"
    );
}

#[test]
fn test_schedule_interval_exact_seconds_per_day_math() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    // 365 days exactly; the generated bill's due date must be
    // next_due + 365 * SECONDS_PER_DAY (exact integer math, no rounding).
    let id = create_schedule(&client, &owner, 100, now + 1_000, 365 * SECONDS_PER_DAY);

    set_ledger_time(&env, 1, now + 2_000);
    client.execute_due_bill_schedules();

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 1);
    assert_eq!(bills.get(0).unwrap().frequency_days, 365);
    assert_eq!(bills.get(0).unwrap().due_date, now + 1_000 + 365 * SECONDS_PER_DAY);
    assert_eq!(client.get_bill_schedule(&id).unwrap().missed_count, 0);
}

// ---------------------------------------------------------------------------
// 4. Schedule execution: exact amounts, checked arithmetic, defence-in-depth
// ---------------------------------------------------------------------------

#[test]
fn test_execution_generates_bills_with_exact_amounts() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    let fractional = 123_456_789_123_456_789i128; // non-trivial digit pattern
    create_schedule(&client, &owner, fractional, now + 1_000, 86_400);
    create_schedule(&client, &owner, MAX_AMOUNT, now + 1_000, 86_400);

    set_ledger_time(&env, 1, now + 2_000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 2);

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 2);
    // Exact round-trip: no rounding, truncation, or scaling anywhere.
    assert_eq!(bills.get(0).unwrap().amount, fractional);
    assert_eq!(bills.get(1).unwrap().amount, MAX_AMOUNT);
    assert_eq!(
        client.get_total_unpaid(&owner),
        fractional + MAX_AMOUNT,
        "unpaid total must equal the exact sum of generated bill amounts"
    );
}

/// Defence-in-depth: a schedule stored with an invalid amount (legacy data,
/// corrupt snapshot) must not mint a bill or move an unpaid total — the
/// executor rejects it deterministically (panic → full revert, no partial
/// state).
#[test]
#[should_panic]
fn test_execution_rejects_corrupt_stored_schedule_amount() {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let schedule_id = create_schedule(&client, &owner, 1_000, now + 1_000, 86_400);

    // Simulate a pre-validation schedule by overwriting the stored amount.
    env.as_contract(&contract_id, || {
        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("BSCHEDS"))
            .unwrap_or_else(|| Map::new(&env));
        if let Some(mut schedule) = schedules.get(schedule_id) {
            schedule.amount = 0;
            schedules.set(schedule_id, schedule);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("BSCHEDS"), &schedules);
    });

    set_ledger_time(&env, 1, now + 2_000);
    // Panics with InvalidAmount; the invocation reverts so no bill exists.
    let _ = client.execute_due_bill_schedules();
}

#[test]
fn test_execution_stays_idempotent_and_exact_on_retry() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    create_schedule(&client, &owner, 7_500, now + 1_000, 86_400);

    set_ledger_time(&env, 1, now + 2_000);
    let first = client.execute_due_bill_schedules();
    let second = client.execute_due_bill_schedules();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 0, "retry in the same ledger must be a no-op");

    assert_eq!(client.get_total_unpaid(&owner), 7_500);
    assert_eq!(client.get_all_unpaid_bills_legacy(&owner).len(), 1);
}

// ---------------------------------------------------------------------------
// 5. Payment / batch payment: exact totals, no partial state
// ---------------------------------------------------------------------------

#[test]
fn test_unpaid_total_matches_independent_oracle_full_cycle() {
    let (env, client, owner) = setup();
    let orch = setup_orchestrator(&client, &owner);
    let now = env.ledger().timestamp();

    let amounts = [1_500_000i128, 3_333_333i128, MAX_AMOUNT, 999_999_999i128];
    let mut ids: Vec<u32> = Vec::new(&env);
    let mut oracle: i128 = 0;
    for (i, amount) in amounts.iter().enumerate() {
        let name = ["Bill0", "Bill1", "Bill2", "Bill3"][i];
        let id = create_owner_bill(&client, &owner, name, *amount, now + 1_000);
        ids.push_back(id);
        oracle = oracle
            .checked_add(*amount)
            .expect("oracle addition must not overflow in the test");
    }

    // Independent oracle: the cached total must equal the checked sum of
    // unpaid bill amounts.
    assert_eq!(client.get_total_unpaid(&owner), oracle);

    // Pay one bill → total drops by exactly its amount.
    client.pay_bill(&orch, &0, &owner, &ids.get(0).unwrap());
    oracle -= amounts[0];
    assert_eq!(client.get_total_unpaid(&owner), oracle);

    // Cancel another → total drops by exactly its amount.
    client.cancel_bill(&owner, &ids.get(1).unwrap());
    oracle -= amounts[1];
    assert_eq!(client.get_total_unpaid(&owner), oracle);

    // Pay the rest via batch → total reaches 0 exactly.
    let mut batch: Vec<u32> = Vec::new(&env);
    batch.push_back(ids.get(2).unwrap());
    batch.push_back(ids.get(3).unwrap());
    client.batch_pay_bills(&owner, &batch);
    oracle = 0;
    assert_eq!(client.get_total_unpaid(&owner), oracle);
}

/// Regression for the broken pre-fix `batch_pay_bills`: the unpaid-total
/// delta was computed but never applied, so the cached total went stale.
#[test]
fn test_batch_pay_bills_updates_unpaid_total_exactly() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    let id1 = create_owner_bill(&client, &owner, "A", 100, now + 1_000);
    let id2 = create_owner_bill(&client, &owner, "B", 200, now + 1_000);
    let id3 = create_owner_bill(&client, &owner, "C", 300, now + 1_000);
    assert_eq!(client.get_total_unpaid(&owner), 600);

    // Batch-pay id1 + id2; the non-existent id 999 is skipped.
    let mut batch: Vec<u32> = Vec::new(&env);
    batch.push_back(id1);
    batch.push_back(999);
    batch.push_back(id2);
    client.batch_pay_bills(&owner, &batch);

    assert_eq!(
        client.get_total_unpaid(&owner),
        300,
        "only id3 (300) remains unpaid — the batch delta must be applied exactly"
    );
    assert!(client.get_bill(&id1).unwrap().paid);
    assert!(client.get_bill(&id2).unwrap().paid);
    assert!(!client.get_bill(&id3).unwrap().paid);
}

/// Paying a recurring bill in a batch is net-zero for the unpaid total:
/// the parent's amount is removed but the child bill adds it back.
#[test]
fn test_batch_pay_recurring_net_zero_total() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    let id1 = client.create_bill(
        &owner,
        &String::from_str(&env, "Sub"),
        &500,
        &(now + 1_000),
        &true,
        &30,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(client.get_total_unpaid(&owner), 500);

    let mut batch: Vec<u32> = Vec::new(&env);
    batch.push_back(id1);
    client.batch_pay_bills(&owner, &batch);

    assert_eq!(
        client.get_total_unpaid(&owner),
        500,
        "recurring payment is net-zero: child inherits the exact parent amount"
    );
    let child = client.get_bill(&2).unwrap();
    assert_eq!(child.amount, 500, "child must preserve the exact amount");
    assert!(!child.paid);
}

#[test]
fn test_pay_recurring_child_preserves_max_amount_and_total() {
    let (env, client, owner) = setup();
    let orch = setup_orchestrator(&client, &owner);
    let now = env.ledger().timestamp();

    let id1 = client.create_bill(
        &owner,
        &String::from_str(&env, "MaxSub"),
        &MAX_AMOUNT,
        &(now + 1_000),
        &true,
        &30,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert_eq!(client.get_total_unpaid(&owner), MAX_AMOUNT);

    client.pay_bill(&orch, &0, &owner, &id1);

    let child = client.get_bill(&2).unwrap();
    assert_eq!(child.amount, MAX_AMOUNT);
    assert_eq!(
        client.get_total_unpaid(&owner),
        MAX_AMOUNT,
        "parent paid + child born with the same amount ⇒ net-zero total"
    );
}

/// Rejected, stale, and repeated operations must leave no partial state.
#[test]
fn test_rejected_and_repeated_operations_leave_no_partial_state() {
    let (env, client, owner) = setup();
    let orch = setup_orchestrator(&client, &owner);
    let now = env.ledger().timestamp();

    let id = create_owner_bill(&client, &owner, "Single", 250, now + 1_000);
    client.pay_bill(&orch, &0, &owner, &id);

    // Repeated pay is rejected and must not mutate anything.
    let repeated = client.try_pay_bill(&orch, &0, &owner, &id);
    assert_eq!(repeated, Err(Ok(BillPaymentsError::BillAlreadyPaid)));
    let bill = client.get_bill(&id).unwrap();
    assert!(bill.paid);
    assert_eq!(client.get_total_unpaid(&owner), 0);

    // Paying a non-existent bill is rejected without side effects.
    let missing = client.try_pay_bill(&orch, &0, &owner, &999);
    assert_eq!(missing, Err(Ok(BillPaymentsError::BillNotFound)));
    assert_eq!(client.get_total_unpaid(&owner), 0);
}

/// Schedules generated through execution pay exact amounts through the
/// full create → schedule-execute → pay lifecycle.
#[test]
fn test_schedule_lifecycle_amounts_exact_end_to_end() {
    let (env, client, owner) = setup();
    let orch = setup_orchestrator(&client, &owner);
    let now = env.ledger().timestamp();

    let amount = 123_456_789_123_456_789i128;
    let schedule_id = create_schedule(&client, &owner, amount, now + 1_000, 86_400);

    set_ledger_time(&env, 1, now + 2_000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 1);
    let bill = bills.get(0).unwrap();
    assert_eq!(bill.amount, amount);
    assert_eq!(bill.schedule_id, Some(schedule_id));
    assert_eq!(client.get_total_unpaid(&owner), amount);

    // Pay the generated bill → total reaches zero exactly.
    client.pay_bill(&orch, &0, &owner, &bill.id);
    assert_eq!(client.get_total_unpaid(&owner), 0);
}
