#![cfg(test)]

extern crate std;

use bill_payments::{
    BillEvent, BillPayments, BillPaymentsClient, BillPaymentsError, BillSchedule,
};
use soroban_sdk::{
    testutils::{EnvTestConfig, Events, Ledger},
    symbol_short, Address, BytesN, Env, Map, String, TryFromVal,
};
use testutils::{generate_test_address, set_ledger_time};

// ─── shared helpers ───────────────────────────────────────────────────────────

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

fn request_key(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

fn count_owner_schedules(client: &BillPaymentsClient, owner: &Address) -> u32 {
    client.get_bill_schedules_page(owner, &0, &50).count
}

fn count_all_bill_events(env: &Env) -> u32 {
    env.events()
        .all()
        .iter()
        .filter(|(_, topics, _)| {
            topics.len() >= 2
                && BillEvent::try_from_val(env, &topics.get(1).unwrap()).is_ok()
        })
        .count() as u32
}

// ─── 1. Schedule creation and bill generation ─────────────────────────────────

/// A bill schedule creates a bill with schedule_id populated when executed.
#[test]
fn test_create_schedule_generates_bill_with_schedule_id() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Monthly Rent"),
        &10000,
        &String::from_str(&env, "XLM"),
        &(now + 86400),
        &86400,
    );

    // Advance time past next_due
    set_ledger_time(&env, 1, now + 2 * 86400);
    let executed = client.execute_due_bill_schedules();

    assert_eq!(executed.len(), 1, "schedule should execute");
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    // The generated bill should have schedule_id set
    let bills = client.get_all_unpaid_bills_legacy(&owner);
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
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &5000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    set_ledger_time(&env, 1, now + 2000);
    let first = client.execute_due_bill_schedules();
    assert_eq!(first.len(), 1, "first call must execute the schedule");

    let second = client.execute_due_bill_schedules();
    assert_eq!(
        second.len(),
        0,
        "second call in same ledger must not execute"
    );

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 1, "exactly one bill must exist");
}

/// One-off schedule is deactivated after execution; second call sees inactive.
#[test]
fn test_one_off_schedule_executed_once() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "OneTime"),
        &3000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &0,
    );

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
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Internet"),
        &2000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    set_ledger_time(&env, 1, now + 5 * 86400);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1);

    let schedule = client.get_bill_schedule(&schedule_id).unwrap();
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
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Phone"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    client.modify_bill_schedule(&owner, &schedule_id, &2500, &(now + 2 * 86400), &86400);

    set_ledger_time(&env, 1, now + 3 * 86400);
    client.execute_due_bill_schedules();

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(bills.len(), 1);
    assert_eq!(bills.get(0).unwrap().amount, 2500);
}

/// Cancelling a schedule prevents further bill generation.
#[test]
fn test_cancel_bill_schedule_prevents_execution() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Gym"),
        &1500,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    client.cancel_bill_schedule(&owner, &schedule_id);

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 0, "cancelled schedule must not execute");
}

// ─── 5. MAX_BILLS_PER_OWNER cap ───────────────────────────────────────────────

/// When owner is at MAX_BILLS_PER_OWNER, schedule execution does not generate
/// a new bill but still advances next_due and increments missed_count.
#[test]
#[ignore = "slow: fills MAX_BILLS_PER_OWNER (1000) slots; run with --ignored"]
fn test_execution_respects_max_bills_per_owner() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let mut ledger_ts = now;
    const BILLS_PER_WINDOW: u32 = bill_payments::CREATE_BILL_RATE_LIMIT;
    let windows = bill_payments::MAX_BILLS_PER_OWNER / BILLS_PER_WINDOW;
    for window in 0..windows {
        ledger_ts = ledger_ts.saturating_add(86_401);
        set_ledger_time(&env, window + 1, ledger_ts);
        env.budget().reset_unlimited();
        env.budget().reset_tracker();
        for offset in 0..BILLS_PER_WINDOW {
            let i = window * BILLS_PER_WINDOW + offset;
            create_owner_bill(
                &client,
                &owner,
                &format!("Bill{}", i),
                1000,
                ledger_ts + 1 + u64::from(offset),
            );
        }
    }

    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Overflow"),
        &5000,
        &String::from_str(&env, "XLM"),
        &(ledger_ts + 1000),
        &86400,
    );

    set_ledger_time(
        &env,
        bill_payments::MAX_BILLS_PER_OWNER + 1,
        ledger_ts + 2000,
    );
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 1, "schedule must execute");

    let bills = client.get_all_unpaid_bills_legacy(&owner);
    assert_eq!(
        bills.len(),
        bill_payments::MAX_BILLS_PER_OWNER,
        "no new bill should be created when owner is at cap"
    );
}

// ─── 6. Schedule queries ──────────────────────────────────────────────────────

#[test]
fn test_get_bill_schedules_returns_owner_schedules() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &8000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let schedules = client.get_bill_schedules(&owner);
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules.get(0).unwrap().amount, 8000);
}

#[test]
fn test_get_bill_schedule_returns_none_for_missing() {
    let (_env, client, _owner) = setup();

    let sched = client.get_bill_schedule(&9999);
    assert!(sched.is_none());
}

// ─── 7. Error paths ───────────────────────────────────────────────────────────

#[test]
fn test_create_bill_schedule_past_due_date_fails() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    let result = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "Test"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now.saturating_sub(1000)),
        &86400,
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidDueDate)));
}

#[test]
fn test_create_bill_schedule_interval_too_short_fails() {
    let (env, client, owner) = setup();

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
    let (env, client, owner) = setup();
    let intruder = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let result =
        client.try_modify_bill_schedule(&intruder, &schedule_id, &2000, &(now + 2000), &86400);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));
}

#[test]
fn test_cancel_bill_schedule_schedule_not_found_fails() {
    let (_env, client, owner) = setup();

    let result = client.try_cancel_bill_schedule(&owner, &9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));
}

// ─── 8. Pause behavior ────────────────────────────────────────────────────────

#[test]
fn test_execute_due_bill_schedules_respects_global_pause() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    client.set_pause_admin(&owner, &owner);
    client.pause(&owner);

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(
        executed.len(),
        0,
        "paused contract must not execute schedules"
    );
}

// ─── 9. Event emission ────────────────────────────────────────────────────────

fn count_bill_event_variant(env: &Env, expected: &BillEvent) -> u32 {
    let mut count = 0u32;
    for (_cid, topics, _data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        if let Ok(event) = BillEvent::try_from_val(env, &topics.get(1).unwrap()) {
            if matches!(
                (&event, expected),
                (BillEvent::ScheduleCreated, BillEvent::ScheduleCreated)
                    | (BillEvent::ScheduleExecuted, BillEvent::ScheduleExecuted)
                    | (BillEvent::ScheduleModified, BillEvent::ScheduleModified)
                    | (BillEvent::ScheduleCancelled, BillEvent::ScheduleCancelled)
            ) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn test_schedule_events_emitted() {
    let (env, client, owner) = setup();

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    assert_eq!(
        count_bill_event_variant(&env, &BillEvent::ScheduleCreated),
        1,
        "ScheduleCreated event must be emitted"
    );

    set_ledger_time(&env, 1, now + 2000);
    client.execute_due_bill_schedules();

    assert_eq!(
        count_bill_event_variant(&env, &BillEvent::ScheduleExecuted),
        1,
        "ScheduleExecuted event must be emitted"
    );
}

// --- 10. Pagination and limits ------------------------------------------------
#[test]
fn test_execute_due_bill_schedules_paginates_at_max_batch_size() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();

    // MAX_BATCH_SIZE is 50. Create 55 schedules.
    for i in 0..55 {
        client.create_bill_schedule(
            &owner,
            &String::from_str(&env, &format!("Bill {}", i)),
            &1000,
            &String::from_str(&env, "XLM"),
            &(now + 1000),
            &86400,
        );
    }

    set_ledger_time(&env, 1, now + 2000);

    // First execution should process 50 schedules
    let executed_first = client.execute_due_bill_schedules();
    assert_eq!(
        executed_first.len(),
        50,
        "First execution should process exactly MAX_BATCH_SIZE schedules"
    );

    // Second execution should process the remaining 5 schedules
    let executed_second = client.execute_due_bill_schedules();
    assert_eq!(
        executed_second.len(),
        5,
        "Second execution should process remaining 5 schedules"
    );

    // Third execution should process 0
    let executed_third = client.execute_due_bill_schedules();
    assert_eq!(
        executed_third.len(),
        0,
        "Third execution should process 0 schedules"
    );
}

// --- 11. Keyed schedule APIs -------------------------------------------------

#[test]
fn test_keyed_schedule_create_duplicate_conflict_and_invalid_retry() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let key = request_key(&env, 21);
    let events_before = count_bill_event_variant(&env, &BillEvent::ScheduleCreated);

    let first = client.create_bill_schedule_keyed(
        &owner,
        &key,
        &String::from_str(&env, "Keyed rent"),
        &1_000i128,
        &String::from_str(&env, "XLM"),
        &(now + 10_000),
        &86_400u64,
    );
    let duplicate = client.create_bill_schedule_keyed(
        &owner,
        &key,
        &String::from_str(&env, "Keyed rent"),
        &1_000i128,
        &String::from_str(&env, "XLM"),
        &(now + 10_000),
        &86_400u64,
    );
    assert_eq!(duplicate, first);
    assert_eq!(count_owner_schedules(&client, &owner), 1);
    assert_eq!(
        count_bill_event_variant(&env, &BillEvent::ScheduleCreated),
        events_before + 1
    );

    let conflict = client.try_create_bill_schedule_keyed(
        &owner,
        &key,
        &String::from_str(&env, "Keyed rent"),
        &1_001i128,
        &String::from_str(&env, "XLM"),
        &(now + 10_000),
        &86_400u64,
    );
    assert_eq!(conflict, Err(Ok(BillPaymentsError::RequestKeyConflict)));
    assert_eq!(client.get_bill_schedule(&first).unwrap().amount, 1_000);
    assert_eq!(count_owner_schedules(&client, &owner), 1);

    let retry_key = request_key(&env, 22);
    for invalid_amount in [0i128, -1i128] {
        let invalid = client.try_create_bill_schedule_keyed(
            &owner,
            &retry_key,
            &String::from_str(&env, "Corrected"),
            &invalid_amount,
            &String::from_str(&env, "XLM"),
            &(now + 20_000),
            &86_400u64,
        );
        assert_eq!(invalid, Err(Ok(BillPaymentsError::InvalidAmount)));
        assert_eq!(count_owner_schedules(&client, &owner), 1);
    }
    let corrected = client.create_bill_schedule_keyed(
        &owner,
        &retry_key,
        &String::from_str(&env, "Corrected"),
        &2_000i128,
        &String::from_str(&env, "XLM"),
        &(now + 20_000),
        &86_400u64,
    );
    assert_ne!(corrected, first);
    assert_eq!(count_owner_schedules(&client, &owner), 2);
}

#[test]
fn test_keyed_schedule_modify_cancel_replays_and_stale_transition_is_stable() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule_keyed(
        &owner,
        &request_key(&env, 23),
        &String::from_str(&env, "Lifecycle"),
        &1_000i128,
        &String::from_str(&env, "USDC"),
        &(now + 10_000),
        &86_400u64,
    );

    let modify_key = request_key(&env, 24);
    let modified_events_before =
        count_bill_event_variant(&env, &BillEvent::ScheduleModified);
    assert!(client.modify_bill_schedule_keyed(
        &owner,
        &modify_key,
        &schedule_id,
        &2_500i128,
        &(now + 20_000),
        &172_800u64,
    ));
    let after_modify = client.get_bill_schedule(&schedule_id).unwrap();
    assert!(client.modify_bill_schedule_keyed(
        &owner,
        &modify_key,
        &schedule_id,
        &2_500i128,
        &(now + 20_000),
        &172_800u64,
    ));
    let after_modify_retry = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(after_modify_retry.amount, after_modify.amount);
    assert_eq!(after_modify_retry.next_due, after_modify.next_due);
    assert_eq!(after_modify_retry.interval, after_modify.interval);
    assert_eq!(after_modify_retry.recurring, after_modify.recurring);
    assert_eq!(after_modify_retry.last_executed, after_modify.last_executed);
    assert_eq!(after_modify_retry.missed_count, after_modify.missed_count);
    assert_eq!(
        count_bill_event_variant(&env, &BillEvent::ScheduleModified),
        modified_events_before + 1
    );

    let conflict = client.try_modify_bill_schedule_keyed(
        &owner,
        &modify_key,
        &schedule_id,
        &2_501i128,
        &(now + 20_000),
        &172_800u64,
    );
    assert_eq!(conflict, Err(Ok(BillPaymentsError::RequestKeyConflict)));
    assert_eq!(client.get_bill_schedule(&schedule_id).unwrap().amount, 2_500);

    let corrected_key = request_key(&env, 25);
    let invalid = client.try_modify_bill_schedule_keyed(
        &owner,
        &corrected_key,
        &schedule_id,
        &3_000i128,
        &now,
        &86_400u64,
    );
    assert_eq!(invalid, Err(Ok(BillPaymentsError::InvalidDueDate)));
    assert!(client.modify_bill_schedule_keyed(
        &owner,
        &corrected_key,
        &schedule_id,
        &3_000i128,
        &(now + 30_000),
        &86_400u64,
    ));

    let cancel_key = request_key(&env, 26);
    let cancelled_events_before =
        count_bill_event_variant(&env, &BillEvent::ScheduleCancelled);
    assert!(client.cancel_bill_schedule_keyed(&owner, &cancel_key, &schedule_id));
    let cancelled = client.get_bill_schedule(&schedule_id).unwrap();
    assert!(!cancelled.active);
    assert!(client.cancel_bill_schedule_keyed(&owner, &cancel_key, &schedule_id));
    let cancelled_retry = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(cancelled_retry.amount, cancelled.amount);
    assert_eq!(cancelled_retry.next_due, cancelled.next_due);
    assert_eq!(cancelled_retry.interval, cancelled.interval);
    assert_eq!(cancelled_retry.last_executed, cancelled.last_executed);
    assert_eq!(cancelled_retry.missed_count, cancelled.missed_count);
    assert_eq!(
        count_bill_event_variant(&env, &BillEvent::ScheduleCancelled),
        cancelled_events_before + 1
    );
    assert_eq!(count_owner_schedules(&client, &owner), 0);

    let stale = client.try_modify_bill_schedule_keyed(
        &owner,
        &request_key(&env, 27),
        &schedule_id,
        &9_999i128,
        &(now + 40_000),
        &86_400u64,
    );
    assert_eq!(stale, Err(Ok(BillPaymentsError::ScheduleNotActive)));
    let after_stale = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(after_stale.amount, cancelled.amount);
    assert_eq!(after_stale.next_due, cancelled.next_due);
    assert_eq!(after_stale.interval, cancelled.interval);
    assert_eq!(after_stale.active, cancelled.active);
    assert_eq!(after_stale.last_executed, cancelled.last_executed);
    assert_eq!(after_stale.missed_count, cancelled.missed_count);
}

#[test]
fn test_keyed_schedule_execution_timeout_retry_freezes_original_result() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule_keyed(
        &owner,
        &request_key(&env, 28),
        &String::from_str(&env, "Execute once"),
        &4_200i128,
        &String::from_str(&env, "XLM"),
        &(now + 1_000),
        &86_400u64,
    );
    let execute_key = request_key(&env, 29);

    set_ledger_time(&env, 1, now + 2_000);
    let first = client.execute_due_bill_schedules_keyed(&owner, &execute_key);
    assert_eq!(first.len(), 1);
    assert_eq!(first.get(0).unwrap(), schedule_id);

    let schedule_after_first = client.get_bill_schedule(&schedule_id).unwrap();
    let bills_after_first = client.get_all_unpaid_bills_legacy(&owner);
    let generated = bills_after_first.get(0).unwrap();
    let unpaid_after_first = client.get_total_unpaid(&owner);
    let events_after_first = count_all_bill_events(&env);

    set_ledger_time(&env, 2, now + 200_000);
    let retry = client.execute_due_bill_schedules_keyed(&owner, &execute_key);
    assert_eq!(retry.len(), first.len());
    assert_eq!(retry.get(0).unwrap(), first.get(0).unwrap());

    let schedule_after_retry = client.get_bill_schedule(&schedule_id).unwrap();
    let bills_after_retry = client.get_all_unpaid_bills_legacy(&owner);
    let generated_retry = bills_after_retry.get(0).unwrap();
    assert_eq!(bills_after_retry.len(), bills_after_first.len());
    assert_eq!(generated_retry.id, generated.id);
    assert_eq!(generated_retry.amount, generated.amount);
    assert_eq!(generated_retry.due_date, generated.due_date);
    assert_eq!(generated_retry.schedule_id, generated.schedule_id);
    assert_eq!(
        schedule_after_retry.last_executed,
        schedule_after_first.last_executed
    );
    assert_eq!(schedule_after_retry.next_due, schedule_after_first.next_due);
    assert_eq!(
        schedule_after_retry.missed_count,
        schedule_after_first.missed_count
    );
    assert_eq!(client.get_total_unpaid(&owner), unpaid_after_first);
    assert_eq!(count_all_bill_events(&env), events_after_first);

    let cross_operation = client.try_cancel_bill_schedule_keyed(
        &owner,
        &execute_key,
        &schedule_id,
    );
    assert_eq!(
        cross_operation,
        Err(Ok(BillPaymentsError::RequestKeyConflict))
    );
    let after_conflict = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(after_conflict.active, schedule_after_first.active);
    assert_eq!(after_conflict.next_due, schedule_after_first.next_due);
    assert_eq!(
        after_conflict.last_executed,
        schedule_after_first.last_executed
    );
    assert_eq!(
        after_conflict.missed_count,
        schedule_after_first.missed_count
    );
}

#[test]
fn test_keyed_schedule_execution_pause_does_not_consume_request_key() {
    let (env, client, owner) = setup();
    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Paused execution"),
        &1_200i128,
        &String::from_str(&env, "XLM"),
        &(now + 1_000),
        &86_400u64,
    );
    let key = request_key(&env, 30);

    client.set_pause_admin(&owner, &owner);
    client.pause(&owner);
    set_ledger_time(&env, 1, now + 2_000);
    assert_eq!(
        client.try_execute_due_bill_schedules_keyed(&owner, &key),
        Err(Ok(BillPaymentsError::ContractPaused))
    );
    assert!(client.get_bill_schedule(&schedule_id).unwrap().last_executed.is_none());

    client.unpause(&owner);
    let executed = client.execute_due_bill_schedules_keyed(&owner, &key);
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);
}

#[test]
fn test_keyed_empty_execution_binds_result_without_mutating_invalid_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 0, 1_000);
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = generate_test_address(&env);
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Legacy invalid amount"),
        &1_000i128,
        &String::from_str(&env, "XLM"),
        &2_000u64,
        &86_400u64,
    );

    env.as_contract(&contract_id, || {
        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("BSCHEDS"))
            .unwrap();
        let mut schedule = schedules.get(schedule_id).unwrap();
        schedule.amount = 0;
        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("BSCHEDS"), &schedules);
    });

    set_ledger_time(&env, 1, 3_000);
    let key = request_key(&env, 31);
    let events_before = count_all_bill_events(&env);
    let first = client.execute_due_bill_schedules_keyed(&owner, &key);
    assert_eq!(first.len(), 0);
    let invalid_after = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(invalid_after.amount, 0);
    assert_eq!(invalid_after.next_due, 2_000);
    assert!(invalid_after.last_executed.is_none());
    assert_eq!(client.get_owner_bill_count(&owner), 0);
    assert_eq!(client.get_total_unpaid(&owner), 0);
    assert_eq!(count_all_bill_events(&env), events_before);

    assert!(client.modify_bill_schedule_keyed(
        &owner,
        &request_key(&env, 32),
        &schedule_id,
        &1_000i128,
        &4_000u64,
        &86_400u64,
    ));
    set_ledger_time(&env, 2, 5_000);

    let frozen_retry = client.execute_due_bill_schedules_keyed(&owner, &key);
    assert_eq!(frozen_retry.len(), 0);
    assert!(client.get_bill_schedule(&schedule_id).unwrap().last_executed.is_none());

    let executed =
        client.execute_due_bill_schedules_keyed(&owner, &request_key(&env, 33));
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);
    assert_eq!(client.get_owner_bill_count(&owner), 1);
    assert_eq!(client.get_total_unpaid(&owner), 1_000);
}
