// Issue #887 — safe-math wrapper tests for `savings_goals`.
//
// # Coverage
//
// `add_to_goal` — checked_add path
// ──────────────────────────────────
// `add_to_goal` internally computes:
//   `new_total = goal.current_amount.checked_add(amount)`
// A `None` result (i128 wrap) returns `SavingsGoalError::Overflow`.
// Additionally, if `current_amount > MAX_SAFE_GOAL_BALANCE (i128::MAX/2)` the
// contract returns `SavingsGoalError::Overflow` as a secondary safety guard.
//
// `withdraw_from_goal` — checked_sub path
// ────────────────────────────────────────
// `withdraw_from_goal` internally computes:
//   `goal.current_amount.checked_sub(amount).ok_or(SavingsGoalError::Overflow)`
// Withdrawing more than the balance causes checked_sub to return None, which
// maps to `SavingsGoalError::Overflow` (underflow guard).
// Attempting to withdraw when the balance would go negative maps to
// `SavingsGoalError::InsufficientBalance` via the pre-check.
//
// Tests verify:
//   • Happy path — normal additions and withdrawals succeed with correct totals.
//   • checked_add overflow — adding beyond MAX_SAFE_GOAL_BALANCE → Overflow.
//   • checked_sub underflow — withdrawing more than balance → returns error.
//   • Zero and negative amounts — rejected with InvalidAmount.
//   • Determinism — identical sequences produce identical results.
//
// All tests are deterministic.  No std:: in non-test paths.

#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as AddressTrait, Address, Env, String};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, SavingsGoalContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let cid = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(&env, &cid);
    let owner = Address::generate(&env);
    client.init();
    (env, client, owner)
}

fn make_goal(env: &Env, client: &SavingsGoalContractClient<'_>, owner: &Address, target: i128) -> u32 {
    client.create_goal(owner, &String::from_str(env, "Test Goal"), &target, &2_000_000u64, &false)
}

// ─── add_to_goal — happy path ─────────────────────────────────────────────────

/// Adding a positive amount to an empty goal returns the same amount as the new total.
#[test]
fn checked_add_returns_new_total_on_valid_addition() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);

    let new_total = client.add_to_goal(&owner, &goal_id, &500i128);

    assert_eq!(new_total, 500i128);
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500i128);
}

/// Sequential additions accumulate correctly via checked_add.
#[test]
fn checked_add_accumulates_correctly_across_sequential_additions() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 10_000i128);

    let after_first = client.add_to_goal(&owner, &goal_id, &100i128);
    assert_eq!(after_first, 100i128);

    let after_second = client.add_to_goal(&owner, &goal_id, &200i128);
    assert_eq!(after_second, 300i128);

    let after_third = client.add_to_goal(&owner, &goal_id, &700i128);
    assert_eq!(after_third, 1_000i128);
}

/// Adding exactly MAX_SAFE_GOAL_BALANCE (i128::MAX/2) to an empty goal succeeds.
#[test]
fn checked_add_succeeds_at_max_safe_goal_balance_boundary() {
    let safe_cap = MAX_SAFE_GOAL_BALANCE;
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, safe_cap);

    let new_total = client.add_to_goal(&owner, &goal_id, &safe_cap);

    assert_eq!(new_total, safe_cap);
}

// ─── add_to_goal — checked_add overflow → SavingsGoalError::Overflow ─────────

/// Adding any amount after reaching MAX_SAFE_GOAL_BALANCE returns Overflow error.
#[test]
fn checked_add_returns_error_on_overflow() {
    let (env, client, owner) = setup();
    let safe_cap = MAX_SAFE_GOAL_BALANCE;

    // Use a target beyond safe_cap so the goal accepts the initial fill.
    let goal_id = make_goal(&env, &client, &owner, i128::MAX);

    // Fill up to safe_cap boundary.
    let filled = client.add_to_goal(&owner, &goal_id, &safe_cap);
    assert_eq!(filled, safe_cap);

    // Any further addition must return Overflow, not panic.
    let result = client.try_add_to_goal(&owner, &goal_id, &1i128);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::Overflow)),
        "addition past MAX_SAFE_GOAL_BALANCE must return Overflow error, not a panic"
    );
}

/// Adding i128::MAX to a fresh goal triggers the safe-cap guard → Overflow.
#[test]
fn checked_add_returns_error_when_single_contribution_exceeds_safe_cap() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, i128::MAX);

    // i128::MAX / 2 + 1 is beyond the safe cap.
    let over_cap = MAX_SAFE_GOAL_BALANCE + 1;
    let result = client.try_add_to_goal(&owner, &goal_id, &over_cap);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::Overflow)),
        "single contribution exceeding MAX_SAFE_GOAL_BALANCE must return Overflow"
    );
}

/// Overflow error is stable across repeated attempts (idempotent error code).
#[test]
fn checked_add_overflow_error_is_stable_across_repeated_attempts() {
    let (env, client, owner) = setup();
    let safe_cap = MAX_SAFE_GOAL_BALANCE;
    let goal_id = make_goal(&env, &client, &owner, i128::MAX);
    client.add_to_goal(&owner, &goal_id, &safe_cap);

    for _ in 0..3 {
        let result = client.try_add_to_goal(&owner, &goal_id, &1i128);
        assert_eq!(
            result,
            Err(Ok(SavingsGoalError::Overflow)),
            "Overflow error must be deterministically stable across repeated attempts"
        );
    }

    // Balance must not have changed.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, safe_cap);
}

// ─── add_to_goal — invalid amount → SavingsGoalError::InvalidAmount ───────────

/// Zero amount is rejected before any arithmetic.
#[test]
fn checked_add_returns_error_on_zero_amount() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);

    let result = client.try_add_to_goal(&owner, &goal_id, &0i128);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::InvalidAmount)),
        "zero amount must return InvalidAmount, not Overflow"
    );
}

/// Negative amount is rejected before any arithmetic.
#[test]
fn checked_add_returns_error_on_negative_amount() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);

    let result = client.try_add_to_goal(&owner, &goal_id, &-1i128);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::InvalidAmount)),
        "negative amount must return InvalidAmount, not Overflow"
    );
}

// ─── withdraw_from_goal — happy path ──────────────────────────────────────────

/// Withdrawing less than the balance returns the correct remaining amount.
#[test]
fn checked_sub_returns_remaining_balance_on_valid_withdrawal() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);

    client.add_to_goal(&owner, &goal_id, &1_000i128);
    client.unlock_goal(&owner, &goal_id);

    let remaining = client.withdraw_from_goal(&owner, &goal_id, &400i128);
    assert_eq!(remaining, 600i128);
}

/// Withdrawing the exact balance leaves the goal at zero (no negative wrap).
#[test]
fn checked_sub_returns_zero_when_withdrawing_exact_balance() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 500i128);

    client.add_to_goal(&owner, &goal_id, &500i128);
    client.unlock_goal(&owner, &goal_id);

    let remaining = client.withdraw_from_goal(&owner, &goal_id, &500i128);
    assert_eq!(remaining, 0i128);

    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 0i128);
}

/// Withdrawing with large amounts near the i128 range boundary succeeds
/// and respects conservation: remaining + withdrawn == initial balance.
#[test]
fn checked_sub_conserves_balance_for_large_amounts() {
    let (env, client, owner) = setup();
    let large_amount = MAX_SAFE_GOAL_BALANCE;
    let goal_id = make_goal(&env, &client, &owner, large_amount);

    client.add_to_goal(&owner, &goal_id, &large_amount);
    client.unlock_goal(&owner, &goal_id);

    let to_withdraw = large_amount / 4;
    let remaining = client.withdraw_from_goal(&owner, &goal_id, &to_withdraw);

    assert_eq!(remaining + to_withdraw, large_amount, "conservation: remaining + withdrawn must equal initial");
}

// ─── withdraw_from_goal — checked_sub underflow → error ──────────────────────

/// Withdrawing more than the balance must return an error (not panic, not underflow).
#[test]
fn checked_sub_returns_error_on_underflow() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 100i128);

    client.add_to_goal(&owner, &goal_id, &100i128);
    client.unlock_goal(&owner, &goal_id);

    let result = client.try_withdraw_from_goal(&owner, &goal_id, &101i128);
    assert!(
        result.is_err(),
        "withdrawing more than balance must return an error, not panic"
    );
    // Balance must be unchanged after failed withdrawal.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 100i128);
}

/// Withdrawing from a zero-balance goal must return an error (not a panic).
#[test]
fn checked_sub_returns_error_when_balance_is_zero() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);
    client.unlock_goal(&owner, &goal_id);

    // No funds were ever added — balance is zero.
    let result = client.try_withdraw_from_goal(&owner, &goal_id, &1i128);
    assert!(
        result.is_err(),
        "withdrawing from zero balance must return an error, not panic"
    );
}

/// Withdrawing i128::MAX from a goal with balance 1 must return an error.
#[test]
fn checked_sub_returns_error_when_withdrawal_is_i128_max() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 1_000_000i128);

    client.add_to_goal(&owner, &goal_id, &1i128);
    client.unlock_goal(&owner, &goal_id);

    let result = client.try_withdraw_from_goal(&owner, &goal_id, &i128::MAX);
    assert!(
        result.is_err(),
        "withdrawing i128::MAX when balance is 1 must return an error, not panic"
    );

    // Balance must be unchanged.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 1i128);
}

/// Underflow error is stable across repeated failed withdrawal attempts.
#[test]
fn checked_sub_underflow_error_is_stable_across_repeated_attempts() {
    let (env, client, owner) = setup();
    let goal_id = make_goal(&env, &client, &owner, 50i128);

    client.add_to_goal(&owner, &goal_id, &50i128);
    client.unlock_goal(&owner, &goal_id);

    for _ in 0..3 {
        let result = client.try_withdraw_from_goal(&owner, &goal_id, &51i128);
        assert!(
            result.is_err(),
            "underflow error must be stable across repeated attempts"
        );
    }

    // Balance must never have changed.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 50i128);
}

// ─── determinism ─────────────────────────────────────────────────────────────

/// The same sequence of additions always produces the same final balance.
#[test]
fn checked_add_is_deterministic_for_identical_sequences() {
    let (env1, client1, owner1) = setup();
    let goal1 = make_goal(&env1, &client1, &owner1, 10_000i128);
    client1.add_to_goal(&owner1, &goal1, &1_000i128);
    client1.add_to_goal(&owner1, &goal1, &2_000i128);
    let total1 = client1.add_to_goal(&owner1, &goal1, &3_000i128);

    let (env2, client2, owner2) = setup();
    let goal2 = make_goal(&env2, &client2, &owner2, 10_000i128);
    client2.add_to_goal(&owner2, &goal2, &1_000i128);
    client2.add_to_goal(&owner2, &goal2, &2_000i128);
    let total2 = client2.add_to_goal(&owner2, &goal2, &3_000i128);

    assert_eq!(total1, total2, "identical addition sequences must produce identical totals");
}
