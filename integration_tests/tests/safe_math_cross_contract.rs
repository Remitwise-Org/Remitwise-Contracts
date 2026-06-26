// Issue #887 — cross-module safe-math integration tests.
//
// These tests exercise the safe-math / checked-arithmetic error paths across
// multiple contracts in a single environment, verifying that:
//
//   1. `remittance_split::calculate_split` returns `Overflow` (not a panic)
//      when `total_amount > i128::MAX / 100`.
//
//   2. `savings_goals::add_to_goal` returns `Overflow` (not a panic) when the
//      running balance would exceed `MAX_SAFE_GOAL_BALANCE (i128::MAX / 2)`.
//
//   3. `savings_goals::withdraw_from_goal` returns an error (not a panic) when
//      the caller tries to withdraw more than the current balance.
//
//   4. A full cross-contract flow (split → add to goals) propagates cleanly
//      when all values are within the safe range, proving the contracts can
//      compose without arithmetic failures.
//
//   5. Overflow errors in one contract do not corrupt the state of sibling
//      contracts sharing the same environment.
//
// All inputs are deterministic constants — no Date::now() or random values.

use remittance_split::{RemittanceSplit, RemittanceSplitClient, RemittanceSplitError};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient, SavingsGoalError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String as SorobanString};

// ─── constants ───────────────────────────────────────────────────────────────

/// Largest total_amount where `total * any_pct` (0–100) fits in i128.
const MAX_SAFE_SPLIT_TOTAL: i128 = i128::MAX / 100;

/// Largest cumulative balance allowed by savings_goals before Overflow fires.
const MAX_SAFE_GOAL_BALANCE: i128 = i128::MAX / 2;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn make_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn new_split_client(env: &Env, sp: u32, sg: u32, sb: u32, si: u32)
    -> (RemittanceSplitClient<'_>, Address)
{
    let cid = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &cid);
    let owner = Address::generate(env);
    let token = Address::generate(env);
    client.initialize_split(&owner, &0, &token, &sp, &sg, &sb, &si);
    (client, owner)
}

fn new_savings_client(env: &Env) -> (SavingsGoalContractClient<'_>, Address) {
    let cid = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(env, &cid);
    let owner = Address::generate(env);
    client.init();
    (client, owner)
}

fn make_goal(
    env: &Env,
    client: &SavingsGoalContractClient<'_>,
    owner: &Address,
    target: i128,
) -> u32 {
    client.create_goal(
        owner,
        &SorobanString::from_str(env, "Integration Goal"),
        &target,
        &2_000_000u64,
        &false,
    )
}

// ─── 1. remittance_split overflow does not panic ──────────────────────────────

/// calculate_split with total > i128::MAX/100 returns RemittanceSplitError::Overflow.
#[test]
fn split_checked_mul_returns_error_on_overflow_not_a_panic() {
    let env = make_env();
    let (client, _) = new_split_client(&env, 50, 25, 15, 10);

    // 50% split means the binding multiplier is 50.
    // i128::MAX / 50 + 1 overflows checked_mul(total, 50).
    let overflow_total = i128::MAX / 50 + 1;
    let result = client.try_calculate_split(&overflow_total);

    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Overflow)),
        "remittance_split: overflow must return Overflow error, not panic"
    );
}

/// calculate_split with i128::MAX returns Overflow, not a panic.
#[test]
fn split_checked_mul_returns_error_on_i128_max() {
    let env = make_env();
    let (client, _) = new_split_client(&env, 25, 25, 25, 25);

    let result = client.try_calculate_split(&i128::MAX);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Overflow)),
        "remittance_split: i128::MAX must return Overflow error, not panic"
    );
}

// ─── 2. savings_goals checked_add overflow does not panic ─────────────────────

/// add_to_goal past MAX_SAFE_GOAL_BALANCE returns SavingsGoalError::Overflow.
#[test]
fn savings_checked_add_returns_error_on_overflow_not_a_panic() {
    let env = make_env();
    let (client, owner) = new_savings_client(&env);
    let goal_id = make_goal(&env, &client, &owner, i128::MAX);

    // Fill to the safe cap.
    client.add_to_goal(&owner, &goal_id, &MAX_SAFE_GOAL_BALANCE);

    // One more unit must fail gracefully.
    let result = client.try_add_to_goal(&owner, &goal_id, &1i128);
    assert_eq!(
        result,
        Err(Ok(SavingsGoalError::Overflow)),
        "savings_goals: checked_add overflow must return Overflow error, not panic"
    );
}

// ─── 3. savings_goals checked_sub underflow does not panic ───────────────────

/// withdraw_from_goal with amount > balance returns an error (not a panic).
#[test]
fn savings_checked_sub_returns_error_on_underflow_not_a_panic() {
    let env = make_env();
    let (client, owner) = new_savings_client(&env);
    let goal_id = make_goal(&env, &client, &owner, 1_000i128);

    client.add_to_goal(&owner, &goal_id, &500i128);
    client.unlock_goal(&owner, &goal_id);

    let result = client.try_withdraw_from_goal(&owner, &goal_id, &501i128);
    assert!(
        result.is_err(),
        "savings_goals: checked_sub underflow must return an error, not panic"
    );

    // Balance must be unchanged.
    let goal = client.get_goal(&goal_id).unwrap();
    assert_eq!(goal.current_amount, 500i128);
}

// ─── 4. Clean cross-contract flow within safe range ──────────────────────────

/// A full split → add-to-goals flow succeeds when all values are in range.
/// Verifies the safe-math paths are exercised end-to-end without errors.
#[test]
fn split_to_savings_flow_succeeds_within_safe_range() {
    let env = make_env();

    // Register split contract (50/25/15/10).
    let (split_client, _) = new_split_client(&env, 50, 25, 15, 10);

    // Register savings contract.
    let (savings_client, owner) = new_savings_client(&env);

    let total = 10_000i128;
    let amounts = split_client.calculate_split(&total);

    // spending bucket = 50% of 10_000 = 5_000.
    let spending_alloc = amounts.get(0).unwrap();
    assert_eq!(spending_alloc, 5_000i128);

    // Add the spending allocation to a savings goal.
    let goal_id = make_goal(&env, &savings_client, &owner, total);
    let new_total = savings_client.add_to_goal(&owner, &goal_id, &spending_alloc);
    assert_eq!(new_total, spending_alloc);

    let sum: i128 = amounts.iter().sum();
    assert_eq!(sum, total, "split conservation must hold before passing values to savings");
}

// ─── 5. Overflow in split does not corrupt savings state ──────────────────────

/// An overflow in remittance_split must not affect savings_goals state.
#[test]
fn overflow_in_split_does_not_corrupt_savings_state() {
    let env = make_env();

    let (split_client, _) = new_split_client(&env, 50, 25, 15, 10);
    let (savings_client, owner) = new_savings_client(&env);

    // Pre-populate a goal with a known balance.
    let goal_id = make_goal(&env, &savings_client, &owner, 5_000i128);
    savings_client.add_to_goal(&owner, &goal_id, &1_000i128);

    // Trigger overflow in split (i128::MAX overflows 50% mul).
    let split_result = split_client.try_calculate_split(&i128::MAX);
    assert!(split_result.is_err(), "split must return an error on overflow");

    // Savings goal balance must be unaffected.
    let goal = savings_client.get_goal(&goal_id).unwrap();
    assert_eq!(
        goal.current_amount, 1_000i128,
        "savings state must not be corrupted by overflow in a sibling contract"
    );
}

// ─── 6. Boundary: one step below and at MAX_SAFE_SPLIT_TOTAL ─────────────────

/// MAX_SAFE_SPLIT_TOTAL itself must succeed and conserve the total.
#[test]
fn split_succeeds_at_max_safe_split_total_boundary() {
    let env = make_env();
    let (client, _) = new_split_client(&env, 25, 25, 25, 25);

    let amounts = client.calculate_split(&MAX_SAFE_SPLIT_TOTAL);
    let sum: i128 = amounts.iter().sum();
    assert_eq!(sum, MAX_SAFE_SPLIT_TOTAL, "sum must equal total at boundary");
}

/// One step above MAX_SAFE_SPLIT_TOTAL must return Overflow.
#[test]
fn split_returns_overflow_one_step_above_max_safe_boundary() {
    let env = make_env();
    let (client, _) = new_split_client(&env, 25, 25, 25, 25);

    let result = client.try_calculate_split(&(MAX_SAFE_SPLIT_TOTAL + 1));
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Overflow)),
        "one step above safe boundary must return Overflow, not panic"
    );
}
