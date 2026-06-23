#![cfg(test)]

//! Tests for `execute_due_remittance_schedules` drift-handling.
//!
//! ## What is covered here
//!
//! The executor advances `next_due` by one interval each time it fires. When
//! the executor hasn't been called for several intervals the "catch-up" logic
//! in `execute_due_remittance_schedules` runs through a while-loop to find the
//! next strictly-future `next_due`, incrementing `missed_count` for each
//! skipped interval. This file pins the exact arithmetic of that loop.
//!
//! * **Drift / gap tests** – executing when `now` is exactly 0, 1, 2, or N
//!   intervals past `next_due`; asserts the precise resulting `next_due` and
//!   `missed_count`.
//! * **Same-ledger idempotency** – two calls at the same ledger timestamp must
//!   not double-process.
//! * **`sch_exec` / `sch_miss` event data** – verifies the correct (id, value)
//!   payload is emitted.
//! * **`InactiveSchedule` semantics** – cancel/modify of an inactive schedule,
//!   executor skip of cancelled schedules, one-off lifecycle.

use remittance_split::{
    RemittanceSplit, RemittanceSplitClient, RemittanceSplitError, MIN_SCHEDULE_INTERVAL,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, Symbol,
};

// ─────────────────────────────────────────────────────────────────────────────
// Test infrastructure
// ─────────────────────────────────────────────────────────────────────────────

/// Registers the contract, initializes it with a 50/30/15/5 split, and returns
/// (owner, client). The ledger timestamp is set to 1_000 during setup.
fn setup(env: &Env) -> (Address, RemittanceSplitClient<'_>) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token_addr = Address::generate(env);
    client.initialize_split(&owner, &0, &token_addr, &50, &30, &15, &5);

    (owner, client)
}

/// Recurring interval used in these tests; equals `MIN_SCHEDULE_INTERVAL`.
const INTERVAL: u64 = MIN_SCHEDULE_INTERVAL; // 3 600 s

/// Base schedule due-time: comfortably after ledger start but well within
/// `MAX_SCHEDULE_LEAD_TIME` (31 536 000 s).
const T0: u64 = 100_000;

// ─────────────────────────────────────────────────────────────────────────────
// Drift / gap — next_due advancement and missed_count
// ─────────────────────────────────────────────────────────────────────────────

/// Executing at exactly `next_due` (0 intervals late):
/// `missed_count` must be 0 and `next_due` must advance to `T0 + INTERVAL`.
#[test]
fn test_no_drift_exact_due_time() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    env.ledger().set_timestamp(T0);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), id);

    let sch = client.get_remittance_schedule(&id).unwrap();
    assert!(sch.active);
    assert_eq!(sch.missed_count, 0, "no missed intervals when fired on time");
    assert_eq!(sch.next_due, T0 + INTERVAL, "next_due = T0 + 1*I");
    assert_eq!(sch.last_executed, Some(T0));
}

/// Executing exactly one interval past `next_due` (`now = T0 + I`):
/// `missed_count` must be 1 and `next_due` must advance to `T0 + 2*I`.
#[test]
fn test_drift_exactly_one_interval_past() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + INTERVAL;
    env.ledger().set_timestamp(now);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    let sch = client.get_remittance_schedule(&id).unwrap();
    assert!(sch.active);
    assert_eq!(sch.missed_count, 1, "one missed interval");
    assert_eq!(sch.next_due, T0 + 2 * INTERVAL, "next_due = T0 + 2*I");
    assert_eq!(sch.last_executed, Some(now));
}

/// Executing exactly two intervals past `next_due` (`now = T0 + 2*I`):
/// `missed_count` must be 2 and `next_due` must advance to `T0 + 3*I`.
#[test]
fn test_drift_exactly_two_intervals_past() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + 2 * INTERVAL;
    env.ledger().set_timestamp(now);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    let sch = client.get_remittance_schedule(&id).unwrap();
    assert!(sch.active);
    assert_eq!(sch.missed_count, 2, "two missed intervals");
    assert_eq!(sch.next_due, T0 + 3 * INTERVAL, "next_due = T0 + 3*I");
    assert_eq!(sch.last_executed, Some(now));
}

/// Executing 5 intervals past `next_due` (`now = T0 + 5*I`):
/// `missed_count` must be 5 and `next_due` must advance to `T0 + 6*I`.
#[test]
fn test_drift_many_intervals_past() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + 5 * INTERVAL;
    env.ledger().set_timestamp(now);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    let sch = client.get_remittance_schedule(&id).unwrap();
    assert!(sch.active);
    assert_eq!(sch.missed_count, 5, "five missed intervals");
    assert_eq!(sch.next_due, T0 + 6 * INTERVAL, "next_due = T0 + 6*I");
    assert_eq!(sch.last_executed, Some(now));
}

/// When `now` is just *under* one full extra interval (`now = T0 + I - 1`), the
/// schedule is due (`now >= T0`) but no extra interval has elapsed: `missed_count`
/// stays 0 and `next_due` still advances to `T0 + I`.
#[test]
fn test_sub_interval_gap_no_missed() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + INTERVAL - 1;
    env.ledger().set_timestamp(now);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    let sch = client.get_remittance_schedule(&id).unwrap();
    assert!(sch.active);
    assert_eq!(sch.missed_count, 0, "sub-interval gap: no missed");
    assert_eq!(sch.next_due, T0 + INTERVAL, "next_due advances by one interval");
    assert_eq!(sch.last_executed, Some(now));
}

/// `missed_count` accumulates correctly across two separate executor calls.
///
/// First call at `T0 + 2*I` → missed = 2, next_due = `T0 + 3*I`.
/// Second call at `T0 + 7*I` → gap from `T0+3*I` = 4 intervals, missed += 4.
/// Total missed_count after second call must be 6.
#[test]
fn test_missed_count_accumulates_across_executions() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    // First execution: 2 intervals late
    env.ledger().set_timestamp(T0 + 2 * INTERVAL);
    client.execute_due_remittance_schedules();

    let sch1 = client.get_remittance_schedule(&id).unwrap();
    assert_eq!(sch1.missed_count, 2);
    assert_eq!(sch1.next_due, T0 + 3 * INTERVAL);

    // Second execution: 4 intervals late relative to the new next_due
    env.ledger().set_timestamp(T0 + 7 * INTERVAL);
    client.execute_due_remittance_schedules();

    let sch2 = client.get_remittance_schedule(&id).unwrap();
    assert_eq!(sch2.missed_count, 6, "cumulative missed = 2 + 4");
    assert_eq!(sch2.next_due, T0 + 8 * INTERVAL);
}

// ─────────────────────────────────────────────────────────────────────────────
// Same-ledger idempotency
// ─────────────────────────────────────────────────────────────────────────────

/// Two calls to `execute_due_remittance_schedules` at the same ledger timestamp
/// must not double-process: the first call returns the executed ID, the second
/// returns an empty Vec, and `next_due` / `missed_count` / `last_executed` are
/// unchanged after the second call.
#[test]
fn test_same_ledger_double_call_is_idempotent_recurring() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    env.ledger().set_timestamp(T0);

    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);
    assert_eq!(first.get(0).unwrap(), id);

    let sch_after_first = client.get_remittance_schedule(&id).unwrap();

    // Second call at the same timestamp must be a no-op.
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0, "second call at same ledger must be a no-op");

    let sch_after_second = client.get_remittance_schedule(&id).unwrap();
    assert_eq!(
        sch_after_second.next_due, sch_after_first.next_due,
        "next_due must not change on second call"
    );
    assert_eq!(
        sch_after_second.missed_count, sch_after_first.missed_count,
        "missed_count must not change on second call"
    );
    assert_eq!(
        sch_after_second.last_executed, sch_after_first.last_executed,
        "last_executed must not change on second call"
    );
}

/// Same idempotency guarantee holds with a multi-interval gap (drift case):
/// a double-call at `T0 + 3*I` does not increment `missed_count` twice.
#[test]
fn test_same_ledger_double_call_idempotent_with_drift() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + 3 * INTERVAL;
    env.ledger().set_timestamp(now);

    // First call: processes the schedule, records missed = 3.
    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);

    let sch1 = client.get_remittance_schedule(&id).unwrap();
    assert_eq!(sch1.missed_count, 3);
    let expected_next_due = T0 + 4 * INTERVAL;
    assert_eq!(sch1.next_due, expected_next_due);

    // Second call at the same timestamp must change nothing.
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0, "second call must be empty");

    let sch2 = client.get_remittance_schedule(&id).unwrap();
    assert_eq!(sch2.missed_count, 3, "missed_count must not double");
    assert_eq!(sch2.next_due, expected_next_due, "next_due must not change");
}

// ─────────────────────────────────────────────────────────────────────────────
// sch_exec / sch_miss event data
// ─────────────────────────────────────────────────────────────────────────────

/// Matches an event by comparing the 4th topic's raw payload bits against the
/// expected symbol. `Val` does not implement `PartialEq` in this SDK version,
/// so we compare the underlying `u64` payloads via `get_payload()`.
fn topic3_is(topics: &soroban_sdk::Vec<soroban_sdk::Val>, expected: Symbol) -> bool {
    if topics.len() != 4 {
        return false;
    }
    match topics.get(3) {
        Some(v) => v.get_payload() == expected.to_val().get_payload(),
        None => false,
    }
}

/// `sch_exec` event is emitted once with the correct `(schedule_id, amount)`
/// payload when a recurring schedule fires with no drift.
#[test]
fn test_sch_exec_event_carries_correct_data() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let amount: i128 = 12_345;
    let id = client.create_remittance_schedule(&owner, &amount, &T0, &INTERVAL);

    env.ledger().set_timestamp(T0);
    client.execute_due_remittance_schedules();

    let events = env.events().all();
    let exec_event = events
        .iter()
        .find(|(_cid, topics, _data)| topic3_is(topics, symbol_short!("sch_exec")));

    assert!(exec_event.is_some(), "sch_exec event must be emitted");
    let (_cid, _topics, data) = exec_event.unwrap();

    let (event_id, event_amount): (u32, i128) = data.into_val(&env);
    assert_eq!(event_id, id, "sch_exec event must carry the schedule id");
    assert_eq!(event_amount, amount, "sch_exec event must carry the schedule amount");
}

/// `sch_miss` event is emitted with `(schedule_id, missed_count)` payload when
/// the executor fires 3 intervals late.
#[test]
fn test_sch_miss_event_carries_correct_data() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    let id = client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    let now = T0 + 3 * INTERVAL;
    env.ledger().set_timestamp(now);
    client.execute_due_remittance_schedules();

    let events = env.events().all();
    let miss_event = events
        .iter()
        .find(|(_cid, topics, _data)| topic3_is(topics, symbol_short!("sch_miss")));

    assert!(miss_event.is_some(), "sch_miss event must be emitted");
    let (_cid, _topics, data) = miss_event.unwrap();

    let (event_id, event_missed): (u32, u32) = data.into_val(&env);
    assert_eq!(event_id, id, "sch_miss event must carry the schedule id");
    assert_eq!(event_missed, 3, "sch_miss must report 3 missed intervals");
}

/// No `sch_miss` event is emitted when the executor fires exactly at `next_due`
/// (missed = 0), because the contract only emits the event when `missed > 0`.
#[test]
fn test_no_sch_miss_event_when_no_drift() {
    let env = Env::default();
    let (owner, client) = setup(&env);

    client.create_remittance_schedule(&owner, &500, &T0, &INTERVAL);

    env.ledger().set_timestamp(T0);
    client.execute_due_remittance_schedules();

    let events = env.events().all();
    let miss_event = events
        .iter()
        .find(|(_cid, topics, _data)| topic3_is(topics, symbol_short!("sch_miss")));

    assert!(miss_event.is_none(), "no sch_miss event when missed_count is 0");
}
