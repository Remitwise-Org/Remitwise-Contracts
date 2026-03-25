use family_wallet::{FamilyWallet, FamilyWalletClient, TransactionType, TransactionData, BatchMemberItem};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, Vec, vec, String, token::{TokenClient, StellarAssetClient}};
use remitwise_common::FamilyRole;

fn bench_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

fn measure<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    let result = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, result)
}

fn setup_wallet(env: &Env) -> (FamilyWalletClient, Address, Vec<Address>) {
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(env, &contract_id);
    let owner = Address::generate(env);
    let member1 = Address::generate(env);
    let member2 = Address::generate(env);
    let initial_members = vec![env, member1.clone(), member2.clone()];
    client.init(&owner, &initial_members);
    (client, owner, initial_members)
}

#[test]
fn bench_init() {
    let env = bench_env();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let mut initial_members = Vec::new(&env);
    for _ in 0..5 {
        initial_members.push_back(Address::generate(&env));
    }

    let (cpu, mem, _) = measure(&env, || client.init(&owner, &initial_members));
    
    println!(
        r#"{{"contract":"family_wallet","method":"init","scenario":"5_initial_members","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_propose_transaction() {
    let env = bench_env();
    let (client, owner, _) = setup_wallet(&env);
    
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    let data = TransactionData::Withdrawal(token, recipient, 1000_0000000);

    let (cpu, mem, tx_id) = measure(&env, || {
        client.propose_transaction(&owner, &TransactionType::LargeWithdrawal, &data)
    });
    assert!(tx_id > 0);

    println!(
        r#"{{"contract":"family_wallet","method":"propose_transaction","scenario":"large_withdrawal","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_sign_transaction_non_executing() {
    let env = bench_env();
    let (client, owner, initial_members) = setup_wallet(&env);
    let member1 = initial_members.get(0).unwrap();
    let member2 = initial_members.get(1).unwrap();

    // Configure multisig with threshold 3
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &3, &signers, &0);
    
    let token = Address::generate(&env);
    let recipient = Address::generate(&env);
    let data = TransactionData::Withdrawal(token, recipient, 1000_0000000);
    let tx_id = client.propose_transaction(&owner, &TransactionType::LargeWithdrawal, &data);

    let (cpu, mem, _) = measure(&env, || client.sign_transaction(&member1, &tx_id));

    println!(
        r#"{{"contract":"family_wallet","method":"sign_transaction","scenario":"non_executing_1_of_3","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_sign_transaction_executing() {
    let env = bench_env();
    let (client, owner, initial_members) = setup_wallet(&env);
    let member1 = initial_members.get(0).unwrap();
    let member2 = initial_members.get(1).unwrap();

    // Setup token for withdrawal
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    // Configure multisig with threshold 2
    let signers = vec![&env, owner.clone(), member1.clone(), member2.clone()];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &2, &signers, &0);
    
    let recipient = Address::generate(&env);
    let data = TransactionData::Withdrawal(token_contract.address(), recipient, 1000_0000000);
    let tx_id = client.propose_transaction(&owner, &TransactionType::LargeWithdrawal, &data);

    // Sign with threshold (member1 triggers execution)
    let (cpu, mem, _) = measure(&env, || client.sign_transaction(&member1, &tx_id));

    println!(
        r#"{{"contract":"family_wallet","method":"sign_transaction","scenario":"executing_2_of_2","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_emergency_transfer_direct() {
    let env = bench_env();
    let (client, owner, _) = setup_wallet(&env);
    
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &5000_0000000);

    client.configure_emergency(&owner, &5000_0000000, &0, &0);
    client.set_emergency_mode(&owner, &true);
    
    let recipient = Address::generate(&env);
    let (cpu, mem, _) = measure(&env, || {
        client.propose_emergency_transfer(&owner, &token_contract.address(), &recipient, &1000_0000000)
    });

    println!(
        r#"{{"contract":"family_wallet","method":"propose_emergency_transfer","scenario":"direct_exec_mode_on","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_archive_transactions() {
    let env = bench_env();
    let (client, owner, initial_members) = setup_wallet(&env);
    let member1 = initial_members.get(0).unwrap();

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    StellarAssetClient::new(&env, &token_contract.address()).mint(&owner, &10000_0000000);

    // Configure multisig
    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &1, &signers, &0);

    // Execute 10 transactions to fill EXEC_TXS
    for _ in 0..10 {
        let recipient = Address::generate(&env);
        let data = TransactionData::Withdrawal(token_contract.address(), recipient, 100_0000000);
        let tx_id = client.propose_transaction(&owner, &TransactionType::LargeWithdrawal, &data);
        client.sign_transaction(&member1, &tx_id);
    }

    let (cpu, mem, archived) = measure(&env, || client.archive_old_transactions(&owner, &1_800_000_000));
    assert_eq!(archived, 10);

    println!(
        r#"{{"contract":"family_wallet","method":"archive_old_transactions","scenario":"10_executed_txs","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_cleanup_expired_pending() {
    let env = bench_env();
    let (client, owner, initial_members) = setup_wallet(&env);
    let member1 = initial_members.get(0).unwrap();

    // Configure multisig with threshold 2
    let signers = vec![&env, owner.clone(), member1.clone()];
    client.configure_multisig(&owner, &TransactionType::LargeWithdrawal, &2, &signers, &0);

    // Propose 10 transactions
    for _ in 0..10 {
        let token = Address::generate(&env);
        let recipient = Address::generate(&env);
        let data = TransactionData::Withdrawal(token, recipient, 100_0000000);
        client.propose_transaction(&owner, &TransactionType::LargeWithdrawal, &data);
    }

    // Warp time so they expire (SIGNATURE_EXPIRATION = 86400)
    let mut ledger = env.ledger().get();
    ledger.timestamp += 100000;
    env.ledger().set(ledger);

    let (cpu, mem, removed) = measure(&env, || client.cleanup_expired_pending(&owner));
    assert_eq!(removed, 10);

    println!(
        r#"{{"contract":"family_wallet","method":"cleanup_expired_pending","scenario":"10_expired_txs","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

#[test]
fn bench_batch_add_family_members() {
    let env = bench_env();
    let (client, owner, _) = setup_wallet(&env);
    
    let mut members = Vec::new(&env);
    for _ in 0..20 {
        members.push_back(BatchMemberItem {
            address: Address::generate(&env),
            role: FamilyRole::Member,
        });
    }

    let (cpu, mem, count) = measure(&env, || client.batch_add_family_members(&owner, &members));
    assert_eq!(count, 20);

    println!(
        r#"{{"contract":"family_wallet","method":"batch_add_family_members","scenario":"20_members","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
