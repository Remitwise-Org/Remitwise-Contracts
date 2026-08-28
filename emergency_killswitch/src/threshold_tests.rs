#![cfg(test)]
#![allow(clippy::all)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{vec, Address, Env};

use crate::{EmergencyKillswitch, EmergencyKillswitchClient, Error, PauseScope, RECOVERY_DELAY};

fn setup() -> (
    Env,
    EmergencyKillswitchClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &id);
    let admin = Address::generate(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin, first, second)
}

fn configure_two(
    env: &Env,
    client: &EmergencyKillswitchClient<'_>,
    admin: &Address,
    first: &Address,
    second: &Address,
) -> u64 {
    let signers = vec![env, first.clone(), second.clone()];
    client.configure_signers(admin, &signers, &2)
}

#[test]
fn configuration_starts_a_new_signer_epoch() {
    let (_env, client, admin, first, second) = setup();
    assert_eq!(configure_two(&_env, &client, &admin, &first, &second), 1);
    assert_eq!(client.get_signer_epoch(), 1);
    assert_eq!(client.get_signer_threshold(), 2);
}

#[test]
fn duplicate_signers_and_impossible_thresholds_fail() {
    let (env, client, admin, first, _second) = setup();
    let duplicate = vec![&env, first.clone(), first.clone()];
    assert_eq!(
        client.try_configure_signers(&admin, &duplicate, &2),
        Err(Ok(Error::DuplicateSigner))
    );
    let one = vec![&env, first];
    assert_eq!(
        client.try_configure_signers(&admin, &one, &2),
        Err(Ok(Error::InvalidSignerThreshold))
    );
}

#[test]
fn activation_requires_current_epoch_and_threshold() {
    let (env, client, admin, first, second) = setup();
    assert_eq!(
        client.try_activate(
            &1,
            &vec![&env, first.clone(), second.clone()],
            &PauseScope::Global
        ),
        Err(Ok(Error::EpochMismatch))
    );
    let epoch = configure_two(&env, &client, &admin, &first, &second);
    let one = vec![&env, first.clone()];
    assert_eq!(
        client.try_activate(&epoch, &one, &PauseScope::Global),
        Err(Ok(Error::InvalidSignerThreshold))
    );
}

#[test]
fn duplicate_and_unknown_approvals_are_rejected() {
    let (env, client, admin, first, second) = setup();
    let epoch = configure_two(&env, &client, &admin, &first, &second);
    let duplicate = vec![&env, first.clone(), first.clone()];
    assert_eq!(
        client.try_activate(&epoch, &duplicate, &PauseScope::Global),
        Err(Ok(Error::DuplicateApproval))
    );
    let stranger = Address::generate(&env);
    let unknown = vec![&env, first, stranger];
    assert_eq!(
        client.try_activate(&epoch, &unknown, &PauseScope::Global),
        Err(Ok(Error::SignerNotConfigured))
    );
}

#[test]
fn activation_is_idempotence_guarded_and_scope_is_explicit() {
    let (env, client, admin, first, second) = setup();
    let epoch = configure_two(&env, &client, &admin, &first, &second);
    let approvals = vec![&env, first, second];
    client.activate(
        &epoch,
        &approvals,
        &PauseScope::Module(soroban_sdk::symbol_short!("bills")),
    );
    assert!(client.is_module_paused(&soroban_sdk::symbol_short!("bills")));
    assert_eq!(
        client.try_activate(&epoch, &approvals, &PauseScope::Global),
        Err(Ok(Error::ActivationAlreadyActive))
    );
}

#[test]
fn recovery_requires_delay_and_quorum_then_clears_marker() {
    let (env, client, admin, first, second) = setup();
    let epoch = configure_two(&env, &client, &admin, &first, &second);
    let approvals = vec![&env, first, second];
    client.activate(&epoch, &approvals, &PauseScope::Global);
    assert!(client.is_paused());
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::RecoveryTooEarly))
    );
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + RECOVERY_DELAY);
    client.recover(&epoch, &approvals);
    assert!(!client.is_paused());
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::NotActive))
    );
}

#[test]
fn signer_epoch_rotation_invalidates_active_recovery() {
    let (env, client, admin, first, second) = setup();
    let epoch = configure_two(&env, &client, &admin, &first, &second);
    let approvals = vec![&env, first.clone(), second.clone()];
    client.activate(&epoch, &approvals, &PauseScope::Global);
    let replacement = Address::generate(&env);
    let next = vec![&env, second, replacement];
    assert_eq!(client.configure_signers(&admin, &next, &2), epoch + 1);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + RECOVERY_DELAY);
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::EpochMismatch))
    );
}
