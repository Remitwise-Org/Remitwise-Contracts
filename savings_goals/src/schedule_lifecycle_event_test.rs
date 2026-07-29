//! Functional tests locking down the *emitted* topics/payloads for
//! goal-lock and savings-schedule lifecycle events.
//!
//! `events_schema_test.rs` proves each event struct's field set is stable
//! (compile-time struct literals + `Val` round-trip). This module goes one
//! step further and drives the real contract entrypoints end-to-end,
//! decoding the events Soroban actually recorded, so a regression that
//! swaps in the wrong values (or reverts to a bare, undocumented tuple) is
//! caught even if the struct shape itself is untouched.

#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short, testutils::Address as AddressTrait, testutils::Events, Address, Env, String,
    Symbol, TryFromVal,
};
use testutils::set_ledger_time;

fn setup(env: &Env) -> (SavingsGoalContractClient<'_>, Address) {
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(env, &contract_id);
    env.mock_all_auths();
    client.init();
    set_ledger_time(env, 1, 1_000);
    let owner = Address::generate(env);
    (client, owner)
}

fn make_goal(env: &Env, client: &SavingsGoalContractClient, owner: &Address, target: i128) -> u32 {
    client.create_goal(
        owner,
        &String::from_str(env, "Test Goal"),
        &target,
        &2_000_000_000u64,
        &false,
    )
}

/// Finds the single most-recently-emitted event matching the given
/// `SavingsEvent` variant and decodes its payload as `T`.
fn find_latest_event<T: TryFromVal<Env, soroban_sdk::Val>>(
    env: &Env,
    variant_matches: impl Fn(&SavingsEvent) -> bool,
) -> T {
    let all = env.events().all();
    let matched = all
        .iter()
        .rev()
        .find(|(_, topics, _)| {
            let t0_ok = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(env, &t).ok())
                .map(|s: Symbol| s == symbol_short!("savings"))
                .unwrap_or(false);
            let t1_ok = topics
                .get(1)
                .and_then(|t| SavingsEvent::try_from_val(env, &t).ok())
                .map(|e| variant_matches(&e))
                .unwrap_or(false);
            t0_ok && t1_ok
        })
        .unwrap_or_else(|| panic!("no matching event found"));

    T::try_from_val(env, &matched.2).expect("payload failed to decode as expected event struct")
}

// ─── Goal lock / unlock ─────────────────────────────────────────────────────

#[test]
fn lock_goal_emits_goal_lock_event_with_locked_true_and_owner_and_timestamp() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 1_000);

    set_ledger_time(&env, 2, 5_000);
    client.lock_goal(&owner, &goal_id);

    let evt: GoalLockEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::GoalLocked));

    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert!(evt.locked);
    assert_eq!(evt.timestamp, 5_000);
}

#[test]
fn unlock_goal_emits_goal_lock_event_with_locked_false() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 1_000);

    client.lock_goal(&owner, &goal_id);
    set_ledger_time(&env, 3, 6_000);
    client.unlock_goal(&owner, &goal_id);

    let evt: GoalLockEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::GoalUnlocked));

    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert!(!evt.locked);
    assert_eq!(evt.timestamp, 6_000);
}

// ─── Schedule create / modify / cancel ──────────────────────────────────────

#[test]
fn create_savings_schedule_emits_schedule_created_event_with_full_payload() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 10_000);

    set_ledger_time(&env, 2, 10_000);
    let schedule_id =
        client.create_savings_schedule(&owner, &goal_id, &500_i128, &(10_000 + 86_400), &86_400);

    let evt: ScheduleCreatedEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::ScheduleCreated));

    assert_eq!(evt.schedule_id, schedule_id);
    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert_eq!(evt.amount, 500);
    assert_eq!(evt.next_due, 10_000 + 86_400);
    assert_eq!(evt.interval, 86_400);
    assert_eq!(evt.timestamp, 10_000);
}

#[test]
fn modify_savings_schedule_emits_schedule_modified_event_with_new_values() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 10_000);

    let schedule_id =
        client.create_savings_schedule(&owner, &goal_id, &500_i128, &(1_000 + 86_400), &86_400);

    set_ledger_time(&env, 2, 20_000);
    client.modify_savings_schedule(&owner, &schedule_id, &750_i128, &(20_000 + 604_800), &604_800);

    let evt: ScheduleModifiedEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::ScheduleModified));

    assert_eq!(evt.schedule_id, schedule_id);
    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert_eq!(evt.amount, 750);
    assert_eq!(evt.next_due, 20_000 + 604_800);
    assert_eq!(evt.interval, 604_800);
    assert_eq!(evt.timestamp, 20_000);
}

#[test]
fn cancel_savings_schedule_emits_schedule_cancelled_event() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 10_000);

    let schedule_id =
        client.create_savings_schedule(&owner, &goal_id, &500_i128, &(1_000 + 86_400), &86_400);

    set_ledger_time(&env, 2, 30_000);
    client.cancel_savings_schedule(&owner, &schedule_id);

    let evt: ScheduleCancelledEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::ScheduleCancelled));

    assert_eq!(evt.schedule_id, schedule_id);
    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert_eq!(evt.timestamp, 30_000);
}

// ─── Schedule execution: executed / missed, and consistent FundsAdded ──────

#[test]
fn execute_due_savings_schedules_emits_schedule_executed_event_with_full_payload() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 10_000);

    let schedule_id =
        client.create_savings_schedule(&owner, &goal_id, &500_i128, &2_000u64, &86_400);

    set_ledger_time(&env, 2, 2_000);
    client.execute_due_savings_schedules();

    let evt: ScheduleExecutedEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::ScheduleExecuted));

    assert_eq!(evt.schedule_id, schedule_id);
    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert_eq!(evt.amount, 500);
    assert_eq!(evt.timestamp, 2_000);
}

#[test]
fn execute_due_savings_schedules_emits_schedule_missed_event_when_intervals_skipped() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 1_000_000);

    let schedule_id =
        client.create_savings_schedule(&owner, &goal_id, &500_i128, &2_000u64, &1_000);

    // Jump far enough ahead that several 1_000-second intervals are skipped.
    set_ledger_time(&env, 2, 6_500);
    client.execute_due_savings_schedules();

    let evt: ScheduleMissedEvent =
        find_latest_event(&env, |e| matches!(e, SavingsEvent::ScheduleMissed));

    assert_eq!(evt.schedule_id, schedule_id);
    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert!(evt.missed_count > 0);
    assert_eq!(evt.timestamp, 6_500);
}

/// Regression test: `execute_due_savings_schedules` must publish the
/// standardized `FundsAddedEvent` (via `RemitwiseEvents::emit`, topic
/// `(Remitwise, category, priority, "funds_add")`) with the same struct
/// shape - including `new_total` and `timestamp` - as the manual
/// `add_to_goal` path. Before this fix, the scheduled-execution path only
/// published a bare `(goal_id, owner, amount)` tuple under the legacy
/// `(savings, SavingsEvent::FundsAdded)` topic, missing `new_total` and
/// `timestamp` entirely for indexers subscribed to the standardized event.
#[test]
fn execute_due_savings_schedules_funds_added_event_carries_new_total_and_timestamp() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 10_000);

    client.create_savings_schedule(&owner, &goal_id, &500_i128, &2_000u64, &86_400);

    set_ledger_time(&env, 2, 2_000);
    client.execute_due_savings_schedules();

    let all = env.events().all();
    let matched = all
        .iter()
        .rev()
        .find(|(_, topics, _)| {
            let t0_ok = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(&env, &t).ok())
                .map(|s: Symbol| s == symbol_short!("Remitwise"))
                .unwrap_or(false);
            let t3_ok = topics
                .get(3)
                .and_then(|t| Symbol::try_from_val(&env, &t).ok())
                .map(|s: Symbol| s == FUNDS_ADDED)
                .unwrap_or(false);
            t0_ok && t3_ok
        })
        .expect("no standardized FundsAdded event found");

    let evt = FundsAddedEvent::try_from_val(&env, &matched.2)
        .expect("FundsAdded payload must decode as FundsAddedEvent");

    assert_eq!(evt.goal_id, goal_id);
    assert_eq!(evt.owner, owner);
    assert_eq!(evt.amount, 500);
    assert_eq!(evt.new_total, 500);
    assert_eq!(evt.timestamp, 2_000);
}

/// Regression test: a goal that is already completed before a scheduled
/// credit lands must NOT re-emit `GoalCompleted`. Before this fix,
/// `execute_due_savings_schedules` compared `current_amount` *after* the
/// credit was applied against `target_amount` with no "was it already
/// completed" guard, so a goal that stayed at or above target across
/// multiple executions would re-fire `GoalCompleted` every single time.
#[test]
fn execute_due_savings_schedules_does_not_reemit_goal_completed_for_already_completed_goal() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    // Target is reached by the very first scheduled credit.
    let goal_id = make_goal(&env, &client, &owner, 500);

    client.create_savings_schedule(&owner, &goal_id, &500_i128, &2_000u64, &1_000);

    set_ledger_time(&env, 2, 2_000);
    client.execute_due_savings_schedules();
    assert!(client.is_goal_completed(&goal_id));

    let completed_after_first = count_goal_completed_events(&env, &owner, goal_id);
    assert_eq!(completed_after_first, 1);

    // Second due execution: the goal is already completed, but the
    // recurring schedule still credits it further.
    set_ledger_time(&env, 3, 3_000);
    client.execute_due_savings_schedules();

    let completed_after_second = count_goal_completed_events(&env, &owner, goal_id);
    assert_eq!(
        completed_after_second, 1,
        "GoalCompleted must not be re-emitted once a goal is already completed"
    );
}

fn count_goal_completed_events(env: &Env, owner: &Address, goal_id: u32) -> usize {
    env.events()
        .all()
        .iter()
        .filter(|(_, topics, data)| {
            let t0_ok = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(env, &t).ok())
                .map(|s: Symbol| s == symbol_short!("savings"))
                .unwrap_or(false);
            let t1_ok = topics
                .get(1)
                .and_then(|t| SavingsEvent::try_from_val(env, &t).ok())
                .map(|e| matches!(e, SavingsEvent::GoalCompleted))
                .unwrap_or(false);
            if !(t0_ok && t1_ok) {
                return false;
            }
            // The legacy tuple payload is `(goal_id, owner)`; only count
            // completions for this specific goal/owner pair.
            <(u32, Address)>::try_from_val(env, data)
                .map(|(g, o)| g == goal_id && &o == owner)
                .unwrap_or(false)
        })
        .count()
}
