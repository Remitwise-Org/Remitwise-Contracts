//! Unit tests for per-owner policy caps and StorageStats determinism.

use insurance::{Insurance, InsuranceClient, MAX_POLICIES_PER_OWNER};
use remitwise_common::CoverageType;
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig},
    Address, Env, String,
};

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env
}

fn setup(env: &Env) -> (Address, InsuranceClient<'_>) {
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(env, &contract_id);
    let owner = Address::generate(env);
    (owner, client)
}

fn create_one(env: &Env, client: &InsuranceClient<'_>, owner: &Address) -> u32 {
    client.create_policy(
        owner,
        &String::from_str(env, "Policy"),
        &CoverageType::Health,
        &100i128,
        &10_000i128,
        &None,
    )
}

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
#[should_panic(expected = "Policy limit exceeded")]
fn cap_over_limit_panics() {
    let env = make_env();
    let (owner, client) = setup(&env);

    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &owner);
    }

    create_one(&env, &client, &owner);
}

#[test]
fn cap_is_per_owner_not_global() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &alice);
        create_one(&env, &client, &bob);
    }

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_policies, MAX_POLICIES_PER_OWNER * 2);
}

#[test]
fn cap_deactivate_frees_slot() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let mut ids = std::vec![];

    for _ in 0..MAX_POLICIES_PER_OWNER {
        ids.push(create_one(&env, &client, &owner));
    }

    assert!(client.deactivate_policy(&owner, &ids[0]));

    let new_id = create_one(&env, &client, &owner);
    assert!(new_id > 0);
    assert_eq!(
        client.get_storage_stats().active_policies,
        MAX_POLICIES_PER_OWNER
    );
}

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
    assert!(client.deactivate_policy(&owner, &id));
    assert_eq!(client.get_storage_stats().active_policies, 0);
}

#[test]
fn stats_deactivate_already_inactive_is_idempotent() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);

    assert!(client.deactivate_policy(&owner, &id));
    assert_eq!(client.get_storage_stats().active_policies, 0);

    assert!(client.deactivate_policy(&owner, &id));
    assert_eq!(client.get_storage_stats().active_policies, 0);
}

#[test]
fn stats_archive_increments_archived_count() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id1 = create_one(&env, &client, &owner);
    let id2 = create_one(&env, &client, &owner);

    assert!(client.deactivate_policy(&owner, &id1));
    assert!(client.deactivate_policy(&owner, &id2));
    assert!(client.archive_policy(&owner, &id1));
    assert!(client.archive_policy(&owner, &id2));

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_policies, 0);
    assert_eq!(stats.archived_policies, 2);
}

#[test]
fn stats_archive_active_policy_changes_active_count() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);

    assert!(client.archive_policy(&owner, &id));

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_policies, 0);
    assert_eq!(stats.archived_policies, 1);
}

#[test]
fn stats_restore_moves_back_to_active() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = create_one(&env, &client, &owner);

    assert!(client.deactivate_policy(&owner, &id));
    assert!(client.archive_policy(&owner, &id));

    let stats_before = client.get_storage_stats();
    assert_eq!(stats_before.active_policies, 0);
    assert_eq!(stats_before.archived_policies, 1);

    assert!(client.restore_policy(&owner, &id));

    let stats_after = client.get_storage_stats();
    assert_eq!(stats_after.active_policies, 1);
    assert_eq!(stats_after.archived_policies, 0);
}

#[test]
fn deactivate_wrong_owner_returns_false() {
    let env = make_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    let id = create_one(&env, &client, &alice);
    assert!(!client.deactivate_policy(&bob, &id));
}

#[test]
fn deactivate_nonexistent_returns_false() {
    let env = make_env();
    let (owner, client) = setup(&env);

    assert!(!client.deactivate_policy(&owner, &999u32));
}

#[test]
fn restore_at_cap_returns_false() {
    let env = make_env();
    let (owner, client) = setup(&env);

    let archived_id = create_one(&env, &client, &owner);
    assert!(client.deactivate_policy(&owner, &archived_id));
    assert!(client.archive_policy(&owner, &archived_id));

    for _ in 0..MAX_POLICIES_PER_OWNER {
        create_one(&env, &client, &owner);
    }

    assert!(!client.restore_policy(&owner, &archived_id));
}

// ---------------------------------------------------------------------------
// Bounds validation for premiums and coverage
// ---------------------------------------------------------------------------

#[test]
fn bounds_monthly_premium_too_high() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &(MAX_MONTHLY_PREMIUM + 1),
        &10_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::MonthlyPremiumTooHigh)));
}

#[test]
fn bounds_coverage_amount_too_high() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &100i128,
        &(MAX_COVERAGE_AMOUNT + 1),
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::CoverageAmountTooHigh)));
}

#[test]
fn bounds_max_values_succeed() {
    let env = make_env();
    let (owner, client) = setup(&env);
    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &MAX_MONTHLY_PREMIUM,
        &MAX_COVERAGE_AMOUNT,
        &None,
    );
    assert!(id > 0);
}
