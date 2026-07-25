// =========================================================================
// Issue #1244 — Dispute-epoch guard boundary tests
//
// The private guard `Orchestrator::verify_matching_epoch`
// (`orchestrator/src/lib.rs` lines 1641–1647) enforces strict equality
// between an actor-supplied `actor_epoch` and the contract's stored
// `current_epoch` in the signed entrypoint
// `execute_remittance_flow_signed`. Any non-equal value — whether smaller
// (prior / ancient) or larger (future) — is rejected with the same typed
// `OrchestratorError::EpochMismatch`. There is no staleness window, no
// off-by-one tolerance, and no upper bound on the gap.
//
// The issue title pins three tiers named in the summary: *current*,
// *prior*, *ancient*. These tests cover all three, plus a symmetric
// *future* tier to lock in that no `>=` / `<=` comparator can sneak in,
// plus a sweep over a representative range that exercises
// `u64::MAX` to catch any silent overflow / wraparound.
//
// Each test is hosted in its own integration-test crate (this file lives
// under `orchestrator/tests/`) so these are compiled independently of the
// in-`src/` test suite. The pre-existing compile failures in
// `orchestrator/src/test.rs` block neither the build of this file nor
// `cargo build --release --target wasm32-unknown-unknown -p orchestrator`.
// =========================================================================

use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Address as _, Address, Env, Symbol, Vec,
};

use orchestrator::{Orchestrator, OrchestratorClient, OrchestratorError};

// ---------------------------------------------------------------------------
// Mock downstream contract — implements the five trait methods the
// orchestrator's signed fan-out actually invokes via `try_*` calls.
// Returning `bool true` matches the behaviour of the existing in-`src/`
// `MockContract`, which is verified end-to-end by the suite at large.
// ---------------------------------------------------------------------------
#[contract]
struct MockDep;

#[contractimpl]
impl MockDep {
    pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
        true
    }
    pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
        soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
    }
    pub fn add_to_goal(_env: Env, _caller: Address, _goal_id: u32, _amount: i128) -> bool {
        true
    }
    pub fn pay_bill(_env: Env, _caller: Address, _bill_id: u32, _amount: i128) -> bool {
        true
    }
    pub fn pay_premium(_env: Env, _caller: Address, _policy_id: u32, _amount: i128) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Replicates `Orchestrator::compute_request_hash` (private to the crate),
/// with `(goal_id, bill_id, policy_id) = (1, 1, 1)` folded in.
///
/// **Invariant**: this mirrors the defaults that `Orchestrator::init` writes
/// to instance storage via `set(&GOAL_ID, &1u32)` etc. If those defaults
/// ever drift in `Orchestrator::init`, update this replica in lockstep —
/// otherwise `accepts_current_epoch_at_non_zero_value` will fail with a
/// hash-mismatch symptom that looks unrelated to the epoch guard.
fn signed_request_hash(operation: Symbol, nonce: u64, amount: i128, deadline: u64) -> u64 {
    let op_bits: u64 = operation.to_val().get_payload();
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

/// Bootstraps a fresh `Env`, registers the orchestrator + 5 distinct
/// `MockDep` instances, calls `init`, and returns `(owner, client, executor)`.
fn boot(env: &Env) -> (Address, OrchestratorClient<'_>, Address) {
    let owner = Address::generate(env);
    let orch_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(env, &orch_id);

    let fw = env.register_contract(None, MockDep);
    let rs = env.register_contract(None, MockDep);
    let sg = env.register_contract(None, MockDep);
    let bp = env.register_contract(None, MockDep);
    let ins = env.register_contract(None, MockDep);

    client.init(&owner, &fw, &rs, &sg, &bp, &ins);

    let executor = Address::generate(env);
    (owner, client, executor)
}

// ---------------------------------------------------------------------------
// `current` tier — accepted when the actor supplies a value equal to the
// stored epoch. Complements the existing
// `test_matching_epoch_allows_execution` (which only exercises the
// initial `current_epoch == 0` baseline) by proving the equality check
// works at a non-zero stored epoch.
// ---------------------------------------------------------------------------
#[test]
fn accepts_current_epoch_at_non_zero_value() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    // Bump 5 times: current_epoch should land on 5.
    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 5);

    let amount = 1000i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 1000;
    let hash = signed_request_hash(symbol_short!("flow"), nonce, amount, deadline);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &amount, &nonce, &deadline, &hash, &current);
    assert_eq!(
        result,
        Ok(Ok(true)),
        "actor_epoch == current_epoch (== {}) must be accepted on a non-zero stored value",
        current,
    );
    // Nonce advanced: confirms the flow actually executed, not silently
    // rejected by some downstream check after passing the epoch guard.
    assert_eq!(client.get_nonce(&executor), 1);
}

// ---------------------------------------------------------------------------
// `prior` tier — `current - 1` is rejected even though it is the value
// that *was* current one bump ago. The smallest non-equal value below
// current must fail; this pins the strict-equality contract.
// ---------------------------------------------------------------------------
#[test]
fn rejects_prior_epoch_one_below_current() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    let prior = current - 1;
    assert_eq!(prior, 4);

    let deadline = env.ledger().timestamp() + 1000;
    // The epoch check (step 5) fires BEFORE the deadline/nonce/hash binding
    // (step 6), so the hash value is irrelevant here — any u64 suffices.
    let hash = signed_request_hash(symbol_short!("flow"), 0, 1000, deadline);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &1000i128, &0u64, &deadline, &hash, &prior);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "actor_epoch == current - 1 must be rejected; equality is the only accepted relation",
    );
}

// ---------------------------------------------------------------------------
// `ancient` tier — `actor_epoch = 0` is rejected after the contract has
// been bumped many times. Mirrors the real-world case of a stale token
// from a long-forgotten signing service; a `current - N` rejection for
// large `N` locks down the absence of any staleness window.
// ---------------------------------------------------------------------------
#[test]
fn rejects_ancient_epoch_at_zero_after_many_bumps() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 5);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = signed_request_hash(symbol_short!("flow"), 0, 1000, deadline);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &1000i128, &0u64, &deadline, &hash, &0u64);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "actor_epoch == 0 must be rejected after many bumps; there is no staleness window",
    );
}

// ---------------------------------------------------------------------------
// Symmetric `future` tier — `actor_epoch > current` is also rejected with
// the same typed error. Pins the symmetry of the guard around equality,
// so an accidental `>=` / `<=` comparator in the implementation cannot
// slip in.
// ---------------------------------------------------------------------------
#[test]
fn rejects_future_epoch_one_above_current() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    let future = current + 1;
    assert_eq!(future, 6);

    let deadline = env.ledger().timestamp() + 1000;
    let hash = signed_request_hash(symbol_short!("flow"), 0, 1000, deadline);

    let result = client
        .try_execute_remittance_flow_signed(&executor, &1000i128, &0u64, &deadline, &hash, &future);
    assert_eq!(
        result,
        Err(Ok(OrchestratorError::EpochMismatch)),
        "actor_epoch == current + 1 must be rejected; the guard is symmetric around equality",
    );
}

// ---------------------------------------------------------------------------
// Sweep — every non-matching tier (prior trough -> ancient ->
// current+1 -> extreme future) must surface the same typed
// `EpochMismatch` error uniformly. This pins down that the guard treats
// every off-by-one value the same way: no special-cased leniency for
// "immediately prior" or "extreme ancient", and no upper bound on the
// gap that would relax the rejection. `u64::MAX` is included to catch
// any silent overflow / wraparound in the guard.
// ---------------------------------------------------------------------------
#[test]
fn rejects_representative_non_matching_epochs_with_identical_error() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let (owner, client, executor) = boot(&env);

    for _ in 0..5u32 {
        client.bump_actor_epoch(&owner);
    }
    let current = client.get_actor_epoch_public();
    assert_eq!(current, 5);

    let amount = 1000i128;
    let nonce = 0u64;
    let deadline = env.ledger().timestamp() + 1000;
    let hash = signed_request_hash(symbol_short!("flow"), nonce, amount, deadline);

    // Every value except `current` must be rejected with EpochMismatch.
    // Includes prior-trough (0..=current-1), immediate-future (current+1),
    // middle (7, 100), and `u64::MAX`.
    for supplied in [0u64, 1, 2, 3, 4, 6, 7, 100, u64::MAX] {
        let result = client.try_execute_remittance_flow_signed(
            &executor, &amount, &nonce, &deadline, &hash, &supplied,
        );
        assert_eq!(
            result,
            Err(Ok(OrchestratorError::EpochMismatch)),
            "actor_epoch = {} (current = {}) must be rejected with EpochMismatch",
            supplied,
            current,
        );
    }
    // And `current` (5) is the only accepted value — covered above by
    // `accepts_current_epoch_at_non_zero_value`.
}
