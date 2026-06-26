#![cfg(test)]

extern crate std;

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError};
use soroban_sdk::{testutils::Address as AddressTrait, Address, Env, String, Symbol};
use testutils::{generate_test_address, set_ledger_time, setup_test_env};

// ─── shared helpers ───────────────────────────────────────────────────────────

fn setup(env: &Env) -> (BillPaymentsClient, Address) {
    setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
    (client, owner)
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
        &String::from_str(&client.env(), name),
        &amount,
        &due_date,
        &false,
        &0,
        &None,
        &String::from_str(&client.env(), "XLM"),
        &None,
    )
}

// ─── 1. Schedule creation and bill generation ─────────────────────────────────

/// A bill schedule creates a bill with schedule_id populated when executed.
#[test]
fn test_create_schedule_generates_bill_with_schedule_id() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Monthly Rent"),
            &10000,
            &String::from_str(&env, "XLM"),
            &(now + 86400),
            &86400,
        )
        .unwrap();

    // Advance time past next_due
    set_ledger_time(&env, 1, now + 2 * 86400);
    let executed = client.execute_due_bill_schedules();

    assert_eq!(executed.len(), 1, "schedule should execute");
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    // The generated bill should have schedule_id set
    let bills = client.get_all_unpaid_bills_legacy(owner.clone());
    assert_eq!(bills.len(), 1, "one bill should be generated");
    assert_eq!(bills.get(0).unwrap().schedule_id, Some(schedule_id));
    assert_eq!(bills.get(0).unwrap().amount, 10000);
    assert!(bills.get(0).unwrap().recurring);
    assert_eq!(bills.get(0).unwrap().frequency_days, 1);
}

// ─── 2. Idempotency ───────────────────────────────────────────────────────────

/// Calling execute_due_bill_schedules twice in the same ledger must not
/// double-generate bills for a recurring schedule.
#[test]
fn test_no_double_execution_same_ledger_recurring() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Rent"),
            &5000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    set_ledger_time(&env, 1, now + 2000);
    let first = client.execute_due_bill_schedules();
    assert_eq!(first.len(), 1, "first call must execute the schedule");

    let second = client.execute_due_bill_schedules();
    assert_eq!(
        second.len(),
        0,
        "second call in same ledger must not execute"
    );

    let bills = client.get_all_unpaid_bills_legacy(owner.clone());
    assert_eq!(bills.len(), 1, "exactly one bill must exist");
}

/// One-off schedule is deactivated after execution; second call sees inactive.
#[test]
fn test_one_off_schedule_executed_once() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "OneTime"),
            &3000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &0,
        )
        .unwrap();

    set_ledger_time(&env, 1, now + 2000);
    let first = client.execute_due_bill_schedules();
    assert_eq!(first.len(), 1);

    let second = client.execute_due_bill_schedules();
    assert_eq!(second.len(), 0, "one-off schedule must not re-execute");
}

// ─── 3. Recurring schedule next_due advancement ──────────────────────────────

/// A recurring schedule whose execution is delayed advances next_due past
/// current_time and increments missed_count.
#[test]
fn test_recurring_schedule_advances_next_due_and_missed_count() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Internet"),
            &2000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    set_ledger_time(&env, 1, now + 5 * 86400);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1);

    let schedule = client.get_bill_schedule(schedule_id).unwrap();
    assert!(
        schedule.next_due > now + 5 * 86400,
        "next_due must be future"
    );
    assert_eq!(
        schedule.missed_count, 4,
        "4 intervals should have been missed"
    );
}

// ─── 4. Modify and cancel ─────────────────────────────────────────────────────

/// Modifying a schedule updates the next generated bill's amount.
#[test]
fn test_modify_bill_schedule_updates_next_bill() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Phone"),
            &1000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    client
        .modify_bill_schedule(&owner, &schedule_id, &2500, &(now + 2 * 86400), &86400)
        .unwrap();

    set_ledger_time(&env, 1, now + 3 * 86400);
    client.execute_due_bill_schedules();

    let bills = client.get_all_unpaid_bills_legacy(owner.clone());
    assert_eq!(bills.len(), 1);
    assert_eq!(bills.get(0).unwrap().amount, 2500);
}

/// Cancelling a schedule prevents further bill generation.
#[test]
fn test_cancel_bill_schedule_prevents_execution() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Gym"),
            &1500,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    client.cancel_bill_schedule(&owner, &schedule_id).unwrap();

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 0, "cancelled schedule must not execute");
}

// ─── 5. MAX_BILLS_PER_OWNER cap ───────────────────────────────────────────────

/// When owner is at MAX_BILLS_PER_OWNER, schedule execution does not generate
/// a new bill but still advances next_due and increments missed_count.
#[test]
fn test_execution_respects_max_bills_per_owner() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    // Fill up to MAX_BILLS_PER_OWNER
    for i in 0..bill_payments::MAX_BILLS_PER_OWNER {
        create_owner_bill(&client, &owner, &format!("Bill{}", i), 1000, now + i);
    }

    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Overflow"),
            &5000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1, "schedule must execute");

    let bills = client.get_all_unpaid_bills_legacy(owner.clone());
    assert_eq!(
        bills.len() as u32,
        bill_payments::MAX_BILLS_PER_OWNER,
        "no new bill should be created when owner is at cap"
    );
}

// ─── 6. Schedule queries ──────────────────────────────────────────────────────

#[test]
fn test_get_bill_schedules_returns_owner_schedules() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Rent"),
            &8000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    let schedules = client.get_bill_schedules(owner);
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules.get(0).unwrap().amount, 8000);
}

#[test]
fn test_get_bill_schedule_returns_none_for_missing() {
    let env = Env::default();
    let (client, _owner) = setup(&env);

    let sched = client.get_bill_schedule(9999);
    assert!(sched.is_none());
}

// ─── 7. Error paths ───────────────────────────────────────────────────────────

#[test]
fn test_create_bill_schedule_past_due_date_fails() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let result = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "Test"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now - 1000),
        &86400,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidDueDate)));
}

#[test]
fn test_create_bill_schedule_interval_too_short_fails() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    let result = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "Test"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &100,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleIntervalTooShort)));
}

#[test]
fn test_modify_bill_schedule_unauthorized_fails() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let intruder = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    let result =
        client.try_modify_bill_schedule(&intruder, &schedule_id, &2000, &(now + 2000), &86400);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));
}

#[test]
fn test_cancel_bill_schedule_schedule_not_found_fails() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let result = client.try_cancel_bill_schedule(&owner, &9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));
}

// ─── 8. Pause behavior ────────────────────────────────────────────────────────

#[test]
fn test_execute_due_bill_schedules_respects_global_pause() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    client.set_pause_admin(owner.clone(), owner.clone());
    client.pause(owner.clone());

    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(
        executed.len(),
        0,
        "paused contract must not execute schedules"
    );
}

// ─── 9. Event emission ────────────────────────────────────────────────────────

fn count_bill_event_variant(env: &Env, expected: BillEvent) -> u32 {
    let mut count = 0u32;
    for (cid, topics, _data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        if let Ok(event) = BillEvent::try_from_val(env, &topics.get(1).unwrap()) {
            if matches!(event, expected) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn test_schedule_events_emitted() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let now = env.ledger().timestamp();
    client
        .create_bill_schedule(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        )
        .unwrap();

    assert_eq!(
        count_bill_event_variant(&env, BillEvent::ScheduleCreated),
        1,
        "ScheduleCreated event must be emitted"
    );

    set_ledger_time(&env, 1, now + 2000);
    client.execute_due_bill_schedules();

    assert_eq!(
        count_bill_event_variant(&env, BillEvent::ScheduleExecuted),
        1,
        "ScheduleExecuted event must be emitted"
    );
}
