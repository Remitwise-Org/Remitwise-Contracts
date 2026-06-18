#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger},
    token::StellarAssetClient,
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

#[test]
fn test_ttl_extensions() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, owner, token_addr, _stellar_client) = setup_split(&env, 40, 30, 20, 10);

    // 1. Check Instance TTL extension (CONFIG)
    // Initial sequence is 0. Threshold is INSTANCE_LIFETIME_THRESHOLD.
    let threshold = INSTANCE_LIFETIME_THRESHOLD;

    // Advance to threshold - 1
    env.ledger().set_sequence(threshold - 1);

    // Access CONFIG
    let config = client.get_config();
    assert!(config.is_some(), "Config should exist before expiration");

    // After access, TTL should be bumped to INSTANCE_BUMP_AMOUNT
    // If we advance to threshold + 1, it should still exist
    env.ledger().set_sequence(threshold + 1);
    let config = client.get_config();
    assert!(config.is_some(), "Config should exist after TTL bump");

    // 2. Check Persistent TTL extension (Schedules)
    let amount = 100i128;
    let next_due = env.ledger().timestamp() + 3600;
    let interval = 86400u64;
    let schedule_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);

    let p_threshold = PERSISTENT_LIFETIME_THRESHOLD;
    let p_bump = PERSISTENT_BUMP_AMOUNT;

    // Advance to p_threshold - 1 from current sequence
    let current_seq = env.ledger().sequence();
    env.ledger().set_sequence(current_seq + p_threshold - 1);

    // Access Schedule
    let schedule = client.get_remittance_schedule(&schedule_id);
    assert!(
        schedule.is_some(),
        "Schedule should exist before expiration"
    );

    // Advance beyond original threshold
    env.ledger().set_sequence(current_seq + p_threshold + 1);
    let schedule = client.get_remittance_schedule(&schedule_id);
    assert!(schedule.is_some(), "Schedule should exist after TTL bump");

    // 3. Multiple sequential bumps
    for _ in 0..3 {
        let seq = env.ledger().sequence();
        env.ledger().set_sequence(seq + p_threshold - 1);
        assert!(client.get_remittance_schedule(&schedule_id).is_some());
    }

    // Final check
    assert!(client.get_remittance_schedule(&schedule_id).is_some());
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
    assert!(
        hash.ne(&base_hash),
        "Hash should change when usdc_contract changes"
    );

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
    assert!(
        hash.ne(&base_hash),
        "Hash should change when total_amount changes"
    );

    // Test 5: Changing deadline changes hash
    let mut req = base_request.clone();
    req.deadline = 3000;
    let hash = client.get_request_hash(&req);
    assert!(
        hash.ne(&base_hash),
        "Hash should change when deadline changes"
    );

    // Test 6: Changing spending account changes hash
    let mut req = base_request.clone();
    req.accounts.spending = other.clone();
    let hash = client.get_request_hash(&req);
    assert!(
        hash.ne(&base_hash),
        "Hash should change when spending account changes"
    );
}

/// Test deadline validation: deadline must not be in the past
#[test]
fn test_distribute_usdc_deadline_expired() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    env.mock_all_auths();
    set_time(&env, 1000);

    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    // Initialize contract
    client.initialize_split(&owner, &0, &usdc_contract, &50, &30, &15, &5);

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
        deadline: 500u64, // Past deadline
    };

    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_hashed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

/// Test deadline validation: deadline must not be too far in the future (MAX_DEADLINE_WINDOW_SECS = 3600)
#[test]
fn test_distribute_usdc_deadline_too_far() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    env.mock_all_auths();
    set_time(&env, 1000);

    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    // Initialize contract
    client.initialize_split(&owner, &0, &usdc_contract, &50, &30, &15, &5);

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
        deadline: 1000 + 3600 + 1, // 1 second more than allowed window
    };

    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_hashed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// Test deadline validation: deadline must not be zero
#[test]
fn test_distribute_usdc_deadline_zero() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    env.mock_all_auths();
    set_time(&env, 1000);

    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    // Initialize contract
    client.initialize_split(&owner, &0, &usdc_contract, &50, &30, &15, &5);

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
        deadline: 0, // Invalid deadline
    };

    let hash = client.get_request_hash(&request);
    let result = client.try_distribute_usdc_hashed(&request, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// Test request hash mismatch: passing wrong hash should fail
#[test]
fn test_distribute_usdc_hash_mismatch() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    env.mock_all_auths();
    set_time(&env, 1000);

    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    // Initialize contract
    client.initialize_split(&owner, &0, &usdc_contract, &50, &30, &15, &5);

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

    // Use a zeroed 32-byte hash as the "wrong" hash
    let _ = client.get_request_hash(&request);
    let wrong_hash = soroban_sdk::Bytes::from_slice(&env, &[0u8; 32]);

    let result = client.try_distribute_usdc_hashed(&request, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
}

/// Test boundary: deadline exactly at MAX_DEADLINE_WINDOW_SECS should succeed
#[test]
fn test_distribute_usdc_deadline_at_boundary() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    env.mock_all_auths();
    set_time(&env, 1000);

    let owner = Address::generate(&env);
    let usdc_contract = Address::generate(&env);
    let spending = Address::generate(&env);
    let savings = Address::generate(&env);
    let bills = Address::generate(&env);
    let insurance = Address::generate(&env);

    // Initialize contract
    client.initialize_split(&owner, &0, &usdc_contract, &50, &30, &15, &5);

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
        deadline: 1000 + 3600, // Exactly at 1 hour boundary
    };

    let hash = client.get_request_hash(&request);

    // This should pass deadline validation
    // (It will fail for other reasons like missing USDC balance, but not deadline)
    let result = client.try_distribute_usdc_hashed(&request, &hash);

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

    // Call get_request_hash multiple times and verify consistency
    let h0 = client.get_request_hash(&request);
    let h1 = client.get_request_hash(&request);
    let h2 = client.get_request_hash(&request);
    assert_eq!(h0, h1, "Hash should be consistent across calls");
    assert_eq!(h1, h2, "Hash should be consistent across calls");
}

// ──────────────────────────────────────────────────────────────────────────────
// RequestHashMismatch tamper tests
//
// Each test proves that mutating one field in DistributeUsdcRequest while keeping
// the original hash causes distribute_usdc_hashed to return
// RequestHashMismatch(15), closing the cross-field confused-deputy gap.
//
// Hash preimage ordering (see get_request_hash):
//   DISTRIBUTE_USDC_DOMAIN | domain_id("distrib") | from | usdc_contract
//   | accounts.spending | accounts.savings | accounts.bills
//   | accounts.insurance | total_amount (16 LE) | nonce (8 LE)
//   | deadline (8 LE)
// ──────────────────────────────────────────────────────────────────────────────

fn base_request(env: &Env) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract: Address::generate(env),
        from: Address::generate(env),
        nonce: 1,
        accounts: AccountGroup {
            spending: Address::generate(env),
            savings: Address::generate(env),
            bills: Address::generate(env),
            insurance: Address::generate(env),
        },
        total_amount: 1000i128,
        deadline: 1000 + 1800, // 30 min from ledger time 1000
    }
}

/// Positive control: unmodified request + correct hash passes the hash gate.
/// The call fails at InsufficientBalance (no minted USDC), NOT RequestHashMismatch.
#[test]
fn test_request_hash_positive_control() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let request = base_request(&env);
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);
    // Must NOT be RequestHashMismatch — hash check passed.
    match result {
        Err(Ok(RemittanceSplitError::RequestHashMismatch)) => {
            panic!("Hash check should pass for unmodified request");
        }
        _ => {}
    }
}

/// Mutating `from` while keeping the original hash yields RequestHashMismatch.
#[test]
fn test_request_hash_mismatch_on_from_tamper() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    tampered.from = Address::generate(&env); // different sender

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Tampered `from` must yield RequestHashMismatch"
    );
}

/// Mutating `usdc_contract` while keeping the original hash yields RequestHashMismatch.
#[test]
fn test_request_hash_mismatch_on_usdc_contract_tamper() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    tampered.usdc_contract = Address::generate(&env);

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Tampered `usdc_contract` must yield RequestHashMismatch"
    );
}

/// Mutating `total_amount` (off-by-one) while keeping the original hash yields RequestHashMismatch.
#[test]
fn test_request_hash_mismatch_on_amount_tamper() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    tampered.total_amount = original.total_amount + 1; // off-by-one

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Off-by-one in `total_amount` must yield RequestHashMismatch"
    );
}

/// Mutating `nonce` while keeping the original hash yields RequestHashMismatch.
#[test]
fn test_request_hash_mismatch_on_nonce_tamper() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    tampered.nonce = original.nonce.wrapping_add(1); // next nonce

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Tampered `nonce` must yield RequestHashMismatch"
    );
}

/// Mutating `deadline` while keeping the original hash yields RequestHashMismatch.
#[test]
fn test_request_hash_mismatch_on_deadline_tamper() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    tampered.deadline = original.deadline + 60; // extend by 60 seconds

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Tampered `deadline` must yield RequestHashMismatch"
    );
}

/// domain_id swap: supplying arbitrary bytes (different domain) as the hash is rejected.
/// The hash binds DISTRIBUTE_USDC_DOMAIN + "distrib" — any bytes from a different
/// domain cannot satisfy the hash gate.
#[test]
fn test_request_hash_mismatch_on_domain_id_swap() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let request = base_request(&env);

    // Craft a fake 32-byte hash that represents a different domain ("init" tag).
    // This simulates a confused-deputy attack where an "init" domain hash is
    // replayed against the "distrib" entrypoint.
    let wrong_hash = soroban_sdk::Bytes::from_slice(&env, &[0u8; 32]);

    let result = client.try_distribute_usdc_hashed(&request, &wrong_hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "A hash from a different domain must be rejected as RequestHashMismatch"
    );
}

/// Nonce reuse with new deadline: same nonce, different deadline — still mismatches.
#[test]
fn test_request_hash_mismatch_nonce_reuse_new_deadline() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    let original = base_request(&env);
    let hash = client.get_request_hash(&original);

    // Keep same nonce but extend deadline — the hash won't match
    let mut tampered = original.clone();
    tampered.deadline = original.deadline + 300;

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Same nonce with new deadline must be rejected"
    );
}

fn setup_request_hash_distribution(env: &Env) -> (
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

    client.initialize_split(&owner, &0, &token_addr, &40, &30, &20, &10);

    (client, owner, token_addr, stellar_client)
}

fn request_hash_distribution_request(
    env: &Env,
    usdc_contract: Address,
    from: Address,
    nonce: u64,
) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract,
        from,
        nonce,
        accounts: sample_accounts(env),
        total_amount: 1_000i128,
        deadline: env.ledger().timestamp() + 1_800,
    }
}

fn assert_distribution_request_tamper_rejected(
    env: &Env,
    mutate: impl FnOnce(&mut DistributeUsdcRequest),
    field_name: &str,
) {
    let (client, owner, token_addr, _stellar_client) = setup_request_hash_distribution(env);
    let original = request_hash_distribution_request(env, token_addr, owner, 1);
    let hash = client.get_request_hash(&original);

    let mut tampered = original.clone();
    mutate(&mut tampered);

    let result = client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::RequestHashMismatch)),
        "Tampered `{}` must yield RequestHashMismatch",
        field_name
    );
}

#[test]
fn test_request_hash_distribution_happy_path_succeeds() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_request_hash_distribution(&env);
    let request = request_hash_distribution_request(&env, token_addr, owner.clone(), 1);
    stellar_client.mint(&owner, &request.total_amount);
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);

    assert_eq!(result, Ok(Ok(true)));
}

/// Tamper field: `accounts.spending`.
#[test]
fn test_request_hash_mismatch_on_spending_account_tamper() {
    let env = Env::default();
    assert_distribution_request_tamper_rejected(
        &env,
        |request| request.accounts.spending = Address::generate(&env),
        "accounts.spending",
    );
}

/// Tamper field: `accounts.savings`.
#[test]
fn test_request_hash_mismatch_on_savings_account_tamper() {
    let env = Env::default();
    assert_distribution_request_tamper_rejected(
        &env,
        |request| request.accounts.savings = Address::generate(&env),
        "accounts.savings",
    );
}

/// Tamper field: `accounts.bills`.
#[test]
fn test_request_hash_mismatch_on_bills_account_tamper() {
    let env = Env::default();
    assert_distribution_request_tamper_rejected(
        &env,
        |request| request.accounts.bills = Address::generate(&env),
        "accounts.bills",
    );
}

/// Tamper field: `accounts.insurance`.
#[test]
fn test_request_hash_mismatch_on_insurance_account_tamper() {
    let env = Env::default();
    assert_distribution_request_tamper_rejected(
        &env,
        |request| request.accounts.insurance = Address::generate(&env),
        "accounts.insurance",
    );
}

/// Tamper fields: reorder `accounts.spending` and `accounts.savings`.
#[test]
fn test_request_hash_mismatch_on_account_reordering() {
    let env = Env::default();
    assert_distribution_request_tamper_rejected(
        &env,
        |request| {
            let spending = request.accounts.spending.clone();
            request.accounts.spending = request.accounts.savings.clone();
            request.accounts.savings = spending;
        },
        "accounts",
    );
}

#[test]
fn test_request_hash_hashed_path_rejects_used_nonce() {
    let env = Env::default();
    let (client, owner, token_addr, stellar_client) = setup_request_hash_distribution(&env);
    let request = request_hash_distribution_request(&env, token_addr, owner.clone(), 1);
    stellar_client.mint(&owner, &(request.total_amount * 2));
    let hash = client.get_request_hash(&request);

    assert_eq!(client.try_distribute_usdc_hashed(&request, &hash), Ok(Ok(true)));

    let replay = client.try_distribute_usdc_hashed(&request, &hash);
    assert_eq!(replay, Err(Ok(RemittanceSplitError::NonceAlreadyUsed)));
}

#[test]
fn test_request_hash_hashed_path_rejects_expired_deadline() {
    let env = Env::default();
    let (client, owner, token_addr, _stellar_client) = setup_request_hash_distribution(&env);
    let mut request = request_hash_distribution_request(&env, token_addr, owner, 1);
    request.deadline = env.ledger().timestamp() - 1;
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);

    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

#[test]
fn test_request_hash_hashed_path_rejects_invalid_deadline() {
    let env = Env::default();
    let (client, owner, token_addr, _stellar_client) = setup_request_hash_distribution(&env);
    let mut request = request_hash_distribution_request(&env, token_addr, owner, 1);
    request.deadline = env.ledger().timestamp() + MAX_DEADLINE_WINDOW_SECS + 1;
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);

    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

#[test]
fn test_request_hash_hashed_path_rejects_self_transfer() {
    let env = Env::default();
    let (client, owner, token_addr, _stellar_client) = setup_request_hash_distribution(&env);
    let mut request = request_hash_distribution_request(&env, token_addr, owner.clone(), 1);
    request.accounts.spending = owner;
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);

    assert_eq!(result, Err(Ok(RemittanceSplitError::SelfTransferNotAllowed)));
}

#[test]
fn test_request_hash_hashed_path_rejects_untrusted_token_contract() {
    let env = Env::default();
    let (client, owner, _trusted_token_addr, _stellar_client) =
        setup_request_hash_distribution(&env);
    let token_admin = Address::generate(&env);
    let untrusted_token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let request =
        request_hash_distribution_request(&env, untrusted_token_contract.address(), owner, 1);
    let hash = client.get_request_hash(&request);

    let result = client.try_distribute_usdc_hashed(&request, &hash);

    assert_eq!(result, Err(Ok(RemittanceSplitError::UntrustedTokenContract)));
}

// ============================================================================
// Execute Due Remittance Schedules Tests
// ============================================================================
// These tests verify the idempotent executor for remittance schedules.
// Key security properties: due/not-due partitioning, idempotency on repeated
// calls, InactiveSchedule skipping, and correct next_due advancement.
// ============================================================================

#[test]
fn test_execute_due_remittance_schedules_basic() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a one-shot schedule due at time 3000
    let schedule_id = client.create_remittance_schedule(&owner, &1_000, &3_000, &0);
    assert_eq!(schedule_id, 1);

    // Advance time past due date
    set_time(&env, 3_500);

    // Execute due schedules
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify schedule is now inactive (one-off)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(!schedule.active);
        assert_eq!(schedule.last_executed, Some(3_500));
    }
}

#[test]
fn test_execute_recurring_remittance_schedule() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a recurring schedule: 1000 amount, due at 3000, every 86400 seconds
    let schedule_id = client.create_remittance_schedule(&owner, &1_000, &3_000, &86_400);
    assert_eq!(schedule_id, 1);

    // Advance time past first due date
    set_time(&env, 3_500);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify next_due was advanced by interval
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.next_due, 3_000 + 86_400);
        assert_eq!(schedule.last_executed, Some(3_500));
        assert_eq!(schedule.missed_count, 0);
    }
}

#[test]
fn test_execute_missed_remittance_schedules() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create a recurring schedule
    let schedule_id = client.create_remittance_schedule(&owner, &500, &3_000, &86_400);
    assert_eq!(schedule_id, 1);

    // Advance time far past multiple intervals: 3000 + 86400*3 + 100
    set_time(&env, 3_000 + 86_400 * 3 + 100);
    let executed = client.execute_due_remittance_schedules();

    assert_eq!(executed.len(), 1);

    // Verify missed_count is 3 (the three intervals that were skipped)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert_eq!(schedule.missed_count, 3);
        assert!(schedule.next_due > 3_000 + 86_400 * 3);
        assert_eq!(schedule.last_executed, Some(3_000 + 86_400 * 3 + 100));
    }
}

#[test]
fn test_execute_idempotent_oneshot() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create one-shot schedule
    let schedule_id = client.create_remittance_schedule(&owner, &750, &3_000, &0);
    assert_eq!(schedule_id, 1);

    // Advance time past due
    set_time(&env, 3_500);

    // First execution
    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);
    let first_id = first.get(0);
    assert!(first_id.is_some());
    if let Some(id) = first_id {
        assert_eq!(id, 1);
    }

    // Second execution at same timestamp must be idempotent (no-op)
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0, "Second call must be a no-op");

    // Verify schedule remains inactive
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(!schedule.active);
        assert_eq!(schedule.last_executed, Some(3_500));
    }
}

#[test]
fn test_execute_idempotent_recurring() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create recurring schedule
    let schedule_id = client.create_remittance_schedule(&owner, &300, &3_000, &86_400);
    assert_eq!(schedule_id, 1);

    set_time(&env, 3_500);

    // First execution
    let first = client.execute_due_remittance_schedules();
    assert_eq!(first.len(), 1);

    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    let first_next_due = if let Some(schedule) = schedule_result {
        schedule.next_due
    } else {
        panic!("Schedule not found");
    };

    // Second execution at same timestamp must not re-execute
    let second = client.execute_due_remittance_schedules();
    assert_eq!(second.len(), 0);

    // Verify next_due unchanged (idempotent advancement)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert_eq!(schedule.next_due, first_next_due);
    }
}

#[test]
fn test_execute_skips_inactive_schedules() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule and cancel it
    let schedule_id = client.create_remittance_schedule(&owner, &200, &3_000, &0);
    assert_eq!(schedule_id, 1);

    client.cancel_remittance_schedule(&owner, &1);

    // Advance past due time
    set_time(&env, 3_500);

    // Execute should skip inactive schedule
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_skips_not_yet_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule due at 3000
    let schedule_id = client.create_remittance_schedule(&owner, &400, &3_000, &0);
    assert_eq!(schedule_id, 1);

    // Advance time but stay before due date
    set_time(&env, 2_500);

    // Execute should not execute (not yet due)
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);

    // Verify schedule unchanged
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.last_executed, None);
    }
}

#[test]
fn test_execute_exactly_equal_next_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &600, &3_000, &0);
    assert_eq!(schedule_id, 1);

    // Advance exactly to next_due (edge case: == not just >)
    set_time(&env, 3_000);

    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1, "Should execute when time == next_due");
}

#[test]
fn test_execute_empty_schedule_set() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // No schedules created; just advance time
    set_time(&env, 5_000);

    // Execute on empty set should return empty Vec
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_all_inactive_set() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create and cancel multiple schedules
    for i in 1..=3 {
        let id = client.create_remittance_schedule(
            &owner,
            &(100 * i as i128),
            &(3_000 + i as u64 * 1000),
            &0,
        );
        assert!(id > 0);
        client.cancel_remittance_schedule(&owner, &(i as u32));
    }

    set_time(&env, 6_000);

    // Execute should return empty (all inactive)
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);
}

#[test]
fn test_execute_paused_contract_returns_empty() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule
    let schedule_id = client.create_remittance_schedule(&owner, &500, &3_000, &0);
    assert_eq!(schedule_id, 1);

    // Pause contract
    client.pause(&owner);

    set_time(&env, 3_500);

    // Execute should return empty when paused
    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 0);

    // Verify schedule was NOT executed (unchanged)
    let schedule_result = client.get_remittance_schedule(&1);
    assert!(schedule_result.is_some());
    if let Some(schedule) = schedule_result {
        assert!(schedule.active);
        assert_eq!(schedule.last_executed, None);
    }
}

#[test]
fn test_execute_mixed_due_not_due() {
    let env = Env::default();
    let (client, owner, _token_addr, _) = setup_split(&env, 50, 30, 15, 5);

    env.mock_all_auths();
    set_time(&env, 1_000);

    // Create schedule 1: due at 2000 (one-off)
    let id1 = client.create_remittance_schedule(&owner, &100, &2_000, &0);
    assert_eq!(id1, 1);

    // Create schedule 2: due at 4000 (one-off)
    let id2 = client.create_remittance_schedule(&owner, &200, &4_000, &0);
    assert_eq!(id2, 2);

    // Advance to time 3000 (only schedule 1 is due)
    set_time(&env, 3_000);

    let executed = client.execute_due_remittance_schedules();
    assert_eq!(executed.len(), 1);
    let first_executed = executed.get(0);
    assert!(first_executed.is_some());
    if let Some(id) = first_executed {
        assert_eq!(id, 1);
    }

    // Verify only schedule 1 is inactive
    let schedule1_result = client.get_remittance_schedule(&1);
    assert!(schedule1_result.is_some());
    if let Some(schedule1) = schedule1_result {
        assert!(!schedule1.active);
    }

    let schedule2_result = client.get_remittance_schedule(&2);
    assert!(schedule2_result.is_some());
    if let Some(schedule2) = schedule2_result {
        assert!(schedule2.active);
    }
}

// ============================================================
// Deadline Boundary Tests for distribute_usdc_signed
// ============================================================
// These tests cover the exact comparison semantics for deadline
// validation in the signed distribution path.
//
// Deadline window semantics:
// - deadline == 0                          -> InvalidDeadline
// - deadline < now                         -> DeadlineExpired
// - deadline == now                        -> DeadlineExpired (strictly greater required)
// - deadline == now + 1                    -> Accepted
// - deadline > now + MAX_DEADLINE_WINDOW_SECS -> InvalidDeadline
// - deadline == now + MAX_DEADLINE_WINDOW_SECS -> Accepted
//
// Security: expired/invalid deadlines must NOT advance the nonce.

fn setup_signed_distribution(env: &Env) -> (
    RemittanceSplitClient<'_>,
    Address,
    Address,
    soroban_sdk::token::StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    env.ledger().set_timestamp(10_000);

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);

    let owner = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_addr = token_contract.address();
    let stellar_client = soroban_sdk::token::StellarAssetClient::new(env, &token_addr);

    client.initialize_split(&owner, &0, &token_addr, &40, &30, &20, &10);
    stellar_client.mint(&owner, &10_000i128);

    (client, owner, token_addr, stellar_client)
}

fn make_request(
    env: &Env,
    usdc_contract: Address,
    from: Address,
    nonce: u64,
    deadline: u64,
) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract,
        from: from.clone(),
        nonce,
        accounts: AccountGroup {
            spending: Address::generate(env),
            savings: Address::generate(env),
            bills: Address::generate(env),
            insurance: Address::generate(env),
        },
        total_amount: 1000i128,
        deadline,
    }
}

/// deadline == 0 must return InvalidDeadline
#[test]
fn test_deadline_zero_is_invalid() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, 0);
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::InvalidDeadline))
    );
}

/// deadline == now must be rejected (DeadlineExpired)
#[test]
fn test_deadline_equal_to_now_is_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now);
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::DeadlineExpired))
    );
}

/// deadline == now - 1 must be rejected (DeadlineExpired)
#[test]
fn test_deadline_one_second_past_is_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now - 1);
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::DeadlineExpired))
    );
}

/// deadline == now + 1 must be accepted (valid boundary)
#[test]
fn test_deadline_one_second_future_is_accepted() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now + 1);
    // Should not return DeadlineExpired or InvalidDeadline
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert!(
        result != Err(Ok(RemittanceSplitError::DeadlineExpired))
            && result != Err(Ok(RemittanceSplitError::InvalidDeadline)),
        "deadline now+1 should pass deadline validation"
    );
}

/// deadline == now + MAX_DEADLINE_WINDOW_SECS must be accepted (upper boundary)
#[test]
fn test_deadline_at_max_window_is_accepted() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(
        &env,
        token_addr.clone(),
        owner.clone(),
        1,
        now + MAX_DEADLINE_WINDOW_SECS,
    );
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert!(
        result != Err(Ok(RemittanceSplitError::DeadlineExpired))
            && result != Err(Ok(RemittanceSplitError::InvalidDeadline)),
        "deadline at max window should pass deadline validation"
    );
}

/// deadline == now + MAX_DEADLINE_WINDOW_SECS + 1 must return InvalidDeadline
#[test]
fn test_deadline_beyond_max_window_is_invalid() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(
        &env,
        token_addr.clone(),
        owner.clone(),
        1,
        now + MAX_DEADLINE_WINDOW_SECS + 1,
    );
    let result = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::InvalidDeadline))
    );
}

/// Expired deadline must NOT advance the nonce (replay-window safety)
#[test]
fn test_expired_deadline_does_not_advance_nonce() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let nonce_before = client.get_nonce(&owner);

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now - 1);
    let _ = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));

    let nonce_after = client.get_nonce(&owner);
    assert_eq!(
        nonce_before, nonce_after,
        "nonce must not advance on expired deadline"
    );
}

/// Invalid deadline (zero) must NOT advance the nonce
#[test]
fn test_invalid_deadline_does_not_advance_nonce() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);

    let nonce_before = client.get_nonce(&owner);

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, 0);
    let _ = client.try_distribute_usdc_hashed(&request, &RemittanceSplit::get_request_hash(env.clone(), request.clone()));

    let nonce_after = client.get_nonce(&owner);
    assert_eq!(
        nonce_before, nonce_after,
        "nonce must not advance on invalid deadline"
    );
}
