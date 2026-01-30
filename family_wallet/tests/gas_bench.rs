use family_wallet::{FamilyWallet, FamilyWalletClient, TransactionType};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, Vec};

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

#[test]
fn bench_configure_multisig_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let mut initial_members = Vec::new(&env);
    let mut signers = Vec::new(&env);

    for _ in 0..8 {
        let member = <Address as AddressTrait>::generate(&env);
        initial_members.push_back(member.clone());
        signers.push_back(member);
    }

    client.init(&owner, &initial_members);

    // Include owner as an authorized signer too.
    signers.push_back(owner.clone());
    let threshold = signers.len();

    let (cpu, mem, configured) = measure(&env, || {
        client.configure_multisig(
            &owner,
            &TransactionType::LargeWithdrawal,
            &threshold,
            &signers,
            &5_000i128,
        )
    });
    assert!(configured);

    println!(
        r#"{{"contract":"family_wallet","method":"configure_multisig","scenario":"9_signers_threshold_all","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
