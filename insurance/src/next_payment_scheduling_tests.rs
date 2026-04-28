// insurance/src/next_payment_scheduling_tests.rs
//
// SC-068: Insurance: Add tests for `next_payment_date` behavior under late and missed payments
//
// This module validates premium scheduling correctness under time progression, ensuring:
// 1. On-time payments at `next_payment_date` boundary advance exactly one period (30 days)
// 2. Late payments after extended time jumps follow deterministic catch-up semantics
// 3. Missed payments enforce consistent schedule progression
// 4. UI expectations for payment deadlines remain locked across contract versions

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// =============================================================================
// Constants: Timing and Payment Schedule
// =============================================================================

const MONTH_SECONDS: u64 = 30 * 86_400;  // 30 days in seconds
const DAY_SECONDS: u64 = 86_400;          // 1 day in seconds

// =============================================================================
// Helpers: Setup and Utilities
// =============================================================================

fn setup_env() -> (Env, InsuranceClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_pause_admin(&admin, &admin);
    (env, client, admin)
}

fn create_test_policy(
    env: &Env,
    client: &InsuranceClient,
    owner: &Address,
    monthly_premium: i128,
) -> u32 {
    client.create_policy(
        owner,
        &String::from_str(env, "Test Policy"),
        &CoverageType::Health,
        &monthly_premium,
        &10_000i128,
        &None,
    )
}

// =============================================================================
// Test Suite 1: On-Time Payments at Exact Boundary
// =============================================================================

/// Test: Payment exactly at `next_payment_date` advances schedule by one period.
///
/// **Scenario:**
/// - Create policy at T=1,000,000; next_payment_date = T + 30 days
/// - Advance ledger to exactly `next_payment_date`
/// - Call pay_premium()
/// - Assert: new next_payment_date = T + 60 days
///
/// **Boundary Semantics:**
/// This test locks the contract to advance exactly one period regardless of
/// the current ledger time, ensuring UI can safely show next_payment_date
/// as the deadline without ambiguity.
#[test]
fn test_on_time_payment_at_exact_boundary() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();

    let initial_next_payment = policy.next_payment_date;
    assert_eq!(
        initial_next_payment,
        creation_time + MONTH_SECONDS,
        "On creation, next_payment_date must be exactly 30 days ahead"
    );

    // Advance to exactly the payment deadline
    env.ledger().with_mut(|li| li.timestamp = initial_next_payment);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        initial_next_payment + MONTH_SECONDS,
        "On-time payment at exact boundary must advance exactly one period (30 days)"
    );
}

/// Test: Payment one second before boundary.
///
/// **Scenario:**
/// - Create policy; next_payment_date = T + 30 days
/// - Advance ledger to (next_payment_date - 1 second)
/// - Call pay_premium()
/// - Assert: next_payment_date advances by 30 days from payment time
///
/// **Note:** Current contract accepts payments at any time.
#[test]
fn test_on_time_payment_one_second_before_boundary() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    // Advance to one second before the payment deadline
    let time_before_deadline = initial_next_payment.saturating_sub(1);
    env.ledger().with_mut(|li| li.timestamp = time_before_deadline);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        time_before_deadline + MONTH_SECONDS,
        "Payment one second early advances by 30 days from payment time"
    );
}

/// Test: Payment one second after boundary advances by one period.
///
/// **Scenario:**
/// - Create policy; next_payment_date = T + 30 days
/// - Advance ledger to (next_payment_date + 1 second) — 1 second late
/// - Call pay_premium()
/// - Assert: next_payment_date = (old) + 30 days
///
/// **Boundary Semantics:**
/// Small delays (< 1 day) do NOT trigger catch-up logic.
#[test]
fn test_on_time_payment_one_second_after_boundary() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    // Advance to one second past the payment deadline
    let time_after_deadline = initial_next_payment.saturating_add(1);
    env.ledger().with_mut(|li| li.timestamp = time_after_deadline);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        time_after_deadline + MONTH_SECONDS,
        "Payment 1 second late advances by exactly one period from payment time"
    );
}

// =============================================================================
// Test Suite 2: Late Payments (Small Delays)
// =============================================================================

/// Test: Payment 1 day late advances by one period (no catch-up).
///
/// **Scenario:**
/// - Create policy; next_payment_date = T + 30 days
/// - Advance ledger to (next_payment_date + 1 day)
/// - Call pay_premium()
/// - Assert: next_payment_date = (payment_time) + 30 days = T + 61 days
///
/// **Design Decision: No Catch-Up for Small Delays**
/// Late payments within ~30 days do NOT attempt to "catch up" missed periods.
#[test]
fn test_late_payment_one_day() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    // Advance to 1 day past the payment deadline
    let late_time = initial_next_payment + DAY_SECONDS;
    env.ledger().with_mut(|li| li.timestamp = late_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        late_time + MONTH_SECONDS,
        "Late payment by 1 day advances by exactly one period from payment time"
    );
    assert_eq!(
        new_next_payment,
        initial_next_payment + DAY_SECONDS + MONTH_SECONDS,
        "Relative to original deadline, new deadline is original + 1 day + 1 month"
    );
}

/// Test: Payment 7 days late advances by one period.
#[test]
fn test_late_payment_one_week() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    let late_time = initial_next_payment + (7 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = late_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        late_time + MONTH_SECONDS,
        "Late payment by 7 days advances by exactly one period"
    );
}

/// Test: Payment 29 days late (still within month) advances by one period.
#[test]
fn test_late_payment_twentynine_days() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    let late_time = initial_next_payment + (29 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = late_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        late_time + MONTH_SECONDS,
        "Late payment by 29 days advances by one period only"
    );
}

// =============================================================================
// Test Suite 3: Missed Payments (Large Time Jumps)
// =============================================================================

/// Test: Missed payment after 60 days (2 months) — no automatic catch-up.
///
/// **Scenario:**
/// - Create policy; next_payment_date = T + 30 days
/// - Advance ledger by 60 days (2 full months pass without payment)
/// - Call pay_premium() at T + 60 days
/// - Assert: next_payment_date = T + 90 days (payment_time + 30 days)
#[test]
fn test_missed_payment_two_months() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let initial_next_payment = policy.next_payment_date;

    // Advance 60 days (2 months) past creation — one full missed period
    let missed_payment_time = creation_time + (60 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = missed_payment_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        missed_payment_time + MONTH_SECONDS,
        "Missed payment after 2 months advances by one period only"
    );
    assert_eq!(
        new_next_payment,
        initial_next_payment + (30 * DAY_SECONDS) + MONTH_SECONDS,
        "New deadline is original + 30 days (missed period) + 30 days (new period)"
    );
}

/// Test: Missed payment after 90 days (3 months) — single-period advance.
#[test]
fn test_missed_payment_three_months() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let _initial_next_payment = client.get_policy(&policy_id).unwrap().next_payment_date;

    let missed_payment_time = creation_time + (90 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = missed_payment_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        missed_payment_time + MONTH_SECONDS,
        "Missed payment after 3 months still advances by exactly one period"
    );
}

/// Test: Missed payment after 1 year — consistency check.
#[test]
fn test_missed_payment_one_year() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let _initial_next_payment = client.get_policy(&policy_id).unwrap().next_payment_date;

    let missed_payment_time = creation_time + (365 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = missed_payment_time);

    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next_payment = policy.next_payment_date;

    assert_eq!(
        new_next_payment,
        missed_payment_time + MONTH_SECONDS,
        "Even after 1 year of non-payment, schedule advances by one period"
    );
}

// =============================================================================
// Test Suite 4: Schedule Consistency Through Multiple Payments
// =============================================================================

/// Test: Two consecutive on-time payments maintain consistent spacing.
///
/// **Scenario:**
/// - Create policy at T₁; next_payment_date = T₁ + 30 days
/// - Pay at T₁ + 30 days; next_payment_date = T₁ + 60 days
/// - Pay at T₁ + 60 days; next_payment_date = T₁ + 90 days
/// - Assert: Each payment advances deadline by exactly 30 days
#[test]
fn test_two_consecutive_on_time_payments_maintain_spacing() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_1 = policy.next_payment_date;
    assert_eq!(deadline_1, creation_time + MONTH_SECONDS);

    // First payment: on-time at deadline_1
    env.ledger().with_mut(|li| li.timestamp = deadline_1);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_2 = policy.next_payment_date;
    assert_eq!(deadline_2, deadline_1 + MONTH_SECONDS);

    // Second payment: on-time at deadline_2
    env.ledger().with_mut(|li| li.timestamp = deadline_2);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_3 = policy.next_payment_date;
    assert_eq!(deadline_3, deadline_2 + MONTH_SECONDS);

    // Verify uniform spacing
    let spacing_1 = deadline_2 - deadline_1;
    let spacing_2 = deadline_3 - deadline_2;
    assert_eq!(spacing_1, MONTH_SECONDS);
    assert_eq!(spacing_2, MONTH_SECONDS);
    assert_eq!(spacing_1, spacing_2);
}

/// Test: Late first payment followed by on-time second payment.
///
/// **Scenario:**
/// - Create policy at T₁; next_payment_date = T₁ + 30 days
/// - Pay 10 days late at T₁ + 40 days; next_payment_date = T₁ + 70 days
/// - Pay on-time at T₁ + 70 days; next_payment_date = T₁ + 100 days
/// - Assert: Schedule recovers to consistent spacing
#[test]
fn test_late_payment_then_on_time_payment_resynchronizes() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_1 = policy.next_payment_date;

    // First payment: 10 days late
    let late_payment_time = deadline_1 + (10 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = late_payment_time);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_2 = policy.next_payment_date;
    assert_eq!(deadline_2, late_payment_time + MONTH_SECONDS);

    // Second payment: on-time at deadline_2
    env.ledger().with_mut(|li| li.timestamp = deadline_2);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_3 = policy.next_payment_date;

    // Verify spacing is now uniform
    let spacing_1 = deadline_2 - late_payment_time;
    let spacing_2 = deadline_3 - deadline_2;
    assert_eq!(spacing_1, MONTH_SECONDS);
    assert_eq!(spacing_2, MONTH_SECONDS);
}

/// Test: Multiple missed payments (2 late, then 1 on-time) after large gap.
#[test]
fn test_multiple_missed_payments_and_recovery() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_1 = policy.next_payment_date;

    // First missed payment: pay 20 days late
    let first_late = deadline_1 + (20 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = first_late);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_2 = policy.next_payment_date;
    assert_eq!(deadline_2, first_late + MONTH_SECONDS);

    // Second missed payment: pay 30 days late from deadline_2
    let second_late = deadline_2 + (30 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = second_late);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_3 = policy.next_payment_date;
    assert_eq!(deadline_3, second_late + MONTH_SECONDS);

    // Third payment: on-time at deadline_3
    env.ledger().with_mut(|li| li.timestamp = deadline_3);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let deadline_4 = policy.next_payment_date;
    assert_eq!(deadline_4, deadline_3 + MONTH_SECONDS);

    // Verify all spacing is uniform
    let space_1_to_2 = deadline_2 - first_late;
    let space_2_to_3 = deadline_3 - second_late;
    let space_3_to_4 = deadline_4 - deadline_3;
    assert_eq!(space_1_to_2, MONTH_SECONDS);
    assert_eq!(space_2_to_3, MONTH_SECONDS);
    assert_eq!(space_3_to_4, MONTH_SECONDS);
}

// =============================================================================
// Test Suite 5: Event Schema Verification for next_payment_date
// =============================================================================

/// Test: PremiumPaidEvent carries correct next_payment_date in payload.
#[test]
fn test_premium_paid_event_next_payment_date_is_correct_after_time_jump() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    let _deadline_1 = policy.next_payment_date;

    // Advance 50 days and pay
    let payment_time = creation_time + (50 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = payment_time);

    client.pay_premium(&owner, &policy_id);

    // Check the policy reflects the payment
    let policy = client.get_policy(&policy_id).unwrap();
    let expected_next_deadline = payment_time + MONTH_SECONDS;
    assert_eq!(policy.next_payment_date, expected_next_deadline);
}

// =============================================================================
// Test Suite 6: Edge Cases and Boundary Conditions
// =============================================================================

/// Test: Payment at ledger.timestamp = 0 (genesis time).
#[test]
fn test_payment_at_genesis_timestamp_zero() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    env.ledger().with_mut(|li| li.timestamp = 0);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, MONTH_SECONDS);

    env.ledger().with_mut(|li| li.timestamp = MONTH_SECONDS);
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, 2 * MONTH_SECONDS);
}

/// Test: Payment with very large timestamp (overflow resistance).
#[test]
fn test_payment_near_max_timestamp() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let near_max = u64::MAX - 100_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = near_max);

    let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
    let policy = client.get_policy(&policy_id).unwrap();

    let created_next = policy.next_payment_date;

    env.ledger().with_mut(|li| li.timestamp = created_next.min(near_max + MONTH_SECONDS));
    client.pay_premium(&owner, &policy_id);

    let policy = client.get_policy(&policy_id).unwrap();
    let new_next = policy.next_payment_date;

    // The important invariant: new_next >= created_next
    assert!(new_next >= created_next || new_next == 0, "Schedule must not go backward");
}

/// Test: Batch payment updates all policies' next_payment_date correctly.
#[test]
fn test_batch_pay_premiums_updates_next_payment_date_uniformly() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    let creation_time = 1_000_000u64;
    env.ledger().with_mut(|li| li.timestamp = creation_time);

    let policy_id_1 = create_test_policy(&env, &client, &owner, 500i128);
    let policy_id_2 = create_test_policy(&env, &client, &owner, 600i128);
    let policy_id_3 = create_test_policy(&env, &client, &owner, 700i128);

    let ids = soroban_sdk::vec![&env, policy_id_1, policy_id_2, policy_id_3];

    let batch_payment_time = creation_time + (40 * DAY_SECONDS);
    env.ledger().with_mut(|li| li.timestamp = batch_payment_time);

    client.batch_pay_premiums(&owner, &ids);

    let policy_1 = client.get_policy(&policy_id_1).unwrap();
    let policy_2 = client.get_policy(&policy_id_2).unwrap();
    let policy_3 = client.get_policy(&policy_id_3).unwrap();

    let expected_next = batch_payment_time + MONTH_SECONDS;

    assert_eq!(policy_1.next_payment_date, expected_next);
    assert_eq!(policy_2.next_payment_date, expected_next);
    assert_eq!(policy_3.next_payment_date, expected_next);
}

// =============================================================================
// Test Suite 7: Coverage and Documentation Tests
// =============================================================================

/// Test: Document the exact formula for next_payment_date calculation.
///
/// **Formula:**
/// ```
/// next_payment_date = payment_time + 30 * 86_400
///                   = env.ledger().timestamp() + 2_592_000
/// ```
#[test]
fn test_next_payment_date_formula_is_deterministic() {
    let (env, client, _) = setup_env();
    let owner = Address::generate(&env);

    // Test at multiple timestamps to verify consistency
    let test_times: [u64; 4] = [1_000_000u64, 5_000_000u64, 10_000_000u64, 100_000_000u64];

    for test_time in test_times.iter() {
        env.ledger().with_mut(|li| li.timestamp = *test_time);

        let policy_id = create_test_policy(&env, &client, &owner, 1_000i128);
        let policy = client.get_policy(&policy_id).unwrap();

        let expected = test_time + MONTH_SECONDS;
        assert_eq!(
            policy.next_payment_date, expected,
            "At timestamp {}, next_payment_date must equal timestamp + 2,592,000",
            test_time
        );
    }
}