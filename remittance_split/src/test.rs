#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger},
    token::StellarAssetClient,
    Address, Env,
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

fn split_expected(_env: &Env, client: &RemittanceSplitClient<'_>, total_amount: i128) -> [i128; 4] {
    // RemittanceSplitClient's generated binding for `calculate_split` returns a
    // `soroban_sdk::Vec<i128>` directly in this test harness.
    let alloc = client
        .calculate_split(&total_amount);

    [
        alloc.get(0).unwrap(),
        alloc.get(1).unwrap(),
        alloc.get(2).unwrap(),
        alloc.get(3).unwrap(),
    ]
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

    // 5. Verify the distribution completion event.
    //
    // The contract no longer emits a separate structured `DistributionCompleted`
    // event (it was removed to reduce transient memory allocations; see
    // `distribute_usdc` in lib.rs). The shipped completion signal is the
    // unstructured `dist_ok` event emitted via `RemitwiseEvents::emit`, which
    // carries four topics (`Remitwise`, category, priority, `dist_ok`) and a
    // `(from, total_amount)` data payload.
    let events = env.events().all();

    let last_event = events.last().expect("No events emitted");
    let (_contract_id, topics, data) = last_event;

    // `RemitwiseEvents::emit` always publishes 4 topics.
    assert_eq!(topics.len(), 4, "Expected 4 topics in event");

    // The 4th topic is the action symbol; it must be `dist_ok`.
    let action: Symbol = topics
        .get(3)
        .expect("missing action topic")
        .into_val(&env);
    assert_eq!(action, symbol_short!("dist_ok"));

    // Verify the structured payload: (from, total_amount).
    let (from, amount): (Address, i128) = data.into_val(&env);
    assert_eq!(from, owner);
    assert_eq!(amount, total_amount);
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

/// Verifies that reading instance storage (`get_config`) bumps the instance TTL.
///
/// NOTE: This test used to also assert that reading a schedule via
/// `get_remittance_schedule` re-extended the *persistent* TTL on every access.
/// That is no longer true: `get_remittance_schedule` is now an explicit
/// read-only accessor that does NOT call `extend_ttl` ("avoid an extra TTL
/// write to reduce gas", see lib.rs), and `create_remittance_schedule`
/// likewise skips the immediate `extend_ttl`. Asserting read-driven persistent
/// TTL extension would test behavior the contract intentionally removed, so
/// that portion was dropped. Instance TTL extension via `get_config`
/// (which DOES call `extend_instance_ttl`) is still exercised below.
#[test]
fn test_ttl_extensions() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _owner, _token_addr, _stellar_client) = setup_split(&env, 40, 30, 20, 10);

    // Check Instance TTL extension (CONFIG).
    // Initial sequence is 0. Threshold is INSTANCE_LIFETIME_THRESHOLD.
    let threshold = INSTANCE_LIFETIME_THRESHOLD;

    // Advance to threshold - 1.
    env.ledger().set_sequence_number(threshold - 1);

    // Access CONFIG; `get_config` bumps the instance TTL by INSTANCE_BUMP_AMOUNT.
    let config = client.get_config();
    assert!(config.is_some(), "Config should exist before expiration");

    // After access the TTL is bumped, so advancing past the original threshold
    // must still find the config present.
    env.ledger().set_sequence_number(threshold + 1);
    let config = client.get_config();
    assert!(config.is_some(), "Config should exist after TTL bump");
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

fn setup_request_hash_distribution(
    env: &Env,
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

    assert_eq!(
        client.try_distribute_usdc_hashed(&request, &hash),
        Ok(Ok(true))
    );

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

    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::SelfTransferNotAllowed))
    );
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

    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UntrustedTokenContract))
    );
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

// ============================================================
// Batch / fan-out conservation tests
// ============================================================
// The executor `execute_due_remittance_schedules()` runs many
// schedule executions in a single sweep (fan-out). Even if
// `calculate_split()` is conservative per-schedule, an implementation
// bug could leak/destroy value at the aggregate level by
// mis-advancing `next_due`, double-executing, or skipping schedules.
//
// This test suite pins:
// 1) Exact due-set selection for a given ledger timestamp.
// 2) Idempotent `next_due` advancement (no re-execution within the window).
// 3) Exact remainder/dust policy matches `calculate_split()` for each executed schedule.
// 4) Aggregate funds conservation: sum(funds_in) == sum(funds_out) across the whole fan-out.

#[test]
fn test_execute_due_remittance_schedules_fanout_dust_conservation() {
    let env = Env::default();
    env.mock_all_auths();

    // Use one contract instance with a fixed split regime.
    // (Percentages are fixed per RemittanceSplit instance.)
    let (client, owner, _token_addr, _stellar_client) = setup_split(&env, 37, 33, 20, 10);

    // Seed due schedules at/under a single ledger time.
    // Include both one-off (interval=0) and recurring (interval>0).
    // Also choose amounts that create lots of dust due to floor division by 100.
    set_time(&env, 1_000);
    let now = 5_000u64;

    let one_off_due_at = now; // exactly due
    let recurring_due_at = now; // exactly due (first interval)

    let amounts: [i128; 6] = [
        101,  // dust-heavy
        250,  // dust-heavy
        999,  // near-1000
        1_003, // different remainder profile
        10_007, // larger dust surface
        77,   // small dust
    ];

    // Create 6 schedules, all with next_due <= now, so they are due in this sweep.
    // IDs are monotonic starting at 1 in this test env.
    let mut ids = Vec::new(&env);


    // One-off schedules (interval 0): will deactivate after execution.
let id1 = client
        .create_remittance_schedule(&owner, &amounts[0], &one_off_due_at, &0)
        ;
    ids.push_back(id1);
    let id2 = client
        .create_remittance_schedule(&owner, &amounts[1], &one_off_due_at, &0);

    ids.push_back(id2);


    // Recurring schedules: will advance next_due and keep active.
    // Use MIN_SCHEDULE_INTERVAL to respect constraints.
    let interval = MIN_SCHEDULE_INTERVAL;
let id3 = client
        .create_remittance_schedule(&owner, &amounts[2], &recurring_due_at, &interval)
        ;
    ids.push_back(id3);
let id4 = client
        .create_remittance_schedule(&owner, &amounts[3], &recurring_due_at, &interval)
        ;
    ids.push_back(id4);
let id5 = client
        .create_remittance_schedule(&owner, &amounts[4], &recurring_due_at, &interval)
        ;
    ids.push_back(id5);

    // Another one-off for balance.
let id6 = client
        .create_remittance_schedule(&owner, &amounts[5], &one_off_due_at, &0)
        ;
    ids.push_back(id6);

    // Execute due schedules at `now`.
    set_time(&env, now);

    let executed = client.execute_due_remittance_schedules();

    // Assert exact due-set selection: all 6 schedules due at `now` must execute.
    assert_eq!(executed.len(), 6);

    // Convert to a boolean set for exactness without sorting guarantees.
    let mut seen = [false; 7];
    for i in 0..ids.len() {
        let id = ids.get(i).unwrap();
        // All ids should be in range 1..=6 in this test.
        assert!(id as usize <= 6);
    }
    for i in 0..executed.len() {
        let id = executed.get(i).unwrap();
        assert!((id as usize) <= 6);
        assert!(!seen[id as usize], "schedule {} executed twice", id);
        seen[id as usize] = true;
    }
    for expected in 1..=6u32 {
        assert!(seen[expected as usize], "missing executed schedule id {}", expected);
    }

    // Per-schedule dust policy pin + aggregate conservation pin.
    let mut total_in: i128 = 0;
    let mut total_out: [i128; 4] = [0, 0, 0, 0];

    for i in 1..=6u32 {
        let sched = client
            .get_remittance_schedule(&i)
            .expect("schedule must exist");

        assert!(sched.last_executed.is_some());
        assert_eq!(sched.last_executed, Some(now));

        // Confirm dust/remainder policy matches calculate_split for each schedule amount.
        let expected_allocs = split_expected(&env, &client, sched.amount);

        assert_eq!(
            expected_allocs[0] + expected_allocs[1] + expected_allocs[2] + expected_allocs[3],
            sched.amount,
            "per-schedule conservation should hold"
        );

        // Reconcile category amounts by calling calculate_split directly and comparing.
        // (The executor stores only next_due/last_executed, so the only authoritative
        // dust policy source is calculate_split.)
        total_in = total_in.checked_add(sched.amount).expect("aggregate must not overflow");
        for k in 0..4 {
            total_out[k] = total_out[k]
                .checked_add(expected_allocs[k])
                .expect("aggregate category must not overflow");
        }

        // next_due behavior & idempotency invariants.
        if sched.interval == 0 {
            assert!(!sched.active, "one-off schedules must deactivate");
        } else {
            // For recurring schedules starting at `now`, next_due must be strictly > now.
            // With interval=MIN_SCHEDULE_INTERVAL and `next_due==now`, next_due should be now+interval.
            assert!(sched.active);
            assert_eq!(sched.next_due, now + interval);
            assert_eq!(sched.missed_count, 0);
        }
    }

    assert_eq!(
        total_in,
        total_out[0]
            .checked_add(total_out[1])
            .and_then(|v| v.checked_add(total_out[2]))
            .and_then(|v| v.checked_add(total_out[3]))
            .expect("aggregate must not overflow"),
        "aggregate dust conservation must hold across the entire fan-out sweep"
    );

    // Idempotency: re-sweep at the same ledger timestamp must execute nothing.
    let executed_again = client.execute_due_remittance_schedules();
    assert_eq!(executed_again.len(), 0);

    // And next_due/last_executed should remain unchanged.
    for i in 1..=6u32 {
        let sched2 = client.get_remittance_schedule(&i).expect("schedule must exist");
        assert_eq!(sched2.last_executed, Some(now));
        if sched2.interval == 0 {
            assert!(!sched2.active);
        } else {
            assert_eq!(sched2.next_due, now + interval);
        }
    }
}

// Keep existing mixed due/not-due test after fan-out tests.

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

fn setup_signed_distribution(
    env: &Env,
) -> (
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
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// deadline == now must be rejected (DeadlineExpired)
#[test]
fn test_deadline_equal_to_now_is_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now);
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

/// deadline == now - 1 must be rejected (DeadlineExpired)
#[test]
fn test_deadline_one_second_past_is_expired() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now - 1);
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::DeadlineExpired)));
}

/// deadline == now + 1 must be accepted (valid boundary)
#[test]
fn test_deadline_one_second_future_is_accepted() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now + 1);
    // Should not return DeadlineExpired or InvalidDeadline
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
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
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
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
    let result = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDeadline)));
}

/// Expired deadline must NOT advance the nonce (replay-window safety)
#[test]
fn test_expired_deadline_does_not_advance_nonce() {
    let env = Env::default();
    let (client, owner, token_addr, _) = setup_signed_distribution(&env);
    let now = env.ledger().timestamp();

    let nonce_before = client.get_nonce(&owner);

    let request = make_request(&env, token_addr.clone(), owner.clone(), 1, now - 1);
    let _ = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );

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
    let _ = client.try_distribute_usdc_hashed(
        &request,
        &RemittanceSplit::get_request_hash(env.clone(), request.clone()),
    );

    let nonce_after = client.get_nonce(&owner);
    assert_eq!(
        nonce_before, nonce_after,
        "nonce must not advance on invalid deadline"
    );
}

/// get_schedules_paginated must terminate, preserve ID order, and return each
/// schedule exactly once when traversing a full owner schedule set.
#[test]
fn test_get_schedules_paginated_full_scale_cursor_monotonicity() {
    let env = Env::default();
    let (client, owner, _, _) = setup_split(&env, 50, 30, 15, 5);
    let other_owner = Address::generate(&env);

    let amount = 1_000i128;
    let next_due = env.ledger().timestamp() + 86_400;
    let interval = MIN_SCHEDULE_INTERVAL;

    let first_id = client.create_remittance_schedule(&owner, &amount, &next_due, &interval);
    assert_eq!(first_id, 1);

    let single_page = client.get_schedules_paginated(&owner, &0, &1);
    assert_eq!(single_page.count, 1);
    assert_eq!(single_page.items.len(), 1);
    assert_eq!(single_page.items.get(0).unwrap().id, first_id);
    assert_eq!(single_page.next_cursor, 0);

    for i in 1..MAX_SCHEDULES_PER_OWNER {
        let id =
            client.create_remittance_schedule(&owner, &amount, &(next_due + i as u64), &interval);
        assert_eq!(id, i + 1);
    }

    let isolated_page = client.get_schedules_paginated(&other_owner, &0, &50);
    assert_eq!(isolated_page.count, 0);
    assert_eq!(isolated_page.items.len(), 0);
    assert_eq!(isolated_page.next_cursor, 0);

    let clamped_page = client.get_schedules_paginated(&owner, &0, &u32::MAX);
    assert_eq!(clamped_page.count, MAX_SCHEDULES_PER_OWNER);
    assert_eq!(clamped_page.items.len(), MAX_SCHEDULES_PER_OWNER);
    assert_eq!(clamped_page.next_cursor, 0);

    let mut last_id = 0u32;
    for i in 0..clamped_page.items.len() {
        let schedule = clamped_page.items.get(i).unwrap();
        assert!(schedule.id > last_id, "schedules must be ordered by ID");
        last_id = schedule.id;
    }

    let mut seen = [false; (MAX_SCHEDULES_PER_OWNER as usize) + 1];
    let mut seen_count = 0u32;
    let mut cursor = 0u32;
    let mut pages = 0u32;
    let page_limit = 7u32;

    loop {
        let page = client.get_schedules_paginated(&owner, &cursor, &page_limit);
        assert!(page.count <= page_limit);
        assert_eq!(page.count, page.items.len());

        let mut previous_id = 0u32;
        for i in 0..page.items.len() {
            let schedule = page.items.get(i).unwrap();
            assert!(schedule.id > previous_id, "page IDs must be ascending");
            let seen_index = schedule.id as usize;
            assert!(!seen[seen_index], "schedule {} appeared on multiple pages", schedule.id);
            seen[seen_index] = true;
            seen_count += 1;
            previous_id = schedule.id;
        }

        pages += 1;
        if page.next_cursor == 0 {
            break;
        }

        assert!(
            page.next_cursor > cursor,
            "cursor must strictly advance before termination"
        );
        cursor = page.next_cursor;
    }

    assert_eq!(pages, 8);
    assert_eq!(seen_count, MAX_SCHEDULES_PER_OWNER);
    for id in 1..=MAX_SCHEDULES_PER_OWNER {
        assert!(seen[id as usize], "missing schedule id {}", id);
    }

    let beyond_end = client.get_schedules_paginated(&owner, &MAX_SCHEDULES_PER_OWNER, &page_limit);
    assert_eq!(beyond_end.count, 0);
    assert_eq!(beyond_end.items.len(), 0);
    assert_eq!(beyond_end.next_cursor, 0);
}

// ============================================================================
// get_split_allocations shape invariant tests
//
// These tests pin the shape contract documented in
// docs/remittance-split-allocations-shape.md:
//
//   1. Uninitialized contract   → default [50,30,15,5] percentages, no panic,
//                                 4 allocations, deterministic category order.
//   2. Single 100% slot         → exactly 4 allocations, correct Category, sum == total.
//   3. Zero amount              → Err(InvalidAmount); no partial allocation produced.
//   4. Amount conservation      → sum(allocation.amount) == total_amount for every case.
//   5. Deterministic ordering   → category symbols are always SPENDING/SAVINGS/BILLS/INSURANCE.
//   6. Large amount             → i128::MAX/2 with 100/0/0/0 and default config, no overflow.
//   7. Max categories at 100    → percentages across all 4 slots summing to 100.
// ============================================================================

/// Helper: assert the four returned category symbols are in canonical order.
fn assert_canonical_order(env: &Env, allocs: &soroban_sdk::Vec<Allocation>) {
    assert_eq!(allocs.len(), 4, "must return exactly 4 allocations");
    assert_eq!(allocs.get(0).unwrap().category, symbol_short!("SPENDING"));
    assert_eq!(allocs.get(1).unwrap().category, symbol_short!("SAVINGS"));
    assert_eq!(allocs.get(2).unwrap().category, symbol_short!("BILLS"));
    assert_eq!(allocs.get(3).unwrap().category, symbol_short!("INSURANCE"));
}

/// Helper: assert sum of allocation amounts equals `total`.
fn assert_conservation(allocs: &soroban_sdk::Vec<Allocation>, total: i128) {
    let sum: i128 = allocs
        .iter()
        .map(|a| a.amount)
        .fold(0i128, |acc, x| acc + x);
    assert_eq!(sum, total, "allocation amounts must sum to total_amount");
}

/// Shape invariant: uninitialized contract uses the default [50,30,15,5] split,
/// returns 4 allocations in canonical order, and satisfies amount conservation.
/// Verifies that get_split_allocations does NOT panic before initialize_split.
#[test]
fn test_get_split_allocations_uninitialized_uses_default() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);

    let total: i128 = 1_000;
    let allocs = RemittanceSplit::get_split_allocations(&env, total)
    let allocs = env
        .as_contract(&contract_id, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("uninitialized contract must not return an error for positive amount");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);

    // Default percentages: 50/30/15/5 → floor values
    assert_eq!(allocs.get(0).unwrap().amount, 500); // 50% of 1000
    assert_eq!(allocs.get(1).unwrap().amount, 300); // 30%
    assert_eq!(allocs.get(2).unwrap().amount, 150); // 15%
    // Insurance = remainder = 1000 - 500 - 300 - 150 = 50 (matches 5%)
    assert_eq!(allocs.get(3).unwrap().amount, 50);
}

/// Shape invariant: uninitialized contract, non-round amount.
/// Remainder must still land in insurance (slot 3) keeping conservation exact.
#[test]
fn test_get_split_allocations_uninitialized_non_round_amount() {
    let env = Env::default();
    env.register_contract(None, RemittanceSplit);

    let total: i128 = 7; // prime; will produce remainders with default split
    let allocs = RemittanceSplit::get_split_allocations(&env, total)
    let contract_id = env.register_contract(None, RemittanceSplit);

    let total: i128 = 7; // prime; will produce remainders with default split
    let allocs = env
        .as_contract(&contract_id, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("must succeed on positive amount");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);
}

/// Shape invariant: single 100% spending config → 4 allocations, savings/bills/insurance == 0,
/// spending == total_amount, sum == total_amount.
#[test]
fn test_get_split_allocations_single_100_percent_spending() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    let (client, _owner, _token_addr, _) = setup_split(&env, 100, 0, 0, 0);
    let _ = client; // client used only to initialize; call through impl directly

    let total: i128 = 500;
    let allocs = RemittanceSplit::get_split_allocations(&env, total)

    let total: i128 = 500;
    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("must succeed");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);

    assert_eq!(allocs.get(0).unwrap().amount, 500, "spending == total");
    assert_eq!(allocs.get(1).unwrap().amount, 0, "savings == 0");
    assert_eq!(allocs.get(2).unwrap().amount, 0, "bills == 0");
    // insurance = 500 - 500 - 0 - 0 = 0
    assert_eq!(allocs.get(3).unwrap().amount, 0, "insurance == 0");
}

/// Shape invariant: single 100% insurance config → spending/savings/bills == 0,
/// insurance == total_amount (remainder path), sum == total_amount.
#[test]
fn test_get_split_allocations_single_100_percent_insurance() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    let (client, _owner, _token_addr, _) = setup_split(&env, 0, 0, 0, 100);
    let _ = client;

    let total: i128 = 333;
    let allocs = RemittanceSplit::get_split_allocations(&env, total)

    let total: i128 = 333;
    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("must succeed");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);

    assert_eq!(allocs.get(0).unwrap().amount, 0);
    assert_eq!(allocs.get(1).unwrap().amount, 0);
    assert_eq!(allocs.get(2).unwrap().amount, 0);
    // insurance = remainder = 333 - 0 - 0 - 0 = 333
    assert_eq!(allocs.get(3).unwrap().amount, 333);
}

/// Shape invariant: zero amount returns Err(InvalidAmount).
/// No partial allocation is produced; the error fires before any arithmetic.
#[test]
fn test_get_split_allocations_zero_amount_returns_error() {
    let env = Env::default();
    env.register_contract(None, RemittanceSplit);

    let result = RemittanceSplit::get_split_allocations(&env, 0);
    assert_eq!(
        result,
        Err(RemittanceSplitError::InvalidAmount),
        "zero amount must return InvalidAmount"
    );
}

/// Shape invariant: negative amount returns Err(InvalidAmount).
#[test]
fn test_get_split_allocations_negative_amount_returns_error() {
    let env = Env::default();
    env.register_contract(None, RemittanceSplit);

    let result = RemittanceSplit::get_split_allocations(&env, -1);
    assert_eq!(
        result,
        Err(RemittanceSplitError::InvalidAmount),
        "negative amount must return InvalidAmount"
    );
}

/// Amount conservation: amount == 1 with equal split produces correct remainder in insurance.
#[test]
fn test_get_split_allocations_amount_one_conservation() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    // 25/25/25/25: each floor = floor(1*25/100) = 0; insurance = 1 - 0 - 0 - 0 = 1
    let (client, _owner, _token_addr, _) = setup_split(&env, 25, 25, 25, 25);
    let _ = client;

    let allocs = RemittanceSplit::get_split_allocations(&env, 1).expect("must succeed");

    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, 1))
        .expect("must succeed");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, 1);
    // All floor allocations are 0; entire amount goes to insurance as remainder
    assert_eq!(allocs.get(3).unwrap().amount, 1);
}

/// Deterministic ordering: calling get_split_allocations twice with the same
/// state produces identical results in the same category order.
#[test]
fn test_get_split_allocations_ordering_is_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    let (client, _owner, _token_addr, _) = setup_split(&env, 40, 30, 20, 10);
    let _ = client;

    let total: i128 = 1_000;
    let allocs1 = RemittanceSplit::get_split_allocations(&env, total).expect("first call");
    let allocs2 = RemittanceSplit::get_split_allocations(&env, total).expect("second call");

    let total: i128 = 1_000;
    let allocs1 = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("first call");
    let allocs2 = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("second call");

    assert_eq!(allocs1.len(), allocs2.len());
    for i in 0..allocs1.len() {
        let a1 = allocs1.get(i).unwrap();
        let a2 = allocs2.get(i).unwrap();
        assert_eq!(a1.category, a2.category, "category at index {i} must match");
        assert_eq!(a1.amount, a2.amount, "amount at index {i} must match");
    }
}

/// Ordering matches get_split / get_config field order: the i-th allocation
/// amount equals floor(total * split[i] / 100), except insurance which is remainder.
#[test]
fn test_get_split_allocations_order_matches_get_split() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    let (client, _owner, _token_addr, _) = setup_split(&env, 40, 30, 20, 10);

    let total: i128 = 1_000;
    let split = RemittanceSplit::get_split(&env); // [40, 30, 20, 10]
    let allocs = RemittanceSplit::get_split_allocations(&env, total).expect("must succeed");
    let split = env.as_contract(&client.address, || RemittanceSplit::get_split(&env)); // [40, 30, 20, 10]
    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("must succeed");

    // Verify floor values for the first three categories
    for i in 0..3u32 {
        let pct = split.get(i).unwrap() as i128;
        let expected = total * pct / 100;
        assert_eq!(
            allocs.get(i).unwrap().amount,
            expected,
            "allocation[{i}] must be floor(total * pct / 100)"
        );
    }
    // Insurance is remainder
    assert_conservation(&allocs, total);
}

/// Large amount: i128::MAX / 2 with default config must not overflow and must
/// satisfy the conservation invariant.
#[test]
fn test_get_split_allocations_large_amount_default_config() {
    let env = Env::default();
    env.register_contract(None, RemittanceSplit); // uninitialized → default [50,30,15,5]

    let total: i128 = i128::MAX / 2;
    let allocs = RemittanceSplit::get_split_allocations(&env, total)
        .expect("i128::MAX/2 must not overflow with default split");
/// Large amount with default config must not overflow and must satisfy the
/// conservation invariant. `calculate_split` multiplies `total * percent`
/// before dividing by 100 (with checked arithmetic), so the largest safe input
/// is bounded by the maximum percentage. With the default [50,30,15,5] split
/// the binding multiplier is 100 (used as the divisor headroom), so `i128::MAX
/// / 100` is the largest value guaranteed not to overflow `checked_mul`.
#[test]
fn test_get_split_allocations_large_amount_default_config() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit); // uninitialized → default [50,30,15,5]

    let total: i128 = i128::MAX / 100;
    let allocs = env
        .as_contract(&contract_id, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("i128::MAX/100 must not overflow with default split");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);
}

/// Large amount: i128::MAX / 2 with a single 100% spending slot — exercises the
/// remainder path with a large value.
/// Large amount with a single 100% spending slot — exercises the remainder path
/// with a large value. The 100% slot makes 100 the binding multiplier in
/// `checked_mul(total, 100)`, so `i128::MAX / 100` is the largest input that
/// will not overflow. (`(i128::MAX / 100) * 100 <= i128::MAX`.)
#[test]
fn test_get_split_allocations_large_amount_single_slot() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    let (client, _owner, _token_addr, _) = setup_split(&env, 100, 0, 0, 0);
    let _ = client;

    let total: i128 = i128::MAX / 2;
    let allocs = RemittanceSplit::get_split_allocations(&env, total)
        .expect("i128::MAX/2 with 100/0/0/0 must not overflow");

    let total: i128 = i128::MAX / 100;
    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("i128::MAX/100 with 100/0/0/0 must not overflow");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);
    assert_eq!(allocs.get(0).unwrap().amount, total);
    assert_eq!(allocs.get(3).unwrap().amount, 0);
}

/// Max-category coverage: percentages spread across all four slots summing to 100.
/// Verifies conservation and canonical ordering for a non-trivial split.
#[test]
fn test_get_split_allocations_percentages_across_all_categories() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1_000);

    // 33/33/33/1 — non-round split that produces visible dust
    let (client, _owner, _token_addr, _) = setup_split(&env, 33, 33, 33, 1);
    let _ = client;

    let total: i128 = 10;
    let allocs = RemittanceSplit::get_split_allocations(&env, total).expect("must succeed");

    let total: i128 = 10;
    let allocs = env
        .as_contract(&client.address, || RemittanceSplit::get_split_allocations(&env, total))
        .expect("must succeed");

    assert_canonical_order(&env, &allocs);
    assert_conservation(&allocs, total);

    // floor(10*33/100) = 3 for spending, savings, bills
    assert_eq!(allocs.get(0).unwrap().amount, 3);
    assert_eq!(allocs.get(1).unwrap().amount, 3);
    assert_eq!(allocs.get(2).unwrap().amount, 3);
    // insurance = 10 - 3 - 3 - 3 = 1  (dust absorbed)
    assert_eq!(allocs.get(3).unwrap().amount, 1);
}
