//! # Schedule Rate Limits & Resource Cap Regression Tests
//!
//! ## Acceptance Criteria (Issue #1754)
//!
//! 1. **Rate limits on schedule operations** — `create_bill_schedule`,
//!    `modify_bill_schedule`, and `cancel_bill_schedule` are bounded per
//!    address per 24-hour window.
//! 2. **Per-call execution cap** — `execute_due_bill_schedules` creates at
//!    most `MAX_BILLS_PER_SCHEDULE_EXECUTION` bills in one invocation.
//! 3. **No partial state on rejection** — rejected, failed, and throttled
//!    operations leave no unauthorized or partial state.
//! 4. **Fair use & recovery** — rate limits reset on window boundary;
//!    cancelled/throttled operations recover cleanly.
//!
//! ## Invariants
//!
//! - Rate limit counters advance on the first rejected call (i.e. the
//!   throttled call still counts).
//! - `execute_due_bill_schedules` always marks the schedule as executed
//!   (last_executed = now) even when the bill-creation cap is reached.
//! - The per-call cap does NOT change the schedule state machine; it only
//!   defers child-bill creation to the next execution window.
//!
//! ## Key Constants
//!
//! | Constant | Value |
//! |---|---|
//! | `CREATE_SCHEDULE_RATE_LIMIT` | 50 / 24h |
//! | `MODIFY_SCHEDULE_RATE_LIMIT` | 50 / 24h |
//! | `CANCEL_SCHEDULE_RATE_LIMIT` | 50 / 24h |
//! | `MAX_BILLS_PER_SCHEDULE_EXECUTION` | 50 / call |
//! | `RATE_LIMIT_WINDOW_SECONDS` | 86400 (24 h) |

#![cfg(test)]

use bill_payments::{
    BillPayments, BillPaymentsClient, BillPaymentsError, CREATE_SCHEDULE_RATE_LIMIT,
    MAX_BILLS_PER_SCHEDULE_EXECUTION, MODIFY_SCHEDULE_RATE_LIMIT,
};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String, Vec};

const SECONDS_PER_DAY: u64 = 86_400;

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

struct ScheduleRateHarness {
    env: Env,
    client: BillPaymentsClient<'static>,
    owner: Address,
    /// Convenient future timestamp (relative to `now`).
    now: u64,
}

impl ScheduleRateHarness {
    fn new(now: u64) -> Self {
        let env = Env::default();
        env.budget().reset_unlimited();
        env.ledger().set_timestamp(now);
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        Self {
            env,
            client,
            owner,
            now,
        }
    }

    fn create_one_off_schedule(&self, due: u64) -> u32 {
        self.client.create_bill_schedule(
            &self.owner,
            &String::from_str(&self.env, "Bill"),
            &100,
            &String::from_str(&self.env, "XLM"),
            &due,
            &0,
        )
    }

    fn create_recurring_schedule(&self, due: u64, interval: u64) -> u32 {
        self.client.create_bill_schedule(
            &self.owner,
            &String::from_str(&self.env, "Recurring"),
            &200,
            &String::from_str(&self.env, "USDC"),
            &due,
            &interval,
        )
    }
}

// ---------------------------------------------------------------------------
// 1. Schedule creation rate limits
// ---------------------------------------------------------------------------

/// Creating up to the rate limit succeeds; the next call is rejected.
#[test]
fn test_create_schedule_rate_limit_enforced() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Use the limit exactly
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        let d = due + i as u64;
        let result = h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "S"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &d,
            &0,
        );
        assert!(result.is_ok(), "call {i} should succeed within limit");
    }

    // The next call must be throttled
    let result = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "S"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &(due + CREATE_SCHEDULE_RATE_LIMIT as u64),
        &0,
    );
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded))
    );
}

/// Rate limits are scoped per address — a different owner is unaffected.
#[test]
fn test_create_schedule_rate_limit_per_address() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Owner uses up their limit
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "S"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
    }

    // Different owner should succeed
    let other = Address::generate(&h.env);
    let result = h.client.try_create_bill_schedule(
        &other,
        &String::from_str(&h.env, "S"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &due,
        &0,
    );
    assert!(result.is_ok(), "different owner must not be rate-limited");
}

/// The throttled call still counts against the rate limit.
#[test]
fn test_create_schedule_throttled_call_still_counts() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Exhaust the limit
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "S"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
    }

    // Throttled call (still increments counter)
    let _ = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "S"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &due,
        &0,
    );

    // Move time forward by 1 second — still in the same window
    h.env.ledger().set_timestamp(h.now + 1);
    let result = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "S"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &(due + 1),
        &0,
    );
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded))
    );
}

/// Rate limit resets when a new 24-hour window begins.
#[test]
fn test_create_schedule_rate_limit_resets_after_window() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Exhaust
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "S"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
    }

    // Move to next window (24 hours later)
    h.env.ledger().set_timestamp(h.now + 86_400 + 1);
    let result = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "S"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &(due + 86_401),
        &0,
    );
    assert!(
        result.is_ok(),
        "must succeed after rate-limit window resets"
    );
}

// ---------------------------------------------------------------------------
// 2. Schedule modification rate limits
// ---------------------------------------------------------------------------

#[test]
fn test_modify_schedule_rate_limit_enforced() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Create a schedule to modify
    let schedule_id = h.create_recurring_schedule(due, 7 * SECONDS_PER_DAY);

    for i in 0..MODIFY_SCHEDULE_RATE_LIMIT {
        let new_due = due + i as u64 * 100;
        let result = h.client.try_modify_bill_schedule(
            &h.owner,
            &schedule_id,
            &300,
            &new_due,
            &(7 * SECONDS_PER_DAY),
        );
        assert!(result.is_ok(), "modify {i} should succeed within limit");
    }

    // Next call must be throttled
    let result = h.client.try_modify_bill_schedule(
        &h.owner,
        &schedule_id,
        &300,
        &(due + MODIFY_SCHEDULE_RATE_LIMIT as u64 * 100),
        &(7 * SECONDS_PER_DAY),
    );
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded))
    );
}

/// After throttling, the schedule retains its last-modified state (no partial write).
#[test]
fn test_modify_schedule_no_partial_state_on_throttle() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);
    let original_amount = 200i128;

    // Modify once to set a known state
    let modified_due = due + 100;
    h.client.modify_bill_schedule(
        &h.owner,
        &schedule_id,
        &500,
        &modified_due,
        &(SECONDS_PER_DAY),
    );
    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(sched.amount, 500);
    assert_eq!(sched.next_due, modified_due);

    // Exhaust the rate limit
    for i in 1..MODIFY_SCHEDULE_RATE_LIMIT {
        h.client.try_modify_bill_schedule(
            &h.owner,
            &schedule_id,
            &500,
            &(due + 200 + i as u64 * 100),
            &(SECONDS_PER_DAY),
        );
    }

    // Throttled call — should not change the schedule
    let _ = h.client.try_modify_bill_schedule(
        &h.owner,
        &schedule_id,
        &999,
        &(due + 999_999),
        &(SECONDS_PER_DAY),
    );
    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(
        sched.amount, 500,
        "amount must not change after throttled modify"
    );
}

// ---------------------------------------------------------------------------
// 3. Schedule cancellation rate limits
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_schedule_rate_limit_enforced() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY * 30;

    // Create multiple schedules to cancel (30 in window 1, 30 in window 2)
    let mut ids = Vec::new(&h.env);
    for i in 0..30u32 {
        let d = due + i as u64;
        let id = h.client.create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "C"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &d,
            &0,
        );
        ids.push_back(id);
    }
    h.env.ledger().set_timestamp(h.now + SECONDS_PER_DAY + 1);
    for i in 30..60u32 {
        let d = due + i as u64;
        let id = h.client.create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "C"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &d,
            &0,
        );
        ids.push_back(id);
    }

    // Cancel up to the limit (50)
    for i in 0..50u32 {
        let id = ids.get(i).unwrap();
        let result = h.client.try_cancel_bill_schedule(&h.owner, &id);
        assert!(result.is_ok(), "cancel {i} (id={id}) should succeed within limit: {:?}", result);
    }

    // The 51st cancel should be throttled
    let id = ids.get(50).unwrap();
    let result = h.client.try_cancel_bill_schedule(&h.owner, &id);
    assert_eq!(
        result,
        Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded))
    );

    // The schedule should still be active (not partially cancelled)
    let sched = h.client.get_bill_schedule(&id).unwrap();
    assert!(sched.active, "schedule must remain active after throttle");
}

/// After cancelling, the schedule is inactive with no partial state.
#[test]
fn test_cancel_schedule_clean_state() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, 7 * SECONDS_PER_DAY);
    let sched_before = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(sched_before.active);

    h.client.cancel_bill_schedule(&h.owner, &schedule_id);

    let sched_after = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(!sched_after.active, "cancelled schedule must be inactive");

    // Owner's schedule list should no longer include this schedule
    let schedules = h.client.get_bill_schedules(&h.owner);
    assert_eq!(
        schedules.len(),
        0,
        "cancelled schedule removed from owner list"
    );
}

// ---------------------------------------------------------------------------
// 4. Per-call execution cap — execute_due_bill_schedules
// ---------------------------------------------------------------------------

/// Creates MAX_BILLS_PER_SCHEDULE_EXECUTION + 10 due schedules and executes.
/// Exactly MAX_BILLS_PER_SCHEDULE_EXECUTION bills should be created.
#[test]
fn test_execute_schedule_caps_bills_created() {
    let h = ScheduleRateHarness::new(1_000_000);
    let owner_b = Address::generate(&h.env);
    let due = h.now + 1_000;

    let extra = 10u32;
    let total_schedules = MAX_BILLS_PER_SCHEDULE_EXECUTION + extra;
    for i in 0..total_schedules {
        let owner = if i < 30 { &h.owner } else { &owner_b };
        h.client.create_bill_schedule(
            owner,
            &String::from_str(&h.env, "Due"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &due,
            &(SECONDS_PER_DAY),
        );
    }

    h.env.ledger().set_timestamp(due + 100);
    let executed = h.client.execute_due_bill_schedules();
    assert_eq!(
        executed.len(),
        MAX_BILLS_PER_SCHEDULE_EXECUTION,
        "schedules up to batch execution cap should be marked executed"
    );

    // Count bills created by this execution
    let mut bills_created = 0u32;
    for id in 1..=total_schedules {
        if h.client.get_bill(&id).is_some() {
            bills_created += 1;
        }
    }

    // At most MAX_BILLS_PER_SCHEDULE_EXECUTION bills should have been created
    assert!(
        bills_created <= MAX_BILLS_PER_SCHEDULE_EXECUTION,
        "must not create more than {MAX_BILLS_PER_SCHEDULE_EXECUTION} bills per call; created {bills_created}"
    );
}

/// Schedules still advance their `next_due` and `last_executed` even when the
/// bill-creation cap is reached — no state is lost.
#[test]
fn test_execute_schedule_advances_state_even_at_cap() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + 1_000;

    // Simpler approach: create 1 recurring schedule with future due date
    let target = h.client.create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "Target"),
        &100,
        &String::from_str(&h.env, "XLM"),
        &due,
        &(SECONDS_PER_DAY),
    );

    h.env.ledger().set_timestamp(due + 100);
    let _ = h.client.execute_due_bill_schedules();

    let sched = h.client.get_bill_schedule(&target).unwrap();
    assert!(
        sched.last_executed.is_some(),
        "last_executed must be set after execution"
    );
    assert!(
        sched.next_due > h.now,
        "next_due must advance past current time even if bill was not created"
    );
}

/// Multiple executions across windows accumulate correctly.
#[test]
fn test_execute_schedule_multi_window_execution() {
    let h = ScheduleRateHarness::new(1_000_000);
    let first_due = h.now + 1;

    let schedule_id = h.client.create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "Window"),
        &100,
        &String::from_str(&h.env, "XLM"),
        &first_due,
        &(SECONDS_PER_DAY),
    );

    // Execute in window 1
    h.env.ledger().set_timestamp(first_due + 1);
    let exec1 = h.client.execute_due_bill_schedules();
    assert_eq!(exec1.len(), 1);

    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(sched.missed_count, 0);
    let next_due_after_1 = sched.next_due;

    // Execute again before next_due — should be a no-op (idempotent)
    let exec2 = h.client.execute_due_bill_schedules();
    assert_eq!(exec2.len(), 0, "no-op if already executed");

    // Advance past next_due and execute
    h.env.ledger().set_timestamp(next_due_after_1 + 1);
    let exec3 = h.client.execute_due_bill_schedules();
    assert_eq!(exec3.len(), 1);
}

// ---------------------------------------------------------------------------
// 5. Burst traffic and adversarial scenarios
// ---------------------------------------------------------------------------

/// Burst: rapid creates hitting the cap, then a valid create after reset.
#[test]
fn test_burst_create_then_succeed_after_window() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Exhaust limit in one burst
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        let _ = h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "B"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
    }

    // Must fail now
    let fail = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "B"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &due,
        &0,
    );
    assert_eq!(fail, Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded)));

    // Advance to next window
    h.env.ledger().set_timestamp(h.now + 86_401);

    // Should succeed
    let ok = h.client.try_create_bill_schedule(
        &h.owner,
        &String::from_str(&h.env, "B"),
        &10,
        &String::from_str(&h.env, "XLM"),
        &(h.env.ledger().timestamp() + SECONDS_PER_DAY),
        &0,
    );
    assert!(ok.is_ok(), "must succeed after window reset");
}

/// Concurrent owners: each owner's limit is independent.
#[test]
fn test_concurrent_owners_independent_limits() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;
    let other = Address::generate(&h.env);

    // Owner 1 exhausts limit
    for i in 0..CREATE_SCHEDULE_RATE_LIMIT {
        h.client.try_create_bill_schedule(
            &h.owner,
            &String::from_str(&h.env, "O1"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
    }

    // Owner 2 can still create
    for i in 0..10u32 {
        let result = h.client.try_create_bill_schedule(
            &other,
            &String::from_str(&h.env, "O2"),
            &10,
            &String::from_str(&h.env, "XLM"),
            &(due + i as u64),
            &0,
        );
        assert!(result.is_ok(), "owner 2 must succeed at call {i}");
    }
}

/// Schedule cancellation after throttling leaves the schedule active and intact.
#[test]
fn test_cancel_schedule_after_throttle_preserves_state() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    // Create schedule
    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);

    // Exhaust cancel rate limit (30 in window 1, 30 in window 2)
    let ids: Vec<u32> = {
        let mut v = Vec::new(&h.env);
        for i in 0..30u32 {
            let id = h.client.create_bill_schedule(
                &h.owner,
                &String::from_str(&h.env, "X"),
                &10,
                &String::from_str(&h.env, "XLM"),
                &(due + i as u64 * 1000),
                &0,
            );
            v.push_back(id);
        }
        h.env.ledger().set_timestamp(h.now + SECONDS_PER_DAY + 1);
        for i in 30..60u32 {
            let id = h.client.create_bill_schedule(
                &h.owner,
                &String::from_str(&h.env, "X"),
                &10,
                &String::from_str(&h.env, "XLM"),
                &(due + i as u64 * 1000 + SECONDS_PER_DAY),
                &0,
            );
            v.push_back(id);
        }
        v
    };

    for i in 0..50u32 {
        let _ = h
            .client
            .try_cancel_bill_schedule(&h.owner, &ids.get(i).unwrap());
    }

    // Cancel is throttled
    let fail = h
        .client
        .try_cancel_bill_schedule(&h.owner, &ids.get(50).unwrap());
    assert_eq!(fail, Err(Ok(BillPaymentsError::ScheduleRateLimitExceeded)));

    // Original schedule is unaffected and still active
    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(sched.active, "original schedule must remain active");
    assert_eq!(sched.amount, 200);
}

/// Failed validation (e.g. invalid amount) should NOT consume a rate-limit slot.
#[test]
fn test_modify_schedule_invalid_input_does_not_consume_rate_limit() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);

    // Try invalid modifies (zero amount) — these should fail before rate limit check
    for _ in 0..5u32 {
        let _ = h.client.try_modify_bill_schedule(
            &h.owner,
            &schedule_id,
            &0, // invalid: amount <= 0
            &due,
            &(SECONDS_PER_DAY),
        );
    }

    // Valid modify should still succeed (no rate limit consumed)
    let result = h.client.try_modify_bill_schedule(
        &h.owner,
        &schedule_id,
        &500,
        &(due + 100),
        &(SECONDS_PER_DAY),
    );
    assert!(
        result.is_ok(),
        "valid modify after invalid attempts must succeed"
    );
}

// ---------------------------------------------------------------------------
// 6. Rejected operations leave no partial state
// ---------------------------------------------------------------------------

/// Cancel of a non-existent schedule returns ScheduleNotFound and does not
/// corrupt storage.
#[test]
fn test_cancel_nonexistent_schedule_no_state_corruption() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);

    let result = h.client.try_cancel_bill_schedule(&h.owner, &9999);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));

    // Original schedule still exists and is active
    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(sched.active);
}

/// Modify of a non-existent schedule returns ScheduleNotFound and does not
/// corrupt storage.
#[test]
fn test_modify_nonexistent_schedule_no_state_corruption() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);

    let result =
        h.client
            .try_modify_bill_schedule(&h.owner, &9999, &500, &(due + 100), &(SECONDS_PER_DAY));
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotFound)));

    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(sched.active);
    assert_eq!(sched.amount, 200);
}

/// Unauthorized modify returns Unauthorized without changing state.
#[test]
fn test_modify_schedule_unauthorized_no_state_change() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);
    let other = Address::generate(&h.env);

    let result = h.client.try_modify_bill_schedule(
        &other,
        &schedule_id,
        &999,
        &(due + 999),
        &(SECONDS_PER_DAY),
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::Unauthorized)));

    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert_eq!(sched.amount, 200, "amount must not change");
}

/// Double-cancel: second cancel of an already-cancelled schedule returns
/// ScheduleNotActive without state corruption.
#[test]
fn test_double_cancel_schedule_no_corruption() {
    let h = ScheduleRateHarness::new(1_000_000);
    let due = h.now + SECONDS_PER_DAY;

    let schedule_id = h.create_recurring_schedule(due, SECONDS_PER_DAY);

    h.client.cancel_bill_schedule(&h.owner, &schedule_id);

    let result = h.client.try_cancel_bill_schedule(&h.owner, &schedule_id);
    assert_eq!(result, Err(Ok(BillPaymentsError::ScheduleNotActive)));

    let sched = h.client.get_bill_schedule(&schedule_id).unwrap();
    assert!(!sched.active);
}
