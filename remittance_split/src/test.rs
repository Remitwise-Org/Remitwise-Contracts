#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events},
    token::StellarAssetClient,
    Address, Env, IntoVal, Symbol, TryFromVal,
};

#[test]
fn test_distribution_completed_event() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(&env, &token_addr);

    // 1. Initialize split
    // percentages: 40, 30, 20, 10
    client.initialize_split(&owner, &0, &token_addr, &40, &30, &20, &10);

    // 2. Setup destination accounts
    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };

    // 3. Mint tokens to owner
    let total_amount = 1000i128;
    stellar_client.mint(&owner, &total_amount);

    // 4. Distribute
    let nonce = 1u64; // nonce 0 used in initialize_split
    let deadline = env.ledger().timestamp() + 3600;
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

    // 5. Verify events
    let events = env.events().all();

    // We expect several events:
    // - init (from initialize_split)
    // - dist_ok (unstructured)
    // - dist_comp (structured) - THIS IS THE ONE WE CARE ABOUT

    let last_event = events.last().expect("No events emitted");
    let (_contract_id, topics, data) = last_event;

    // Verify topic schema
    assert_eq!(
        topics.get(0).unwrap(),
        symbol_short!("Remitwise").into_val(&env)
    );
    assert_eq!(topics.get(1).unwrap(), (0u32).into_val(&env)); // Category: Transaction
    assert_eq!(topics.get(2).unwrap(), (1u32).into_val(&env)); // Priority: Medium
    assert_eq!(
        topics.get(3).unwrap(),
        symbol_short!("dist_comp").into_val(&env)
    );

    // Verify structured payload
    let event: DistributionCompletedEvent = DistributionCompletedEvent::try_from_val(&env, &data)
        .expect("Failed to parse DistributionCompletedEvent data");

    assert_eq!(event.from, owner);
    assert_eq!(event.total_amount, total_amount);
    assert_eq!(event.spending_amount, 400); // 40% of 1000
    assert_eq!(event.savings_amount, 300); // 30% of 1000
    assert_eq!(event.bills_amount, 200); // 20% of 1000
    assert_eq!(event.insurance_amount, 100); // 10% of 1000 handled by remainder
    assert_eq!(event.timestamp, env.ledger().timestamp());
}

#[test]
fn test_distribution_event_topic_correctness() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(&env, &token_addr);

    client.initialize_split(&owner, &0, &token_addr, &50, &50, &0, &0);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };

    stellar_client.mint(&owner, &100);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3600;
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
        .find(|e| {
            let topics = &e.1;
            topics.len() == 4 && topics.get(3).unwrap() == symbol_short!("dist_comp").into_val(&env)
        })
        .expect("DistributionCompleted event not found");

    let topics = &dist_comp_event.1;
    assert_eq!(
        topics.get(0).unwrap(),
        symbol_short!("Remitwise").into_val(&env)
    );
    assert_eq!(topics.get(1).unwrap(), (0u32).into_val(&env)); // Transaction
    assert_eq!(topics.get(2).unwrap(), (1u32).into_val(&env)); // Medium
}

// ---------------------------------------------------------------------------
// Security tests: UntrustedTokenContract rejection and nonce state
// ---------------------------------------------------------------------------

/// Proves that `distribute_usdc` rejects when the supplied `usdc_contract`
/// does not match the trusted address pinned at initialization time.
/// Also proves that the failure does NOT advance the nonce or mark it used.
#[test]
fn test_distribute_usdc_rejects_untrusted_token() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = Address::generate(&env);

    // Token A — trusted at initialization
    let token_admin_a = Address::generate(&env);
    let token_a = env.register_stellar_asset_contract_v2(token_admin_a);
    let token_addr_a = token_a.address();
    let stellar_client_a = StellarAssetClient::new(&env, &token_addr_a);

    // Token B — untrusted attacker contract
    let token_admin_b = Address::generate(&env);
    let token_b = env.register_stellar_asset_contract_v2(token_admin_b);
    let token_addr_b = token_b.address();

    // Initialize with token A
    client.initialize_split(&owner, &0, &token_addr_a, &40, &30, &20, &10);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };

    stellar_client_a.mint(&owner, &1000i128);

    let nonce = 1u64; // nonce 0 consumed by initialize_split
    let deadline = env.ledger().timestamp() + 3600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        1000i128,
        deadline,
    );

    let nonce_before = client.get_nonce(&owner);

    // Attempt distribution with token B → must fail
    let result = client.try_distribute_usdc(
        &token_addr_b,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &1000i128,
    );

    assert!(result.is_ok(), "unexpected env error: {:?}", result);
    assert_eq!(
        result.unwrap(),
        Err(RemittanceSplitError::UntrustedTokenContract)
    );

    // Nonce must remain unchanged on failure
    let nonce_after = client.get_nonce(&owner);
    assert_eq!(nonce_after, nonce_before);

    // Nonce must NOT be in the used-nonce set
    assert!(
        !RemittanceSplit::is_nonce_used(&env, &owner, nonce),
        "nonce must not be marked used on UntrustedTokenContract failure"
    );
}

/// Proves that a successful `distribute_usdc` both advances the sequential
/// nonce counter and marks the consumed nonce in the used-nonce set.
#[test]
fn test_distribute_usdc_nonce_advances_and_marks_used_on_success() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = StellarAssetClient::new(&env, &token_addr);

    client.initialize_split(&owner, &0, &token_addr, &40, &30, &20, &10);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };

    let total_amount = 1000i128;
    stellar_client.mint(&owner, &total_amount);

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        total_amount,
        deadline,
    );

    let nonce_before = client.get_nonce(&owner);
    assert!(
        !RemittanceSplit::is_nonce_used(&env, &owner, nonce),
        "nonce must not be used before distribution"
    );

    // Successful distribution
    let result = client.distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &total_amount,
    );
    assert!(result);

    // Nonce must be advanced by exactly 1
    let nonce_after = client.get_nonce(&owner);
    assert_eq!(
        nonce_after,
        nonce_before + 1,
        "nonce must advance by 1 on success"
    );

    // Consumed nonce must be in the used-nonce set
    assert!(
        RemittanceSplit::is_nonce_used(&env, &owner, nonce),
        "nonce must be marked used on success"
    );
}

/// Control test: proves that a non-token failure path (InvalidAmount) also
/// does not advance the nonce or mark it used.
#[test]
fn test_distribute_usdc_nonce_unchanged_on_failure() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();

    client.initialize_split(&owner, &0, &token_addr, &40, &30, &20, &10);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };

    let nonce = 1u64;
    let deadline = env.ledger().timestamp() + 3600;
    let request_hash = RemittanceSplit::compute_request_hash(
        symbol_short!("distrib"),
        owner.clone(),
        nonce,
        0i128,
        deadline,
    );

    let nonce_before = client.get_nonce(&owner);

    // Invalid amount (0) → must fail with InvalidAmount before nonce is touched
    let result = client.try_distribute_usdc(
        &token_addr,
        &owner,
        &nonce,
        &deadline,
        &request_hash,
        &accounts,
        &0i128,
    );

    assert!(result.is_ok(), "unexpected env error: {:?}", result);
    assert_eq!(result.unwrap(), Err(RemittanceSplitError::InvalidAmount));

    // Nonce must remain unchanged
    let nonce_after = client.get_nonce(&owner);
    assert_eq!(nonce_after, nonce_before);

    // Nonce must NOT be in the used-nonce set
    assert!(
        !RemittanceSplit::is_nonce_used(&env, &owner, nonce),
        "nonce must not be marked used on InvalidAmount failure"
    );
}
