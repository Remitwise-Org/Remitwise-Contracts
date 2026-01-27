use super::*;
use soroban_sdk::{
    testutils::Address as _,
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

    // Verify owner
    let stored_owner = client.get_owner();
    assert_eq!(stored_owner, owner);

    // Verify members
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

    // Configure 2-of-3 multi-sig for large withdrawals
    let signers = vec![&env, member1.clone(), member2.clone(), member3.clone()];
    let result = client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );
    assert!(result);

    // Verify configuration
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

    // Try to configure as regular member (should fail)
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    // Mint tokens to owner
    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    // Configure multi-sig with spending limit of 1000
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    // Withdraw amount below threshold (should execute immediately)
    let recipient = Address::generate(&env);
    let withdraw_amount = 500_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    // Should return 0 for immediate execution
    assert_eq!(tx_id, 0);

    // Verify tokens were transferred
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    // Mint tokens to owner
    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    // Configure 2-of-3 multi-sig with spending limit of 1000
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    // Propose withdrawal above threshold
    let recipient = Address::generate(&env);
    let withdraw_amount = 2000_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    // Should return transaction ID (not 0)
    assert!(tx_id > 0);

    // Verify transaction is pending
    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    let pending_tx = pending_tx.unwrap();
    assert_eq!(pending_tx.tx_type, TransactionType::LargeWithdrawal);
    assert_eq!(pending_tx.signatures.len(), 1); // Owner auto-signed

    // Verify tokens not yet transferred
    assert_eq!(token_client.balance(&recipient), 0);
    assert_eq!(token_client.balance(&owner), amount);

    // Second signer signs (should execute)
    client.sign_transaction(&member1, &tx_id);

    // Verify tokens were transferred
    assert_eq!(token_client.balance(&recipient), withdraw_amount);
    assert_eq!(token_client.balance(&owner), amount - withdraw_amount);

    // Verify transaction is no longer pending
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    // Mint tokens to owner
    let amount = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &amount);

    // Configure 3-of-3 multi-sig
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3,
        &signers,
        &1000_0000000,
    );

    // Propose withdrawal
    let recipient = Address::generate(&env);
    let withdraw_amount = 2000_0000000;
    let tx_id = client.withdraw(
        &owner,
        &token_contract.address(),
        &recipient,
        &withdraw_amount,
    );

    // Owner already signed, need 2 more
    client.sign_transaction(&member1, &tx_id);

    // Verify still pending (only 2 signatures, need 3)
    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    assert_eq!(token_client.balance(&recipient), 0);

    // Third signature should execute
    client.sign_transaction(&member2, &tx_id);

    // Verify executed
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    // Mint tokens
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure multi-sig with threshold of 3 (so transaction stays pending after first signature)
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &3, // Need 3 signatures, so after first signature it's still pending
        &signers,
        &1000_0000000,
    );

    // Propose withdrawal
    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);

    // Try to sign twice (should fail with "Already signed")
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

    // Configure multi-sig for split changes
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::SplitConfigChange,
        &2,
        &signers,
        &0,
    );

    // Propose split config change
    let tx_id = client.propose_split_config_change(&owner, &40, &30, &20, &10);

    assert!(tx_id > 0);

    // Verify pending
    let pending_tx = client.get_pending_transaction(&tx_id);
    assert!(pending_tx.is_some());
    assert_eq!(
        pending_tx.unwrap().tx_type,
        TransactionType::SplitConfigChange
    );

    // Second signature should execute
    client.sign_transaction(&member1, &tx_id);

    // Verify executed
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

    // Configure multi-sig for role changes
    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::RoleChange, &2, &signers, &0);

    // Propose role change
    let tx_id = client.propose_role_change(&owner, &member2, &FamilyRole::Admin);

    assert!(tx_id > 0);

    // Second signature should execute
    client.sign_transaction(&member1, &tx_id);

    // Verify role changed
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    // Mint tokens
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure multi-sig for emergency transfers
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::EmergencyTransfer,
        &2,
        &signers,
        &0,
    );

    // Propose emergency transfer
    let recipient = Address::generate(&env);
    let transfer_amount = 3000_0000000;
    let tx_id = client.propose_emergency_transfer(
        &owner,
        &token_contract.address(),
        &recipient,
        &transfer_amount,
    );

    assert!(tx_id > 0);

    // Second signature should execute
    client.sign_transaction(&member1, &tx_id);

    // Verify transfer executed
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = TokenClient::new(&env, &token_contract.address());

    // Mint tokens
    let total = 5000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    // Configure emergency settings
    client.configure_emergency(&owner, &2000_0000000, &3600u64, &1000_0000000);

    // Enable emergency mode
    client.set_emergency_mode(&owner, &true);
    assert!(client.is_emergency_mode());

    // One-click emergency transfer within limits
    let recipient = Address::generate(&env);
    let amount = 1500_0000000;
    let tx_id =
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);

    // Should execute immediately (no pending transaction id)
    assert_eq!(tx_id, 0);
    assert_eq!(token_client.balance(&recipient), amount);
    assert_eq!(token_client.balance(&owner), total - amount);

    // Last emergency timestamp should be set
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    // Mint tokens
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure emergency settings with small max_amount
    client.configure_emergency(&owner, &1000_0000000, &3600u64, &0);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    // This should exceed max_amount and panic
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    // Mint tokens
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure emergency settings with non-zero cooldown
    client.configure_emergency(&owner, &2000_0000000, &3600u64, &0);
    client.set_emergency_mode(&owner, &true);

    let recipient = Address::generate(&env);
    let amount = 1000_0000000;

    // First emergency transfer should succeed
    let tx_id =
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &amount);
    assert_eq!(tx_id, 0);

    // Second immediate emergency transfer should fail due to cooldown
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());

    // Mint tokens
    let total = 3000_0000000;
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &total);

    // Require at least 2500 remaining, attempt to send 1000 (would leave 2000)
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

    // Add new member as Admin
    let new_member = Address::generate(&env);
    let result = client.add_family_member(&owner, &new_member, &FamilyRole::Admin);
    assert!(result);

    // Verify member added
    let member_data = client.get_family_member(&new_member);
    assert!(member_data.is_some());
    assert_eq!(member_data.unwrap().role, FamilyRole::Admin);

    // Remove member
    let result = client.remove_family_member(&owner, &new_member);
    assert!(result);

    // Verify member removed
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

    // Try to add member as regular member (should fail)
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

    // Configure different thresholds for different transaction types
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2, // 2-of-5
        &all_signers,
        &1000_0000000,
    );

    client.configure_multisig(
        &owner,
        &TransactionType::RoleChange,
        &3, // 3-of-5 (more secure)
        &all_signers,
        &0,
    );

    client.configure_multisig(
        &owner,
        &TransactionType::EmergencyTransfer,
        &4, // 4-of-5 (most secure)
        &all_signers,
        &0,
    );

    // Verify configurations
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

    // Setup token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure multi-sig with only owner and member1 as signers
    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(
        &owner,
        &TransactionType::LargeWithdrawal,
        &2,
        &signers,
        &1000_0000000,
    );

    // Propose withdrawal
    let recipient = Address::generate(&env);
    let tx_id = client.withdraw(&owner, &token_contract.address(), &recipient, &2000_0000000);

    // Try to sign with member2 (not authorized) - should fail
    client.sign_transaction(&member2, &tx_id);
}
