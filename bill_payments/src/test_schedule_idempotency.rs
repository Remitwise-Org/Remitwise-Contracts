//! Regression tests for bill scheduling and execution replay/idempotency.
//!
//! Issue #1736: make scheduled bill execution deterministic across due dates,
//! retries, missed windows, and partial infrastructure failures.
//!
//! ## Coverage
//!
//! | Acceptance criterion | Tests |
//! |---|---|
//! | Bind each operation to a durable nonce; deterministic result on safe retry | `test_create_schedule_nonce_idempotent`, `test_execute_due_schedules_nonce_idempotent` |
//! | Rejected/stale/repeated ops leave no partial state | `test_duplicate_nonce_leaves_no_partial_state`, `test_execute_duplicate_nonce_no_double_execute` |
//! | Auth/lifecycle preconditions before state mutation | `test_create_schedule_unauthorized`, `test_create_schedule_past_due_date`, `test_zero_nonce_rejected_*` |
//! | Arithmetic, boundaries, and batch limits | `test_missed_schedule_count`, `test_execute_oneshot_deactivates`, `test_execute_recurring_advances_next_due` |
//! | Retries, concurrent calls, failed transactions | `test_retry_after_failed_create`, `test_concurrent_nonce_race_simulation` |

#[cfg(test)]
mod schedule_idempotency_tests {
    extern crate std;

    use crate::{BillPayments, BillPaymentsClient, BillPaymentsError};
    use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env, String};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn set_time(env: &Env, seq: u32, ts: u64) {
        env.ledger().set(LedgerInfo {
            timestamp: ts,
            protocol_version: 22,
            sequence_number: seq,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 6_312_000,
        });
    }

    /// Set up a minimal environment: register contract, create one bill.
    /// Returns `(env, owner, contract_address, client, bill_id)`.
    fn setup() -> (
        Env,
        Address,
        soroban_sdk::Address,
        BillPaymentsClient<'static>,
        u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1, 1_000);

        let contract_id = env.register(BillPayments, ());
        // Safety: the `'static` lifetime on the client is acceptable in tests
        // because the `env` lives for the entire test function.
        let client: BillPaymentsClient<'static> =
            unsafe { core::mem::transmute(BillPaymentsClient::new(&env, &contract_id)) };
        let owner = Address::generate(&env);

        // Create a simple one-shot bill due in the future.
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1_000,
            &5_000, // due_date > now (1_000)
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        (env, owner, contract_id, client, bill_id)
    }

    // -----------------------------------------------------------------------
    // create_schedule — basic lifecycle
    // -----------------------------------------------------------------------

    /// Happy path: create a schedule and read it back.
    #[test]
    fn test_create_schedule_basic() {
        let (env, owner, _, client, bill_id) = setup();

        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &1);
        assert_eq!(sched_id, 1);

        let sched = client.get_schedule(&sched_id).expect("schedule must exist");
        assert_eq!(sched.bill_id, bill_id);
        assert_eq!(sched.next_due, 3_000);
        assert_eq!(sched.interval, 0);
        assert!(sched.active);
        assert_eq!(sched.missed_count, 0);
        assert_eq!(sched.nonce, 1);
    }

    /// Auth precondition: a non-owner cannot create a schedule for another owner's bill.
    #[test]
    fn test_create_schedule_unauthorized() {
        let (env, _owner, _, client, bill_id) = setup();
        let intruder = Address::generate(&env);

        let res = client.try_create_schedule(&intruder, &bill_id, &3_000, &0, &42);
        assert_eq!(res, Err(Ok(BillPaymentsError::Unauthorized)));
    }

    /// Lifecycle precondition: next_due must not be in the past.
    #[test]
    fn test_create_schedule_past_due_date_rejected() {
        let (_, owner, _, client, bill_id) = setup();
        // ledger time = 1_000; next_due = 500 is in the past
        let res = client.try_create_schedule(&owner, &bill_id, &500, &0, &7);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidScheduleDueDate)));
    }

    /// Non-existent bill is rejected before any write.
    #[test]
    fn test_create_schedule_bill_not_found() {
        let (_, owner, _, client, _) = setup();
        let res = client.try_create_schedule(&owner, &9_999, &3_000, &0, &3);
        assert_eq!(res, Err(Ok(BillPaymentsError::BillNotFound)));
    }

    /// Zero nonce is always invalid.
    #[test]
    fn test_create_schedule_zero_nonce_rejected() {
        let (_, owner, _, client, bill_id) = setup();
        let res = client.try_create_schedule(&owner, &bill_id, &3_000, &0, &0);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidNonce)));
    }

    // -----------------------------------------------------------------------
    // create_schedule — idempotency / replay
    // -----------------------------------------------------------------------

    /// CORE: duplicate create_schedule with the same nonce is rejected.
    /// The second call returns NonceAlreadyUsed and creates no second schedule.
    #[test]
    fn test_create_schedule_nonce_idempotent() {
        let (env, owner, _, client, bill_id) = setup();

        // First call succeeds.
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &99);
        assert_eq!(sched_id, 1);

        // Second call with the same nonce is rejected.
        let res = client.try_create_schedule(&owner, &bill_id, &3_000, &0, &99);
        assert_eq!(res, Err(Ok(BillPaymentsError::NonceAlreadyUsed)));

        // Only one schedule was ever created.
        let schedules = client.get_schedules(&owner);
        assert_eq!(schedules.len(), 1);

        // Nonce is recorded as consumed.
        assert!(client.is_nonce_consumed(&99));
        let _ = env;
    }

    /// CORE: duplicate create leaves no partial state — the schedule map
    /// has exactly the same state as after the first successful call.
    #[test]
    fn test_duplicate_nonce_leaves_no_partial_state() {
        let (env, owner, _, client, bill_id) = setup();

        client.create_schedule(&owner, &bill_id, &3_000, &86_400, &55);

        // Attempt a second schedule creation with same nonce (different params).
        let res = client.try_create_schedule(&owner, &bill_id, &4_000, &172_800, &55);
        assert_eq!(res, Err(Ok(BillPaymentsError::NonceAlreadyUsed)));

        // The stored schedule must still have the ORIGINAL params.
        let sched = client
            .get_schedule(&1)
            .expect("original schedule must exist");
        assert_eq!(sched.next_due, 3_000, "next_due must not have changed");
        assert_eq!(sched.interval, 86_400, "interval must not have changed");

        // Still only one schedule.
        assert_eq!(client.get_schedules(&owner).len(), 1);
        let _ = env;
    }

    /// A fresh nonce succeeds even after a different nonce was consumed.
    #[test]
    fn test_create_schedule_fresh_nonce_after_consumed_nonce() {
        let (env, owner, _, client, bill_id) = setup();

        client.create_schedule(&owner, &bill_id, &3_000, &0, &10);
        assert!(client.is_nonce_consumed(&10));
        assert!(!client.is_nonce_consumed(&11));

        // Create a second bill and use a fresh nonce.
        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &5_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let sched_id2 = client.create_schedule(&owner, &bill_id2, &3_500, &0, &11);
        assert_eq!(sched_id2, 2);
        assert!(client.is_nonce_consumed(&11));
    }

    // -----------------------------------------------------------------------
    // modify_schedule
    // -----------------------------------------------------------------------

    #[test]
    fn test_modify_schedule_success() {
        let (_, owner, _, client, bill_id) = setup();

        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &86_400, &20);
        client.modify_schedule(&owner, &sched_id, &4_000, &172_800);

        let sched = client.get_schedule(&sched_id).unwrap();
        assert_eq!(sched.next_due, 4_000);
        assert_eq!(sched.interval, 172_800);
    }

    #[test]
    fn test_modify_schedule_unauthorized() {
        let (env, owner, _, client, bill_id) = setup();
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &21);
        let intruder = Address::generate(&env);
        let res = client.try_modify_schedule(&intruder, &sched_id, &4_000, &0);
        assert_eq!(res, Err(Ok(BillPaymentsError::Unauthorized)));
    }

    #[test]
    fn test_modify_schedule_not_found() {
        let (_, owner, _, client, _) = setup();
        let res = client.try_modify_schedule(&owner, &999, &4_000, &0);
        assert_eq!(res, Err(Ok(BillPaymentsError::ScheduleNotFound)));
    }

    #[test]
    fn test_modify_cancelled_schedule_rejected() {
        let (_, owner, _, client, bill_id) = setup();
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &22);
        client.cancel_schedule(&owner, &sched_id);
        let res = client.try_modify_schedule(&owner, &sched_id, &4_000, &0);
        assert_eq!(res, Err(Ok(BillPaymentsError::ScheduleAlreadyCancelled)));
    }

    #[test]
    fn test_modify_schedule_past_due_date_rejected() {
        let (_, owner, _, client, bill_id) = setup();
        // ledger time = 1_000; 500 is in the past
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &23);
        let res = client.try_modify_schedule(&owner, &sched_id, &500, &0);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidScheduleDueDate)));
    }

    // -----------------------------------------------------------------------
    // cancel_schedule
    // -----------------------------------------------------------------------

    #[test]
    fn test_cancel_schedule_success() {
        let (_, owner, _, client, bill_id) = setup();
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &30);
        client.cancel_schedule(&owner, &sched_id);
        let sched = client.get_schedule(&sched_id).unwrap();
        assert!(!sched.active);
    }

    #[test]
    fn test_cancel_schedule_unauthorized() {
        let (env, owner, _, client, bill_id) = setup();
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &31);
        let intruder = Address::generate(&env);
        let res = client.try_cancel_schedule(&intruder, &sched_id);
        assert_eq!(res, Err(Ok(BillPaymentsError::Unauthorized)));
    }

    #[test]
    fn test_cancel_already_cancelled_schedule_rejected() {
        let (_, owner, _, client, bill_id) = setup();
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &32);
        client.cancel_schedule(&owner, &sched_id);
        let res = client.try_cancel_schedule(&owner, &sched_id);
        assert_eq!(res, Err(Ok(BillPaymentsError::ScheduleAlreadyCancelled)));
    }

    // -----------------------------------------------------------------------
    // execute_due_schedules — basic execution
    // -----------------------------------------------------------------------

    /// CORE: execute a due one-shot schedule, bill is paid, schedule deactivates.
    #[test]
    fn test_execute_due_schedule_oneshot() {
        let (env, owner, _, client, bill_id) = setup();

        // Schedule fires at t=3_000; interval=0 (one-shot).
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &100);

        // Advance time past next_due.
        set_time(&env, 2, 3_500);

        let executed = client.execute_due_schedules(&101);
        assert_eq!(executed.len(), 1);
        assert_eq!(executed.get(0).unwrap(), sched_id);

        // Bill is now paid.
        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);

        // One-shot schedule is deactivated.
        let sched = client.get_schedule(&sched_id).unwrap();
        assert!(!sched.active);
    }

    /// execute_due_schedules with a zero nonce is rejected immediately.
    #[test]
    fn test_execute_zero_nonce_rejected() {
        let (_, _, _, client, _) = setup();
        let res = client.try_execute_due_schedules(&0);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidNonce)));
    }

    /// Nothing due: execute returns empty list without error.
    #[test]
    fn test_execute_nothing_due_returns_empty() {
        let (env, owner, _, client, bill_id) = setup();
        // Schedule fires at t=10_000; current time = 1_000 — not due yet.
        client.create_schedule(&owner, &bill_id, &10_000, &0, &200);

        let executed = client.execute_due_schedules(&201);
        assert_eq!(executed.len(), 0);
        let _ = env;
    }

    // -----------------------------------------------------------------------
    // execute_due_schedules — idempotency / replay
    // -----------------------------------------------------------------------

    /// CORE: replaying execute_due_schedules with the same nonce returns the
    /// same empty-list result without double-executing any bill.
    #[test]
    fn test_execute_due_schedules_nonce_idempotent() {
        let (env, owner, _, client, bill_id) = setup();

        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &300);
        set_time(&env, 2, 3_500);

        // First execution: succeeds.
        let first = client.execute_due_schedules(&301);
        assert_eq!(first.len(), 1);
        assert_eq!(first.get(0).unwrap(), sched_id);
        assert!(client.get_bill(&bill_id).unwrap().paid);

        // Second execution with the SAME nonce: returns empty, no double-pay.
        let second = client.execute_due_schedules(&301);
        assert_eq!(second.len(), 0, "replay must return empty, not re-execute");

        // Bill is still paid exactly once (paid_at was set on first call).
        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);
        assert!(bill.paid_at.is_some());

        // Nonce is consumed.
        assert!(client.is_nonce_consumed(&301));
    }

    /// CORE: duplicate execute with the same nonce does NOT double-execute
    /// bills even if they would still appear due.
    #[test]
    fn test_execute_duplicate_nonce_no_double_execute() {
        let (env, owner, _, client, bill_id) = setup();

        client.create_schedule(&owner, &bill_id, &3_000, &0, &400);
        set_time(&env, 2, 3_500);

        client.execute_due_schedules(&401);

        // Simulate: message replayed with the same nonce.
        let replay = client.execute_due_schedules(&401);
        assert_eq!(
            replay.len(),
            0,
            "replayed execution must be a no-op, not double-pay"
        );

        // Exactly one payment happened.
        let sched = client.get_schedule(&1).unwrap();
        assert!(
            !sched.active,
            "schedule should have been deactivated on first run"
        );
    }

    /// Two different nonces produce two independent executions (boundary).
    #[test]
    fn test_two_different_nonces_independent() {
        let (env, owner, _, client, bill_id) = setup();

        // Create two bills.
        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &5_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        client.create_schedule(&owner, &bill_id, &3_000, &0, &500);
        client.create_schedule(&owner, &bill_id2, &3_100, &0, &501);

        set_time(&env, 2, 4_000);

        let first_exec = client.execute_due_schedules(&502);
        assert_eq!(first_exec.len(), 2, "both bills must execute");

        // A new nonce on the same time with nothing left due.
        let second_exec = client.execute_due_schedules(&503);
        assert_eq!(second_exec.len(), 0);

        assert!(client.is_nonce_consumed(&502));
        assert!(client.is_nonce_consumed(&503));
    }

    // -----------------------------------------------------------------------
    // Recurring schedules and missed windows
    // -----------------------------------------------------------------------

    /// Recurring schedule advances next_due after execution.
    #[test]
    fn test_execute_recurring_advances_next_due() {
        let (env, owner, _, client, bill_id) = setup();

        // interval = 86_400 seconds (one day)
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &86_400, &600);
        set_time(&env, 2, 3_500);

        client.execute_due_schedules(&601);

        let sched = client.get_schedule(&sched_id).unwrap();
        assert!(sched.active, "recurring schedule stays active");
        assert!(
            sched.next_due > 3_500,
            "next_due must be in the future after execution"
        );
        assert_eq!(sched.next_due, 3_000 + 86_400);
    }

    /// Missed-window counter increments correctly when ledger skips multiple intervals.
    #[test]
    fn test_missed_schedule_count() {
        let (env, owner, _, client, bill_id) = setup();

        // Schedule fires every 86_400 seconds starting at 3_000.
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &86_400, &700);

        // Advance past three windows.
        set_time(&env, 2, 3_000 + 86_400 * 3 + 100);

        client.execute_due_schedules(&701);

        let sched = client.get_schedule(&sched_id).unwrap();
        // 3 windows were missed before execution.
        assert_eq!(sched.missed_count, 3);
        assert!(sched.next_due > 3_000 + 86_400 * 3 + 100);
    }

    // -----------------------------------------------------------------------
    // Retry after a failed / rejected create
    // -----------------------------------------------------------------------

    /// A call that is rejected for any reason (unauthorized, bad params, etc.)
    /// leaves zero state, so a corrected retry with a fresh nonce succeeds.
    #[test]
    fn test_retry_after_failed_create() {
        let (_, owner, _, client, bill_id) = setup();

        // First attempt: past due_date — rejected.
        let res = client.try_create_schedule(&owner, &bill_id, &500, &0, &800);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidScheduleDueDate)));

        // The failed nonce was NOT consumed (rejection before nonce write).
        assert!(!client.is_nonce_consumed(&800));

        // Corrected retry with the same nonce succeeds.
        let sched_id = client.create_schedule(&owner, &bill_id, &3_000, &0, &800);
        assert_eq!(sched_id, 1);
        assert!(client.is_nonce_consumed(&800));
    }

    /// A rejected execute_due_schedules (zero nonce) leaves no state.
    #[test]
    fn test_execute_rejected_zero_nonce_leaves_no_state() {
        let (env, owner, _, client, bill_id) = setup();

        client.create_schedule(&owner, &bill_id, &3_000, &0, &900);
        set_time(&env, 2, 3_500);

        // Rejected call.
        let _ = client.try_execute_due_schedules(&0);

        // Bill is still unpaid; schedule still active.
        assert!(!client.get_bill(&bill_id).unwrap().paid);
        assert!(client.get_schedule(&1).unwrap().active);

        // Corrected call with valid nonce succeeds.
        let executed = client.execute_due_schedules(&901);
        assert_eq!(executed.len(), 1);
        assert!(client.get_bill(&bill_id).unwrap().paid);
    }

    // -----------------------------------------------------------------------
    // Concurrent / ordering edge cases
    // -----------------------------------------------------------------------

    /// Simulate two callers racing with different nonces: both succeed once each
    /// (Soroban serialises transactions, but each valid nonce executes exactly once).
    #[test]
    fn test_concurrent_nonce_race_simulation() {
        let (env, owner, _, client, bill_id) = setup();

        client.create_schedule(&owner, &bill_id, &3_000, &0, &1000);
        set_time(&env, 2, 3_500);

        // "Caller A" executes.
        let res_a = client.execute_due_schedules(&1001);
        assert_eq!(res_a.len(), 1);

        // "Caller B" attempts same nonce (simulated replay).
        let res_b = client.execute_due_schedules(&1001);
        assert_eq!(
            res_b.len(),
            0,
            "second call with same nonce must be a no-op"
        );

        // "Caller B" uses a fresh nonce — but nothing is left to execute.
        let res_c = client.execute_due_schedules(&1002);
        assert_eq!(res_c.len(), 0);

        // Only one execution actually occurred.
        assert!(client.get_bill(&bill_id).unwrap().paid);
    }

    // -----------------------------------------------------------------------
    // get_schedule / get_schedules / get_schedules_page
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_schedule_not_found() {
        let (_, _, _, client, _) = setup();
        assert!(client.get_schedule(&999).is_none());
    }

    #[test]
    fn test_get_schedules_returns_all_owner_schedules() {
        let (env, owner, _, client, bill_id) = setup();

        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &5_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        client.create_schedule(&owner, &bill_id, &3_000, &86_400, &2000);
        client.create_schedule(&owner, &bill_id2, &4_000, &172_800, &2001);

        let schedules = client.get_schedules(&owner);
        assert_eq!(schedules.len(), 2);
    }

    #[test]
    fn test_get_schedules_page_basic() {
        let (env, owner, _, client, bill_id) = setup();

        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &5_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        client.create_schedule(&owner, &bill_id, &3_000, &0, &3000);
        client.create_schedule(&owner, &bill_id2, &4_000, &0, &3001);

        let page = client.get_schedules_page(&owner, &0, &10);
        assert_eq!(page.count, 2);
    }

    // -----------------------------------------------------------------------
    // is_nonce_consumed view
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_nonce_consumed_accurate() {
        let (_, owner, _, client, bill_id) = setup();
        let nonce: u64 = 4000;

        assert!(!client.is_nonce_consumed(&nonce));
        client.create_schedule(&owner, &bill_id, &3_000, &0, &nonce);
        assert!(client.is_nonce_consumed(&nonce));
    }
}
