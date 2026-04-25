use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token::{StellarAssetClient, TokenClient},
    vec, Env,
};
use testutils::set_ledger_time;

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
#[should_panic(expected = "Already signed this transaction")]
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

    client.sign_transaction(&member1, &tx_id);
    client.sign_transaction(&member1, &tx_id);
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
#[should_panic(expected = "Emergency transfer would violate minimum balance requirement")]
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

    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    client.configure_emergency(&owner, &2000_0000000, &0u64, &2500_0000000, &5000_0000000);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &1000_0000000);
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
#[should_panic(expected = "Signer not authorized for this transaction type")]
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

    client.sign_transaction(&member2, &tx_id);
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
