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

mod dust_policy {
    use crate::RemittanceSplitError;
    use soroban_sdk::Env;

    /// Table-driven conservation invariant: for every (amount, percentages) pair,
    /// spending + savings + bills + insurance == amount, and insurance holds the remainder.
    #[test]
    fn test_conservation_invariant() {
        // (amount, spending%, savings%, bills%, insurance%)
        let cases: [(i128, u32, u32, u32, u32); 11] = [
            (1, 25, 25, 25, 25),
            (1, 50, 50, 0, 0),
            (3, 33, 33, 33, 1),
            (7, 40, 30, 20, 10),
            (11, 34, 33, 33, 0),
            (97, 33, 33, 33, 1),
            (100, 50, 50, 0, 0),
            (999, 33, 33, 33, 1),
            (i128::MAX / 1_000_000, 25, 25, 25, 25),
            (i128::MAX / 1_000_000, 33, 33, 33, 1),
            (i128::MAX / 1_000_000, 40, 30, 20, 10),
        ];

        for &(amount, sp, sv, bl, ins) in cases.iter() {
            let env = Env::default();
            let (client, _, _, _) = super::setup_split(&env, sp, sv, bl, ins);

            // (a) calculate_split returns Ok
            let result = client.calculate_split(&amount);

            let spending_alloc = result.get(0).unwrap();
            let savings_alloc = result.get(1).unwrap();
            let bills_alloc = result.get(2).unwrap();
            let insurance_alloc = result.get(3).unwrap();

            // (b) conservation: sum of all allocations equals total amount
            assert_eq!(spending_alloc + savings_alloc + bills_alloc + insurance_alloc, amount);

            // (c) remainder lands in insurance
            let expected_insurance = amount - (spending_alloc + savings_alloc + bills_alloc);
            assert_eq!(insurance_alloc, expected_insurance);
        }
    }

    /// Isolation test: with amount=10 and 33/33/33/1, all three floor categories get 3
    /// and the 1-unit remainder goes entirely to insurance.
    #[test]
    fn test_remainder_to_insurance_isolation() {
        let env = Env::default();
        let amount = 10i128;
        let (client, _, _, _) = super::setup_split(&env, 33, 33, 33, 1);

        let result = client.calculate_split(&amount);
        let spending_alloc = result.get(0).unwrap();
        let savings_alloc = result.get(1).unwrap();
        let bills_alloc = result.get(2).unwrap();
        let insurance_alloc = result.get(3).unwrap();

        // floor(10 * 33 / 100) = 3 for each of spending, savings, bills
        assert_eq!(spending_alloc, 3);
        assert_eq!(savings_alloc, 3);
        assert_eq!(bills_alloc, 3);

        // floor share of insurance = floor(10 * 1 / 100) = 0; remainder = 1
        let floor_insurance: i128 = amount * 1 / 100;
        let remainder = amount - (spending_alloc + savings_alloc + bills_alloc);
        assert_eq!(insurance_alloc, floor_insurance + remainder);
        assert_eq!(insurance_alloc, 1);
    }

    /// With spending_percent = 50, total_amount * 50 overflows i128 when total_amount = i128::MAX.
    #[test]
    fn test_overflow_guard() {
        let env = Env::default();
        let (client, _, _, _) = super::setup_split(&env, 50, 50, 0, 0);

        let result = client.try_calculate_split(&i128::MAX);
        assert_eq!(result, Err(Ok(RemittanceSplitError::Overflow)));
    }

    /// amount = 0 must be rejected as InvalidAmount before any allocation is attempted.
    #[test]
    fn test_zero_amount_rejected() {
        let env = Env::default();
        let (client, _, _, _) = super::setup_split(&env, 25, 25, 25, 25);

        let result = client.try_calculate_split(&0i128);
        assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
    }
}
