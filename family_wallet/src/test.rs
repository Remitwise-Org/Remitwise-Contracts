use super::*;
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token::{StellarAssetClient, TokenClient},
    vec, Env,
};

#[test]
fn test_init_family_wallet() {
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
#[should_panic(expected = "Only Owner or Admin can configure multi-sig")]
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
    client.configure_multisig(
        &member1,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
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
        &TransactionType::EmergencyTransfer,
        &2,
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

    client.configure_emergency(&owner, &2000_0000000, &3600u64, &1000_0000000);
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

    client.configure_emergency(&owner, &1000_0000000, &3600u64, &0);
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

    client.configure_emergency(&owner, &2000_0000000, &3600u64, &0);
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

    client.configure_emergency(&owner, &2000_0000000, &0u64, &2500_0000000);
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

    client.init(&owner, &initial_members);

    let archived_count = client.archive_old_transactions(&owner, &1_000_000);
    assert_eq!(archived_count, 0);

    let archived = client.get_archived_transactions(&10);
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

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

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

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);

    client.init(&owner, &vec![&env, member1.clone()]);

    // Advance ledger so TTL drops below threshold (17,280)
    // After init at seq 100: live_until = 518,500
    // At seq 510,000: TTL = 8,500 < 17,280 ✓
    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 510_000,
        timestamp: 500_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

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

    env.ledger().set(LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

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
    let _archived = client.archive_old_transactions(&owner, &2_000_000);

    // TTL should be extended
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after archiving",
        ttl
    );
}
