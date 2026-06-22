use family_wallet::{FamilyWallet, FamilyWalletClient, TransactionType};
use soroban_sdk::{
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    vec, Address, Env,
};

/// Attack/safety property: a signature collected before signer rotation must not
/// count toward quorum after that signer is removed from the configured signer set.
///
/// Scenario:
/// 1. Owner proposes a 3-of-3 large withdrawal and auto-signs.
/// 2. `signer_a` signs, leaving the proposal partially approved.
/// 3. Owner rotates `signer_a` out and rotates `signer_c` in.
/// 4. `signer_c` signs.
///
/// Expected: stale `signer_a` approval must be ignored or the proposal must be
/// invalidated, so the transaction must remain pending and funds must not move.
#[test]
fn signer_rotation_stale_signature_does_not_count_toward_quorum() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);
    let recipient = Address::generate(&env);

    let initial_members = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
    let token_client = TokenClient::new(&env, &token);
    StellarAssetClient::new(&env, &token).mint(&owner, &10_000_0000000);

    let original_signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &original_signers,
        &1_000_0000000,
    );

    let tx_id = client.withdraw(&owner, &token, &recipient, &2_000_0000000);
    client.sign_transaction(&signer_a, &tx_id);

    let rotated_signers = vec![&env, owner.clone(), signer_b.clone(), signer_c.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &rotated_signers,
        &1_000_0000000,
    );

    client.sign_transaction(&signer_c, &tx_id);

    assert!(
        client.get_pending_transaction(&tx_id).is_some(),
        "rotated-out signer_a signature must not help reach quorum"
    );
    assert_eq!(
        token_client.balance(&recipient),
        0,
        "funds must not move when quorum relies on a stale rotated-out signature"
    );
}

/// Safety property: a newly rotated-in signer must be able to sign a still-valid
/// proposal, and quorum should be reached only by currently configured signers.
#[test]
fn signer_rotation_new_signer_can_sign_and_reach_quorum() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);
    let recipient = Address::generate(&env);

    let initial_members = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
    let token_client = TokenClient::new(&env, &token);
    StellarAssetClient::new(&env, &token).mint(&owner, &10_000_0000000);

    let original_signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &original_signers,
        &1_000_0000000,
    );

    let tx_id = client.withdraw(&owner, &token, &recipient, &2_000_0000000);

    let rotated_signers = vec![&env, owner.clone(), signer_b.clone(), signer_c.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &rotated_signers,
        &1_000_0000000,
    );

    client.sign_transaction(&signer_b, &tx_id);
    assert!(client.get_pending_transaction(&tx_id).is_some());

    client.sign_transaction(&signer_c, &tx_id);

    assert!(client.get_pending_transaction(&tx_id).is_none());
    assert_eq!(token_client.balance(&recipient), 2_000_0000000);
}

/// Safety property: a rotation must not accept a threshold that cannot be met by
/// the configured signer set. This is the minimum guard for quorum achievability.
#[test]
// `configure_multisig` returns `Err(Error::InvalidThreshold)` (contract error #2)
// when the threshold exceeds the signer count. The non-`try_` client method
// auto-unwraps that `Err`, which the host escalates to a panic carrying the
// contract error code rather than the human-readable name.
#[should_panic(expected = "Error(Contract, #2)")]
fn signer_rotation_rejects_threshold_above_signer_count() {

    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let initial_members = vec![&env, signer_a.clone(), signer_b.clone()];
    client.init(&owner, &initial_members);

    let impossible_signers = vec![&env, owner.clone(), signer_a.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &impossible_signers,
        &1_000_0000000,
    );

    assert_eq!(result, Err(Ok(family_wallet::Error::InvalidThreshold)));
}


/// Safety property: removing the proposer from the signer set should invalidate
/// the old auto-signature or prevent it from contributing to quorum.
#[test]
fn signer_rotation_removing_proposer_invalidates_or_ignores_auto_signature() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);
    let recipient = Address::generate(&env);

    let initial_members = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token = token_contract.address();
    let token_client = TokenClient::new(&env, &token);
    StellarAssetClient::new(&env, &token).mint(&owner, &10_000_0000000);

    let original_signers = vec![&env, owner.clone(), signer_a.clone(), signer_b.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &original_signers,
        &1_000_0000000,
    );

    let tx_id = client.withdraw(&owner, &token, &recipient, &2_000_0000000);

    let rotated_signers = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &rotated_signers,
        &1_000_0000000,
    );

    client.sign_transaction(&signer_a, &tx_id);
    client.sign_transaction(&signer_b, &tx_id);

    assert!(
        client.get_pending_transaction(&tx_id).is_some(),
        "removed proposer auto-signature must not count toward the rotated quorum"
    );
    assert_eq!(token_client.balance(&recipient), 0);
}
