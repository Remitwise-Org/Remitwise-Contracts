#![cfg(test)]

/// # Exactly-once delivery invariant for `execute_due_savings_schedules`
///
/// The executor is permissionless and may be invoked multiple times within the
/// same Stellar ledger (which shares a single ledger timestamp), or retried
/// after a transient failure. The critical invariant is:
///
/// - A schedule that becomes due credits its linked goal's `current_amount` by
///   exactly `schedule.amount` on the **first** execution within that due window.
/// - Subsequent calls in the same ledger (same timestamp) must NOT credit the
///   goal again — for recurring schedules the `next_due > current_time` guard
///   blocks re-entry after `next_due` is advanced; for one-shot schedules the
///   `active = false` flag blocks re-entry.
/// - Multiple distinct schedules targeting the same goal each contribute their
///   own `amount` in one executor pass; amounts accumulate with checked
///   arithmetic so balances near `MAX_SAFE_GOAL_BALANCE` do not overflow.
///
/// ## Locked-goal behavior
/// The executor does NOT check `goal.locked`. Locking a goal only prevents
/// manual withdrawals via `withdraw_from_goal`. Scheduled credits to a locked
/// goal proceed normally — this is intentional and allows automated saving to
/// continue even when the goal is protected from withdrawal.
///
/// ## Archived-goal behavior (documented gap)
/// When a goal is archived via `archive_goal`, it is moved from
/// `DataKey::Goal` to `DataKey::ArchivedGoal`. The executor looks up
/// `DataKey::Goal`, finds `None`, and silently `continue`s — the scheduled
/// credit is **dropped** and `missed_count` is **not** incremented for that
/// skip. Callers should cancel outstanding schedules before archiving their
/// linked goals to avoid silent credit drops.
extern crate std;

use super::*;
use soroban_sdk::{
    symbol_short, testutils::Address as AddressTrait, Address, Env, String, Symbol, TryFromVal,
};
use testutils::set_ledger_time;

// ─── shared helpers ───────────────────────────────────────────────────────────

fn setup(env: &Env) -> (SavingsGoalContractClient, Address) {
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

fn count_events_matching(env: &Env, check: impl Fn(&SavingsEvent) -> bool) -> usize {
    soroban_sdk::testutils::Events::all(&env.events())
        .iter()
        .filter(|ev| {
            let topics = ev.1.clone();
            let t0_ok = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(env, &t).ok())
                .map(|s: Symbol| s == symbol_short!("savings"))
                .unwrap_or(false);
            let t1_ok = topics
                .get(1)
                .and_then(|t| SavingsEvent::try_from_val(env, &t).ok())
                .map(|e| check(&e))
                .unwrap_or(false);
            t0_ok && t1_ok
        })
        .count()
}

// ─── 1. Single due schedule credits exactly once ──────────────────────────────

/// A due one-shot schedule credits `current_amount` by exactly `amount` and
/// returns the schedule ID in the executed list.
#[test]
fn test_single_due_schedule_credits_exactly_once() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(executed.len(), 1, "exactly one schedule should execute");
    assert_eq!(executed.get(0).unwrap(), sched_id);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 500,
        "goal must be credited exactly once"
    );
}

/// Calling `execute_due_savings_schedules` a second time within the same ledger
/// (identical timestamp) must not double-credit a one-shot schedule.
/// After the first execution the schedule is marked inactive, so the second
/// call finds `!schedule.active` and skips it.
#[test]
fn test_no_double_credit_same_ledger_one_shot() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let first = client.execute_due_savings_schedules();
    assert_eq!(first.len(), 1, "first call must execute the schedule");

    // Same ledger, same timestamp — must not re-execute.
    let second = client.execute_due_savings_schedules();
    assert_eq!(
        second.len(),
        0,
        "second call in same ledger must not execute any schedule"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 500,
        "current_amount must not double after idempotent second call"
    );
}

/// Calling `execute_due_savings_schedules` a second time within the same ledger
/// must not double-credit a **recurring** schedule.
/// After the first execution `next_due` is advanced beyond `current_time`, so
/// the second call is blocked by the `next_due > current_time` guard.
#[test]
fn test_no_double_credit_same_ledger_recurring() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 10_000);
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &86_400);

    set_ledger_time(&env, 2, 3_500);
    let first = client.execute_due_savings_schedules();
    assert_eq!(first.len(), 1);

    let second = client.execute_due_savings_schedules();
    assert_eq!(
        second.len(),
        0,
        "recurring schedule must not re-execute within the same ledger"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 500,
        "no double-credit for recurring schedule in same ledger"
    );

    // Confirm next_due was advanced into the future so the guard holds.
    let sched = client.get_savings_schedule(&sched_id).unwrap();
    assert!(
        sched.next_due > 3_500,
        "next_due must be in the future after execution"
    );
}

// ─── 2. Multiple schedules for the same goal aggregate correctly ──────────────

/// Two schedules targeting the same goal both execute in one pass, and their
/// amounts accumulate with correct arithmetic.
#[test]
fn test_two_schedules_same_goal_aggregate() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 10_000);
    let sched1 = client.create_savings_schedule(&owner, &goal_id, &300, &3_000, &0);
    let sched2 = client.create_savings_schedule(&owner, &goal_id, &700, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(executed.len(), 2, "both schedules must execute");
    assert!(
        executed.iter().any(|id| id == sched1),
        "sched1 must be in executed list"
    );
    assert!(
        executed.iter().any(|id| id == sched2),
        "sched2 must be in executed list"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 1_000,
        "two schedules must aggregate: 300 + 700 = 1000"
    );
}

/// A schedule whose amount would push `current_amount` past `MAX_SAFE_GOAL_BALANCE`
/// is silently skipped by the executor (`checked_add` overflows → `continue`).
/// The goal balance must remain unchanged.
#[test]
fn test_overflow_near_limit_schedule_skipped() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let cap = i128::MAX / 2; // == MAX_SAFE_GOAL_BALANCE
    let goal_id = make_goal(&env, &client, &owner, cap);

    // Pre-fill to one unit below the safe cap.
    client.add_to_goal(&owner, &goal_id, &(cap - 1));

    // Schedule: amount=10 → new_total = cap - 1 + 10 = cap + 9 > cap → skipped.
    client.create_savings_schedule(&owner, &goal_id, &10, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        0,
        "overflow schedule must be silently skipped"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount,
        cap - 1,
        "goal balance must be unchanged when schedule is skipped due to overflow"
    );
}

/// When two schedules target the same goal and the first brings `current_amount`
/// exactly to `MAX_SAFE_GOAL_BALANCE`, the second is skipped without error.
/// Checked arithmetic must not allow the balance to exceed the safe cap.
#[test]
fn test_second_schedule_skipped_when_first_fills_to_cap() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let cap = i128::MAX / 2;
    let goal_id = make_goal(&env, &client, &owner, cap);

    // Pre-fill to (cap - 5) via add_to_goal so the first schedule can finish filling.
    client.add_to_goal(&owner, &goal_id, &(cap - 5));

    // Schedule 1: amount=5 → total becomes cap (exactly at limit, allowed).
    let sched1 = client.create_savings_schedule(&owner, &goal_id, &5, &3_000, &0);
    // Schedule 2: amount=1 → total becomes cap+1 > MAX_SAFE_GOAL_BALANCE → skipped.
    let _sched2 = client.create_savings_schedule(&owner, &goal_id, &1, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        1,
        "only the first schedule (safe amount) must execute"
    );
    assert_eq!(
        executed.get(0).unwrap(),
        sched1,
        "only sched1 must be in executed list"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, cap,
        "balance must be exactly at cap after first schedule; second must be skipped"
    );
}

// ─── 3. Locked-goal behavior ──────────────────────────────────────────────────

/// A **locked** goal still receives scheduled credits: the executor does not
/// check `goal.locked`. Goals are locked by default on creation.
/// This test intentionally leaves the goal locked and confirms the credit lands.
///  FIND:
/// Goals are locked by default on creation.
// REPLACE:
/// The goal is explicitly locked before the schedule runs.
#[test]
fn test_locked_goal_receives_scheduled_credit() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let goal_id = make_goal(&env, &client, &owner, 2_000);
    client.lock_goal(&owner, &goal_id);
    let locked_goal = client.get_goal(&goal_id).unwrap();
    assert!(
        locked_goal.locked,
        "goal must be locked for this test to be meaningful"
    );

    client.create_savings_schedule(&owner, &goal_id, &400, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        1,
        "schedule for a locked goal must still execute"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 400,
        "locked goal must receive the scheduled credit"
    );
    assert!(
        goal.locked,
        "goal must remain locked after scheduler credit"
    );
}

// ─── 4. Archived-goal behavior (documented gap) ───────────────────────────────

/// When the linked goal has been archived (removed from `DataKey::Goal`),
/// the executor silently skips the schedule: the credit is dropped and
/// `missed_count` is NOT incremented. This is a known behavioral gap —
/// callers should cancel schedules before archiving goals.
#[test]
fn test_archived_goal_schedule_skipped_missed_count_not_incremented() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // Create and complete a goal so it is eligible for archival.
    let goal_id = make_goal(&env, &client, &owner, 500);
    client.add_to_goal(&owner, &goal_id, &500);

    // Create a schedule while the goal is still active.
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &100, &3_000, &86_400);

    // Archive the goal — removes it from DataKey::Goal storage.
    client.archive_goal(&owner, &goal_id);
    assert!(
        client.get_goal(&goal_id).is_none(),
        "goal must be in archived storage, not active"
    );

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        0,
        "schedule targeting an archived goal must be silently skipped"
    );

    let sched = client.get_savings_schedule(&sched_id).unwrap();
    assert_eq!(
        sched.missed_count, 0,
        "missed_count must NOT be incremented when the goal is archived \
         (executor silently continues without updating the schedule)"
    );
}

// ─── 5. GoalCompleted event fires when scheduled credit crosses target_amount ──

/// `SavingsEvent::GoalCompleted` must fire when a scheduled credit brings
/// `current_amount` to or above `target_amount`.
#[test]
fn test_goal_completed_event_fires_on_scheduled_credit() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 1_000);
    client.create_savings_schedule(&owner, &goal_id, &1_000, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    assert!(
        client.is_goal_completed(&goal_id),
        "goal must be marked complete after the scheduled credit reaches target"
    );

    let completed = count_events_matching(&env, |e| matches!(e, SavingsEvent::GoalCompleted));
    assert!(
        completed >= 1,
        "SavingsEvent::GoalCompleted must be emitted when a scheduled credit reaches target"
    );
}

/// `GoalCompleted` must NOT fire when a scheduled credit does not reach the target.
#[test]
fn test_goal_completed_event_not_fired_when_target_not_reached() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    // Amount 500 is below target 2000.
    client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    assert!(
        !client.is_goal_completed(&goal_id),
        "goal must not be complete when credit is below target"
    );

    let completed = count_events_matching(&env, |e| matches!(e, SavingsEvent::GoalCompleted));
    assert_eq!(
        completed, 0,
        "GoalCompleted must not be emitted when credit is still below target"
    );
}

/// When two schedules target the same goal and their aggregate crosses the
/// target, exactly one `GoalCompleted` event fires (from the schedule that
/// pushes the balance over the line).
#[test]
fn test_goal_completed_fires_once_when_aggregate_crosses_target() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    // Goal target = 1000; schedule 1 contributes 600 (not enough alone),
    // schedule 2 contributes 500 (aggregate = 1100, crosses target).
    let goal_id = make_goal(&env, &client, &owner, 1_000);
    client.create_savings_schedule(&owner, &goal_id, &600, &3_000, &0);
    client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 1_100);

    let completed = count_events_matching(&env, |e| matches!(e, SavingsEvent::GoalCompleted));
    assert_eq!(
        completed, 1,
        "GoalCompleted must fire exactly once — from the schedule that crosses the target"
    );
}

// ─── 6. ScheduleExecuted event is emitted ─────────────────────────────────────

/// `SavingsEvent::ScheduleExecuted` must be emitted (with the schedule ID as
/// data) for every successfully executed schedule.
#[test]
fn test_schedule_executed_event_emitted_with_correct_id() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 5_000);
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &200, &3_000, &86_400);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    let events = soroban_sdk::testutils::Events::all(&env.events());
    let mut found = false;
    for ev in events.iter() {
        let topics = ev.1;
        let t0_ok = topics
            .get(0)
            .and_then(|t| Symbol::try_from_val(&env, &t).ok())
            .map(|s: Symbol| s == symbol_short!("savings"))
            .unwrap_or(false);
        let t1_ok = topics
            .get(1)
            .and_then(|t| SavingsEvent::try_from_val(&env, &t).ok())
            .map(|e| matches!(e, SavingsEvent::ScheduleExecuted))
            .unwrap_or(false);

        if t0_ok && t1_ok {
            let data_id: u32 = u32::try_from_val(&env, &ev.2).unwrap();
            assert_eq!(
                data_id, sched_id,
                "ScheduleExecuted event data must carry the schedule ID"
            );
            found = true;
        }
    }
    assert!(
        found,
        "SavingsEvent::ScheduleExecuted must be emitted for an executed schedule"
    );
}

/// `SavingsEvent::FundsAdded` is emitted once per executed schedule, with the
/// correct goal ID and amount.
#[test]
fn test_funds_added_event_emitted_per_executed_schedule() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 5_000);
    client.create_savings_schedule(&owner, &goal_id, &300, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    let funds_added = count_events_matching(&env, |e| matches!(e, SavingsEvent::FundsAdded));
    assert!(
        funds_added >= 1,
        "SavingsEvent::FundsAdded must be emitted for each scheduled credit"
    );
}

// ─── 7. Schedule due exactly at current ledger timestamp ──────────────────────

/// A schedule with `next_due == current_time` is considered due (`<=` check),
/// not future. It must execute.
#[test]
fn test_schedule_due_exactly_at_current_time_executes() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    client.create_savings_schedule(&owner, &goal_id, &250, &3_000, &0);

    // Advance ledger to exactly `next_due`.
    set_ledger_time(&env, 2, 3_000);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        1,
        "schedule must execute when current_time == next_due (boundary condition)"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 250);
}

/// A schedule with `next_due` one second in the future must NOT execute.
#[test]
fn test_schedule_not_yet_due_does_not_execute() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    client.create_savings_schedule(&owner, &goal_id, &250, &3_000, &0);

    // One second before next_due.
    set_ledger_time(&env, 2, 2_999);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        0,
        "schedule must NOT execute when current_time < next_due"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 0,
        "goal balance must be unchanged when no schedule is due"
    );
}

// ─── 8. Missed interval tracking ──────────────────────────────────────────────

/// When execution is delayed past multiple intervals, `missed_count` is
/// incremented for each skipped interval and `next_due` advances to the
/// next future slot. Only a single credit is applied.
#[test]
fn test_missed_count_increments_for_skipped_intervals() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 100_000);
    // next_due=3000, interval=1000; execute at t=6500.
    // Intervals skipped: 4000, 5000, 6000 (all ≤ 6500) → 3 missed.
    // next_due advances to 7000.
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &1_000);

    set_ledger_time(&env, 2, 6_500);
    client.execute_due_savings_schedules();

    let sched = client.get_savings_schedule(&sched_id).unwrap();
    assert_eq!(sched.missed_count, 3, "three intervals were skipped");
    assert_eq!(
        sched.next_due, 7_000,
        "next_due must be at the next future slot"
    );

    // Only one credit is applied regardless of how many intervals were missed.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 500,
        "only one credit is applied; missed intervals do not backfill"
    );
}

/// `missed_count` accumulates correctly across multiple executor passes.
#[test]
fn test_missed_count_accumulates_across_multiple_passes() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 100_000);
    // next_due=2000, interval=1000
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &100, &2_000, &1_000);

    // Pass 1 at t=5000:
    // Skipped intervals: 3000, 4000, 5000 (all ≤ 5000) → 3 missed.
    // next_due → 6000.
    set_ledger_time(&env, 2, 5_000);
    client.execute_due_savings_schedules();
    let s1 = client.get_savings_schedule(&sched_id).unwrap();
    assert_eq!(s1.missed_count, 3);
    assert_eq!(s1.next_due, 6_000);

    // Pass 2 at t=7500:
    // Skipped interval: 7000 (≤ 7500) → 1 missed; cumulative = 4.
    // next_due → 8000.
    set_ledger_time(&env, 3, 7_500);
    client.execute_due_savings_schedules();
    let s2 = client.get_savings_schedule(&sched_id).unwrap();
    assert_eq!(
        s2.missed_count, 4,
        "missed_count must accumulate across passes"
    );
    assert_eq!(s2.next_due, 8_000);

    // Two executor passes → two credits.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 200,
        "two credits applied across two passes: 100 + 100 = 200"
    );
}

/// A cancelled schedule is never executed, even after its `next_due` passes.
#[test]
fn test_cancelled_schedule_not_executed() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 2_000);
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &500, &3_000, &86_400);
    client.cancel_savings_schedule(&owner, &sched_id);

    set_ledger_time(&env, 2, 3_500);
    let executed = client.execute_due_savings_schedules();

    assert_eq!(
        executed.len(),
        0,
        "cancelled schedule must never execute regardless of timestamp"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 0,
        "cancelled schedule must not credit the goal"
    );
}

/// One-shot schedules deactivate after a single execution and are never
/// re-executed even if the executor is called again.
#[test]
fn test_one_shot_schedule_deactivates_after_execution() {
    let env = Env::default();
    let (client, owner) = setup(&env);

    let goal_id = make_goal(&env, &client, &owner, 5_000);
    let sched_id = client.create_savings_schedule(&owner, &goal_id, &200, &3_000, &0);

    set_ledger_time(&env, 2, 3_500);
    client.execute_due_savings_schedules();

    let sched = client.get_savings_schedule(&sched_id).unwrap();
    assert!(
        !sched.active,
        "one-shot schedule must be inactive after execution"
    );

    // Second call in a later ledger must not re-execute the deactivated schedule.
    set_ledger_time(&env, 3, 4_000);
    let second = client.execute_due_savings_schedules();
    assert_eq!(
        second.len(),
        0,
        "deactivated one-shot schedule must never re-execute"
    );

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 200,
        "goal balance must not change after the schedule is deactivated"
    );
}
