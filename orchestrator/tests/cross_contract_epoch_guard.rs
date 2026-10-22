// =========================================================================
// Issue #1286 — Cross-contract epoch guard boundary tests
//
// The `Orchestrator` publishes an actor-epoch value via `get_actor_epoch_public`
// and mutates it via `bump_actor_epoch`.  Downstream callers (other contracts,
// off-chain signers, or relayer services) embed the epoch in every signed
// request.  The guard `verify_matching_epoch` enforces strict equality between
// the embedded value and the on-chain epoch.
//
// The cross-contract dimension: an *external* actor reads the current epoch
// from Contract A (the orchestrator), embeds it in a signed message, and then
// that message is replayed against the same contract after the epoch has
// changed.  Three tiers need to be locked in:
//
// * **Same** — actor_epoch == current epoch → accepted.
// * **Off-by-one** — actor_epoch == current ± 1 → rejected with EpochMismatch.
// * **Ancient** — actor_epoch is far behind the current epoch (e.g. 0 after
//   many bumps) → rejected with the same EpochMismatch error.
//
// These tests are hosted in the integration-test crate
// (`orchestrator/tests/`) so they compile independently of the in-`src/`
// suite and exercise the public contract API exactly as an external caller
// would.
// =========================================================================

use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, Vec,
};

use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};

// ---------------------------------------------------------------------------
// Minimal mock downstream contract — mirrors the one in dispute_epoch_guard.rs.
// All methods succeed unconditionally; we only care about the epoch guard.
// ---------------------------------------------------------------------------
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
// Helpers
// ---------------------------------------------------------------------------

/// Replicate `Orchestrator::compute_request_hash` (private) with the
/// default routing IDs that `Orchestrator::init` writes
/// (`goal_id = 1, bill_id = 1, policy_id = 1`).
///
/// If those defaults ever drift in `init`, update this replica in lockstep.
fn build_request_hash(nonce: u64, amount: i128, deadline: u64) -> u64 {
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

/// Boot a fresh orchestrator with five MockDownstream contracts and return
/// `(owner, client, executor)`.  Mirrors the pattern in `dispute_epoch_guard.rs`
/// so both test files exercise exactly the same setup path.
fn boot(env: &Env) -> (Address, OrchestratorClient<'_>, Address) {
    let owner = Address::generate(env);
    let orch_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(env, &orch_id);

    let fw = env.register_contract(None, MockDownstream);
    let rs = env.register_contract(None, MockDownstream);
    let sg = env.register_contract(None, MockDownstream);
    let bp = env.register_contract(None, MockDownstream);
    let ins = env.register_contract(None, MockDownstream);

    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let executor = Address::generate(env);
    (owner, client, executor)
}

// ---------------------------------------------------------------------------
// Same tier — the cross-contract call uses the epoch it just queried
// ---------------------------------------------------------------------------

/// An external actor queries `get_actor_epoch_public` and immediately embeds
/// the returned value in a signed request.  The call must succeed: querying
/// then using the same epoch value is the canonical cross-contract pattern.
#[test]
fn cross_contract_same_epoch_is_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    // Simulate several epoch bumps (as another contract might trigger).
    for _ in 0..3u32 {
        client.bump_actor_epoch(&owner);
    }

    // External actor reads the epoch — this is the cross-contract read.
    let queried_epoch = client.get_actor_epoch_public();
    assert_eq!(queried_epoch, 3, "epoch must reflect all bumps");

    let amount = 500i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(nonce, amount, deadline);

    // Cross-contract call: embed the queried epoch in the signed request.
    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &amount,
        &nonce,
        &deadline,
        &hash,
        &queried_epoch,
    );
    assert_eq!(
        result,
        Ok(Ok(true)),
        "request with the epoch obtained from get_actor_epoch_public must be accepted"
    );
}

/// Same-epoch acceptance holds at the initial epoch (0) before any bumps —
/// the epoch guard must not treat zero as a special-cased "unset" value.
#[test]
fn cross_contract_same_epoch_accepted_at_zero_before_any_bump() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (_owner, client, executor) = boot(&env);

    let epoch_zero = client.get_actor_epoch_public();
    assert_eq!(epoch_zero, 0);

    let amount = 100i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(nonce, amount, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &amount,
        &nonce,
        &deadline,
        &hash,
        &epoch_zero,
    );
    assert_eq!(
        result,
        Ok(Ok(true)),
        "epoch 0 must be accepted when no bumps have occurred"
    );
}

// ---------------------------------------------------------------------------
// Off-by-one tier — the external actor's cached epoch is stale by exactly one
// ---------------------------------------------------------------------------

/// An external actor caches the epoch, then the owner bumps once before the
/// signed request arrives.  The cached (now prior) value must be rejected with
/// `EpochMismatch`.  This is the exact cross-contract replay scenario the guard
/// is designed to prevent.
#[test]
fn cross_contract_off_by_one_below_current_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..3u32 {
        client.bump_actor_epoch(&owner);
    }

    // External actor reads and caches the current epoch (3).
    let cached_epoch = client.get_actor_epoch_public();
    assert_eq!(cached_epoch, 3);

    // Owner bumps the epoch — the cached value is now stale (off by one).
    client.bump_actor_epoch(&owner);
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 4);

    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, 500, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &500i128,
        &0u64,
        &deadline,
        &hash,
        &cached_epoch, // stale: current - 1
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "off-by-one (current - 1 = {cached_epoch}) must be rejected; there is no off-by-one tolerance"
    );
}

/// An external actor somehow has an epoch that is one *ahead* of the current
/// on-chain value (e.g. constructed a future epoch from a side channel).
/// The guard must reject it with the same `EpochMismatch`.
#[test]
fn cross_contract_off_by_one_above_current_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..3u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 3);

    let future_epoch = current + 1; // one ahead of on-chain value
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, 500, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &500i128,
        &0u64,
        &deadline,
        &hash,
        &future_epoch,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "off-by-one above current ({future_epoch}) must also be rejected"
    );
}

// ---------------------------------------------------------------------------
// Ancient tier — the external actor's cached epoch is many bumps behind
// ---------------------------------------------------------------------------

/// An actor that signed a token when the epoch was 0 and tries to replay it
/// after the epoch has advanced to a large value must be rejected.  Models the
/// real-world scenario of a long-dormant relayer re-submitting an old request.
#[test]
fn cross_contract_ancient_epoch_zero_rejected_after_many_bumps() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    // Many bumps — simulates a contract that has been live for a long time.
    for _ in 0..10u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 10);

    let ancient_epoch = 0u64;
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, 200, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &200i128,
        &0u64,
        &deadline,
        &hash,
        &ancient_epoch,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "ancient epoch 0 must be rejected when current epoch is {current}"
    );
}

/// A moderately stale epoch (5 bumps behind) is rejected with the same error
/// as an epoch that is 0.  There is no grace window for "recently stale"
/// cross-contract tokens.
#[test]
fn cross_contract_moderately_stale_epoch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..10u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();

    // 5 bumps behind the current epoch.
    let stale_epoch = current - 5;
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, 300, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &300i128,
        &0u64,
        &deadline,
        &hash,
        &stale_epoch,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "stale epoch {stale_epoch} (current - 5, current = {current}) must be rejected"
    );
}

/// `u64::MAX` as a cross-contract epoch value is rejected.  This pins the
/// absence of any overflow / wraparound behaviour in the guard.
#[test]
fn cross_contract_u64_max_epoch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..3u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_ne!(current, u64::MAX);

    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, 100, deadline);

    let result = client.try_execute_remittance_flow_signed(
        &executor,
        &100i128,
        &0u64,
        &deadline,
        &hash,
        &u64::MAX,
    );
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "u64::MAX must be rejected; guard must not wrap around"
    );
}

// ---------------------------------------------------------------------------
// Sweep — every non-matching value produces the same typed error
// ---------------------------------------------------------------------------

/// Exhaustive sweep over representative epoch values (prior trough, immediate
/// future, extreme values including `u64::MAX`).  Every value except `current`
/// must produce `EpochMismatch`.  Pins uniform error semantics across the full
/// cross-contract call surface.
#[test]
fn cross_contract_non_matching_epochs_all_produce_epoch_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 5);

    let amount = 400i128;
    let deadline = env.ledger().timestamp() + 500;
    let hash = build_request_hash(0, amount, deadline);

    // A representative set: prior-trough (0..=4), immediate-future (6),
    // mid-range (50, 1000), and the maximum u64 value.
    for bad_epoch in [0u64, 1, 2, 3, 4, 6, 50, 1000, u64::MAX] {
        let result = client.try_execute_remittance_flow_signed(
            &executor, &amount, &0u64, &deadline, &hash, &bad_epoch,
        );
        assert_eq!(
            result,
            Err(Ok(OrchestratorError::EpochMismatch)),
            "epoch {bad_epoch} (current = {current}) must produce EpochMismatch"
        );
    }
}

// ---------------------------------------------------------------------------
// Transition correctness — bump changes what is accepted
// ---------------------------------------------------------------------------

/// After the owner bumps the epoch the previously-accepted value is no longer
/// valid and the new value becomes the only accepted one.  Models the lifecycle
/// of a cross-contract client that must refresh its cached epoch after each bump.
#[test]
fn cross_contract_bump_makes_old_epoch_stale_and_new_epoch_valid() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    // Initial accepted epoch.
    let initial_epoch = client.get_actor_epoch_public();
    assert_eq!(initial_epoch, 0);

    // Bump: old value must now be rejected, new value must be accepted.
    let new_epoch = client.bump_actor_epoch(&owner);
    assert_eq!(new_epoch, 1);

    let amount = 200i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 500;
    let hash_new = build_request_hash(nonce, amount, deadline);

    // Old (stale) epoch rejected.
    let stale_result = client.try_execute_remittance_flow_signed(
        &executor,
        &amount,
        &nonce,
        &deadline,
        &hash_new,
        &initial_epoch,
    );
    assert_eq!(
        stale_result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "initial epoch {initial_epoch} must be stale after bump to {new_epoch}"
    );

    // New epoch accepted.
    let fresh_result = client.try_execute_remittance_flow_signed(
        &executor, &amount, &nonce, &deadline, &hash_new, &new_epoch,
    );
    assert_eq!(
        fresh_result,
        Ok(Ok(true)),
        "new epoch {new_epoch} must be accepted immediately after bump"
    );
}
