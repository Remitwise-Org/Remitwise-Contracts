//! Unit tests for per-owner policy caps and StorageStats determinism.

use insurance::{Insurance, InsuranceClient, InsuranceError, MAX_POLICIES_PER_OWNER};
use remitwise_common::CoverageType;
use soroban_sdk::{
    testutils::{Address as AddressTrait, EnvTestConfig},
    Address, Env, String,
};

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env
}

fn setup(env: &Env) -> (Address, InsuranceClient<'_>) {
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(env, &contract_id);
    let owner = Address::generate(env);
    (owner, client)
}

fn create_one(env: &Env, client: &InsuranceClient, owner: &Address) -> u32 {
    client.create_policy(
        owner,
        &String::from_str(env, "Policy"),
        &CoverageType::Health,
        &100i128,
        &10_000i128,
        &None,
    )
}

// ---------------------------------------------------------------------------
// Per-owner cap
// ---------------------------------------------------------------------------

#[test]
fn cap_first_policy_succeeds() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);
    assert!(id > 0);
}

#[test]
fn cap_at_limit_succeeds() {
    let env = make_env();
    let (owner, client) = setup(&env);
    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &owner);
    }
    let stats = client.get_storage_stats();
    assert_eq!(stats.active_policies, MAX_POLICIES_PER_OWNER);
}

#[test]
fn cap_over_limit_returns_error() {
    let env = make_env();
    let (owner, client) = setup(&env);
    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &owner);
    }
    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Over"),
        &CoverageType::Health,
        &100i128,
        &10_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::PolicyLimitExceeded)));
}

#[test]
fn cap_is_per_owner_not_global() {
    // Two owners each at the cap must not interfere with each other.
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &alice);
        create_one(&env, &client, &bob);
    }

    // Both at cap — each additional create must fail independently.
    assert_eq!(
        client.try_create_policy(
            &alice,
            &String::from_str(&env, "X"),
            &CoverageType::Health,
            &1i128,
            &1i128,
            &None
        ),
        Err(Ok(InsuranceError::PolicyLimitExceeded))
    );
    assert_eq!(
        client.try_create_policy(
            &bob,
            &String::from_str(&env, "X"),
            &CoverageType::Health,
            &1i128,
            &1i128,
            &None
        ),
        Err(Ok(InsuranceError::PolicyLimitExceeded))
    );
}

#[test]
fn cap_deactivate_frees_slot() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let mut ids = std::vec![];
    for _ in 0..MAX_POLICIES_PER_OWNER {
        ids.push(create_one(&env, &client, &owner));
    }

    // At cap — must fail.
    assert_eq!(
        client.try_create_policy(
            &owner,
            &String::from_str(&env, "X"),
            &CoverageType::Health,
            &1i128,
            &1i128,
            &None
        ),
        Err(Ok(InsuranceError::PolicyLimitExceeded))
    );

    // Free one slot.
    client.deactivate_policy(&owner, &ids[0]);

    // Now must succeed.
    let new_id = create_one(&env, &client, &owner);
    assert!(new_id > 0);
}

// ---------------------------------------------------------------------------
// StorageStats — active_policies counter
// ---------------------------------------------------------------------------

#[test]
fn stats_initial_state_is_zero() {
    let env = make_env();
    let (_, client) = setup(&env);
    let stats = client.get_storage_stats();
    assert_eq!(stats.active_policies, 0);
    assert_eq!(stats.archived_policies, 0);
}

#[test]
fn stats_increments_on_create() {
    let env = make_env();
    let (owner, client) = setup(&env);
    create_one(&env, &client, &owner);
    assert_eq!(client.get_storage_stats().active_policies, 1);
    create_one(&env, &client, &owner);
    assert_eq!(client.get_storage_stats().active_policies, 2);
}

#[test]
fn stats_decrements_on_deactivate() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);
    assert_eq!(client.get_storage_stats().active_policies, 1);
    client.deactivate_policy(&owner, &id);
    assert_eq!(client.get_storage_stats().active_policies, 0);
}

#[test]
fn stats_deactivate_already_inactive_is_idempotent() {
    // Deactivating an already-inactive policy must not double-decrement.
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);
    client.deactivate_policy(&owner, &id);
    assert_eq!(client.get_storage_stats().active_policies, 0);
    // Second deactivate on same policy.
    client.deactivate_policy(&owner, &id);
    assert_eq!(
        client.get_storage_stats().active_policies,
        0,
        "active_policies must not underflow on double-deactivate"
    );
}

#[test]
fn stats_archive_increments_archived_count() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id1 = create_one(&env, &client, &owner);
    let id2 = create_one(&env, &client, &owner);
    client.deactivate_policy(&owner, &id1);
    client.deactivate_policy(&owner, &id2);

    let archived = client.archive_policies(&owner);
    assert_eq!(archived, 2);

    let stats = client.get_storage_stats();
    assert_eq!(stats.archived_policies, 2);
    assert_eq!(stats.active_policies, 0);
}

#[test]
fn stats_archive_does_not_change_active_count() {
    // active_policies was already decremented by deactivate; archive must not touch it.
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);
    create_one(&env, &client, &owner); // stays active

    client.deactivate_policy(&owner, &id);
    assert_eq!(client.get_storage_stats().active_policies, 1);

    client.archive_policies(&owner);
    assert_eq!(
        client.get_storage_stats().active_policies,
        1,
        "archive must not change active_policies"
    );
}

#[test]
fn stats_restore_decrements_archived_increments_nothing() {
    // restore_policy brings a policy back as inactive, so active_policies stays the same.
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);
    client.deactivate_policy(&owner, &id);
    client.archive_policies(&owner);

    let stats_before = client.get_storage_stats();
    assert_eq!(stats_before.archived_policies, 1);
    assert_eq!(stats_before.active_policies, 0);

    client.restore_policy(&owner, &id);

    let stats_after = client.get_storage_stats();
    assert_eq!(stats_after.archived_policies, 0);
    // Restored as inactive — active_policies unchanged.
    assert_eq!(stats_after.active_policies, 0);
}

#[test]
fn stats_cleanup_decrements_archived_count() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id1 = create_one(&env, &client, &owner);
    let id2 = create_one(&env, &client, &owner);
    client.deactivate_policy(&owner, &id1);
    client.deactivate_policy(&owner, &id2);
    client.archive_policies(&owner);

    assert_eq!(client.get_storage_stats().archived_policies, 2);

    // Cleanup with before_timestamp = u64::MAX removes all archived records.
    let deleted = client.cleanup_policies(&owner, &u64::MAX);
    assert_eq!(deleted, 2);
    assert_eq!(client.get_storage_stats().archived_policies, 0);
}

#[test]
fn stats_full_lifecycle_determinism() {
    // create → deactivate → archive → cleanup: verify each step's counter.
    let env = make_env();
    let (owner, client) = setup(&env);

    // Step 1: create 3 policies.
    let ids: std::vec::Vec<u32> = (0..3).map(|_| create_one(&env, &client, &owner)).collect();
    assert_eq!(client.get_storage_stats().active_policies, 3);
    assert_eq!(client.get_storage_stats().archived_policies, 0);

    // Step 2: deactivate 2.
    client.deactivate_policy(&owner, &ids[0]);
    client.deactivate_policy(&owner, &ids[1]);
    assert_eq!(client.get_storage_stats().active_policies, 1);
    assert_eq!(client.get_storage_stats().archived_policies, 0);

    // Step 3: archive the 2 inactive ones.
    client.archive_policies(&owner);
    assert_eq!(client.get_storage_stats().active_policies, 1);
    assert_eq!(client.get_storage_stats().archived_policies, 2);

    // Step 4: cleanup all archived.
    client.cleanup_policies(&owner, &u64::MAX);
    assert_eq!(client.get_storage_stats().active_policies, 1);
    assert_eq!(client.get_storage_stats().archived_policies, 0);
}

// ---------------------------------------------------------------------------
// deactivate_policy — authorization
// ---------------------------------------------------------------------------

#[test]
fn deactivate_wrong_owner_returns_unauthorized() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let id = create_one(&env, &client, &alice);
    let result = client.try_deactivate_policy(&bob, &id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
}

#[test]
fn deactivate_nonexistent_returns_not_found() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let result = client.try_deactivate_policy(&owner, &999u32);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyNotFound)));
}

// ---------------------------------------------------------------------------
// restore_policy — cap enforcement
// ---------------------------------------------------------------------------

#[test]
fn restore_at_cap_returns_limit_exceeded() {
    let env = make_env();
    let (owner, client) = setup(&env);

    // Create one, deactivate, archive it — this is the candidate for restore.
    let archived_id = create_one(&env, &client, &owner);
    client.deactivate_policy(&owner, &archived_id);
    client.archive_policies(&owner);

    // Fill up to cap with new active policies.
    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &owner);
    }

    // Restore must be rejected because owner is at cap.
    let result = client.try_restore_policy(&owner, &archived_id);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyLimitExceeded)));
}
