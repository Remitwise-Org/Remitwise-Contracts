// =========================================================================
// Issue #1250 — Investigation-epoch guard: active and cleared states
//
// The orchestrator's actor-epoch mechanism (`bump_actor_epoch` /
// `verify_matching_epoch`) supports two semantically distinct states:
//
//   • **Active**  — The current epoch is a value `N > 0` and an actor token
//     carrying `actor_epoch == N` is accepted by
//     `execute_remittance_flow_signed`.
//
//   • **Cleared** — The owner bumped the epoch (i.e., "cleared" the old
//     cohort).  Any token carrying the pre-bump epoch is now rejected with
//     `OrchestratorError::EpochMismatch`, while tokens carrying the new
//     epoch are accepted.
//
// These tests add coverage for:
//   1. `active` — tokens issued for the current epoch are accepted.
//   2. `cleared` — a single bump invalidates tokens from the prior epoch.
//   3. `boundary` — multiple sequential bumps each produce a fresh valid
//      epoch; only the latest epoch is ever accepted.
//   4. Ownership guard — non-owner callers cannot bump the epoch.
//
// The dispute_epoch_guard.rs test suite (Issue #1244) covers numeric
// tier sweep (prior/ancient/future).  This file is scoped strictly to the
// active/cleared lifecycle described in the Issue #1250 summary.
// =========================================================================

use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, Vec,
};

use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};

// ---------------------------------------------------------------------------
// Mock downstream contract
// ---------------------------------------------------------------------------

/// A lightweight mock that satisfies the five cross-contract interfaces the
/// orchestrator fan-out calls.  All methods succeed immediately.
#[contract]
struct MockDownstream;

#[contractimpl]
impl MockDownstream {
    pub fn check_spending_limit(
        _env: Env,
        _orchestrator: Address,
        _epoch: u64,
        _user: Address,
        _amount: i128,
    ) -> bool {
        true
    }
    pub fn calculate_split(
        env: Env,
        _orchestrator: Address,
        _epoch: u64,
        _total_amount: i128,
    ) -> Vec<i128> {
        soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
    }
    pub fn add_to_goal(
        _env: Env,
        _orchestrator: Address,
        _epoch: u64,
        _caller: Address,
        _goal_id: u32,
        _amount: i128,
    ) {
    }
    pub fn pay_bill(
        _env: Env,
        _orchestrator: Address,
        _epoch: u64,
        _caller: Address,
        _bill_id: u32,
    ) {
    }
    pub fn pay_premium(
        _env: Env,
        _orchestrator: Address,
        _epoch: u64,
        _caller: Address,
        _policy_id: u32,
    ) {
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Bootstraps a fresh `Env` with one orchestrator + five independent
/// `MockDownstream` instances, calls `init`, and returns `(owner, client)`.
fn boot(env: &Env) -> (Address, OrchestratorClient<'_>) {
    let owner = Address::generate(env);
    let orch_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(env, &orch_id);

    let fw = env.register_contract(None, MockDownstream);
    let rs = env.register_contract(None, MockDownstream);
    let sg = env.register_contract(None, MockDownstream);
    let bp = env.register_contract(None, MockDownstream);
    let ins = env.register_contract(None, MockDownstream);

    client.init(&owner, &fw, &rs, &sg, &bp, &ins);
    (owner, client)
}

/// Replicates the request-hash computation used internally by the orchestrator.
/// Folds in the default routing IDs written by `Orchestrator::init`
/// (`goal_id = 1`, `bill_id = 1`, `policy_id = 1`).
fn request_hash(nonce: u64, amount: i128, deadline: u64) -> u64 {
    let op_bits: u64 = symbol_short!("flow").to_val().get_payload();
    let amt_lo = amount as u64;
    let amt_hi = (amount >> 64) as u64;
    op_bits
        .wrapping_add(nonce)
        .wrapping_add(amt_lo)
        .wrapping_add(amt_hi)
        .wrapping_add(deadline)
        .wrapping_add(1u64) // goal_id
        .wrapping_add(1u64) // bill_id
        .wrapping_add(1u64) // policy_id
        .wrapping_mul(1_000_000_007)
}

// ---------------------------------------------------------------------------
// State 1 — ACTIVE: a token issued for the current epoch is accepted
// ---------------------------------------------------------------------------

/// Fresh contract: epoch starts at 0.  A token with `actor_epoch = 0` must
/// be accepted, confirming the "active" baseline state.
#[test]
fn active_epoch_zero_token_accepted_on_fresh_contract() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (_owner, client) = boot(&env);

    let current = client.get_actor_epoch_public();
    assert_eq!(current, 0, "epoch must be 0 after init");

    let amount = 1_000i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let hash = request_hash(nonce, amount, deadline);
    let executor = Address::generate(&env);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &amount, &nonce, &deadline, &hash, &current);
    assert_eq!(
        result,
        Ok(Ok(true)),
        "epoch-0 token must be accepted on a fresh contract (active state)"
    );
}

/// After one bump, epoch = 1.  A token with `actor_epoch = 1` must be
/// accepted (still in the "active" state for the new epoch).
#[test]
fn active_epoch_token_accepted_after_single_bump() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    client.bump_actor_epoch(&owner);
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 1);

    let amount = 500i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let hash = request_hash(nonce, amount, deadline);
    let executor = Address::generate(&env);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &amount, &nonce, &deadline, &hash, &current);
    assert_eq!(
        result,
        Ok(Ok(true)),
        "epoch-1 token must be accepted when current epoch is 1"
    );
    // Nonce must have advanced, confirming the flow actually ran.
    assert_eq!(client.get_nonce(&executor), 1);
}

// ---------------------------------------------------------------------------
// State 2 — CLEARED: bumping the epoch invalidates all prior-epoch tokens
// ---------------------------------------------------------------------------

/// After one bump the old epoch (0) is cleared.  Any token carrying
/// `actor_epoch = 0` must now be rejected with `EpochMismatch`.
#[test]
fn cleared_old_epoch_token_rejected_after_single_bump() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    let old_epoch: u64 = 0; // epoch before bump
    client.bump_actor_epoch(&owner);
    let new_epoch = client.get_actor_epoch_public();
    assert_eq!(new_epoch, 1);

    let deadline = env.ledger().timestamp() + 3_600;
    let hash = request_hash(0, 1_000, deadline);
    let executor = Address::generate(&env);

    let result = client.try_execute_remittance_flow_signed(
        &executor, &1_000i128, &0u64, &deadline, &hash, &old_epoch,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "epoch-0 token must be rejected (cleared) after a bump to epoch 1"
    );
}

/// A token from two bumps ago is also rejected — "cleared" is permanent,
/// not a one-step grace window.
#[test]
fn cleared_two_bumps_ago_epoch_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    let stale_epoch: u64 = 0;
    client.bump_actor_epoch(&owner); // epoch → 1
    client.bump_actor_epoch(&owner); // epoch → 2
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 2);

    let deadline = env.ledger().timestamp() + 3_600;
    let hash = request_hash(0, 1_000, deadline);
    let executor = Address::generate(&env);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &1_000i128,
        &0u64,
        &deadline,
        &hash,
        &stale_epoch,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "stale epoch-0 token must still be rejected two bumps later"
    );
}

/// After clearing, the intermediate epoch (1 in a 0→1→2 sequence) is also
/// rejected — only the latest epoch is ever valid.
#[test]
fn cleared_intermediate_epoch_token_rejected_after_second_bump() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    client.bump_actor_epoch(&owner); // epoch → 1
    let intermediate: u64 = client.get_actor_epoch_public(); // 1
    client.bump_actor_epoch(&owner); // epoch → 2
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 2);
    assert_eq!(intermediate, 1);

    let deadline = env.ledger().timestamp() + 3_600;
    let hash = request_hash(0, 1_000, deadline);
    let executor = Address::generate(&env);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &1_000i128,
        &0u64,
        &deadline,
        &hash,
        &intermediate,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "intermediate epoch-1 token must be rejected once epoch has moved to 2"
    );
}

// ---------------------------------------------------------------------------
// State 3 — BOUNDARY: many bumps, only the final epoch is active
// ---------------------------------------------------------------------------

/// After N bumps the epoch is exactly N.  The only accepted token is the one
/// carrying epoch N; every prior value [0, N-1] is rejected.
#[test]
fn boundary_only_latest_epoch_accepted_after_many_bumps() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    const N: u64 = 10;
    for _ in 0..N {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, N);

    let amount = 750i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let executor = Address::generate(&env);

    // All prior epochs must be rejected.
    for stale in 0..N {
        let hash = request_hash(nonce, amount, deadline);
        let result = client.try_execute_remittance_flow_signed(
            &executor, &amount, &nonce, &deadline, &hash, &stale,
        );
        assert_eq!(
            result,
            Err(Ok(OrchestratorError::EpochMismatch)),
            "epoch {} must be rejected; current is {}",
            stale,
            N
        );
    }

    // Current epoch must be accepted.
    let hash = request_hash(nonce, amount, deadline);
    let result = client
        .try_execute_remittance_flow_signed(&executor, &amount, &nonce, &deadline, &hash, &current);
    assert_eq!(
        result,
        Ok(Ok(true)),
        "current epoch {} must be the only accepted value",
        current
    );
}

/// Each sequential bump produces a monotonically increasing epoch value.
/// The guard emits an epoch-bump event with (old, new) each time.
#[test]
fn boundary_epoch_increments_monotonically_with_each_bump() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client) = boot(&env);

    let mut prev = client.get_actor_epoch_public();
    assert_eq!(prev, 0);

    for _ in 0..5 {
        let returned = client.bump_actor_epoch(&owner);
        let next = client.get_actor_epoch_public();
        assert_eq!(returned, next, "returned value must equal stored value");
        assert_eq!(next, prev + 1, "each bump must increase epoch by exactly 1");
        prev = next;
    }
    assert_eq!(prev, 5);
}

// ---------------------------------------------------------------------------
// Ownership guard
// ---------------------------------------------------------------------------

/// A non-owner address cannot bump the epoch.  The error must be
/// `OrchestratorError::Unauthorized`, not a panic.
#[test]
fn non_owner_cannot_bump_epoch() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (_owner, client) = boot(&env);

    let intruder = Address::generate(&env);
    let result = client.try_bump_actor_epoch(&intruder);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::Unauthorized)),
        "a non-owner bump attempt must be rejected with Unauthorized"
    );

    // Epoch must be unchanged.
    assert_eq!(client.get_actor_epoch_public(), 0);
}
