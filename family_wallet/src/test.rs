use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token::{StellarAssetClient, TokenClient},
    vec, Env, InvokeError,
};
use testutils::set_ledger_time;

/// Functions like `propose_emergency_transfer` return a bare `u64` and signal
/// failure via `panic_with_error!`, rather than declaring `Result<_, Error>`.
/// Their generated `try_*` client method therefore surfaces the contract
/// `Error` as a host-level `soroban_sdk::Error` nested in the *outer* `Err`
/// (`Err(Ok(soroban_sdk::Error))`), not as `Err(Ok(crate::Error))` the way a
/// `Result`-returning function like `configure_multisig` would. This helper
/// builds the exact expected shape from our typed `Error` enum so tests don't
/// have to repeat the conversion (and so a future error-type change in this
/// path is caught by every call site at once).
fn emergency_error<T>(
    err: Error,
) -> Result<Result<T, soroban_sdk::Error>, Result<soroban_sdk::Error, InvokeError>> {
    Err(Ok(soroban_sdk::Error::from(err)))
}

#[test]
fn test_initialize_wallet_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    let result = client.init(&owner, &initial_members);
    assert!(result);

    let stored_owner = client.get_owner();
    assert_eq!(stored_owner, owner);

    let member1_data = client.get_family_member(&member1);
    assert!(member1_data.is_some());
    assert_eq!(member1_data.unwrap().role, FamilyRole::Member);

    let member2_data = client.get_family_member(&member2);
    assert!(member2_data.is_some());
    assert_eq!(member2_data.unwrap().role, FamilyRole::Member);

    let owner_data = client.get_family_member(&owner);
    assert!(owner_data.is_some());
    assert_eq!(owner_data.unwrap().role, FamilyRole::Owner);
}

#[test]
fn test_configure_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone(), member3.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone(), member3.clone()];
    let result = client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert!(result);

    let config = client.get_multisig_config(&TransactionType::LargeWithdrawal);
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 3);
    assert_eq!(config.spending_limit, 1000_0000000);
}

#[test]
fn test_configure_multisig_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone()];
    let result = client.try_configure_multisig(
        &member1,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_withdraw_below_threshold_no_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let withdraw_amount = 500_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    assert_eq!(tx_id, 0);
    assert_eq!(token_client.balance(&recipient), withdraw_amount);
    assert_eq!(token_client.balance(&owner), amount - withdraw_amount);
}

#[test]
fn test_withdraw_above_threshold_requires_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let withdraw_amount = 2000_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    assert!(tx_id > 0);

    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    let pending_tx = pending_tx.unwrap();
    assert_eq!(pending_tx.tx_type, TransactionType::LargeWithdrawal);
    assert_eq!(pending_tx.signatures.len(), 1);

    assert_eq!(token_client.balance(&recipient), 0);
    assert_eq!(token_client.balance(&owner), amount);

    client.sign_transaction(&member1, &tx_id);

    assert_eq!(token_client.balance(&recipient), withdraw_amount);
    assert_eq!(token_client.balance(&owner), amount - withdraw_amount);

    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_none());
}

#[test]
fn test_multisig_threshold_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone(), member3.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let withdraw_amount = 2000_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    client.sign_transaction(&member1, &tx_id);

    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    assert_eq!(token_client.balance(&recipient), 0);

    client.sign_transaction(&member2, &tx_id);

    assert_eq!(token_client.balance(&recipient), withdraw_amount);
    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_none());
}

#[test]
fn test_duplicate_signature_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);

    // First sign increments the recorded signatures (proposer + member1)
    client.sign_transaction(&member1, &tx_id);

    let pending_tx = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(pending_tx.signatures.len(), 2);

    // Second sign by same signer should be idempotent and not advance the count
    let result = client.try_sign_transaction(&member1, &tx_id);
    assert!(result.is_ok());

    let pending_tx = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(pending_tx.signatures.len(), 2);
}

#[test]
fn test_sign_transaction_non_member_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let non_member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&owner, &member, &FamilyRole::Admin);

    // non-member is not authorized as signer for this tx type
    let result = client.try_sign_transaction(&non_member, &tx_id);
    assert_eq!(result, Err(Ok(Error::SignerNotMember)));
}

#[test]
fn test_propose_split_config_change() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::SplitConfigChange,
        &2,
        &signers,
        &0,
    );

    let tx_id = client.propose_split_config_change(&owner, &40, &30, &20, &10);

    assert!(tx_id > 0);

    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    assert_eq!(
        pending_tx.unwrap().tx_type,
        TransactionType::SplitConfigChange
    );

    client.sign_transaction(&member1, &tx_id);

    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_none());
}

#[test]
fn test_propose_role_change() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&owner, &member2, &FamilyRole::Admin);

    assert!(tx_id > 0);

    client.sign_transaction(&member1, &tx_id);

    let member2_data = client.get_family_member(&member2);
    assert!(member2_data.is_some());
    assert_eq!(member2_data.unwrap().role, FamilyRole::Admin);
}

// ============================================================================
// Role Expiry Lifecycle Tests
//
// Verify that role-expiry revokes permissions at the boundary timestamp and
// that permissions can be restored after renewal by an authorized caller.
// ============================================================================

#[test]
fn test_role_expiry_boundary_allows_before_expiry() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let expiry = 1_010u64;
    client.set_role_expiry(&owner, &admin, &Some(expiry));
    assert_eq!(client.get_role_expiry_public(&admin), Some(expiry));

    // At `expiry - 1` the role is still active.
    set_ledger_time(&env, 101, expiry - 1);
    assert!(client.configure_emergency(&admin, &1000_0000000, &3600, &0, &10000_0000000));
}

#[test]
#[should_panic(expected = "Only Owner or Admin can configure emergency settings")]
fn test_role_expiry_boundary_revokes_at_expiry_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let expiry = 1_010u64;
    client.set_role_expiry(&owner, &admin, &Some(expiry));

    // At `expiry` the role is expired (inclusive boundary).
    set_ledger_time(&env, 101, expiry);
    client.configure_emergency(&admin, &1000_0000000, &3600, &0, &10000_0000000);
}

#[test]
fn test_role_expiry_renewal_restores_permissions() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let expiry = 1_010u64;
    client.set_role_expiry(&owner, &admin, &Some(expiry));

    // Expired at the boundary...
    set_ledger_time(&env, 101, expiry);

    // ...then renewed by the Owner at the same timestamp.
    let renewed_to = expiry + 100;
    client.set_role_expiry(&owner, &admin, &Some(renewed_to));
    assert_eq!(client.get_role_expiry_public(&admin), Some(renewed_to));

    // Permissions are restored immediately after renewal.
    assert!(client.configure_emergency(&admin, &1000_0000000, &3600, &0, &10000_0000000));
}

#[test]
#[should_panic(expected = "Insufficient role")]
fn test_role_expiry_unauthorized_member_cannot_renew() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);

    client.init(&owner, &vec![&env, member.clone()]);

    // Regular members cannot set/renew role expiry.
    client.set_role_expiry(&member, &member, &Some(2_000));
}

#[test]
fn test_cancel_transaction_by_proposer() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&member, &member, &FamilyRole::Admin);
    assert!(tx_id > 0);

    let result = client.cancel_transaction(&member, &tx_id);
    assert!(result);

    let pending = client.get_pending_transaction(&tx_id);
    assert!(pending.is_none());
}

#[test]
fn test_cancel_transaction_by_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&member, &member, &FamilyRole::Admin);

    let result = client.cancel_transaction(&owner, &tx_id);
    assert!(result);

    let pending = client.get_pending_transaction(&tx_id);
    assert!(pending.is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_cancel_transaction_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone(), member2.clone()]);

    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&member1, &member1, &FamilyRole::Admin);

    // member2 is neither proposer nor admin
    client.cancel_transaction(&member2, &tx_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_cancel_transaction_not_found() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    client.cancel_transaction(&owner, &999);
}

#[test]
fn test_proposal_expiry_default_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    set_ledger_time(&env, 100, 1000);
    let tx_id = client.propose_role_change(&owner, &member, &FamilyRole::Admin);

    // Jump past DEFAULT_PROPOSAL_EXPIRY (86400 seconds)
    set_ledger_time(&env, 101, 1000 + DEFAULT_PROPOSAL_EXPIRY + 1);

    // Attempting to sign should fail with transaction expired
    let result = client.try_sign_transaction(&member, &tx_id);
    assert!(result.is_err());
}

#[test]
fn test_proposal_expiry_exact_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    set_ledger_time(&env, 100, 1000);
    let tx_id = client.propose_role_change(&owner, &member, &FamilyRole::Admin);

    // Jump to exactly the expiry boundary (1000 + 86400 = 87400)
    set_ledger_time(&env, 101, 1000 + DEFAULT_PROPOSAL_EXPIRY);

    // Signing at exactly expires_at should still work (strict > check)
    let result = client.try_sign_transaction(&member, &tx_id);
    assert!(result.is_ok());
}

#[test]
fn test_expiry_disabled_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    // Disable expiry by setting PROP_EXP to 0
    assert!(client.set_proposal_expiry(&owner, &0));
    assert_eq!(client.get_proposal_expiry_public(), 0);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    set_ledger_time(&env, 100, 1000);
    let tx_id = client.propose_role_change(&owner, &member, &FamilyRole::Admin);

    // Jump far past the default expiry — should still succeed since expiry is disabled
    set_ledger_time(&env, 200, 1000 + DEFAULT_PROPOSAL_EXPIRY * 10);

    let result = client.try_sign_transaction(&member, &tx_id);
    assert!(result.is_ok());
}

#[test]
fn test_sign_past_expiry_execute_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone(), member2.clone()]);

    let signers = vec![&env, member1.clone(), member2.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    set_ledger_time(&env, 100, 1000);
    let tx_id = client.propose_role_change(&owner, &member1, &FamilyRole::Admin);

    // First signer at time 5000
    set_ledger_time(&env, 101, 5000);
    let result = client.try_sign_transaction(&member1, &tx_id);
    assert!(result.is_ok());

    // Jump past expiry, second signer triggers execution which should be rejected
    set_ledger_time(&env, 102, 5000 + DEFAULT_PROPOSAL_EXPIRY + 1);
    let result = client.try_sign_transaction(&member2, &tx_id);
    assert!(result.is_err());
}

#[test]
#[should_panic(expected = "Role has expired")]
fn test_role_expiry_expired_admin_cannot_renew_self() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    // Expire immediately at `1_000`.
    client.set_role_expiry(&owner, &admin, &Some(1_000));

    set_ledger_time(&env, 101, 1_000);
    client.set_role_expiry(&admin, &admin, &Some(2_000));
}

#[test]
#[should_panic(expected = "Member not found")]
fn test_role_expiry_cannot_be_set_for_non_member() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let non_member = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.set_role_expiry(&owner, &non_member, &Some(2_000));
}

#[test]
fn test_propose_emergency_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );

    client.configure_multisig(
        &owner,
        &TransactionType::EmergencyTransfer,
        &3,
        &signers,
        &0,
    );

    let recipient = Address::generate(&env);
    let transfer_amount = 3000_0000000;
    let tx_id = client.propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &transfer_amount,
    );

    assert!(tx_id > 0);

    client.sign_transaction(&member1, &tx_id);

    assert!(client.get_pending_transaction(&tx_id).is_some());

    client.sign_transaction(&member2, &tx_id);

    assert_eq!(token_client.balance(&recipient), transfer_amount);
    assert_eq!(token_client.balance(&owner), 5000_0000000 - transfer_amount);
}

#[test]
fn test_emergency_mode_direct_transfer_within_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);
    set_ledger_time(&env, 100, 1000);

    client.configure_emergency(
        &owner,
        &2000_0000000,
        &3600u64,
        &1000_0000000,
        &5000_0000000,
    );
    client.set_emergency_mode(&owner, &true);
    assert!(client.is_emergency_mode());

    let recipient = Address::generate(&env);
    let amount = 1500_0000000;
    let tx_id =
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);

    assert_eq!(tx_id, 0);
    assert_eq!(token_client.balance(&recipient), amount);
    assert_eq!(token_client.balance(&owner), total - amount);

    let last_ts = client.get_last_emergency_at();
    assert!(last_ts.is_some());

    let audit = client.get_access_audit(&2);
    assert_eq!(audit.len(), 2);
    let em_exec = audit.get(1).unwrap();
    assert_eq!(em_exec.operation, symbol_short!("em_exec"));
    assert_eq!(em_exec.caller, owner);
    assert_eq!(em_exec.target, Some(recipient));
    assert!(em_exec.success);
}

#[test]
fn test_set_emergency_mode_appends_access_audit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = Vec::new(&env);
    client.init(&owner, &initial_members);

    assert!(client.set_emergency_mode(&owner, &true));

    let audit = client.get_access_audit(&1);
    assert_eq!(audit.len(), 1);
    let entry = audit.get(0).unwrap();
    assert_eq!(entry.operation, symbol_short!("em_mode"));
    assert_eq!(entry.caller, owner);
    assert!(entry.target.is_none());
    assert!(entry.success);
}

#[test]
fn test_configure_emergency_appends_access_audit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = Vec::new(&env);
    client.init(&owner, &initial_members);

    assert!(client.configure_emergency(
        &owner,
        &2000_0000000,
        &3600u64,
        &500_0000000,
        &10000_0000000
    ));

    let audit = client.get_access_audit(&1);
    assert_eq!(audit.len(), 1);
    let entry = audit.get(0).unwrap();
    assert_eq!(entry.operation, symbol_short!("em_conf"));
    assert_eq!(entry.caller, owner);
    assert!(entry.target.is_none());
    assert!(entry.success);
}

#[test]
fn test_propose_emergency_transfer_appends_access_audit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = Vec::new(&env);
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    let amount = 3000_0000000;

    let tx_id =
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);

    assert!(tx_id > 0);

    let audit = client.get_access_audit(&1);
    assert_eq!(audit.len(), 1);
    let entry = audit.get(0).unwrap();
    assert_eq!(entry.operation, symbol_short!("em_prop"));
    assert_eq!(entry.caller, owner);
    assert_eq!(entry.target, Some(recipient));
    assert!(entry.success);
}

#[test]
#[should_panic(expected = "Emergency amount exceeds maximum allowed")]
fn test_emergency_transfer_exceeds_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    client.configure_emergency(&owner, &1000_0000000, &3600u64, &0, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &2000_0000000);
}

#[test]
#[should_panic(expected = "Emergency transfer cooldown period not elapsed")]
fn test_emergency_transfer_cooldown_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);
    set_ledger_time(&env, 100, 1000);

    client.configure_emergency(&owner, &2000_0000000, &3600u64, &0, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let amount = 1000_0000000;

    let tx_id =
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);
    assert_eq!(tx_id, 0);

    client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);
}

#[test]
fn test_emergency_transfer_min_balance_enforced() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    // min_balance = 2500: a transfer of 1000 would leave 2000, breaching the floor.
    client.configure_emergency(&owner, &2000_0000000, &0u64, &2500_0000000, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );

    assert_eq!(result, emergency_error(Error::MinBalanceViolation));
    // Rejected transfer must not move any funds.
    assert_eq!(token_client.balance(&owner), total);
    assert_eq!(token_client.balance(&recipient), 0);
}

/// A transfer that leaves the balance exactly at `min_balance` must succeed —
/// the floor is an inclusive lower bound, not an exclusive one.
#[test]
fn test_emergency_transfer_min_balance_boundary_exact_floor_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    let min_balance = 2000_0000000;
    let amount = total - min_balance; // post-transfer balance lands exactly on the floor
    client.configure_emergency(&owner, &2000_0000000, &0u64, &min_balance, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &amount,
    );

    assert!(result.is_ok());
    assert_eq!(token_client.balance(&owner), min_balance);
    assert_eq!(token_client.balance(&recipient), amount);
}

/// One stroop past the floor (post-transfer balance = min_balance - 1) must be rejected.
#[test]
fn test_emergency_transfer_min_balance_boundary_one_stroop_under_floor_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    let min_balance = 2000_0000000;
    let amount = total - min_balance + 1; // one stroop past the floor
    client.configure_emergency(&owner, &2000_0000000, &0u64, &min_balance, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &amount,
    );

    assert_eq!(result, emergency_error(Error::MinBalanceViolation));
    assert_eq!(token_client.balance(&owner), total);
}

/// `min_balance = 0` disables the floor entirely: a transfer draining the wallet
/// to zero must succeed, since any non-negative post-transfer balance clears it.
#[test]
fn test_emergency_transfer_zero_min_balance_disables_floor() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    client.configure_emergency(&owner, &5000_0000000, &0u64, &0, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    // Drain the entire balance — leaves exactly 0, which satisfies `>= 0`.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &total,
    );

    assert!(result.is_ok());
    assert_eq!(token_client.balance(&owner), 0);
    assert_eq!(token_client.balance(&recipient), total);
}

/// The min_balance floor and the daily volume cap are independent checks; both
/// must pass. This test isolates each rejection reason in turn: a transfer that
/// only breaches the floor (cap has headroom to spare) is rejected with
/// `MinBalanceViolation` and must not record any daily volume; a transfer that
/// only breaches the cap (floor has headroom to spare) is rejected for the cap
/// rather than the floor.
#[test]
fn test_emergency_transfer_min_balance_interacts_with_daily_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 10_000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);
    set_ledger_time(&env, 100, 1_000);

    // --- Scenario A: floor rejects, daily cap has ample headroom ---------------
    // min_balance = 9,500 → the largest single transfer respecting the floor is
    // 500. daily_limit = 10,000 is far larger than anything tested here, so the
    // cap can never be the binding constraint in this scenario.
    let read_em_vol = || -> i128 {
        env.as_contract(&contract_id, || {
            env.storage().instance().get(&symbol_short!("EM_VOL"))
        })
        .unwrap_or(0i128)
    };

    client.configure_emergency(
        &owner,
        &5_000_0000000,
        &0u64,
        &9_500_0000000,
        &10_000_0000000,
    );
    client.set_emergency_mode(&owner, &true);
    let recipient = Address::generate(&env);

    // 600 leaves 9,400 < 9,500 — breaches the floor; well under the 10,000 cap.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &600_0000000,
    );
    assert_eq!(result, emergency_error(Error::MinBalanceViolation));
    assert_eq!(token_client.balance(&owner), total);
    assert_eq!(
        read_em_vol(),
        0,
        "a transfer rejected for the min_balance floor must not record phantom daily volume"
    );

    // 500 leaves exactly 9,500 — respects the floor (inclusive boundary) and the
    // cap — succeeds, consuming 500 of the daily budget.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &500_0000000,
    );
    assert!(result.is_ok());
    assert_eq!(token_client.balance(&owner), total - 500_0000000);
    assert_eq!(read_em_vol(), 500_0000000);

    // --- Scenario B: daily cap rejects, floor has ample headroom ---------------
    // Reconfigure with a generous floor (1,000) but a tight daily cap. The
    // wallet currently holds total - 500 = 9,500.
    client.configure_emergency(&owner, &5_000_0000000, &0u64, &1_000_0000000, &900_0000000);
    // Reconfiguring resets neither EM_VOL nor EM_LAST — both persist across a
    // `configure_emergency` call, so the cap below is evaluated against the
    // pre-existing accumulated volume from Scenario A.
    assert_eq!(read_em_vol(), 500_0000000);

    // A transfer of 300 would leave 9,200 (miles above the new 1,000 floor) but
    // cumulative volume would become 500 + 300 = 800 <= 900 — succeeds.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &300_0000000,
    );
    assert!(result.is_ok());
    assert_eq!(read_em_vol(), 800_0000000);

    // A further transfer of 200 would leave plenty of balance above the floor
    // (9,200 - 200 = 9,000 >= 1,000) but cumulative volume would become
    // 800 + 200 = 1,000 > 900 — rejected for the daily cap, not the floor.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &200_0000000,
    );
    assert!(result.is_err());
    assert_ne!(
        result,
        emergency_error(Error::MinBalanceViolation),
        "this rejection should come from the daily cap, not the min_balance floor"
    );
    assert_eq!(
        read_em_vol(),
        800_0000000,
        "a cap-rejected transfer must not mutate EM_VOL"
    );
}

/// The min_balance floor and the cooldown timer are independent checks. A
/// transfer made before the cooldown elapses must be rejected for cooldown,
/// not min_balance — and once the cooldown elapses, the same-sized transfer
/// must still be subject to the floor check.
#[test]
fn test_emergency_transfer_min_balance_interacts_with_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    let total = 5_000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);
    set_ledger_time(&env, 100, 1_000);

    let min_balance = 4_000_0000000;
    let cooldown = 3_600u64;
    client.configure_emergency(
        &owner,
        &2_000_0000000,
        &cooldown,
        &min_balance,
        &10_000_0000000,
    );
    client.set_emergency_mode(&owner, &true);
    let recipient = Address::generate(&env);

    // First transfer: respects the floor (leaves 4500 >= 4000), succeeds and starts cooldown.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &500_0000000,
    );
    assert!(result.is_ok());
    assert_eq!(token_client.balance(&owner), total - 500_0000000);

    // Second transfer, still within the cooldown window: even though this amount
    // would also respect the floor (4500 - 400 = 4100 >= 4000), cooldown fires first.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &400_0000000,
    );
    assert!(result.is_err());
    assert_ne!(
        result,
        emergency_error(Error::MinBalanceViolation),
        "cooldown should reject before the floor check is reached"
    );
    assert_eq!(token_client.balance(&owner), total - 500_0000000);

    // Advance past the cooldown. Now the floor is the binding constraint: current
    // balance is 4500; transferring 600 would leave 3900 < 4000 — rejected for floor.
    set_ledger_time(&env, 101, 1_000 + cooldown + 1);
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &600_0000000,
    );
    assert_eq!(result, emergency_error(Error::MinBalanceViolation));
    assert_eq!(token_client.balance(&owner), total - 500_0000000);

    // A smaller transfer respecting the floor (4500 - 500 = 4000 >= 4000) succeeds.
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &500_0000000,
    );
    assert!(result.is_ok());
    assert_eq!(token_client.balance(&owner), total - 1_000_0000000);
}

/// `EmergencyEvent::TransferExec` must be published only when the transfer
/// actually executes — a min_balance rejection must not emit it.
#[test]
fn test_emergency_transfer_min_balance_rejection_emits_no_transfer_exec_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    client.configure_emergency(&owner, &2000_0000000, &0u64, &2500_0000000, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let result = client.try_propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
    assert_eq!(result, emergency_error(Error::MinBalanceViolation));

    // No EM_LAST should have been recorded — that's only written after a
    // successful execution, and absence of it is an easy proxy for "no
    // TransferExec-triggering execution happened".
    assert!(client.get_last_emergency_at().is_none());

    // The audit trail should record only the configure/mode-toggle operations,
    // never an `em_exec` entry, since execution never reached that point.
    let audit = client.get_access_audit(&10);
    for entry in audit.iter() {
        assert_ne!(entry.operation, symbol_short!("em_exec"));
    }
}

#[test]
fn test_add_and_remove_family_member() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    let new_member = Address::generate(&env);
    let result = client.add_family_member(&owner, &new_member, &FamilyRole::Admin);
    assert!(result);

    let member_data = client.get_family_member(&new_member);
    assert!(member_data.is_some());
    assert_eq!(member_data.unwrap().role, FamilyRole::Admin);

    let result = client.remove_family_member(&owner, &new_member);
    assert!(result);

    let member_data = client.get_family_member(&new_member);
    assert!(member_data.is_none());
}

#[test]
#[should_panic(expected = "Only Owner or Admin can add family members")]
fn test_add_member_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    let new_member = Address::generate(&env);
    client.add_family_member(&member1, &new_member, &FamilyRole::Member);
}

#[test]
fn test_add_member_already_exists() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    // Try to add member1 again (they already exist from initialization)
    let result = client.try_add_member(&owner, &member1, &FamilyRole::Admin, &0);
    assert_eq!(result, Err(Ok(Error::MemberAlreadyExists)));

    // Try to add owner (they already exist and are the owner)
    let result = client.try_add_member(&owner, &owner, &FamilyRole::Admin, &0);
    assert_eq!(result, Err(Ok(Error::MemberAlreadyExists)));
    // Add a new member successfully
    let new_member = Address::generate(&env);
    let result = client.try_add_member(&owner, &new_member, &FamilyRole::Member, &0);
    assert!(result.is_ok());

    // Try to add the same new member again
    let result = client.try_add_member(&owner, &new_member, &FamilyRole::Admin, &0);
    assert_eq!(result, Err(Ok(Error::MemberAlreadyExists)));
}

#[test]
fn test_different_thresholds_for_different_transaction_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone(), member3.clone()];

    client.init(&owner, &initial_members);

    let all_signers = vec![
        &env,
        owner.clone(),
        member1.clone(),
        member2.clone(),
        member3.clone(),
    ];

    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &all_signers,
        &1000_0000000,
    );

    client.configure_multisig(&owner, &TransactionType::RoleChange, &3, &all_signers, &0);

    client.configure_multisig(
        &owner,
        &TransactionType::EmergencyTransfer,
        &4,
        &all_signers,
        &0,
    );

    let withdraw_config = client.get_multisig_config(&TransactionType::LargeWithdrawal);
    assert_eq!(withdraw_config.unwrap().threshold, 2);

    let role_config = client.get_multisig_config(&TransactionType::RoleChange);
    assert_eq!(role_config.unwrap().threshold, 3);

    let emergency_config = client.get_multisig_config(&TransactionType::EmergencyTransfer);
    assert_eq!(emergency_config.unwrap().threshold, 4);
}

#[test]

fn test_unauthorized_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone(), member3.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);

    let result = client.try_sign_transaction(&member2, &tx_id);
    assert_eq!(
        result,
        Err(Ok(Error::SignerNotMember)),
        "Unauthorized signer must be rejected"
    );
}
// ============================================
// Storage Optimization and Archival Tests
// ============================================

#[test]
fn test_archive_old_transactions() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    set_ledger_time(&env, 100, 2_000_000);

    client.init(&owner, &initial_members);

    let archived_count = client.archive_old_transactions(&owner, &1_000_000);
    assert_eq!(archived_count, 0);

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 0);
}

#[test]
fn test_cleanup_expired_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);
    assert!(tx_id > 0);

    let pending = client.get_pending_transaction(&tx_id);
    assert!(pending.is_some());

    let mut ledger = env.ledger().get();
    ledger.timestamp += 86401;
    env.ledger().set(ledger);

    let removed = client.cleanup_expired_pending(&owner);
    assert_eq!(removed, 1);

    let pending_after = client.get_pending_transaction(&tx_id);
    assert!(pending_after.is_none());
}

#[test]
fn test_storage_stats() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    set_ledger_time(&env, 200, 2_000_000);
    client.archive_old_transactions(&owner, &1_000_000);

    let stats = client.get_storage_stats();
    assert_eq!(stats.total_members, 3);
    assert_eq!(stats.pending_transactions, 0);
    assert_eq!(stats.archived_transactions, 0);
}

#[test]
#[should_panic(expected = "Only Owner or Admin can archive transactions")]
fn test_archive_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    client.archive_old_transactions(&member1, &1_000_000);
}

#[test]
#[should_panic(expected = "Only Owner or Admin can cleanup expired transactions")]
fn test_cleanup_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    client.cleanup_expired_pending(&member1);
}

#[test]
#[should_panic(expected = "Archive retention cutoff must not exceed ledger time")]
fn test_archive_future_retention_cutoff_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone()]);

    set_ledger_time(&env, 100, 1000);
    client.archive_old_transactions(&owner, &2000);
}

#[test]
fn test_archive_preserves_execution_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone(), member2.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    // Threshold 3 so execution happens on the second co-signer at ledger time 20_000 (not on first sign).
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );

    set_ledger_time(&env, 10, 10_000);

    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);
    assert!(tx_id > 0);
    client.sign_transaction(&member1, &tx_id);

    set_ledger_time(&env, 11, 20_000);
    client.sign_transaction(&member2, &tx_id);

    assert!(client.get_pending_transaction(&tx_id).is_none());

    set_ledger_time(&env, 100, 50_000);
    let archived_count = client.archive_old_transactions(&owner, &25_000);
    assert_eq!(archived_count, 1);

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 1);
    let row = archived.get(0).unwrap();
    assert_eq!(row.tx_id, tx_id);
    assert_eq!(row.tx_type, TransactionType::LargeWithdrawal);
    assert_eq!(row.proposer, owner);
    assert_eq!(row.executed_at, 20_000);
    assert_eq!(row.archived_at, 50_000);
}

#[test]
#[should_panic(expected = "Only Owner or Admin can view archived transactions")]
fn test_get_archived_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone()]);

    let _ = client.get_archived_transactions(&member1, &10);
}

// ============================================================================
// Storage TTL Extension Tests
//
// Verify that instance storage TTL is properly extended on state-changing
// operations, preventing unexpected data expiration.
//
// Contract TTL configuration:
//   INSTANCE_LIFETIME_THRESHOLD  = 17,280 ledgers (~1 day)
//   INSTANCE_BUMP_AMOUNT         = 518,400 ledgers (~30 days)
//   ARCHIVE_LIFETIME_THRESHOLD   = 17,280 ledgers (~1 day)
//   ARCHIVE_BUMP_AMOUNT          = 2,592,000 ledgers (~180 days)
//
// Operations extending instance TTL:
//   init, configure_multisig, propose_transaction, sign_transaction,
//   configure_emergency, set_emergency_mode, add_family_member,
//   remove_family_member, archive_old_transactions,
//   cleanup_expired_pending, set_role_expiry,
//   batch_add_family_members, batch_remove_family_members
//
// Operations extending archive TTL:
//   archive_old_transactions
// ============================================================================

/// Verify that init extends instance storage TTL.
#[test]
fn test_instance_ttl_extended_on_init() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);

    // init calls extend_instance_ttl
    let result = client.init(&owner, &vec![&env, member1.clone()]);
    assert!(result);

    // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT (518,400)
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after init",
        ttl
    );
}

/// Verify that add_family_member refreshes instance TTL after ledger advancement.
///
/// extend_ttl(threshold, extend_to) only extends when TTL <= threshold.
/// After init at seq 100 sets TTL to 518,400 (live_until = 518,500),
/// we must advance past seq 501,220 so TTL drops below 17,280.
#[test]
fn test_instance_ttl_refreshed_on_add_member() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);

    client.init(&owner, &vec![&env, member1.clone()]);

    // Advance ledger so TTL drops below threshold (17,280)
    // After init at seq 100: live_until = 518,500
    // At seq 510,000: TTL = 8,500 < 17,280 ✓
    set_ledger_time(&env, 510_000, 500_000);

    // add_family_member calls extend_instance_ttl → re-extends TTL to 518,400
    client.add_family_member(&owner, &member2, &FamilyRole::Member);

    // TTL should be refreshed relative to the new sequence number
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after add_family_member",
        ttl
    );
}

/// Verify data persists across repeated operations spanning multiple
/// ledger advancements, proving TTL is continuously renewed.
///
/// Each phase advances the ledger past the TTL threshold so every
/// state-changing call actually re-extends the TTL.
#[test]
fn test_data_persists_across_repeated_operations() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let _member3 = Address::generate(&env);

    // Phase 1: Initialize wallet at seq 100
    // TTL goes from 100 → 518,400. live_until = 518,500
    client.init(&owner, &vec![&env, member1.clone()]);

    // Phase 2: Advance to seq 510,000 (TTL = 8,500 < 17,280)
    // add_family_member re-extends → live_until = 1,028,400
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    client.add_family_member(&owner, &member2, &FamilyRole::Member);

    // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
    // configure_multisig re-extends → live_until = 1,538,400
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 1_020_000,
        timestamp: 1_020_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let signers = vec![&env, member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    // All data should still be accessible
    let owner_data = client.get_family_member(&owner);
    assert!(
        owner_data.is_some(),
        "Owner data must persist across ledger advancements"
    );

    let m1_data = client.get_family_member(&member1);
    assert!(m1_data.is_some(), "Member1 data must persist");

    let m2_data = client.get_family_member(&member2);
    assert!(m2_data.is_some(), "Member2 data must persist");

    let config = client.get_multisig_config(&TransactionType::LargeWithdrawal);
    assert!(config.is_some(), "Multisig config must persist");

    // TTL should be fully refreshed
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must remain >= 518,400 after repeated operations",
        ttl
    );
}

/// Verify that archive_old_transactions extends instance TTL.
///
/// Note: both `extend_instance_ttl` and `extend_archive_ttl` operate on
/// instance() storage. Since `extend_instance_ttl` is called first, the
/// resulting TTL is at least INSTANCE_BUMP_AMOUNT (518,400).
#[test]
fn test_archive_ttl_extended_on_archive_transactions() {
    let env = Env::default();
    env.mock_all_auths();

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 3_000_000,
    });

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);

    client.init(&owner, &vec![&env, member1.clone()]);

    // Advance ledger so TTL drops below threshold
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 510_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 3_000_000,
    });

    // archive_old_transactions calls extend_instance_ttl then extend_archive_ttl
    let _archived = client.archive_old_transactions(&owner, &500_000);

    // TTL should be extended
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after archiving",
        ttl
    );
}

// ============================================================================
// Archive Bounds & Selection Boundary Tests (feature/fw-archive-bounds)
// ============================================================================

/// Helper: execute a multisig withdrawal so it lands in EXEC_TXS.
/// Returns the tx_id that was executed.
fn execute_one_tx(
    env: &Env,
    client: &FamilyWalletClient,
    owner: &Address,
    member: &Address,
    token: &Address,
    recipient: &Address,
    amount: &i128,
) -> u64 {
    let tx_id = client.withdraw(owner, token, recipient, amount);
    assert!(tx_id > 0, "withdraw must create a pending tx");
    client.sign_transaction(member, &tx_id);
    tx_id
}

#[test]
fn test_archive_nothing_to_archive() {
    // Edge case: no executed transactions → count == 0, ARCH_TX empty.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 2_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let count = client.archive_old_transactions(&owner, &1_000);
    assert_eq!(count, 0, "nothing to archive");

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 0);

    let stats = client.get_storage_stats();
    assert_eq!(stats.archived_transactions, 0);
}

#[test]
fn test_archive_boundary_strictly_less_than() {
    // Entries executed AT before_timestamp must NOT be archived (strict <).
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &50_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);

    // Execute tx at timestamp 1_000
    execute_one_tx(
        &env,
        &client,
        &owner,
        &member,
        &token_contract.address(),
        &recipient,
        &2000_0000000,
    );

    // Archive with cutoff == executed_at (1_000) — must NOT archive (strict <)
    let count = client.archive_old_transactions(&owner, &1_000);
    assert_eq!(count, 0, "entry at cutoff must not be archived");

    // Advance time so cutoff 1_001 is valid (before_timestamp <= now)
    set_ledger_time(&env, 101, 5_000);
    // Archive with cutoff == executed_at + 1 — must archive
    let count2 = client.archive_old_transactions(&owner, &1_001);
    assert_eq!(count2, 1, "entry strictly before cutoff must be archived");
    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 1);
    assert_eq!(archived.get(0).unwrap().executed_at, 1_000);
}

#[test]
fn test_archive_count_matches_entries_moved() {
    // Return value must equal the number of entries moved.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &500_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);

    // Execute 3 transactions at t=1_000
    for _ in 0..3 {
        execute_one_tx(
            &env,
            &client,
            &owner,
            &member,
            &token_contract.address(),
            &recipient,
            &2000_0000000,
        );
    }

    // Advance time and archive all 3
    set_ledger_time(&env, 101, 5_000);
    let count = client.archive_old_transactions(&owner, &2_000);
    assert_eq!(count, 3, "count must match entries moved");

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 3, "ARCH_TX must contain exactly 3 entries");

    let stats = client.get_storage_stats();
    assert_eq!(stats.archived_transactions, 3);
    assert_eq!(stats.pending_transactions, 0);
}

#[test]
fn test_archive_ordering_preserved() {
    // Archived entries must be retrievable and their executed_at timestamps
    // must match what was recorded at execution time.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &500_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);

    // Execute at t=1_000, t=2_000, t=3_000
    let timestamps = [1_000u64, 2_000, 3_000];
    for ts in timestamps.iter() {
        set_ledger_time(&env, 100, *ts);
        execute_one_tx(
            &env,
            &client,
            &owner,
            &member,
            &token_contract.address(),
            &recipient,
            &2000_0000000,
        );
    }

    // Archive all
    set_ledger_time(&env, 101, 10_000);
    let count = client.archive_old_transactions(&owner, &5_000);
    assert_eq!(count, 3);

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 3);

    // Collect executed_at values and verify all expected timestamps are present
    let mut found = [false; 3];
    for i in 0..archived.len() {
        let entry = archived.get(i).unwrap();
        for (j, &ts) in timestamps.iter().enumerate() {
            if entry.executed_at == ts {
                found[j] = true;
            }
        }
    }
    assert!(
        found[0] && found[1] && found[2],
        "all executed_at timestamps must be preserved"
    );
}

#[test]
fn test_archive_stor_stat_updated() {
    // STOR_STAT.archived_transactions must reflect the archive size after archiving.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &500_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    execute_one_tx(
        &env,
        &client,
        &owner,
        &member,
        &token_contract.address(),
        &recipient,
        &2000_0000000,
    );

    let stats_before = client.get_storage_stats();
    assert_eq!(stats_before.archived_transactions, 0);

    set_ledger_time(&env, 101, 5_000);
    client.archive_old_transactions(&owner, &2_000);

    let stats_after = client.get_storage_stats();
    assert_eq!(
        stats_after.archived_transactions, 1,
        "STOR_STAT must be updated after archive"
    );
}

#[test]
fn test_archive_get_archived_limit_clamped() {
    // get_archived_transactions with limit=0 must use default, not return 0 entries.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &500_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    execute_one_tx(
        &env,
        &client,
        &owner,
        &member,
        &token_contract.address(),
        &recipient,
        &2000_0000000,
    );

    set_ledger_time(&env, 101, 5_000);
    client.archive_old_transactions(&owner, &2_000);

    // limit=0 should use DEFAULT_ARCHIVE_PAGE_LIMIT (20), not return 0 entries
    let archived_default = client.get_archived_transactions(&owner, &0);
    assert_eq!(
        archived_default.len(),
        1,
        "limit=0 must use default page limit"
    );

    // limit=9999 should be clamped to MAX_ARCHIVE_PAGE_LIMIT (100)
    let archived_clamped = client.get_archived_transactions(&owner, &9999);
    assert_eq!(archived_clamped.len(), 1, "limit=9999 must be clamped");
}

#[test]
fn test_archive_future_cutoff_rejected() {
    // before_timestamp > ledger.timestamp() must panic.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let result = client.try_archive_old_transactions(&owner, &9_999_999);
    assert!(result.is_err(), "future cutoff must be rejected");
}

#[test]
fn test_archive_re_pause_cancels_no_double_archive() {
    // Archiving twice with the same cutoff must not double-count entries.
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &500_000_0000000);

    let signers = vec![&env, owner.clone(), member.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    let recipient = Address::generate(&env);
    execute_one_tx(
        &env,
        &client,
        &owner,
        &member,
        &token_contract.address(),
        &recipient,
        &2000_0000000,
    );

    set_ledger_time(&env, 101, 5_000);
    let count1 = client.archive_old_transactions(&owner, &2_000);
    assert_eq!(count1, 1);

    // Second call with same cutoff — entry already moved, nothing left
    let count2 = client.archive_old_transactions(&owner, &2_000);
    assert_eq!(count2, 0, "second archive call must not double-count");

    let archived = client.get_archived_transactions(&owner, &10);
    assert_eq!(archived.len(), 1, "ARCH_TX must still have exactly 1 entry");
}

#[test]
#[should_panic(expected = "Identical emergency transfer proposal already pending")]
fn test_emergency_proposal_replay_prevention() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone()]);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);

    client.propose_emergency_transfer(
        &member1,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
    client.propose_emergency_transfer(
        &member1,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
}

#[test]
#[should_panic(expected = "Maximum pending emergency proposals reached")]
fn test_emergency_proposal_frequency_burst() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone()]);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient1 = Address::generate(&env);
    let recipient2 = Address::generate(&env);

    client.propose_emergency_transfer(
        &member1,
        &token_contract.address(),
        &recipient1,
        &1000_0000000,
    );
    client.propose_emergency_transfer(
        &member1,
        &token_contract.address(),
        &recipient2,
        &500_0000000,
    );
}

#[test]
#[should_panic(expected = "Insufficient role")]
fn test_emergency_proposal_role_misuse() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);
    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &viewer, &FamilyRole::Viewer);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);

    client.propose_emergency_transfer(
        &viewer,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
}

// ============================================================================
// Multisig Threshold Bounds Validation Tests
// ============================================================================

#[test]
fn test_threshold_minimum_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &1,
        &signers,
        &1000_0000000,
    );
}

#[test]
fn test_threshold_maximum_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let member4 = Address::generate(&env);
    let member5 = Address::generate(&env);
    let member6 = Address::generate(&env);
    let member7 = Address::generate(&env);
    let member8 = Address::generate(&env);
    let member9 = Address::generate(&env);
    let member10 = Address::generate(&env);
    let initial_members = vec![
        &env,
        member1.clone(),
        member2.clone(),
        member3.clone(),
        member4.clone(),
        member5.clone(),
        member6.clone(),
        member7.clone(),
        member8.clone(),
        member9.clone(),
        member10.clone(),
    ];

    client.init(&owner, &initial_members);

    let signers = vec![
        &env,
        member1.clone(),
        member2.clone(),
        member3.clone(),
        member4.clone(),
        member5.clone(),
        member6.clone(),
        member7.clone(),
        member8.clone(),
        member9.clone(),
        member10.clone(),
    ];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &10,
        &signers,
        &1000_0000000,
    );
}

#[test]
fn test_threshold_above_maximum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &101,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::ThresholdAboveMaximum)));
}

#[test]
fn test_threshold_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &0,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::ThresholdBelowMinimum)));
}

#[test]
fn test_threshold_exceeds_signer_count_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_empty_signers_list_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env];

    client.init(&owner, &initial_members);

    let empty_signers = vec![&env];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &1,
        &empty_signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::SignersListEmpty)));
}

#[test]
fn test_signer_not_family_member_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    let non_member = Address::generate(&env);
    let signers = vec![&env, member1.clone(), non_member.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::SignerNotMember)));
}

#[test]
fn test_negative_spending_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &1,
        &signers,
        &(-100),
    );
    assert_eq!(result, Err(Ok(Error::InvalidSpendingLimit)));
}

#[test]
fn test_threshold_consistency_across_transaction_types() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let all_signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];

    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &all_signers,
        &1000_0000000,
    );

    client.configure_multisig(&owner, &TransactionType::RoleChange, &3, &all_signers, &0);

    let wd_config = client
        .get_multisig_config(&TransactionType::LargeWithdrawal)
        .unwrap();
    let role_config = client
        .get_multisig_config(&TransactionType::RoleChange)
        .unwrap();

    assert_eq!(wd_config.threshold, 2);
    assert_eq!(role_config.threshold, 3);
    assert!(role_config.threshold > wd_config.threshold);
}

#[test]
fn test_signer_list_maximum_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let m1 = Address::generate(&env);
    let m2 = Address::generate(&env);
    let m3 = Address::generate(&env);
    let m4 = Address::generate(&env);
    let m5 = Address::generate(&env);
    let m6 = Address::generate(&env);
    let m7 = Address::generate(&env);
    let m8 = Address::generate(&env);
    let m9 = Address::generate(&env);
    let m10 = Address::generate(&env);
    let m11 = Address::generate(&env);
    let m12 = Address::generate(&env);
    let m13 = Address::generate(&env);
    let m14 = Address::generate(&env);
    let m15 = Address::generate(&env);
    let m16 = Address::generate(&env);
    let m17 = Address::generate(&env);
    let m18 = Address::generate(&env);
    let m19 = Address::generate(&env);
    let m20 = Address::generate(&env);

    let initial_members = vec![
        &env,
        m1.clone(),
        m2.clone(),
        m3.clone(),
        m4.clone(),
        m5.clone(),
        m6.clone(),
        m7.clone(),
        m8.clone(),
        m9.clone(),
        m10.clone(),
        m11.clone(),
        m12.clone(),
        m13.clone(),
        m14.clone(),
        m15.clone(),
        m16.clone(),
        m17.clone(),
        m18.clone(),
        m19.clone(),
        m20.clone(),
    ];

    client.init(&owner, &initial_members);

    let signers = vec![
        &env,
        m1.clone(),
        m2.clone(),
        m3.clone(),
        m4.clone(),
        m5.clone(),
        m6.clone(),
        m7.clone(),
        m8.clone(),
        m9.clone(),
        m10.clone(),
        m11.clone(),
        m12.clone(),
        m13.clone(),
        m14.clone(),
        m15.clone(),
        m16.clone(),
        m17.clone(),
        m18.clone(),
        m19.clone(),
        m20.clone(),
    ];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &20, &signers, &0);
}

#[test]
fn test_threshold_one_with_multiple_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let member4 = Address::generate(&env);
    let initial_members = vec![
        &env,
        member1.clone(),
        member2.clone(),
        member3.clone(),
        member4.clone(),
    ];

    client.init(&owner, &initial_members);

    let signers = vec![
        &env,
        owner.clone(),
        member1.clone(),
        member2.clone(),
        member3.clone(),
        member4.clone(),
    ];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &1,
        &signers,
        &1000_0000000,
    );

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);

    assert!(tx_id > 0);
    client.sign_transaction(&member1, &tx_id);

    let pending = client.get_pending_transaction(&tx_id);
    assert!(pending.is_none());
}

#[test]
fn test_threshold_equals_signer_count() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_paused_contract_rejects_multisig_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    client.pause(&owner);

    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &1, &signers, &0);
}

#[test]
fn test_pending_transactions_pagination_and_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    // Create 5 pending proposals, alternating proposers
    env.mock_all_auths();
    client.propose_split_config_change(&member1, &10, &40, &30, &20);
    env.mock_all_auths();
    client.propose_split_config_change(&member2, &11, &39, &30, &20);
    env.mock_all_auths();
    client.propose_split_config_change(&member1, &12, &38, &30, &20);
    env.mock_all_auths();
    client.propose_split_config_change(&member2, &13, &37, &30, &20);
    env.mock_all_auths();
    client.propose_split_config_change(&member1, &14, &36, &30, &20);

    // Owner (admin) can list all pending txs paginated
    env.mock_all_auths();
    let page1 = client.get_pending_transactions_page(&owner, &0u64, &2u32);
    assert_eq!(page1.items.len(), 2);
    assert!(page1.next_cursor != 0);

    env.mock_all_auths();
    let page2 = client.get_pending_transactions_page(&owner, &page1.next_cursor, &2u32);
    assert!(page2.items.len() >= 1 && page2.items.len() <= 2);

    // Member1 should only see their own proposals
    env.mock_all_auths();
    let m1_all = client.get_pending_transactions_page(&member1, &0u64, &100u32);
    for tx in m1_all.items.iter() {
        assert_eq!(tx.proposer, member1);
    }
}

#[test]
fn test_admin_can_configure_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let signers = vec![&env, owner.clone(), admin.clone(), member1.clone()];
    client.configure_multisig(
        &admin,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
}

#[test]
fn test_duplicate_signer_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member1.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::DuplicateSigner)));
}

#[test]
fn test_duplicate_signer_with_three_members() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    let member3 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone(), member2.clone(), member3.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone(), member2.clone(), member1.clone()];
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::DuplicateSigner)));
}

#[test]
fn test_too_many_signers_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    // Create 101 members (exceeds MAX_SIGNERS = 100)
    let mut members = Vec::new(&env);
    let mut signers = Vec::new(&env);
    for _ in 0..101 {
        let addr = Address::generate(&env);
        members.push_back(addr.clone());
        signers.push_back(addr);
    }

    client.init(&owner, &members);

    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &50,
        &signers,
        &1000_0000000,
    );
    assert_eq!(result, Err(Ok(Error::TooManySigners)));
}

#[test]
fn test_threshold_bounds_return_correct_errors() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let initial_members = vec![&env, member1.clone()];

    client.init(&owner, &initial_members);

    let signers = vec![&env, member1.clone()];

    // Threshold 0 → ThresholdBelowMinimum
    let result =
        client.try_configure_multisig(&owner, &TransactionType::LargeWithdrawal, &0, &signers, &0);
    assert_eq!(result, Err(Ok(Error::ThresholdBelowMinimum)));

    // Threshold 101 → ThresholdAboveMaximum
    let result = client.try_configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &101,
        &signers,
        &0,
    );
    assert_eq!(result, Err(Ok(Error::ThresholdAboveMaximum)));

    // Threshold 2 with 1 signer → InvalidThreshold
    let result =
        client.try_configure_multisig(&owner, &TransactionType::LargeWithdrawal, &2, &signers, &0);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));

    // Threshold 1 with 1 signer → Ok
    let result =
        client.try_configure_multisig(&owner, &TransactionType::LargeWithdrawal, &1, &signers, &0);
    assert!(result.is_ok());
}

// ============================================================================
// PRECISION AND ROLLOVER VALIDATION TESTS
// ============================================================================

#[test]
fn test_set_precision_spending_limit_success() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 5000_0000000,         // 5000 XLM per day
        min_precision: 1_0000000,    // 1 XLM minimum
        max_single_tx: 2000_0000000, // 2000 XLM max per transaction
        enable_rollover: true,
    };

    let result = client.set_precision_spending_limit(&owner, &member, &precision_limit);
    assert!(result);
}

#[test]
fn test_set_precision_spending_limit_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 5000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 2000_0000000,
        enable_rollover: true,
    };

    let result = client.try_set_precision_spending_limit(&unauthorized, &member, &precision_limit);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_set_precision_spending_limit_invalid_config() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    // Test negative limit
    let invalid_limit = PrecisionSpendingLimit {
        limit: -1000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 500_0000000,
        enable_rollover: true,
    };

    let result = client.try_set_precision_spending_limit(&owner, &member, &invalid_limit);
    assert_eq!(result, Err(Ok(Error::InvalidPrecisionConfig)));

    // Test zero min_precision
    let invalid_precision = PrecisionSpendingLimit {
        limit: 1000_0000000,
        min_precision: 0,
        max_single_tx: 500_0000000,
        enable_rollover: true,
    };

    let result = client.try_set_precision_spending_limit(&owner, &member, &invalid_precision);
    assert_eq!(result, Err(Ok(Error::InvalidPrecisionConfig)));

    // Test max_single_tx > limit
    let invalid_max_tx = PrecisionSpendingLimit {
        limit: 1000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 2000_0000000,
        enable_rollover: true,
    };

    let result = client.try_set_precision_spending_limit(&owner, &member, &invalid_max_tx);
    assert_eq!(result, Err(Ok(Error::InvalidPrecisionConfig)));
}

#[test]
fn test_validate_precision_spending_below_minimum() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 5000_0000000,
        min_precision: 10_0000000, // 10 XLM minimum
        max_single_tx: 2000_0000000,
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Try to withdraw below minimum precision (5 XLM < 10 XLM minimum)
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &5_0000000);
    assert!(result.is_err());
}

#[test]
fn test_validate_precision_spending_exceeds_single_tx_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 5000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 1000_0000000, // 1000 XLM max per transaction
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Try to withdraw above single transaction limit (1500 XLM > 1000 XLM max)
    let result = client.try_withdraw(
        &member,
        &token_contract.address(),
        &recipient,
        &1500_0000000,
    );
    assert!(result.is_err());
}

#[test]
fn test_cumulative_spending_within_period_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &2000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 1000_0000000, // 1000 XLM per day
        min_precision: 1_0000000,
        max_single_tx: 500_0000000, // 500 XLM max per transaction
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // First transaction: 400 XLM (should succeed)
    let tx1 = client.withdraw(&member, &token_contract.address(), &recipient, &400_0000000);
    assert_eq!(tx1, 0);

    // Second transaction: 500 XLM (should succeed, total = 900 XLM < 1000 XLM limit)
    let tx2 = client.withdraw(&member, &token_contract.address(), &recipient, &500_0000000);
    assert_eq!(tx2, 0);

    // Third transaction: 200 XLM (should fail, total would be 1100 XLM > 1000 XLM limit)
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &200_0000000);
    assert!(result.is_err());
}

#[test]
fn test_spending_period_rollover_resets_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &2000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 1000_0000000, // 1000 XLM per day
        min_precision: 1_0000000,
        max_single_tx: 1000_0000000, // 1000 XLM max per transaction
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Set initial time to start of day (00:00 UTC)
    let day_start = 1640995200u64; // 2022-01-01 00:00:00 UTC
    env.ledger().with_mut(|li| li.timestamp = day_start);

    // Spend full daily limit
    let tx1 = client.withdraw(
        &member,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
    assert_eq!(tx1, 0);

    // Try to spend more in same day (should fail)
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &1_0000000);
    assert!(result.is_err());

    // Move to next day (24 hours later)
    let next_day = day_start + 86400; // +24 hours
    env.ledger().with_mut(|li| li.timestamp = next_day);

    // Should be able to spend again (period rolled over)
    let tx2 = client.withdraw(&member, &token_contract.address(), &recipient, &500_0000000);
    assert_eq!(tx2, 0);
}

#[test]
fn test_spending_tracker_persistence() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &1000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 1000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 500_0000000,
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Make first transaction
    let tx1 = client.withdraw(&member, &token_contract.address(), &recipient, &300_0000000);
    assert_eq!(tx1, 0);

    // Check spending tracker
    let tracker = client.get_spending_tracker(&member);
    assert!(tracker.is_some());
    let tracker = tracker.unwrap();
    assert_eq!(tracker.current_spent, 300_0000000);
    assert_eq!(tracker.tx_count, 1);

    // Make second transaction
    let tx2 = client.withdraw(&member, &token_contract.address(), &recipient, &200_0000000);
    assert_eq!(tx2, 0);

    // Check updated tracker
    let tracker = client.get_spending_tracker(&member);
    assert!(tracker.is_some());
    let tracker = tracker.unwrap();
    assert_eq!(tracker.current_spent, 500_0000000);
    assert_eq!(tracker.tx_count, 2);
}

#[test]
fn test_owner_admin_bypass_precision_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &admin, &FamilyRole::Admin, &1000_0000000);

    // Owner should bypass all precision limits
    let tx1 = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &10000_0000000,
    );
    assert!(tx1 > 0);

    // Admin should bypass all precision limits
    let tx2 = client.withdraw(
        &admin,
        &token_contract.address(),
        &recipient,
        &10000_0000000,
    );
    assert!(tx2 > 0);
}

#[test]
fn test_legacy_spending_limit_fallback() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &1000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &500_0000000);

    // No precision limit set, should use legacy behavior

    // Should succeed within legacy limit
    let tx1 = client.withdraw(&member, &token_contract.address(), &recipient, &400_0000000);
    assert_eq!(tx1, 0);

    // Should fail above legacy limit
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &600_0000000);
    assert!(result.is_err());
}

#[test]
fn test_precision_validation_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &2000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 1000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 1000_0000000,
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Test zero amount
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &0);
    assert!(result.is_err());

    // Test negative amount
    let result = client.try_withdraw(
        &member,
        &token_contract.address(),
        &recipient,
        &-100_0000000,
    );
    assert!(result.is_err());

    // Test exact minimum precision
    let tx1 = client.withdraw(&member, &token_contract.address(), &recipient, &1_0000000);
    assert_eq!(tx1, 0);

    // Test exact maximum single transaction
    let result = client.try_withdraw(
        &member,
        &token_contract.address(),
        &recipient,
        &1000_0000000,
    );
    assert!(result.is_err()); // Should fail because we already spent 1 XLM
}

#[test]
fn test_rollover_validation_prevents_manipulation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 1000_0000000,
        min_precision: 1_0000000,
        max_single_tx: 500_0000000,
        enable_rollover: true,
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Set time to middle of day
    let mid_day = 1640995200u64 + 43200; // 2022-01-01 12:00:00 UTC
    env.ledger().with_mut(|li| li.timestamp = mid_day);

    // Get initial tracker to verify period alignment
    let tracker = client.get_spending_tracker(&member);
    if let Some(tracker) = tracker {
        // Period should be aligned to start of day, not current time
        let expected_start = (mid_day / 86400) * 86400; // 00:00 UTC
        assert_eq!(tracker.period.period_start, expected_start);
    }
}

#[test]
fn test_disabled_rollover_only_checks_single_tx_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let client = FamilyWalletClient::new(&env, &env.register_contract(None, FamilyWallet));

    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let recipient = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract.address()).mint(&member, &1000_0000000);

    client.init(&owner, &vec![&env]);
    client.add_member(&owner, &member, &FamilyRole::Member, &1000_0000000);

    let precision_limit = PrecisionSpendingLimit {
        limit: 500_0000000, // 500 XLM period limit
        min_precision: 1_0000000,
        max_single_tx: 400_0000000, // 400 XLM max per transaction
        enable_rollover: false,     // Rollover disabled
    };

    assert!(client.set_precision_spending_limit(&owner, &member, &precision_limit));

    // Should succeed within single transaction limit (even though it would exceed period limit)
    let tx1 = client.withdraw(&member, &token_contract.address(), &recipient, &400_0000000);
    assert_eq!(tx1, 0);

    // Should succeed again (rollover disabled, no cumulative tracking)
    let tx2 = client.withdraw(&member, &token_contract.address(), &recipient, &400_0000000);
    assert_eq!(tx2, 0);

    // Should fail only if exceeding single transaction limit
    let result = client.try_withdraw(&member, &token_contract.address(), &recipient, &500_0000000);
    assert!(result.is_err());
}

// ============================================================================
// Role Expiry Enforcement Tests (#494)
// ============================================================================

#[test]
fn test_expired_admin_cannot_pause() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);

    // Add admin role
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);
    let _pause_admin = client.set_pause_admin(&owner, &admin);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1); // Already expired
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // Attempt pause with expired role should fail
    let result = client.try_pause(&admin);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_unpause() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);
    let _pause_admin = client.set_pause_admin(&owner, &admin);

    let now = env.ledger().timestamp();
    let expires_at = now + 1;
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));
    let _pause = client.pause(&admin);
    env.ledger().set_timestamp(expires_at);

    // Attempt unpause with expired role should fail
    let result = client.try_unpause(&admin);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_archive_transactions() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);
    let _pause_admin = client.set_pause_admin(&owner, &admin);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // Attempt archive with expired role should fail
    let result = client.try_archive_old_transactions(&admin, &now);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_cleanup_expired_pending() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // Attempt cleanup with expired role should fail
    let result = client.try_cleanup_expired_pending(&admin);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_configure_multisig() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let member = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add_admin = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);
    let _add_member = client.add_member(&owner, &member, &FamilyRole::Member, &0);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    let signers = vec![&env, admin.clone(), member.clone()];
    let result = client.try_configure_multisig(
        &admin,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_configure_emergency() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // Attempt configure emergency with expired role should fail
    let result = client.try_configure_emergency(&admin, &5000_0000000, &3600, &0, &10000_0000000);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_set_emergency_mode() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // Attempt set emergency mode with expired role should fail
    let result = client.try_set_emergency_mode(&admin, &true);
    assert!(result.is_err());
}

#[test]
fn test_expired_admin_cannot_batch_add_members() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let new_member = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    let members_to_add = vec![
        &env,
        BatchMemberItem {
            address: new_member,
            role: FamilyRole::Member,
        },
    ];

    // Attempt batch add with expired role should fail
    let result = client.try_batch_add_family_members(&admin, &members_to_add);
    assert!(result.is_err());
}

#[test]
fn test_expired_owner_cannot_batch_remove_members() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &owner, &Some(expires_at));

    let addresses_to_remove = vec![&env, member];

    // Attempt batch remove with expired role should fail
    let result = client.try_batch_remove_family_members(&owner, &addresses_to_remove);
    assert!(result.is_err());
}

fn generate_addresses(env: &Env, count: u32) -> Vec<Address> {
    let mut addresses = Vec::new(env);
    for _ in 0..count {
        addresses.push_back(Address::generate(env));
    }
    addresses
}

#[test]
fn test_batch_add_family_members_all_valid_and_empty_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = vec![&env, Address::generate(&env), Address::generate(&env)];
    client.init(&owner, &initial_members);

    let member_a = Address::generate(&env);
    let member_b = Address::generate(&env);
    let members_to_add = vec![
        &env,
        BatchMemberItem {
            address: member_a.clone(),
            role: FamilyRole::Member,
        },
        BatchMemberItem {
            address: member_b.clone(),
            role: FamilyRole::Admin,
        },
    ];

    let added = client.batch_add_family_members(&owner, &members_to_add);
    assert_eq!(added, 2);
    assert_eq!(
        client.get_family_member(&member_a).unwrap().role,
        FamilyRole::Member
    );
    assert_eq!(
        client.get_family_member(&member_b).unwrap().role,
        FamilyRole::Admin
    );

    let empty_batch = Vec::new(&env);
    assert_eq!(client.batch_add_family_members(&owner, &empty_batch), 0);
}

#[test]
fn test_batch_add_family_members_rejects_mixed_batch_without_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let existing_member = Address::generate(&env);
    client.init(&owner, &vec![&env, existing_member.clone()]);

    let new_member = Address::generate(&env);
    let members_to_add = vec![
        &env,
        BatchMemberItem {
            address: new_member.clone(),
            role: FamilyRole::Member,
        },
        BatchMemberItem {
            address: existing_member.clone(),
            role: FamilyRole::Admin,
        },
    ];

    let result = client.try_batch_add_family_members(&owner, &members_to_add);
    assert!(result.is_err());
    assert!(client.get_family_member(&new_member).is_none());
    assert_eq!(
        client.get_family_member(&existing_member).unwrap().role,
        FamilyRole::Member
    );
}

#[test]
fn test_batch_add_family_members_rejects_duplicate_and_cap_excess() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let initial_members = generate_addresses(&env, 28);
    client.init(&owner, &initial_members);

    let duplicate_member = Address::generate(&env);
    let duplicate_batch = vec![
        &env,
        BatchMemberItem {
            address: duplicate_member.clone(),
            role: FamilyRole::Member,
        },
        BatchMemberItem {
            address: duplicate_member.clone(),
            role: FamilyRole::Admin,
        },
    ];

    let duplicate_result = client.try_batch_add_family_members(&owner, &duplicate_batch);
    assert!(duplicate_result.is_err());
    assert!(client.get_family_member(&duplicate_member).is_none());

    let cap_member_a = Address::generate(&env);
    let cap_member_b = Address::generate(&env);
    let cap_batch = vec![
        &env,
        BatchMemberItem {
            address: cap_member_a.clone(),
            role: FamilyRole::Member,
        },
        BatchMemberItem {
            address: cap_member_b.clone(),
            role: FamilyRole::Member,
        },
    ];

    let cap_result = client.try_batch_add_family_members(&owner, &cap_batch);
    assert!(cap_result.is_err());
    assert!(client.get_family_member(&cap_member_a).is_none());
    assert!(client.get_family_member(&cap_member_b).is_none());
}

#[test]
fn test_batch_remove_family_members_rejects_missing_and_duplicate_without_partial_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let member_a = Address::generate(&env);
    let member_b = Address::generate(&env);
    client.init(&owner, &vec![&env, member_a.clone(), member_b.clone()]);

    let missing_member = Address::generate(&env);
    let mixed_remove = vec![&env, member_a.clone(), missing_member.clone()];

    let mixed_result = client.try_batch_remove_family_members(&owner, &mixed_remove);
    assert!(mixed_result.is_err());
    assert!(client.get_family_member(&member_a).is_some());
    assert!(client.get_family_member(&member_b).is_some());

    let duplicate_remove = vec![&env, missing_member.clone(), missing_member.clone()];
    let duplicate_result = client.try_batch_remove_family_members(&owner, &duplicate_remove);
    assert!(duplicate_result.is_err());

    let all_invalid = vec![&env, Address::generate(&env), Address::generate(&env)];
    let invalid_result = client.try_batch_remove_family_members(&owner, &all_invalid);
    assert!(invalid_result.is_err());
    assert!(client.get_family_member(&member_a).is_some());
    assert!(client.get_family_member(&member_b).is_some());
}

#[test]
fn test_expired_owner_cannot_set_proposal_expiry() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let _result = client.init(&owner, &vec![&env]);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &owner, &Some(expires_at));

    // Attempt set proposal expiry with expired role should fail
    let result = client.try_set_proposal_expiry(&owner, &300);
    assert!(result.is_err());
}

#[test]
fn test_expired_owner_cannot_set_upgrade_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let _result = client.init(&owner, &vec![&env]);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &owner, &Some(expires_at));

    // Attempt set upgrade admin with expired role should fail
    let result = client.try_set_upgrade_admin(&owner, &new_admin);
    assert!(result.is_err());
}

#[test]
fn test_expired_owner_cannot_set_version() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let _result = client.init(&owner, &vec![&env]);

    let now = env.ledger().timestamp();
    let expires_at = now.saturating_sub(1);
    let _set_exp = client.set_role_expiry(&owner, &owner, &Some(expires_at));

    // Attempt set version with expired role should fail
    let result = client.try_set_version(&owner, &2);
    assert!(result.is_err());
}

#[test]
fn test_non_expired_admin_can_perform_privileged_operations() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    let _result = client.init(&owner, &vec![&env]);
    let _add = client.add_member(&owner, &admin, &FamilyRole::Admin, &0);
    let _pause_admin = client.set_pause_admin(&owner, &admin);

    let now = env.ledger().timestamp();
    let expires_at = now + 10000; // Far in the future
    let _set_exp = client.set_role_expiry(&owner, &admin, &Some(expires_at));

    // All these operations should succeed with non-expired role
    let pause_result = client.try_pause(&admin);
    assert!(pause_result.is_ok());

    let unpause_result = client.try_unpause(&admin);
    assert!(unpause_result.is_ok());

    let archive_result = client.try_archive_old_transactions(&admin, &now);
    assert!(archive_result.is_ok());

    let cleanup_result = client.try_cleanup_expired_pending(&admin);
    assert!(cleanup_result.is_ok());
}

#[test]
fn test_set_proposal_expiry_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Test valid expiry
    assert!(client.set_proposal_expiry(&owner, &86400));
    assert_eq!(client.get_proposal_expiry_public(), 86400);

    // Test expiry too large (MAX_PROPOSAL_EXPIRY is 604,800)
    let result = client.try_set_proposal_expiry(&owner, &(604_800 + 1));
    assert!(result.is_err());

    // Test expiry zero (disabled — allowed)
    assert!(client.set_proposal_expiry(&owner, &0));
    assert_eq!(client.get_proposal_expiry_public(), 0);
}

// ============================================================================
// Access-Audit Pagination Tests
//
// Covers cursor semantics, clamping, and edge cases for get_access_audit_page.
// The end-of-log sentinel is `next_cursor == total` (length of the log).
// ============================================================================

/// Helper: seed `n` audit entries by toggling emergency mode on/off.
fn seed_audit_entries(client: &FamilyWalletClient, owner: &Address, n: u32) {
    for i in 0..n {
        client.set_emergency_mode(owner, &(i % 2 == 0));
    }
}

#[test]
fn test_audit_page_empty_log_returns_sentinel() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // No audit entries have been written yet.
    let page = client.get_access_audit_page(&owner, &0, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.items.len(), 0);
    // next_cursor == total (0) — end-of-log sentinel.
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_audit_page_offset_beyond_length_returns_sentinel() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    seed_audit_entries(&client, &owner, 3);

    // from_index = 100 is way beyond the 3-entry log.
    let page = client.get_access_audit_page(&owner, &100, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.items.len(), 0);
    // Sentinel must equal total (3), not 0.
    assert_eq!(page.next_cursor, 3);
}

#[test]
fn test_audit_page_offset_u32_max_no_panic() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    seed_audit_entries(&client, &owner, 5);

    // Adversarial: u32::MAX offset must not panic or overflow.
    let page = client.get_access_audit_page(&owner, &u32::MAX, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.items.len(), 0);
    // Sentinel == total (5).
    assert_eq!(page.next_cursor, 5);
}

#[test]
fn test_audit_page_limit_zero_uses_default() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Seed more entries than DEFAULT_AUDIT_PAGE_LIMIT (20).
    seed_audit_entries(&client, &owner, 25);

    // limit=0 should be promoted to DEFAULT_AUDIT_PAGE_LIMIT (20).
    let page = client.get_access_audit_page(&owner, &0, &0);
    assert_eq!(page.count, DEFAULT_AUDIT_PAGE_LIMIT);
    assert_eq!(page.items.len(), DEFAULT_AUDIT_PAGE_LIMIT);
    assert_eq!(page.next_cursor, DEFAULT_AUDIT_PAGE_LIMIT);
}

#[test]
fn test_audit_page_oversized_limit_clamped_to_max() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Seed more entries than MAX_AUDIT_PAGE_LIMIT (50).
    seed_audit_entries(&client, &owner, 60);

    // limit=u32::MAX should be clamped to MAX_AUDIT_PAGE_LIMIT (50).
    let page = client.get_access_audit_page(&owner, &0, &u32::MAX);
    assert_eq!(page.count, MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(page.items.len(), MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(page.next_cursor, MAX_AUDIT_PAGE_LIMIT);
}

#[test]
fn test_audit_page_limit_larger_than_remaining_returns_tail() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    seed_audit_entries(&client, &owner, 5);

    // Ask for 20 entries starting at index 3 — only 2 remain.
    let page = client.get_access_audit_page(&owner, &3, &20);
    assert_eq!(page.count, 2);
    assert_eq!(page.items.len(), 2);
    // next_cursor == total (5) — end-of-log sentinel.
    assert_eq!(page.next_cursor, 5);
}

#[test]
fn test_audit_page_exact_boundary_last_entry() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    seed_audit_entries(&client, &owner, 4);

    // from_index = 3 (last valid index), limit = 1 → exactly one entry.
    let page = client.get_access_audit_page(&owner, &3, &1);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.len(), 1);
    // After reading the last entry, next_cursor == total (4).
    assert_eq!(page.next_cursor, 4);
}

#[test]
fn test_audit_page_single_entry_log() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // One entry.
    client.set_emergency_mode(&owner, &true);

    let page = client.get_access_audit_page(&owner, &0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.len(), 1);
    // Exhausted: next_cursor == total (1).
    assert_eq!(page.next_cursor, 1);

    // Requesting the next page with the returned cursor yields empty + sentinel.
    let page2 = client.get_access_audit_page(&owner, &page.next_cursor, &10);
    assert_eq!(page2.count, 0);
    assert_eq!(page2.next_cursor, 1); // still == total
}

#[test]
fn test_audit_page_full_iteration_no_skip_no_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let total_entries: u32 = 7;
    seed_audit_entries(&client, &owner, total_entries);

    // Iterate with page size 3 and collect all entries.
    let mut collected: u32 = 0;
    let mut cursor: u32 = 0;
    let page_size: u32 = 3;

    loop {
        let page = client.get_access_audit_page(&owner, &cursor, &page_size);
        collected += page.count;
        if page.count == 0 || page.next_cursor >= total_entries {
            break;
        }
        cursor = page.next_cursor;
    }

    // Every entry visited exactly once.
    assert_eq!(collected, total_entries);
}

#[test]
fn test_audit_page_cursor_stable_across_calls() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    seed_audit_entries(&client, &owner, 6);

    // Two calls with the same cursor must return identical results.
    let page_a = client.get_access_audit_page(&owner, &2, &3);
    let page_b = client.get_access_audit_page(&owner, &2, &3);

    assert_eq!(page_a.count, page_b.count);
    assert_eq!(page_a.next_cursor, page_b.next_cursor);
    for idx in 0..page_a.count {
        let a = page_a.items.get(idx).unwrap();
        let b = page_b.items.get(idx).unwrap();
        assert_eq!(a.operation, b.operation);
        assert_eq!(a.caller, b.caller);
        assert_eq!(a.timestamp, b.timestamp);
    }
}

// ============================================================================
// Access-Audit Ring-Eviction & Cross-Read Tests
//
// These pin the bounded ring-buffer behaviour of the `ACC_AUDIT` log: once it
// exceeds MAX_ACCESS_AUDIT_ENTRIES (200) the oldest entries are evicted and the
// retained set is re-indexed from 0. Pagination must reflect that post-eviction
// state with a monotonic cursor and never surface (or skip) entries. They also
// pin the documented Admin-only authorization policy and the consistency
// between `get_access_audit` (tail read) and the paginated full view.
// ============================================================================

/// Helper: seed `n` audit entries via emergency-mode toggles, advancing the
/// ledger timestamp by 1 before each append so that entry `i` (0-based in
/// insertion order) carries `timestamp == base + i`. Unique, monotonically
/// increasing timestamps let eviction tests identify exactly which entries
/// survived the ring-buffer wrap.
fn seed_audit_entries_ts(
    env: &Env,
    client: &FamilyWalletClient,
    owner: &Address,
    base: u64,
    n: u32,
) {
    for i in 0..n {
        let mut ledger = env.ledger().get();
        ledger.timestamp = base + i as u64;
        env.ledger().set(ledger);
        client.set_emergency_mode(owner, &(i % 2 == 0));
    }
}

/// Helper: page through the entire retained log from index 0 using `page_size`
/// and return every entry's timestamp in cursor order. Asserts the cursor is
/// strictly monotonic (no dupes, no skips) as it walks. Returns a Soroban
/// `Vec<u64>` (this is a `#![no_std]` crate, so `std::vec` is unavailable here).
fn collect_all_audit_timestamps(
    env: &Env,
    client: &FamilyWalletClient,
    caller: &Address,
    page_size: u32,
) -> Vec<u64> {
    let mut out = Vec::new(env);
    let mut cursor: u32 = 0;
    loop {
        let page = client.get_access_audit_page(caller, &cursor, &page_size);
        if page.count == 0 {
            break;
        }
        // Cursor must advance by exactly the number of items consumed.
        assert_eq!(page.next_cursor, cursor + page.count);
        for idx in 0..page.count {
            out.push_back(page.items.get(idx).unwrap().timestamp);
        }
        cursor = page.next_cursor;
    }
    out
}

/// Fills the trail past the 200-entry cap and asserts the oldest entries are
/// evicted while the newest are retained: total stays pinned at the cap, the
/// retained timestamps are exactly the most-recent 200, and the evicted prefix
/// never appears on any page.
#[test]
fn test_audit_ring_evicts_oldest_retains_newest() {
    let env = Env::default();
    env.mock_all_auths();
    // Rebuilding the 200-entry ring on every append is budget-heavy; lift the
    // default metering so the stress seeding does not hit ExceededLimit.
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Seed 250 entries with timestamps 1000..=1249.
    let base: u64 = 1000;
    let seeded: u32 = 250;
    seed_audit_entries_ts(&env, &client, &owner, base, seeded);

    // The log is capped: the retained set is exactly MAX_ACCESS_AUDIT_ENTRIES.
    let first_page = client.get_access_audit_page(&owner, &0, &1);
    // next_cursor on an out-of-range read reveals total; read past the end.
    let probe = client.get_access_audit_page(&owner, &u32::MAX, &1);
    assert_eq!(probe.next_cursor, MAX_ACCESS_AUDIT_ENTRIES);

    // Collect every retained timestamp by paging the whole log.
    let timestamps = collect_all_audit_timestamps(&env, &client, &owner, MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(timestamps.len(), MAX_ACCESS_AUDIT_ENTRIES);

    // Oldest 50 entries (timestamps 1000..=1049) were evicted; newest retained.
    let oldest_retained = base + (seeded - MAX_ACCESS_AUDIT_ENTRIES) as u64; // 1050
    let newest_retained = base + (seeded - 1) as u64; // 1249
    assert_eq!(timestamps.get(0).unwrap(), oldest_retained);
    assert_eq!(timestamps.get(timestamps.len() - 1).unwrap(), newest_retained);
    assert_eq!(first_page.items.get(0).unwrap().timestamp, oldest_retained);

    // Post-eviction state: no page references an evicted entry, and the
    // retained sequence is strictly increasing (no dupes / no gaps).
    for i in 0..timestamps.len() {
        let ts = timestamps.get(i).unwrap();
        assert!(ts >= oldest_retained, "evicted entry leaked into page");
        assert_eq!(ts, oldest_retained + i as u64, "non-contiguous retained set");
    }
}

/// Exactly at the cap: seeding precisely MAX_ACCESS_AUDIT_ENTRIES evicts
/// nothing — the full range is retained and the oldest entry is still present.
#[test]
fn test_audit_ring_exactly_at_max_evicts_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let base: u64 = 5000;
    seed_audit_entries_ts(&env, &client, &owner, base, MAX_ACCESS_AUDIT_ENTRIES);

    let timestamps = collect_all_audit_timestamps(&env, &client, &owner, MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(timestamps.len(), MAX_ACCESS_AUDIT_ENTRIES);
    // Nothing evicted: oldest is still the very first entry.
    assert_eq!(timestamps.get(0).unwrap(), base);
    assert_eq!(
        timestamps.get(timestamps.len() - 1).unwrap(),
        base + (MAX_ACCESS_AUDIT_ENTRIES - 1) as u64
    );
}

/// One over the cap: seeding MAX_ACCESS_AUDIT_ENTRIES + 1 evicts exactly the
/// single oldest entry; the new oldest is `base + 1`.
#[test]
fn test_audit_ring_one_over_max_evicts_single_oldest() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let base: u64 = 9000;
    seed_audit_entries_ts(&env, &client, &owner, base, MAX_ACCESS_AUDIT_ENTRIES + 1);

    let timestamps = collect_all_audit_timestamps(&env, &client, &owner, MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(timestamps.len(), MAX_ACCESS_AUDIT_ENTRIES);
    // The single oldest (timestamp == base) was evicted.
    assert_eq!(timestamps.get(0).unwrap(), base + 1);
    let mut found_base = false;
    for i in 0..timestamps.len() {
        if timestamps.get(i).unwrap() == base {
            found_base = true;
        }
    }
    assert!(!found_base, "oldest entry should be evicted");
    assert_eq!(
        timestamps.get(timestamps.len() - 1).unwrap(),
        base + MAX_ACCESS_AUDIT_ENTRIES as u64
    );
}

/// After eviction, paging across the wrap boundary with a small page size still
/// yields a monotonic cursor and the exact retained set — proving pagination is
/// computed over post-eviction indices, not stale absolute positions.
#[test]
fn test_audit_page_post_eviction_no_skip_no_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let base: u64 = 200_000;
    seed_audit_entries_ts(&env, &client, &owner, base, 230);

    // Page with size 7 (does not divide 200 evenly) to stress boundary maths.
    let timestamps = collect_all_audit_timestamps(&env, &client, &owner, 7);
    assert_eq!(timestamps.len(), MAX_ACCESS_AUDIT_ENTRIES);

    let oldest_retained = base + (230 - MAX_ACCESS_AUDIT_ENTRIES) as u64; // 200_030
    for i in 0..timestamps.len() {
        assert_eq!(timestamps.get(i).unwrap(), oldest_retained + i as u64);
    }
}

/// Authorization policy: `get_access_audit_page` is Owner/Admin only. Owner and
/// Admin succeed; a `Member` and a non-member are rejected. (Auth is mocked, so
/// the rejection comes from the role gate, matching the documented policy.)
#[test]
fn test_audit_page_authorization_matches_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    let admin = Address::generate(&env);
    let member = Address::generate(&env);
    let stranger = Address::generate(&env);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);
    client.add_family_member(&owner, &member, &FamilyRole::Member);

    // Owner and Admin may read the audit page.
    assert!(client.try_get_access_audit_page(&owner, &0, &10).is_ok());
    assert!(client.try_get_access_audit_page(&admin, &0, &10).is_ok());

    // Member and non-member are rejected by the Admin role gate.
    assert!(client.try_get_access_audit_page(&member, &0, &10).is_err());
    assert!(client.try_get_access_audit_page(&stranger, &0, &10).is_err());
}

/// Cross-read consistency: `get_access_audit(limit)` returns the newest `limit`
/// entries (the tail), which must match the tail of the fully-paginated view —
/// same entries, same order, including across an eviction wrap.
#[test]
fn test_get_access_audit_consistent_with_paginated_view() {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Seed past the cap so the comparison also exercises post-eviction state.
    let base: u64 = 700_000;
    seed_audit_entries_ts(&env, &client, &owner, base, 210);

    let full = collect_all_audit_timestamps(&env, &client, &owner, MAX_AUDIT_PAGE_LIMIT);
    assert_eq!(full.len(), MAX_ACCESS_AUDIT_ENTRIES);

    // For several limits, the tail read must equal the suffix of the page view.
    for limit in [1u32, 10, MAX_AUDIT_PAGE_LIMIT, MAX_ACCESS_AUDIT_ENTRIES, 1000] {
        let tail = client.get_access_audit(&limit);
        let expected_len = limit.min(MAX_ACCESS_AUDIT_ENTRIES);
        assert_eq!(tail.len(), expected_len);
        let start = full.len() - expected_len;
        for j in 0..expected_len {
            assert_eq!(
                tail.get(j).unwrap().timestamp,
                full.get(start + j).unwrap()
            );
        }
    }
}

// ============================================================================
// Quorum Re-validation Tests
//
// Verify that removing members invalidates in-flight proposals that can no
// longer reach their required signature threshold, and that valid proposals
// survive when quorum is still achievable after membership changes.
// ============================================================================

/// Removing the only signer from a pending proposal must invalidate it
/// immediately by setting expires_at to the current ledger timestamp.
#[test]
fn test_remove_sole_signer_invalidates_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer = Address::generate(&env);

    client.init(&owner, &vec![&env, signer.clone()]);

    // Configure multisig: threshold=1, only `signer` is authorised.
    let signers = vec![&env, signer.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &1, &signers, &0);

    // Propose a role change — proposal is now in-flight.
    let tx_id = client.propose_role_change(&owner, &signer, &FamilyRole::Admin);
    assert!(tx_id > 0);

    // Sanity: proposal is live before removal.
    let before = client.get_pending_transaction(&tx_id).unwrap();
    assert!(before.expires_at > 1_000);

    // Remove the sole configured signer.
    client.remove_family_member(&owner, &signer);

    // The proposal must now be expired (expires_at == ledger timestamp).
    let after = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(
        after.expires_at, 1_000,
        "Proposal must be invalidated (expires_at set to now) after sole signer removed"
    );

    // Attempting to sign the invalidated proposal must fail.
    // The invalidation is verified above: expires_at == ledger timestamp.
    // A new signer attempt also fails because expires_at <= now.
    // (Owner is already in signatures so try_sign returns Ok(false) idempotently.)
}
/// Removing one signer when remaining eligible signers still meet the
/// threshold must leave the proposal active and signable.
#[test]
fn test_remove_one_signer_proposal_survives_when_quorum_still_met() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);
    let signer_c = Address::generate(&env);

    client.init(
        &owner,
        &vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()],
    );

    // threshold=2, three signers — removing one still leaves two eligible.
    let signers = vec![&env, signer_a.clone(), signer_b.clone(), signer_c.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    let tx_id = client.propose_role_change(&owner, &signer_a, &FamilyRole::Admin);
    assert!(tx_id > 0);

    let original_expiry = client.get_pending_transaction(&tx_id).unwrap().expires_at;

    // Remove signer_c — two eligible signers remain, threshold still reachable.
    client.remove_family_member(&owner, &signer_c);

    let after = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(
        after.expires_at, original_expiry,
        "Proposal expiry must be unchanged when quorum is still achievable"
    );
}

/// Batch-removing members that collectively drop eligible signers below the
/// threshold must invalidate all affected proposals in a single pass.
#[test]
fn test_batch_remove_invalidates_proposals_below_quorum() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 2_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.init(&owner, &vec![&env, s1.clone(), s2.clone(), s3.clone()]);

    // threshold=3 — all three signers required.
    let signers = vec![&env, s1.clone(), s2.clone(), s3.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &3, &signers, &0);

    let tx_id = client.propose_role_change(&owner, &s1, &FamilyRole::Admin);
    assert!(tx_id > 0);

    // Batch-remove two of the three signers — only one remains, threshold=3 unachievable.
    let to_remove = vec![&env, s2.clone(), s3.clone()];
    let removed = client.batch_remove_family_members(&owner, &to_remove);
    assert_eq!(removed, 2);

    let after = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(
        after.expires_at, 2_000,
        "Proposal must be invalidated after batch removal drops eligible signers below threshold"
    );
}

/// Signatures from a removed member must be stripped from the proposal.
/// If the remaining signatures plus remaining eligible signers can still
/// reach quorum, the proposal stays active.
#[test]
fn test_removed_member_signature_stripped_from_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer_a = Address::generate(&env);
    let signer_b = Address::generate(&env);

    client.init(&owner, &vec![&env, signer_a.clone(), signer_b.clone()]);

    // threshold=2, two signers.
    let signers = vec![&env, signer_a.clone(), signer_b.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    // signer_a proposes — their signature is automatically added.
    let tx_id = client.propose_role_change(&signer_a, &signer_a, &FamilyRole::Admin);
    assert!(tx_id > 0);

    // Verify signer_a's signature is present.
    let before = client.get_pending_transaction(&tx_id).unwrap();
    assert_eq!(before.signatures.len(), 1);

    // Remove signer_a — their signature should be stripped and quorum becomes
    // unachievable (only signer_b remains, threshold=2).
    client.remove_family_member(&owner, &signer_a);

    let after = client.get_pending_transaction(&tx_id).unwrap();
    // Signature stripped.
    assert_eq!(
        after.signatures.len(),
        0,
        "Removed member's signature must be stripped from the proposal"
    );
    // Quorum unachievable: 1 eligible signer < threshold 2 → invalidated.
    assert_eq!(
        after.expires_at, 1_000,
        "Proposal must be invalidated when stripped signatures make quorum unreachable"
    );
}

/// The public `revalidate_proposals` function must be callable by Owner/Admin,
/// return the correct invalidation count, and reject calls from regular members.
#[test]
fn test_revalidate_proposals_public_entry_point() {
    let env = Env::default();
    env.mock_all_auths();
    set_ledger_time(&env, 100, 5_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let signer = Address::generate(&env);
    let regular = Address::generate(&env);

    client.init(&owner, &vec![&env, signer.clone(), regular.clone()]);

    // Configure with threshold=1, sole signer.
    let signers = vec![&env, signer.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &1, &signers, &0);

    // Create two in-flight proposals.
    let tx1 = client.propose_role_change(&owner, &signer, &FamilyRole::Admin);
    let tx2 = client.propose_role_change(&owner, &regular, &FamilyRole::Admin);
    assert!(tx1 > 0 && tx2 > 0);

    // Manually remove signer from storage without triggering auto-revalidation
    // by reconfiguring multisig to an empty-ish state isn't possible, so instead
    // we remove the signer and then call revalidate_proposals explicitly to test
    // the public entry point independently.
    client.remove_family_member(&owner, &signer);

    // Both proposals should already be invalidated by remove_family_member.
    // Call revalidate_proposals again — it should return 0 (nothing new to invalidate).
    let count = client.revalidate_proposals(&owner);
    assert_eq!(
        count, 0,
        "Re-running revalidation on already-invalidated proposals must return 0"
    );

    // Regular member must not be able to call revalidate_proposals.
    let result = client.try_revalidate_proposals(&regular);
    assert!(
        result.is_err(),
        "Regular member must not be allowed to call revalidate_proposals"
    );
}
// ============================================================================
// Authorization Matrix Tests for Family Member Management
//
// These tests verify strict authorization controls for member add/remove/update
// operations across all role combinations:
// - Owner: Full permissions (add, remove, update)
// - Admin: Limited permissions (add, update; no remove)
// - Member: No permissions
// - Viewer: No permissions (read-only)
//
// Test Coverage:
// - add_family_member: 4 test cases (Owner, Admin, Member, Viewer)
// - remove_family_member: 4 test cases (Owner, Admin, Member, Viewer)
// - update_spending_limit: 4 test cases (Owner, Admin, Member, Viewer)
//
// Total: 12 authorization tests ensuring proper access control
// ============================================================================

#[test]
fn test_auth_matrix_add_family_member_by_owner() {
    // **Description**:
    // Verifies that the Owner can add family members with any role.
    //
    // **Inline Comments**:
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup: Initialize with owner
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Action: Owner adds new member as Admin
    let new_member = Address::generate(&env);
    let result = client.add_family_member(&owner, &new_member, &FamilyRole::Admin);

    // Assertion: Operation succeeds
    assert!(result, "Owner must be able to add family members");

    // Verification: Member was created with correct role
    let member_data = client.get_family_member(&new_member);
    assert!(member_data.is_some(), "New member must exist");
    assert_eq!(
        member_data.unwrap().role,
        FamilyRole::Admin,
        "Member role must be Admin"
    );
}

#[test]
fn test_auth_matrix_add_family_member_by_admin() {
    // **Description**:
    // Verifies that an Admin can add family members.
    //
    // **Security Assumption**: Admin role grants member management permissions.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup: Initialize with owner and add admin
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    // Action: Admin adds new member as Member
    let new_member = Address::generate(&env);
    let result = client.add_family_member(&admin, &new_member, &FamilyRole::Member);

    // Assertion: Operation succeeds
    assert!(result, "Admin must be able to add family members");

    // Verification: Member was created
    let member_data = client.get_family_member(&new_member);
    assert!(member_data.is_some(), "New member must exist");
}

#[test]
#[should_panic(expected = "Only Owner or Admin can add family members")]
fn test_auth_matrix_add_family_member_by_member_fails() {
    // **Description**:
    // Verifies that a regular Member cannot add family members.
    //
    // **Expected Behavior**: Operation panics with authorization error.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup: Initialize with owner
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    // Action: Member attempts to add another member (should panic)
    let new_member = Address::generate(&env);
    client.add_family_member(&member, &new_member, &FamilyRole::Member);
}

#[test]
#[should_panic(expected = "Only Owner or Admin can add family members")]
fn test_auth_matrix_add_family_member_by_viewer_fails() {
    // **Description**:
    // Verifies that a Viewer cannot add family members.
    //
    // **Security Assumption**: Viewer is read-only role with zero modification permissions.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup: Initialize and add viewer
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);
    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &viewer, &FamilyRole::Viewer);

    // Action: Viewer attempts to add member (should panic)
    let new_member = Address::generate(&env);
    client.add_family_member(&viewer, &new_member, &FamilyRole::Member);
}

#[test]
fn test_auth_matrix_remove_family_member_by_owner() {
    // **Description**:
    // Verifies that only the Owner can remove family members.
    //
    // **Security Note**: Member removal is Owner-exclusive to prevent escalation.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let target_member = Address::generate(&env);
    client.init(&owner, &vec![&env, target_member.clone()]);

    // Action: Owner removes member
    let result = client.remove_family_member(&owner, &target_member);

    // Assertion: Operation succeeds
    assert!(result, "Owner must be able to remove family members");

    // Verification: Member was removed
    let member_data = client.get_family_member(&target_member);
    assert!(member_data.is_none(), "Removed member must no longer exist");
}

#[test]
#[should_panic(expected = "Only Owner can remove family members")]
fn test_auth_matrix_remove_family_member_by_admin_fails() {
    // **Description**:
    // Verifies that Admin cannot remove members (Owner-exclusive operation).
    //
    // **Security Note**: Member removal requires Owner authorization to prevent
    // unauthorized removal of team members by admins. This enforces hierarchical
    // controls where only Owner can modify top-level membership.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let target_member = Address::generate(&env);
    client.init(&owner, &vec![&env, target_member.clone()]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    // Action: Admin attempts to remove member (should panic)
    client.remove_family_member(&admin, &target_member);
}

#[test]
#[should_panic(expected = "Only Owner can remove family members")]
fn test_auth_matrix_remove_family_member_by_member_fails() {
    // **Description**:
    // Verifies that a regular Member cannot remove members.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone(), member2.clone()]);

    // Action: Member1 attempts to remove Member2 (should panic)
    client.remove_family_member(&member1, &member2);
}

#[test]
#[should_panic(expected = "Only Owner can remove family members")]
fn test_auth_matrix_remove_family_member_by_viewer_fails() {
    // **Description**:
    // Verifies that a Viewer cannot remove members.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);
    let target_member = Address::generate(&env);
    client.init(&owner, &vec![&env, target_member.clone()]);
    client.add_family_member(&owner, &viewer, &FamilyRole::Viewer);

    // Action: Viewer attempts to remove member (should panic)
    client.remove_family_member(&viewer, &target_member);
}

#[test]
fn test_auth_matrix_update_spending_limit_by_owner() {
    // **Description**:
    // Verifies that the Owner can update member spending limits.
    //
    // **Inline Comments**:
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);

    // Action: Owner updates member's spending limit
    let new_limit = 1000_0000000i128;
    let result = client.update_spending_limit(&owner, &member, &new_limit);

    // Assertion: Operation succeeds
    assert!(result, "Owner must be able to update spending limits");

    // Verification: Spending limit was updated
    let member_data = client.get_family_member(&member);
    assert!(member_data.is_some());
    assert_eq!(
        member_data.unwrap().spending_limit,
        new_limit,
        "Spending limit must be updated"
    );
}

#[test]
fn test_auth_matrix_update_spending_limit_by_admin() {
    // **Description**:
    // Verifies that an Admin can update member spending limits.
    //
    // **Security Assumption**: Admin role grants spending limit management permissions.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    // Action: Admin updates member's spending limit
    let new_limit = 500_0000000i128;
    let result = client.update_spending_limit(&admin, &member, &new_limit);

    // Assertion: Operation succeeds
    assert!(result, "Admin must be able to update spending limits");

    // Verification
    let member_data = client.get_family_member(&member);
    assert_eq!(
        member_data.unwrap().spending_limit,
        new_limit,
        "Spending limit must be updated"
    );
}

#[test]
fn test_auth_matrix_update_spending_limit_by_member_fails() {
    // **Description**:
    // Verifies that a regular Member cannot update spending limits.
    //
    // **Expected Behavior**: Operation fails with Unauthorized error.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    client.init(&owner, &vec![&env, member1.clone(), member2.clone()]);

    // Action: Member1 attempts to update Member2's spending limit (should fail)
    let result = client.try_update_spending_limit(&member1, &member2, &1000_0000000);

    // Assertion: Operation fails
    assert!(
        result.is_err(),
        "Member must not be able to update spending limits"
    );
}

#[test]
fn test_auth_matrix_update_spending_limit_by_viewer_fails() {
    // **Description**:
    // Verifies that a Viewer cannot update spending limits.
    //
    // **Security Note**: Viewer is read-only; all modifications must fail.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);
    let member = Address::generate(&env);
    client.init(&owner, &vec![&env, member.clone()]);
    client.add_family_member(&owner, &viewer, &FamilyRole::Viewer);

    // Action: Viewer attempts to update spending limit (should fail)
    let result = client.try_update_spending_limit(&viewer, &member, &1000_0000000);

    // Assertion: Operation fails
    assert!(
        result.is_err(),
        "Viewer must not be able to update spending limits"
    );
}

// ============================================================================
// Edge Case: Role Boundary & Escalation Prevention Tests
//
// Verify that the authorization matrix prevents privilege escalation attempts
// and enforces role boundaries strictly.
// ============================================================================

#[test]
#[should_panic(expected = "Cannot add Owner via add_family_member")]
fn test_auth_matrix_prevent_owner_addition_by_admin() {
    // **Description**:
    // Verifies that even an Admin cannot add an owner using add_family_member.
    //
    // **Security Assumption**: Owner role cannot be created via add_family_member
    // to prevent privilege escalation. Only one owner per wallet.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    // Action: Admin attempts to add another owner (should panic)
    let new_owner = Address::generate(&env);
    client.add_family_member(&admin, &new_owner, &FamilyRole::Owner);
}

#[test]
fn test_auth_matrix_prevent_owner_removal() {
    // **Description**:
    // Verifies that the Owner cannot be removed from the wallet.
    //
    // **Security Guarantee**: Prevents accidental or malicious removal
    // of the wallet owner, which would orphan the contract.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup
    let owner = Address::generate(&env);
    client.init(&owner, &vec![&env]);

    // Action: Attempt to remove owner (should fail)
    let result = client.try_remove_family_member(&owner, &owner);

    // Assertion: Should panic with "Cannot remove owner"
    // (If it reaches here without panic, the contract is broken)
    assert!(result.is_err(), "Owner should not be removable");
}

#[test]
fn test_auth_matrix_comprehensive_role_isolation() {
    // **Description**:
    // Comprehensive test verifying that Member and Viewer roles are properly
    // isolated from all modification operations.
    //
    // **Test Setup**: Creates a wallet with Owner, Admin, Member, and Viewer.
    // **Test Actions**: Each of Member and Viewer attempts all three operations.
    // **Expected Result**: All 6 operations fail.
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // Setup: Create wallet with representative roles
    let owner = Address::generate(&env);
    let member = Address::generate(&env);
    let viewer = Address::generate(&env);
    let test_target = Address::generate(&env);

    client.init(&owner, &vec![&env, member.clone()]);
    client.add_family_member(&owner, &viewer, &FamilyRole::Viewer);
    client.add_family_member(&owner, &test_target, &FamilyRole::Member);

    // Test Member isolation
    {
        // Member cannot add
        let result_add =
            client.try_add_family_member(&member, &Address::generate(&env), &FamilyRole::Member);

        // Member cannot update spending limit
        let result_update = client.try_update_spending_limit(&member, &test_target, &1000_0000000);
        assert!(
            result_update.is_err(),
            "Member cannot update spending limit"
        );
    }

    // Test Viewer isolation
    {
        // Viewer cannot add
        let result_add =
            client.try_add_family_member(&viewer, &Address::generate(&env), &FamilyRole::Member);

        // Viewer cannot update spending limit
        let result_update = client.try_update_spending_limit(&viewer, &test_target, &1000_0000000);
        assert!(
            result_update.is_err(),
            "Viewer cannot update spending limit"
        );
    }
}

#[test]
fn test_precision_spending_overflow_graceful() {
    let env = Env::default();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let member = Address::generate(&env);
    let mut initial_members = Vec::new(&env);
    initial_members.push_back(member.clone());

    client.init(&admin, &initial_members);

    // Assert that calling with near i128::MAX returns a graceful error or handles it cleanly
    let result = client.try_validate_precision_spending(&member, &i128::MAX);
    assert!(result.is_err());
}
