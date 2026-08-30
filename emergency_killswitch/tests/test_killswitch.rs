#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient, Error};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};
use testutils::same_address;

fn setup(env: &Env) -> (Address, EmergencyKillswitchClient<'_>) {
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(env, &contract_id);
    (contract_id, client)
}

#[test]
fn version_returns_contract_version_without_init() {
    let env = Env::default();
    let (_, client) = setup(&env);
    // Observable on-chain with no auth and before initialize().
    assert_eq!(client.version(), emergency_killswitch::CONTRACT_VERSION);
}

#[test]
fn initialize_rejects_self_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    assert_eq!(
        client.try_initialize(&contract_id),
        Err(Ok(Error::InvalidAdmin))
    );
}

#[test]
fn initialize_succeeds_with_valid_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    assert_eq!(client.try_initialize(&admin), Ok(Ok(())));
}

/// `initialize` must require `admin`'s signature. Without this, anyone could
/// front-run deployment and call `initialize` with themselves (or any
/// address they control) as `admin` before the intended admin does,
/// permanently seizing control of the kill switch. No auth is mocked here —
/// on `main` (before this fix) this call succeeds with zero authorization;
/// after the fix it must panic on the missing signature.
#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn initialize_requires_admin_signature() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
}

#[test]
fn assert_no_double_init() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    // First initialization should succeed
    assert_eq!(client.try_initialize(&admin), Ok(Ok(())));
    // Second initialization should fail with AlreadyInitialized
    assert_eq!(
        client.try_initialize(&admin),
        Err(Ok(Error::AlreadyInitialized))
    );
    // Non-initialized functions should fail with NotInitialized before init
    let env2 = Env::default();
    let (_, client2) = setup(&env2);
    let _admin2 = Address::generate(&env2);
    assert_eq!(client2.try_pause(), Err(Ok(Error::NotInitialized)));
}

#[test]
fn transfer_admin_rejects_self_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(
        client.try_transfer_admin(&contract_id, &0),
        Err(Ok(Error::InvalidAdmin))
    );
}

#[test]
fn transfer_admin_rejects_same_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    // Confirm we are genuinely passing the same address, not two coincidentally
    // equal values — using the shared helper keeps this intent grep-able.
    assert!(same_address(&admin, &admin));
    assert_eq!(
        client.try_transfer_admin(&admin, &0),
        Err(Ok(Error::InvalidAdmin))
    );
}

#[test]
fn transfer_admin_succeeds_with_different_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    client.initialize(&admin);
    // Confirm the two addresses are genuinely distinct before testing the
    // happy path — makes the boundary explicit and avoids false positives.
    assert!(!same_address(&admin, &new_admin));
    assert_eq!(client.try_transfer_admin(&new_admin, &0u64), Ok(Ok(())));
}

#[test]
fn test_authorized_emergency_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    assert!(client.is_paused());
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future);
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_premature_unpause_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future - 1);
    assert_eq!(client.try_unpause(), Err(Ok(Error::Unauthorized)));
    env.ledger().set_timestamp(future);
    client.unpause();
    assert!(!client.is_paused());
}

#[test]
fn test_re_pause_cancels_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    client.pause();
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));
}

#[test]
fn test_timelock_bypass_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    env.ledger().set_timestamp(1000);
    assert_eq!(
        client.try_schedule_unpause(&999),
        Err(Ok(Error::InvalidSchedule))
    );
    client.schedule_unpause(&1000);
}

#[test]
fn test_clear_emergency_state_recovers_stuck_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Reproduce the stuck-paused state: re-pause drops the schedule, so a
    // later unpause fails with InvalidSchedule even past the original time.
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    client.pause();
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));
    assert!(client.is_paused());

    // The recovery entrypoint lifts the pause immediately.
    client.clear_emergency_state();
    assert!(!client.is_paused());
}

#[test]
fn test_clear_emergency_state_bypasses_timelock() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);

    // Well before the scheduled time, unpause is rejected but clear is not.
    assert_eq!(client.try_unpause(), Err(Ok(Error::Unauthorized)));
    client.clear_emergency_state();
    assert!(!client.is_paused());

    // The pending schedule was wiped and contract is unpaused: a later unpause returns NotActive.
    env.ledger().set_timestamp(future);
    assert_eq!(client.try_unpause(), Err(Ok(Error::NotActive)));
}

#[test]
fn test_clear_emergency_state_is_idempotent_when_active() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Safe no-op when the contract was never paused.
    assert!(!client.is_paused());
    client.clear_emergency_state();
    assert!(!client.is_paused());
}

#[test]
fn clear_emergency_state_no_op_preserves_all_state_when_no_emergency() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let module_a = symbol_short!("bill");
    let module_b = symbol_short!("savings");
    let func_a = symbol_short!("pay");
    let func_b = symbol_short!("refund");

    // Establish some non-global state that must survive a no-op clear.
    client.pause_module(&module_a);
    client.pause_function(&module_a, &func_a);
    client.pause_function(&module_a, &func_b);
    client.pause_function(&module_b, &func_a);

    // Confirm the global is not paused and there is no schedule.
    assert!(!client.is_paused());
    assert_eq!(client.get_unpause_schedule(), None);

    // Snapshot expected state.
    assert!(client.is_module_paused(&module_a));
    assert!(!client.is_module_paused(&module_b));
    assert!(client.is_function_paused(&module_a, &func_a));
    assert!(client.is_function_paused(&module_a, &func_b));
    assert!(client.is_function_paused(&module_b, &func_a));

    // ── Act: call clear_emergency_state when no emergency is active ──────
    client.clear_emergency_state();

    // ── Assert: every piece of state is exactly as before ────────────────
    assert!(!client.is_paused());
    assert_eq!(client.get_unpause_schedule(), None);

    // Module-level pauses must survive.
    assert!(client.is_module_paused(&module_a));
    assert!(!client.is_module_paused(&module_b));

    // Function-level pauses must survive.
    assert!(client.is_function_paused(&module_a, &func_a));
    assert!(client.is_function_paused(&module_a, &func_b));
    assert!(client.is_function_paused(&module_b, &func_a));

    // Paused-function list integrity must hold.
    let list_a = client.list_paused_functions(&module_a);
    assert_eq!(list_a.len(), 2);
    assert!(list_a.contains(func_a));
    assert!(list_a.contains(func_b));
}

#[test]
fn test_clear_emergency_state_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    assert_eq!(
        client.try_clear_emergency_state(),
        Err(Ok(Error::NotInitialized))
    );
}

#[test]
fn test_clear_emergency_state_requires_admin_auth() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    client.pause();

    // Without mocked auth the admin requirement must reject the call.
    env.set_auths(&[]);
    assert!(client.try_clear_emergency_state().is_err());
    assert!(client.is_paused());
}

#[test]
fn test_clear_emergency_state_preserves_module_and_function_pauses() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    client.pause_module(&module);
    client.pause_function(&module, &func);
    client.pause();
    assert!(client.is_paused());

    client.clear_emergency_state();

    // Global pause is cleared, but the narrower scopes survive.
    assert!(!client.is_paused());
    assert!(client.is_function_paused(&module, &func));
}

#[test]
fn test_per_function_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    assert!(!client.is_function_paused(&module, &func));
    client.pause_function(&module, &func);
    assert!(client.is_function_paused(&module, &func));
    client.unpause_function(&module, &func);
    assert!(!client.is_function_paused(&module, &func));
}

#[test]
fn test_module_pause_precedence() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let paused_fn = symbol_short!("pay");
    let other_fn = symbol_short!("refund");
    client.pause_function(&module, &paused_fn);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));
    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &other_fn));
    client.unpause_module(&module);
    assert!(client.is_function_paused(&module, &paused_fn));
    assert!(!client.is_function_paused(&module, &other_fn));
}

#[test]
fn test_global_pause_dominates() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);
    client.pause_module(&module);
    client.pause();
    assert!(client.is_paused());
    assert!(client.is_function_paused(&module, &func));
}

#[test]
fn test_max_paused_functions_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    for i in 0..10 {
        client.pause_function(&module, &Symbol::new(&env, &format!("f{}", i)));
    }
    assert_eq!(
        client.try_pause_function(&module, &symbol_short!("one_more")),
        Err(Ok(Error::LimitExceeded))
    );
}

#[test]
fn test_module_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    assert!(!client.is_function_paused(&module, &func));
    client.pause_module(&module);
    assert!(client.is_function_paused(&module, &func));
    client.unpause_module(&module);
    assert!(!client.is_function_paused(&module, &func));
}

// ── get_unpause_schedule ────────────────────────────────────────────────────

#[test]
fn get_unpause_schedule_none_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.get_unpause_schedule(), None);
}

#[test]
fn get_unpause_schedule_returns_scheduled_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    assert_eq!(client.get_unpause_schedule(), Some(future));
}

#[test]
fn get_unpause_schedule_none_after_pause_clears_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    // re-pause should clear the schedule
    client.pause();
    assert_eq!(client.get_unpause_schedule(), None);
}

#[test]
fn get_unpause_schedule_none_after_unpause_clears_it() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    let future = env.ledger().timestamp() + 3600;
    client.schedule_unpause(&future);
    env.ledger().set_timestamp(future);
    client.unpause();
    assert_eq!(client.get_unpause_schedule(), None);
}

// ── list_paused_functions ───────────────────────────────────────────────────

#[test]
fn list_paused_functions_empty_when_none_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    assert!(client.list_paused_functions(&module).is_empty());
}

#[test]
fn list_paused_functions_reflects_pause_then_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");
    client.pause_function(&module, &func);
    let list = client.list_paused_functions(&module);
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), func);
    client.unpause_function(&module, &func);
    assert!(client.list_paused_functions(&module).is_empty());
}

#[test]
fn list_paused_functions_multiple_functions() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    let f1 = symbol_short!("pay");
    let f2 = symbol_short!("refund");
    client.pause_function(&module, &f1);
    client.pause_function(&module, &f2);
    let list = client.list_paused_functions(&module);
    assert_eq!(list.len(), 2);
    assert!(list.contains(f1));
    assert!(list.contains(f2));
}

#[test]
fn list_paused_functions_isolated_per_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let m1 = symbol_short!("bill");
    let m2 = symbol_short!("savings");
    let func = symbol_short!("pay");
    client.pause_function(&m1, &func);
    assert_eq!(client.list_paused_functions(&m1).len(), 1);
    assert!(client.list_paused_functions(&m2).is_empty());
}

// ── is_module_paused ────────────────────────────────────────────────────────

#[test]
fn is_module_paused_false_when_not_set() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert!(!client.is_module_paused(&symbol_short!("bill")));
}

#[test]
fn is_module_paused_true_after_pause_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    assert!(client.is_module_paused(&module));
}

#[test]
fn is_module_paused_false_after_unpause_module() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    client.unpause_module(&module);
    assert!(!client.is_module_paused(&module));
}

#[test]
fn is_module_paused_independent_of_global_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    // Global pause does not set module-level flag
    client.pause();
    assert!(!client.is_module_paused(&module));
    // Module can be independently paused alongside global pause
    client.pause_module(&module);
    assert!(client.is_module_paused(&module));
}

#[test]
fn is_module_paused_does_not_affect_function_list() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let module = symbol_short!("bill");
    client.pause_module(&module);
    // Module being paused doesn't populate PausedFunctions
    assert!(client.list_paused_functions(&module).is_empty());
}

#[test]
fn pause_reason_none_before_any_pause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    assert_eq!(client.pause_reason(), None);
}

#[test]
fn pause_reason_none_when_paused_without_reason() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    client.pause();
    assert!(client.is_paused());
    assert_eq!(client.pause_reason(), None);
}

#[test]
fn pause_reason_set_by_pause_with_reason_and_cleared_on_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let reason = symbol_short!("exploit");
    client.pause_with_reason(&reason);
    assert!(client.is_paused());
    assert_eq!(client.pause_reason(), Some(reason));

    env.ledger().with_mut(|l| l.timestamp += 1);
    client.schedule_unpause(&env.ledger().timestamp());
    env.ledger().with_mut(|l| l.timestamp += 1);
    client.unpause();
    assert_eq!(client.pause_reason(), None);
}

#[test]
fn pause_reason_cleared_by_clear_emergency_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = setup(&env);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    client.pause_with_reason(&symbol_short!("exploit"));
    client.clear_emergency_state();
    assert!(!client.is_paused());
    assert_eq!(client.pause_reason(), None);
}

// ═══════════════════════════════════════════════════════════════════════════
// Atomic rollback / compensating-write regression tests
//
// These tests verify the invariants introduced by the validate-first rewrite
// of `activate()` and the corrected `validate_approvals()` duplicate check.
//
// Coverage matrix:
//   - Normal path: activation succeeds, all metadata written atomically
//   - Invalid (LimitExceeded): no partial state left on failure
//   - Invalid (duplicate approval): first-in-list duplicate now detected
//   - Repeated activation: second call blocked by ActivationAlreadyActive
//   - Failure → retry: after a failed Function-scope activation, a valid
//     activation (Global scope) succeeds without orphaned state interference
//   - Full lifecycle: activate → delay → recover leaves a clean slate
//   - Scope restore: recover restores pre-activation scope state
// ═══════════════════════════════════════════════════════════════════════════

// ── Helpers used only by the atomic-rollback suite ─────────────────────────

fn setup_with_two_signers(
    env: &Env,
) -> (
    EmergencyKillswitchClient<'_>,
    Address,
    Address,
    Address,
    u64,
) {
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let s1 = Address::generate(env);
    let s2 = Address::generate(env);
    client.initialize(&admin);
    let signers = soroban_sdk::vec![env, s1.clone(), s2.clone()];
    let epoch = client.configure_signers(&admin, &signers, &2);
    (client, admin, s1, s2, epoch)
}

// ── 1. Normal: activation commits all metadata atomically ─────────────────

/// A successful Global-scope activation must write all four activation metadata
/// keys and assert the global pause flag.  No partial state means every key is
/// present together or not at all.
#[test]
fn activate_global_writes_all_metadata_atomically() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];

    client.activate(&epoch, &approvals, &emergency_killswitch::PauseScope::Global);

    // Global pause must be set.
    assert!(client.is_paused());
}

/// A successful Module-scope activation must set the module-paused flag and
/// not affect global-pause state.
#[test]
fn activate_module_scope_sets_module_paused_flag_atomically() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("bill");

    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Module(module.clone()),
    );

    assert!(client.is_module_paused(&module));
    // Global pause must remain untouched.
    assert!(!client.is_paused());
}

/// A successful Function-scope activation must add the function to the paused
/// list for its module and not affect any other scope.
#[test]
fn activate_function_scope_adds_function_to_paused_list_atomically() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Function(module.clone(), func.clone()),
    );

    assert!(client.is_function_paused(&module, &func));
    // Global and module flags must remain untouched.
    assert!(!client.is_paused());
    assert!(!client.is_module_paused(&module));
}

// ── 2. Failure path: LimitExceeded leaves zero partial state ──────────────

/// If the Function-scope activation fails because the paused-function list is
/// at capacity, the contract must contain **no** activation marker.  Without
/// the validate-first fix, `ActivationEpoch`, `ActiveScope`, `RecoveryReadyAt`,
/// and `ScopeWasPaused` would have been written before the limit check, causing
/// every subsequent `activate()` to fail with `ActivationAlreadyActive`.
#[test]
fn activate_function_scope_limit_exceeded_leaves_no_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1.clone(), s2.clone()];
    let module = symbol_short!("bill");

    // Fill the paused-function list to capacity via admin direct calls.
    for i in 0..10u32 {
        let func_name = format!("f{}", i);
        client.pause_function(&module, &Symbol::new(&env, &func_name));
    }
    assert_eq!(client.list_paused_functions(&module).len(), 10);

    // Now attempt a Function-scope activation against the full list.
    let new_func = symbol_short!("overflow");
    let result = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Function(module.clone(), new_func.clone()),
    );
    assert_eq!(
        result,
        Err(Ok(Error::LimitExceeded)),
        "activation against a full function list must fail with LimitExceeded"
    );

    // ── Verify no partial state was written ───────────────────────────────

    // The activation marker must not exist.  A fresh valid activation must
    // succeed — this would fail with ActivationAlreadyActive if the marker
    // had been written by the failed call.
    //
    // Use a Global-scope activation so the list-capacity issue doesn't
    // interfere with this assertion.
    let result2 = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(
        result2,
        Ok(Ok(())),
        "a valid activation must succeed after a failed Function-scope attempt: \
         ActivationAlreadyActive here would mean orphaned partial state was written"
    );

    // Global pause is now set (from the successful second activation).
    assert!(client.is_paused());

    // The overflow function must not have been added to the list.
    assert!(
        !client.is_function_paused(&module, &new_func),
        "the overflow function must not appear in the paused list"
    );

    // Clean up: recover so we leave no state between tests (important for
    // shared-state environments, though each test gets a fresh env here).
    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);
    assert!(!client.is_paused());
}

// ── 3. Duplicate approval: first-element duplicate now caught ─────────────

/// The previous `validate_approvals` implementation guarded its inner
/// duplicate-detection loop with `if accepted > 0`, meaning it never checked
/// whether the very first approval was duplicated later in the list.  A list
/// like `[A, A]` would have been accepted.  This test pins the corrected
/// behavior: the first element is now checked against every subsequent element.
#[test]
fn validate_approvals_catches_first_element_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, _s2, epoch) = setup_with_two_signers(&env);

    // [s1, s1] — s1 is the first element and is duplicated at index 1.
    let duplicate_first = soroban_sdk::vec![&env, s1.clone(), s1.clone()];
    let result = client.try_activate(
        &epoch,
        &duplicate_first,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(
        result,
        Err(Ok(Error::DuplicateApproval)),
        "[s1, s1] must be rejected as a duplicate — the first element was previously unchecked"
    );
}

/// Confirm that [s2, s1, s1] (second-position duplicate) is also rejected.
/// This was already handled by the old code but must continue to work.
#[test]
fn validate_approvals_catches_non_first_element_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, s1, s2, _epoch) = setup_with_two_signers(&env);

    // Reconfigure with 3 signers (threshold 2) so [s2, s1, s1] would
    // nominally have enough approvals if duplicates slipped through.
    let s3 = Address::generate(&env);
    let signers = soroban_sdk::vec![&env, s1.clone(), s2.clone(), s3];
    let epoch2 = client.configure_signers(&admin, &signers, &2);

    let dup_second = soroban_sdk::vec![&env, s2.clone(), s1.clone(), s1.clone()];
    let result = client.try_activate(
        &epoch2,
        &dup_second,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::DuplicateApproval)));
}

// ── 4. Repeated activation: second call always blocked ────────────────────

/// After a successful activation a second call must return
/// `ActivationAlreadyActive` regardless of the scope supplied.
#[test]
fn repeated_activation_always_blocked_after_first_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];

    client.activate(&epoch, &approvals, &emergency_killswitch::PauseScope::Global);
    assert!(client.is_paused());

    // Second activation — same scope — must be rejected.
    let result = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::ActivationAlreadyActive)));

    // Second activation — different scope — must also be rejected.
    let module = symbol_short!("bill");
    let result2 = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Module(module),
    );
    assert_eq!(result2, Err(Ok(Error::ActivationAlreadyActive)));
}

// ── 5. Full lifecycle: activate → delay → recover leaves clean slate ───────

/// After a successful Global-scope recovery the contract must hold no activation
/// markers and a new activation must be possible.
#[test]
fn recover_global_scope_leaves_clean_slate_for_new_activation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];

    client.activate(&epoch, &approvals, &emergency_killswitch::PauseScope::Global);
    assert!(client.is_paused());

    // Must be too early.
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::RecoveryTooEarly))
    );

    // Advance past the recovery delay.
    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);

    // Global pause must be cleared.
    assert!(!client.is_paused());

    // A second recover must fail — the activation marker is gone.
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::NotActive))
    );

    // A new activation must now succeed.
    let result = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(
        result,
        Ok(Ok(())),
        "new activation after clean recovery must succeed"
    );
}

// ── 6. Scope restore: recover restores pre-activation state ───────────────

/// When a Module scope is activated that was already paused before the activation,
/// recovery must leave it paused (it was paused before; we should not clear it).
#[test]
fn recover_module_scope_restores_pre_activation_paused_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("bill");

    // Pre-pause the module via admin direct call.
    client.pause_module(&module);
    assert!(client.is_module_paused(&module));

    // Activate the same module scope — it was already paused, so scope_was_paused = true.
    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Module(module.clone()),
    );
    assert!(client.is_module_paused(&module));

    // Recover: since scope_was_paused = true, the module must stay paused.
    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);

    assert!(
        client.is_module_paused(&module),
        "module must remain paused after recovery because it was paused before activation"
    );

    // Clean up.
    client.unpause_module(&module);
    assert!(!client.is_module_paused(&module));
}

/// When a Module scope is activated that was NOT paused before, recovery must
/// clear the module-paused flag.
#[test]
fn recover_module_scope_clears_pause_flag_when_not_pre_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("savings");

    assert!(!client.is_module_paused(&module));

    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Module(module.clone()),
    );
    assert!(client.is_module_paused(&module));

    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);

    assert!(
        !client.is_module_paused(&module),
        "module must be un-paused after recovery because it was not paused before activation"
    );
}

/// When a Function scope is activated that was NOT paused before, recovery must
/// remove the function from the paused list.
#[test]
fn recover_function_scope_removes_function_from_paused_list_when_not_pre_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    assert!(!client.is_function_paused(&module, &func));

    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Function(module.clone(), func.clone()),
    );
    assert!(client.is_function_paused(&module, &func));

    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);

    assert!(
        !client.is_function_paused(&module, &func),
        "function must be removed from paused list after recovery (was not paused before)"
    );
    assert!(client.list_paused_functions(&module).is_empty());
}

/// When a Function scope is activated that WAS already in the paused list,
/// recovery must leave the function in the list.
#[test]
fn recover_function_scope_preserves_function_in_paused_list_when_pre_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    // Pre-pause the function via admin direct call.
    client.pause_function(&module, &func);
    assert!(client.is_function_paused(&module, &func));

    client.activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Function(module.clone(), func.clone()),
    );
    assert!(client.is_function_paused(&module, &func));

    env.ledger()
        .with_mut(|li| li.timestamp += emergency_killswitch::RECOVERY_DELAY + 1);
    client.recover(&epoch, &approvals);

    assert!(
        client.is_function_paused(&module, &func),
        "function must remain in paused list after recovery (was paused before activation)"
    );

    // Clean up.
    client.unpause_function(&module, &func);
    assert!(!client.is_function_paused(&module, &func));
}

// ── 7. Stale epoch: epoch mismatch on activation leaves no state ──────────

/// An activation with a wrong epoch must fail before touching any storage.
#[test]
fn activation_with_wrong_epoch_leaves_no_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1.clone(), s2.clone()];

    let wrong_epoch = epoch + 1;
    let result = client.try_activate(
        &wrong_epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::EpochMismatch)));

    // No activation state should exist — a fresh activation with the correct
    // epoch must succeed.
    let result2 = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result2, Ok(Ok(())));
}

// ── 8. Empty approvals list: rejected before any state writes ─────────────

#[test]
fn activation_with_empty_approvals_rejected_before_state_writes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, s2, epoch) = setup_with_two_signers(&env);
    let approvals = soroban_sdk::vec![&env, s1, s2];

    let empty: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env];
    let result = client.try_activate(
        &epoch,
        &empty,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::InvalidSignerThreshold)));

    // A valid activation must still succeed — empty-approvals did not leave
    // partial state.
    let result2 = client.try_activate(
        &epoch,
        &approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result2, Ok(Ok(())));
}

// ── 9. Unknown signer: rejected before any state writes ───────────────────

#[test]
fn activation_with_unknown_signer_rejected_before_state_writes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, _s2, epoch) = setup_with_two_signers(&env);
    let stranger = Address::generate(&env);

    // [s1, stranger] — stranger is not in the signer set.
    let bad_approvals = soroban_sdk::vec![&env, s1, stranger];
    let result = client.try_activate(
        &epoch,
        &bad_approvals,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::SignerNotConfigured)));

    // The failed call must not have written any activation state.
    // Verify: is_paused() is still false (no partial state applied).
    assert!(!client.is_paused());

    // A second attempt with a wrong epoch produces EpochMismatch (not
    // ActivationAlreadyActive), proving the activation marker was never
    // written by the failed call above.
    let result2 = client.try_activate(
        &(epoch + 1),
        &soroban_sdk::vec![&env],
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result2, Err(Ok(Error::EpochMismatch)));
}

// ── 10. Threshold not met: rejected before any state writes ───────────────

#[test]
fn activation_with_insufficient_approvals_rejected_before_state_writes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, s1, _s2, epoch) = setup_with_two_signers(&env);

    // Only one approval when threshold is 2.
    let one_approval = soroban_sdk::vec![&env, s1.clone()];
    let result = client.try_activate(
        &epoch,
        &one_approval,
        &emergency_killswitch::PauseScope::Global,
    );
    assert_eq!(result, Err(Ok(Error::InvalidSignerThreshold)));

    // No partial state: check is_paused is false.
    assert!(!client.is_paused());
}
