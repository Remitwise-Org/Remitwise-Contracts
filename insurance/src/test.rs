#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger, LedgerInfo},
    Address, Env, String,
};

use testutils::set_ledger_time;

fn set_time(env: &Env, timestamp: u64) {
    set_ledger_time(env, 1, timestamp);
}

fn make_client(env: &Env) -> (soroban_sdk::Address, InsuranceClient) {
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(env, &contract_id);
    (contract_id, client)
}

fn create_policy(env: &Env, client: &InsuranceClient, owner: &Address) -> u32 {
    client
        .create_policy(
            owner,
            &String::from_str(env, "Test Policy"),
            &CoverageType::Health,
            &100,
            &10000,
        )
        .unwrap()
}

// -----------------------------------------------------------------------
// Basic policy CRUD
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = client
        .create_policy(&owner, &String::from_str(&env, "Health"), &CoverageType::Health, &100, &10000)
        .unwrap();

    assert_eq!(policy_id, 1);
    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.monthly_premium, 100);
    assert!(policy.active);
}

#[test]
fn test_create_policy_invalid_premium() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &0,
        &10000,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_policy_invalid_coverage() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &100,
        &0,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_policy_requires_auth() {
    let env = Env::default();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &100,
        &10000,
    );
    assert!(result.is_err());
}

#[test]
fn test_get_policy_nonexistent() {
    let env = Env::default();
    let (_, client) = make_client(&env);
    assert!(client.get_policy(&999).is_none());
}

#[test]
fn test_pay_premium() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let initial_due = client.get_policy(&policy_id).unwrap().next_payment_date;

    set_time(&env, env.ledger().timestamp() + 1000);
    client.pay_premium(&owner, &policy_id).unwrap();

    let updated = client.get_policy(&policy_id).unwrap();
    assert!(updated.next_payment_date > initial_due);
}

#[test]
fn test_pay_premium_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let result = client.try_pay_premium(&other, &policy_id);
    assert!(result.is_err());
}

#[test]
fn test_pay_premium_inactive_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    client.deactivate_policy(&owner, &policy_id).unwrap();

    let result = client.try_pay_premium(&owner, &policy_id);
    assert!(result.is_err());
}

#[test]
fn test_deactivate_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    assert!(client.deactivate_policy(&owner, &policy_id).unwrap());
    assert!(!client.get_policy(&policy_id).unwrap().active);
}

#[test]
fn test_deactivate_policy_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let result = client.try_deactivate_policy(&other, &policy_id);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// Total monthly premium
// -----------------------------------------------------------------------

#[test]
fn test_get_total_monthly_premium_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    assert_eq!(client.get_total_monthly_premium(&owner), 0);
}

#[test]
fn test_get_total_monthly_premium_multiple() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    client.create_policy(&owner, &String::from_str(&env, "P1"), &CoverageType::Health, &100, &1000).unwrap();
    client.create_policy(&owner, &String::from_str(&env, "P2"), &CoverageType::Life, &200, &2000).unwrap();

    assert_eq!(client.get_total_monthly_premium(&owner), 300);
}

#[test]
fn test_get_total_monthly_premium_excludes_deactivated() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let p1 = create_policy(&env, &client, &owner);
    client.create_policy(&owner, &String::from_str(&env, "P2"), &CoverageType::Life, &200, &2000).unwrap();

    client.deactivate_policy(&owner, &p1).unwrap();
    assert_eq!(client.get_total_monthly_premium(&owner), 200);
}

// -----------------------------------------------------------------------
// Pagination
// -----------------------------------------------------------------------

#[test]
fn test_get_active_policies_excludes_deactivated() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let p1 = create_policy(&env, &client, &owner);
    let p2 = client.create_policy(&owner, &String::from_str(&env, "P2"), &CoverageType::Life, &200, &2000).unwrap();

    client.deactivate_policy(&owner, &p1).unwrap();

    let page = client.get_active_policies(&owner, &0, &DEFAULT_PAGE_LIMIT);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items.get(0).unwrap().id, p2);
}

#[test]
fn test_get_all_policies_for_owner_includes_inactive() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let p1 = create_policy(&env, &client, &owner);
    client.create_policy(&owner, &String::from_str(&env, "P2"), &CoverageType::Life, &200, &2000).unwrap();
    client.create_policy(&owner, &String::from_str(&env, "P3"), &CoverageType::Auto, &300, &3000).unwrap();
    client.create_policy(&other, &String::from_str(&env, "Other"), &CoverageType::Health, &500, &5000).unwrap();

    client.deactivate_policy(&owner, &p1).unwrap();

    let page = client.get_all_policies_for_owner(&owner, &0, &10);
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.count, 3);
}

// -----------------------------------------------------------------------
// Premium schedules
// -----------------------------------------------------------------------

#[test]
fn test_create_premium_schedule_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000).unwrap();
    assert_eq!(schedule_id, 1);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 3000);
    assert!(schedule.active);
}

#[test]
fn test_modify_premium_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000).unwrap();
    client.modify_premium_schedule(&owner, &schedule_id, &4000, &2678400).unwrap();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.next_due, 4000);
    assert_eq!(schedule.interval, 2678400);
}

#[test]
fn test_cancel_premium_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000).unwrap();
    client.cancel_premium_schedule(&owner, &schedule_id).unwrap();

    assert!(!client.get_premium_schedule(&schedule_id).unwrap().active);
}

#[test]
fn test_execute_due_premium_schedules() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &0).unwrap();

    set_time(&env, 3500);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.next_payment_date, 3500 + 30 * 86400);
}

#[test]
fn test_execute_recurring_premium_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000).unwrap();

    set_time(&env, 3500);
    client.execute_due_premium_schedules();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(schedule.active);
    assert_eq!(schedule.next_due, 3000 + 2592000);
}

#[test]
fn test_execute_missed_premium_schedules() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000).unwrap();

    set_time(&env, 3000 + 2592000 * 3 + 100);
    client.execute_due_premium_schedules();

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.missed_count, 3);
    assert!(schedule.next_due > 3000 + 2592000 * 3);
}

// -----------------------------------------------------------------------
// Time drift resilience
// -----------------------------------------------------------------------

#[test]
fn test_time_drift_premium_schedule_not_executed_before_next_due() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    client.create_premium_schedule(&owner, &policy_id, &5000, &2592000).unwrap();

    set_time(&env, 4999);
    assert_eq!(client.execute_due_premium_schedules().len(), 0);
}

#[test]
fn test_time_drift_premium_schedule_executes_at_exact_next_due() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &5000, &2592000).unwrap();

    set_time(&env, 5000);
    let executed = client.execute_due_premium_schedules();
    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0).unwrap(), schedule_id);
}

#[test]
fn test_time_drift_no_double_execution_after_schedule_advances() {
    let env = Env::default();
    env.mock_all_auths();
    set_time(&env, 1000);
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = create_policy(&env, &client, &owner);
    client.create_premium_schedule(&owner, &policy_id, &5000, &2592000).unwrap();

    set_time(&env, 5000);
    assert_eq!(client.execute_due_premium_schedules().len(), 1);

    set_time(&env, 6000);
    assert_eq!(client.execute_due_premium_schedules().len(), 0);
}

// ══════════════════════════════════════════════════════════════════════════
// Policy Tagging: Authorization, Deduplication, and Remove-on-Missing (#283)
// ══════════════════════════════════════════════════════════════════════════

fn setup_policy(env: &Env, client: &InsuranceClient, owner: &Address) -> u32 {
    create_policy(env, client, owner)
}

// --- Authorization ---

#[test]
fn test_add_tags_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "vip"));

    let result = client.try_add_tags_to_policy(&attacker, &policy_id, &tags);
    assert!(result.is_err(), "non-owner must not be able to add tags");
}

#[test]
fn test_remove_tags_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut seed = soroban_sdk::Vec::new(&env);
    seed.push_back(String::from_str(&env, "vip"));
    client.add_tags_to_policy(&owner, &policy_id, &seed);

    let result = client.try_remove_tags_from_policy(&attacker, &policy_id, &seed);
    assert!(result.is_err(), "non-owner must not be able to remove tags");
}

#[test]
fn test_add_tags_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    // New env without mock_all_auths — auth will fail
    let env2 = Env::default();
    let client2 = InsuranceClient::new(&env2, &contract_id);
    let mut tags = soroban_sdk::Vec::new(&env2);
    tags.push_back(String::from_str(&env2, "urgent"));

    let result = client2.try_add_tags_to_policy(&owner, &policy_id, &tags);
    assert!(result.is_err(), "add_tags_to_policy must require auth");
}

// --- Deduplication ---

#[test]
fn test_add_duplicate_tag_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "priority"));

    client.add_tags_to_policy(&owner, &policy_id, &tags);
    client.add_tags_to_policy(&owner, &policy_id, &tags);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 1, "duplicate tag must not be stored twice");
}

#[test]
fn test_add_tags_batch_with_internal_duplicate() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "alpha"));
    tags.push_back(String::from_str(&env, "alpha"));

    client.add_tags_to_policy(&owner, &policy_id, &tags);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 1, "internal batch duplicate must be collapsed");
}

#[test]
fn test_add_multiple_distinct_tags() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut t1 = soroban_sdk::Vec::new(&env);
    t1.push_back(String::from_str(&env, "health"));
    client.add_tags_to_policy(&owner, &policy_id, &t1);

    let mut t2 = soroban_sdk::Vec::new(&env);
    t2.push_back(String::from_str(&env, "urgent"));
    client.add_tags_to_policy(&owner, &policy_id, &t2);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 2);
}

// --- Remove on missing tag ---

#[test]
fn test_remove_nonexistent_tag_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "ghost"));

    client.remove_tags_from_policy(&owner, &policy_id, &tags);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 0, "removing a missing tag must leave tags empty");
}

#[test]
fn test_remove_tag_leaves_others_intact() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "keep"));
    tags.push_back(String::from_str(&env, "remove_me"));
    client.add_tags_to_policy(&owner, &policy_id, &tags);

    let mut to_remove = soroban_sdk::Vec::new(&env);
    to_remove.push_back(String::from_str(&env, "remove_me"));
    client.remove_tags_from_policy(&owner, &policy_id, &to_remove);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 1);
    assert_eq!(policy.tags.get(0).unwrap(), String::from_str(&env, "keep"));
}

#[test]
fn test_remove_all_tags() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "a"));
    tags.push_back(String::from_str(&env, "b"));
    client.add_tags_to_policy(&owner, &policy_id, &tags);
    client.remove_tags_from_policy(&owner, &policy_id, &tags);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.tags.len(), 0);
}

// --- Event emissions ---

#[test]
fn test_add_tags_emits_event() {
    use soroban_sdk::testutils::Events;

    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "vip"));

    let before = env.events().all().len();
    client.add_tags_to_policy(&owner, &policy_id, &tags);
    assert!(env.events().all().len() > before, "add_tags_to_policy must emit at least one event");
}

#[test]
fn test_remove_tags_emits_event() {
    use soroban_sdk::testutils::Events;

    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let policy_id = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "vip"));
    client.add_tags_to_policy(&owner, &policy_id, &tags);

    let before = env.events().all().len();
    client.remove_tags_from_policy(&owner, &policy_id, &tags);
    assert!(env.events().all().len() > before, "remove_tags_from_policy must emit at least one event");
}

// --- Cross-policy isolation ---

#[test]
fn test_tags_are_isolated_per_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = make_client(&env);
    let owner = Address::generate(&env);

    let p1 = setup_policy(&env, &client, &owner);
    let p2 = setup_policy(&env, &client, &owner);

    let mut tags = soroban_sdk::Vec::new(&env);
    tags.push_back(String::from_str(&env, "exclusive"));
    client.add_tags_to_policy(&owner, &p1, &tags);

    let policy2 = client.get_policy(&p2).unwrap();
    assert_eq!(policy2.tags.len(), 0, "tags must not leak between policies");
}
