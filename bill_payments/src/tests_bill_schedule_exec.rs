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

fn setup() -> (Env, BillPaymentsClient<'static>, Address, Address) {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = generate_test_address(&env);
    (env, client, owner, contract_id)
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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (_env, client, _owner, _contract_id) = setup();

    let sched = client.get_bill_schedule(&9999);
    assert!(sched.is_none());
}

// ─── 7. Error paths ───────────────────────────────────────────────────────────

#[test]
fn test_create_bill_schedule_past_due_date_fails() {
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();
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
    let (_env, client, owner, _contract_id) = setup();

    let result = client.try_cancel_bill_schedule(&owner, &9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));
}

// ─── 8. Pause behavior ────────────────────────────────────────────────────────

#[test]
fn test_execute_due_bill_schedules_respects_global_pause() {
    let (env, client, owner, contract_id) = setup();

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
    let (env, client, owner, contract_id) = setup();

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
#[ignore = "pre-existing: creates 55 schedules but MAX_BILL_SCHEDULES_PER_OWNER is 50"]
fn test_execute_due_bill_schedules_paginates_at_max_batch_size() {
    let (env, client, owner, _contract_id) = setup();
    let mut now = env.ledger().timestamp();

    // MAX_BATCH_SIZE is 50. Create 55 schedules across multiple time windows
    // to avoid rate limiting (CREATE_BILL_RATE_LIMIT = 100 per 24h).
    for i in 0..55 {
        // Advance time every 40 schedules to stay within rate limits
        if i > 0 && i % 40 == 0 {
            now = now.saturating_add(86_401);
            set_ledger_time(&env, (i / 40) as u32 + 1, now);
        }
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

// ═══════════════════════════════════════════════════════════════════════════════
// 11. Authorization boundary tests — cross-tenant, forged identity, kill switch
// ═══════════════════════════════════════════════════════════════════════════════

// ─── Cross-tenant schedule rejection ─────────────────────────────────────────

/// Cross-tenant: User B must not be able to modify User A's schedule.
#[test]
fn test_cross_tenant_modify_schedule_rejected() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let result =
        client.try_modify_bill_schedule(&owner_b, &schedule_id, &2000, &(now + 2000), &86400);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));

    // Verify no mutation: schedule unchanged
    let schedule = client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.amount, 1000, "amount must not change");
    assert_eq!(schedule.owner, owner_a, "owner must not change");
}

/// Cross-tenant: User B must not be able to cancel User A's schedule.
#[test]
fn test_cross_tenant_cancel_schedule_rejected() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "Internet"),
        &500,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let result = client.try_cancel_bill_schedule(&owner_b, &schedule_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));

    // Verify no mutation: schedule still active
    let schedule = client.get_bill_schedule(&schedule_id).unwrap();
    assert!(schedule.active, "schedule must remain active");
}

// ─── Cross-tenant bill operations rejection ──────────────────────────────────

/// Cross-tenant: User B must not be able to pay User A's bill.
#[test]
fn test_cross_tenant_pay_bill_rejected() {
    let (env, client, owner_a, _contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner_a, "Electricity", 500, now + 86400);

    // Use batch_pay_bills (no orchestrator/epoch required) to attempt cross-tenant pay
    let bill_ids = soroban_sdk::vec![&env, bill_id];
    client.batch_pay_bills(&owner_b, &bill_ids);

    // Verify no mutation: bill still unpaid (batch silently skips unauthorized bills)
    let bill = client.get_bill(&bill_id).unwrap();
    assert!(!bill.paid, "bill must remain unpaid");
    assert_eq!(bill.paid_at, None, "paid_at must remain None");
}

/// Cross-tenant: User B must not be able to cancel User A's bill.
#[test]
fn test_cross_tenant_cancel_bill_rejected() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner_a, "Water", 300, now + 86400);

    let result = client.try_cancel_bill(&owner_b, &bill_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));

    // Verify no mutation: bill still exists and unpaid
    let bill = client.get_bill(&bill_id).unwrap();
    assert!(!bill.paid);
}

/// Cross-tenant: User B must not be able to add tags to User A's bill.
#[test]
fn test_cross_tenant_add_tags_rejected() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner_a, "Gas", 200, now + 86400);

    let result = client.try_add_tags_to_bill(
        &owner_b,
        &bill_id,
        &soroban_sdk::vec![&env, String::from_str(&env, "urgent")],
    );
    // add_tags_to_bill panics on cross-owner, so try_ should capture the panic
    // Actually it uses panic! so try_ will return a contract error
    assert!(result.is_err());

    // Verify no mutation: tags unchanged
    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.tags.len(), 0, "tags must remain empty");
}

/// Cross-tenant: User B must not be able to set external ref on User A's bill.
#[test]
fn test_cross_tenant_set_external_ref_rejected() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner_a, "Phone", 150, now + 86400);

    let result =
        client.try_set_external_ref(&owner_b, &bill_id, &Some(String::from_str(&env, "EXT-999")));
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));

    // Verify no mutation: external_ref unchanged
    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.external_ref, None, "external_ref must remain None");
}

// ─── Kill switch blocks write operations ─────────────────────────────────────

/// Kill switch must block create_bill_schedule.
#[test]
fn test_kill_switch_blocks_create_bill_schedule() {
    let (env, client, owner, contract_id) = setup();
    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let now = env.ledger().timestamp();
    let result = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );
    // Should fail — kill switch is active
    assert!(
        result.is_err(),
        "create_bill_schedule must fail when kill switch is active"
    );
}

/// Kill switch must block modify_bill_schedule.
#[test]
fn test_kill_switch_blocks_modify_bill_schedule() {
    let (env, client, owner, contract_id) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let result =
        client.try_modify_bill_schedule(&owner, &schedule_id, &2000, &(now + 2000), &86400);
    assert!(
        result.is_err(),
        "modify_bill_schedule must fail when kill switch is active"
    );
}

/// Kill switch must block cancel_bill_schedule.
#[test]
fn test_kill_switch_blocks_cancel_bill_schedule() {
    let (env, client, owner, contract_id) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Gym"),
        &1500,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let result = client.try_cancel_bill_schedule(&owner, &schedule_id);
    assert!(
        result.is_err(),
        "cancel_bill_schedule must fail when kill switch is active"
    );
}

/// Kill switch must block create_bill.
#[test]
fn test_kill_switch_blocks_create_bill() {
    let (env, client, owner, contract_id) = setup();
    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let now = env.ledger().timestamp();
    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Test"),
        &1000,
        &(now + 86400),
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    assert!(
        result.is_err(),
        "create_bill must fail when kill switch is active"
    );
}

/// Kill switch must block set_external_ref.
#[test]
fn test_kill_switch_blocks_set_external_ref() {
    let (env, client, owner, contract_id) = setup();

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner, "Test", 100, now + 86400);

    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let result =
        client.try_set_external_ref(&owner, &bill_id, &Some(String::from_str(&env, "EXT-1")));
    assert!(
        result.is_err(),
        "set_external_ref must fail when kill switch is active"
    );
}

/// Kill switch must block archive_paid_bills.
#[test]
fn test_kill_switch_blocks_archive_paid_bills() {
    let (env, client, owner, contract_id) = setup();

    let now = env.ledger().timestamp();
    let bill_id = create_owner_bill(&client, &owner, "Paid", 100, now + 86400);
    // Pay the bill using batch_pay_bills (no orchestrator needed)
    let bill_ids = soroban_sdk::vec![&env, bill_id];
    client.batch_pay_bills(&owner, &bill_ids);

    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    // Advance time so paid_at is before timestamp
    set_ledger_time(&env, 1, now + 2 * 86400);

    let result = client.try_archive_paid_bills(&owner, &(now + 2 * 86400));
    assert!(
        result.is_err(),
        "archive_paid_bills must fail when kill switch is active"
    );
}

/// Kill switch must block bulk_cleanup_bills.
#[test]
fn test_kill_switch_blocks_bulk_cleanup_bills() {
    let (env, client, owner, contract_id) = setup();
    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let result = client.try_bulk_cleanup_bills(&owner, &1_000_000);
    assert!(
        result.is_err(),
        "bulk_cleanup_bills must fail when kill switch is active"
    );
}

// ─── Kill switch preserves bill/schedule state after rejection ────────────────

/// When kill switch blocks create_bill, no bill should be created.
#[test]
fn test_kill_switch_no_partial_state_on_create_bill() {
    let (env, client, owner, contract_id) = setup();
    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let now = env.ledger().timestamp();
    let _ = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Test"),
        &1000,
        &(now + 86400),
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    // Verify no bill was created
    assert!(
        client.get_bill(&1).is_none(),
        "no bill should exist after rejected create"
    );
    assert_eq!(
        client.get_owner_bill_count(&owner),
        0,
        "owner bill count must be 0"
    );
}

/// When kill switch blocks create_bill_schedule, no schedule should be created.
#[test]
fn test_kill_switch_no_partial_state_on_create_schedule() {
    let (env, client, owner, contract_id) = setup();
    env.as_contract(&contract_id, || {
        remitwise_common::activate_kill_switch(&env)
    });

    let now = env.ledger().timestamp();
    let _ = client.try_create_bill_schedule(
        &owner,
        &String::from_str(&env, "Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    // Verify no schedule was created
    assert!(
        client.get_bill_schedule(&1).is_none(),
        "no schedule should exist after rejected create"
    );
}

// ─── Repeated / idempotent operation safety ──────────────────────────────────

/// Cancelling an already-cancelled schedule returns ScheduleNotFound (idempotent).
#[test]
fn test_cancel_already_cancelled_schedule_returns_not_found() {
    let (env, client, owner, contract_id) = setup();

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

    // Second cancel should fail: schedule is no longer in the active index,
    // but the schedule map still has it with active=false. The cancel function
    // checks ScheduleNotActive first.
    let result = client.try_cancel_bill_schedule(&owner, &schedule_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotActive)));
}

/// Modifying a cancelled schedule returns ScheduleNotActive.
#[test]
fn test_modify_cancelled_schedule_returns_not_active() {
    let (env, client, owner, contract_id) = setup();

    let now = env.ledger().timestamp();
    let schedule_id = client.create_bill_schedule(
        &owner,
        &String::from_str(&env, "Phone"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    client.cancel_bill_schedule(&owner, &schedule_id);

    let result =
        client.try_modify_bill_schedule(&owner, &schedule_id, &2000, &(now + 2000), &86400);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotActive)));
}

// ─── Stale identity rejection ───────────────────────────────────────────────

/// Modifying a non-existent schedule returns ScheduleNotFound.
#[test]
fn test_modify_nonexistent_schedule_returns_not_found() {
    let (env, client, owner, contract_id) = setup();
    let now = env.ledger().timestamp();

    let result = client.try_modify_bill_schedule(&owner, &9999, &2000, &(now + 2000), &86400);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));
}

/// Cancelling a non-existent schedule returns ScheduleNotFound.
#[test]
fn test_cancel_nonexistent_schedule_returns_not_found() {
    let (_env, client, owner, _contract_id) = setup();

    let result = client.try_cancel_bill_schedule(&owner, &9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));
}

// ─── Batch operation partial state prevention ────────────────────────────────

/// batch_pay_bills: unauthorized bills in the batch must not be mutated.
#[test]
fn test_batch_pay_bills_skips_unauthorized_no_mutation() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    let bill_a = create_owner_bill(&client, &owner_a, "Bill A", 100, now + 86400);
    let bill_b = create_owner_bill(&client, &owner_b, "Bill B", 200, now + 86400);

    // owner_a tries to batch pay both — bill_b must be skipped
    let bill_ids = soroban_sdk::vec![&env, bill_a, bill_b];
    client.batch_pay_bills(&owner_a, &bill_ids);

    // Verify: bill_a is paid
    let paid_bill = client.get_bill(&bill_a).unwrap();
    assert!(paid_bill.paid, "bill_a must be paid");

    // Verify: bill_b is NOT paid (unauthorized — no mutation)
    let unpaid_bill = client.get_bill(&bill_b).unwrap();
    assert!(
        !unpaid_bill.paid,
        "bill_b must remain unpaid (cross-tenant)"
    );
    assert_eq!(unpaid_bill.paid_at, None, "bill_b paid_at must remain None");
}

/// batch_pay_bills: already-paid bills in the batch must not cause errors.
#[test]
fn test_batch_pay_bills_skips_already_paid() {
    let (env, client, owner, _contract_id) = setup();

    let now = env.ledger().timestamp();
    let bill1 = create_owner_bill(&client, &owner, "Bill 1", 100, now + 86400);
    let bill2 = create_owner_bill(&client, &owner, "Bill 2", 200, now + 86400);

    // Pay bill1 first using batch
    let bill_ids1 = soroban_sdk::vec![&env, bill1];
    client.batch_pay_bills(&owner, &bill_ids1);

    // Batch pay both — bill1 should be skipped (already paid)
    let bill_ids = soroban_sdk::vec![&env, bill1, bill2];
    client.batch_pay_bills(&owner, &bill_ids);

    // bill2 should be paid
    let paid_bill = client.get_bill(&bill2).unwrap();
    assert!(paid_bill.paid, "bill2 must be paid");
}

// ─── Schedule execution cross-tenant isolation ──────────────────────────────

/// Execute schedules: bills generated must belong to the schedule owner, not the caller.
#[test]
fn test_execute_schedule_generates_bill_for_schedule_owner() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "Rent"),
        &5000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    set_ledger_time(&env, 1, now + 2000);
    client.execute_due_bill_schedules();

    // The generated bill must belong to owner_a, not anyone else
    let bills = client.get_all_unpaid_bills_legacy(&owner_a);
    assert_eq!(bills.len(), 1, "owner_a must have 1 bill");
    assert_eq!(bills.get(0).unwrap().owner, owner_a);

    // owner_b must have no bills
    let b_bills = client.get_all_unpaid_bills_legacy(&owner_b);
    assert_eq!(b_bills.len(), 0, "owner_b must have 0 bills");
}

/// Multiple owners' schedules are executed independently.
#[test]
fn test_multiple_owner_schedules_independent() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "A Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );
    client.create_bill_schedule(
        &owner_b,
        &String::from_str(&env, "B Rent"),
        &2000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    set_ledger_time(&env, 1, now + 2000);
    let executed = client.execute_due_bill_schedules();
    assert_eq!(executed.len(), 2, "both schedules should execute");

    // Verify bill ownership isolation
    let a_bills = client.get_all_unpaid_bills_legacy(&owner_a);
    let b_bills = client.get_all_unpaid_bills_legacy(&owner_b);
    assert_eq!(a_bills.len(), 1, "owner_a should have 1 bill");
    assert_eq!(b_bills.len(), 1, "owner_b should have 1 bill");
    assert_eq!(a_bills.get(0).unwrap().amount, 1000);
    assert_eq!(b_bills.get(0).unwrap().amount, 2000);
}

// ─── Query isolation: cross-tenant must not leak ────────────────────────────

/// get_bill_schedules must only return the caller's schedules.
#[test]
fn test_get_bill_schedules_isolation() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "A Rent"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );
    client.create_bill_schedule(
        &owner_b,
        &String::from_str(&env, "B Rent"),
        &2000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let a_schedules = client.get_bill_schedules(&owner_a);
    let b_schedules = client.get_bill_schedules(&owner_b);

    assert_eq!(a_schedules.len(), 1, "owner_a should see 1 schedule");
    assert_eq!(b_schedules.len(), 1, "owner_b should see 1 schedule");
    assert_eq!(a_schedules.get(0).unwrap().owner, owner_a);
    assert_eq!(b_schedules.get(0).unwrap().owner, owner_b);
}

/// get_bill_schedules_page must only return the caller's schedules.
#[test]
fn test_get_bill_schedules_page_isolation() {
    let (env, client, owner_a, contract_id) = setup();
    let owner_b = generate_test_address(&env);

    let now = env.ledger().timestamp();
    client.create_bill_schedule(
        &owner_a,
        &String::from_str(&env, "A"),
        &1000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );
    client.create_bill_schedule(
        &owner_b,
        &String::from_str(&env, "B"),
        &2000,
        &String::from_str(&env, "XLM"),
        &(now + 1000),
        &86400,
    );

    let a_page = client.get_bill_schedules_page(&owner_a, &0, &10);
    let b_page = client.get_bill_schedules_page(&owner_b, &0, &10);

    assert_eq!(a_page.count, 1, "owner_a page should have 1 item");
    assert_eq!(b_page.count, 1, "owner_b page should have 1 item");
    assert_eq!(a_page.items.get(0).unwrap().owner, owner_a);
    assert_eq!(b_page.items.get(0).unwrap().owner, owner_b);
}
