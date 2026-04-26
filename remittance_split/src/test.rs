#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, TryFromVal,
};

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set_timestamp(timestamp);
}

fn setup_split(
    env: &Env,
    spending: u32,
    savings: u32,
    bills: u32,
    insurance: u32,
) -> (
    RemittanceSplitClient<'_>,
    Address,
    Address,
    StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    set_time(env, 1_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(env, &token_addr);

    client.initialize_split(
        &owner,
        &0,
        &token_addr,
        &spending,
        &savings,
        &bills,
        &insurance,
    );

    (client, owner, token_addr, stellar_client)
}

fn sample_accounts(env: &Env) -> AccountGroup {
    AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    }
}

#[test]
fn test_distribution_completed_event() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 40, 30, 20, 10);
    let accounts = sample_accounts(&env);

    let total_amount = 1_000i128;
    stellar_client.mint(&owner, &total_amount);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        total_amount,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &total_amount,
    );

    let events = env.events().all();
    let last_event = events.last().expect("no events emitted");
    let (_, topics, data) = last_event;

    assert_eq!(topics.len(), 4);

    let event: DistributionCompletedEvent = DistributionCompletedEvent::try_from_val(&env, &data)
        .expect("failed to decode distribution event");

    assert_eq!(event.from, owner);
    assert_eq!(event.total_amount, total_amount);
    assert_eq!(event.spending_amount, 400);
    assert_eq!(event.savings_amount, 300);
    assert_eq!(event.bills_amount, 200);
    assert_eq!(event.insurance_amount, 100);
    assert_eq!(event.timestamp, env.ledger().timestamp());
}

#[test]
fn test_distribution_event_topic_correctness() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 50, 0, 0);
    let accounts = sample_accounts(&env);

    stellar_client.mint(&owner, &100);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3_600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        100,
        deadline,
    );

    client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &100,
    );

    let events = env.events().all();
    let dist_comp_event = events
        .iter()
        .find(|event| event.1.len() == 4)
        .expect("distribution completed event not found");

    assert_eq!(dist_comp_event.1.len(), 4);
}

#[test]
fn test_request_hash_deterministic() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let hash1 = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        7,
        1_000,
        2_000,
    );
    let hash2 =
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 7, 1_000, 2_000);

    assert_eq!(hash1, hash2);
}

#[test]
fn test_request_hash_changes_with_parameters() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let base = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        0,
        1_000,
        2_000,
    );

    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            1,
            1_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(
            symbol_short!("distH"),
            owner.clone(),
            0,
            2_000,
            2_000
        )
    );
    assert_ne!(
        base,
        RemittanceSplit::compute_request_hash(symbol_short!("distH"), owner, 0, 1_000, 3_000)
    );
}

#[test]
fn test_distribute_usdc_signed_success() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_split(&env, 50, 30, 15, 5);
    let accounts = sample_accounts(&env);
    let token = TokenClient::new(&env, &token_addr);

    stellar_client.mint(&owner, &1_000);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: accounts.clone(),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner.clone(),
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.distribute_usdc_signed(&request, &hash);
    assert!(result);
    assert_eq!(token.balance(&accounts.spending), 500);
    assert_eq!(token.balance(&accounts.savings), 300);
    assert_eq!(token.balance(&accounts.bills), 150);
    assert_eq!(token.balance(&accounts.insurance), 50);
    assert_eq!(client.get_nonce(&owner), 2);
}

#[test]
fn test_distribute_usdc_signed_deadline_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() - 1,
    };

    let hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

#[test]
fn test_distribute_usdc_signed_hash_mismatch() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    let request = DistributeUsdcRequest {
        usdc_contract: token_addr,
        from: owner.clone(),
        nonce: 1,
        accounts: sample_accounts(&env),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 100,
    };

    let wrong_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distH"),
        owner,
        request.nonce,
        request.total_amount + 1,
        request.deadline,
    );

    let result = client.try_distribute_usdc_signed(&request, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
}
