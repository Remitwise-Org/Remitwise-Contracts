#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events},
    token::StellarAssetClient,
    Address, Env, IntoVal, TryFromVal,
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

    // Verify topic schema count
    assert_eq!(topics.len(), 4, "Expected 4 topics in event");

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
            topics.len() == 4
        })
        .expect("DistributionCompleted event not found");

    let topics = &dist_comp_event.1;
    assert_eq!(topics.len(), 4, "Event should have 4 topics");
}

// ──────────────────────────────────────────────────────────────────────────
// Request Hash Tests - Test Vectors for distribute_usdc Signing
// ──────────────────────────────────────────────────────────────────────────

/// Test that get_request_hash produces a deterministic 32-byte SHA-256 hash
#[test]
fn test_request_hash_deterministic() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    let usdc_contract = Address::generate(&env);
    let from = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: from.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 2000u64,
    };
    
    // Hash the same request twice
    let hash1 = client.get_request_hash(&request);
    let hash2 = client.get_request_hash(&request);
    
    // Both hashes should be identical (deterministic)
    assert_eq!(hash1, hash2);
    // SHA-256 produces 32 bytes
    assert_eq!(hash1.len(), 32);
}

/// Test that changing any parameter changes the hash (no collisions)
#[test]
fn test_request_hash_changes_with_parameters() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    let usdc_contract = Address::generate(&env);
    let from = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    let other = Address::generate(&env);
    
    let base_request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: from.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 2000u64,
    };
    
    let base_hash = client.get_request_hash(&base_request);
    
    // Test 1: Changing usdc_contract changes hash
    let mut req = base_request.clone();
    req.usdc_contract = other.clone();
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when usdc_contract changes");
    
    // Test 2: Changing from address changes hash
    let mut req = base_request.clone();
    req.from = other.clone();
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when from changes");
    
    // Test 3: Changing nonce changes hash
    let mut req = base_request.clone();
    req.nonce = 1;
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when nonce changes");
    
    // Test 4: Changing total_amount changes hash
    let mut req = base_request.clone();
    req.total_amount = 2000;
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when total_amount changes");
    
    // Test 5: Changing deadline changes hash
    let mut req = base_request.clone();
    req.deadline = 3000;
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when deadline changes");
    
    // Test 6: Changing spending account changes hash
    let mut req = base_request.clone();
    req.accounts.spending = other.clone();
    let hash = client.get_request_hash(&req);
    assert!(hash.ne(&base_hash), "Hash should change when spending account changes");
}

/// Test deadline validation: deadline must not be in the past
#[test]
fn test_distribute_usdc_deadline_expired() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    env.mock_all_auths();
    set_ledger_time(&env, 1000);
    
    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    // Initialize contract
    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    
    // Create request with deadline in the past (500 < 1000)
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: owner.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 500u64,  // Past deadline
    };
    
    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_with_hash_and_deadline(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

/// Test deadline validation: deadline must not be too far in the future (MAX_DEADLINE_WINDOW_SECS = 3600)
#[test]
fn test_distribute_usdc_deadline_too_far() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    env.mock_all_auths();
    set_ledger_time(&env, 1000);
    
    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    // Initialize contract
    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    
    // Create request with deadline > MAX_DEADLINE_WINDOW_SECS from now
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: owner.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 1000 + 3600 + 1,  // 1 second more than allowed window
    };
    
    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_with_hash_and_deadline(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// Test deadline validation: deadline must not be zero
#[test]
fn test_distribute_usdc_deadline_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    env.mock_all_auths();
    set_ledger_time(&env, 1000);
    
    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    // Initialize contract
    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    
    // Create request with deadline = 0
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: owner.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 0,  // Invalid deadline
    };
    
    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_with_hash_and_deadline(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// Test request hash mismatch: passing wrong hash should fail
#[test]
fn test_distribute_usdc_hash_mismatch() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    env.mock_all_auths();
    set_ledger_time(&env, 1000);
    
    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    // Initialize contract
    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    
    // Create valid request
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: owner.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 2000u64,
    };
    
    // Get correct hash and then create a wrong one
    let correct_hash = client.get_request_hash(&request);
    let mut wrong_hash = correct_hash.clone();
    // Flip a byte to create a different hash
    if wrong_hash.get(0).unwrap() != &0xFFu8 {
        wrong_hash.set(0, &(wrong_hash.get(0).unwrap() + 1));
    } else {
        wrong_hash.set(0, &(wrong_hash.get(0).unwrap() - 1));
    }
    
    let result = client.try_distribute_usdc_with_hash_and_deadline(&request, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
}

/// Test boundary: deadline exactly at MAX_DEADLINE_WINDOW_SECS should succeed
#[test]
fn test_distribute_usdc_deadline_at_boundary() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    env.mock_all_auths();
    set_ledger_time(&env, 1000);
    
    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    // Initialize contract
    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    
    // Create request with deadline exactly at MAX_DEADLINE_WINDOW_SECS boundary
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: owner.clone(),
        nonce: 0,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 1000i128,
        deadline: 1000 + 3600,  // Exactly at 1 hour boundary
    };
    
    let hash = client.get_request_hash(&request);
    
    // This should pass deadline validation
    // (It will fail for other reasons like missing USDC balance, but not deadline)
    let result = client.try_distribute_usdc_with_hash_and_deadline(&request, &hash);
    
    // Should fail due to other reasons (e.g., balance), not deadline validation
    // We can't assert equality here since we didn't register USDC token,
    // but we can check it's not a DeadlineExpired or InvalidDeadline error
    match result {
        Err(Ok(RemittanceSplitError::DeadlineExpired)) => {
            panic!("Should not fail with DeadlineExpired");
        }
        Err(Ok(RemittanceSplitError::InvalidDeadline)) => {
            panic!("Should not fail with InvalidDeadline");
        }
        _ => {} // Any other result is acceptable for this boundary test
    }
}

/// Test that the same request always produces the same hash (cross-call consistency)
#[test]
fn test_request_hash_cross_call_consistency() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    
    let usdc_contract = Address::generate(&env);
    let from = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);
    
    let request = DistributeUsdcRequest {
        usdc_contract: usdc_contract.clone(),
        from: from.clone(),
        nonce: 42,
        accounts: AccountGroup {
            spending: spending.clone(),
            savings: savings.clone(),
            bills: bills.clone(),
            insurance: insurance.clone(),
        },
        total_amount: 12345i128,
        deadline: 9999u64,
    };
    
    // Call get_request_hash multiple times
    let hashes: Vec<_> = (0..5)
        .map(|_| client.get_request_hash(&request))
        .collect();
    
    // All hashes should be identical
    for hash in &hashes[1..] {
        assert_eq!(hash, &hashes[0], "Hash should be consistent across calls");
    }
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set(LedgerInfo {
        timestamp,
        protocol_version: 20,
        sequence_number: 0,
        network_id: Default::default(),
        base_reserve: 0,
        max_tx_size: 0,
        min_temp_entry_ttl: 0,
        min_persistent_entry_ttl: 0,
        max_entry_ttl: 0,
    });
}

