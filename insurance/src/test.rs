#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Ledger},
    Address, Env, String,
};



fn setup() -> (Env, InsuranceClient<'static>, Address) {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);
    env.mock_all_auths();
    (env, client, owner)
}

fn short_name(env: &Env) -> Result<String, ()> {
    Ok(String::from_str(env, "Short"))
}


use ::testutils::{set_ledger_time, setup_test_env};

// Removed local set_time in favor of testutils::set_ledger_time

#[test]
fn test_create_policy_succeeds() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.initialize(&owner);

    let name = String::from_str(&env, "Health Policy");
    let coverage_type = CoverageType::Health;

    let policy_id = client.create_policy(
        &owner,
        &name,
        &coverage_type,
        &100,   // monthly_premium
        &10000, // coverage_amount
    &None);

    assert_eq!(policy_id, 1);

    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.monthly_premium, 100);
    assert_eq!(policy.coverage_amount, 10000);
    assert!(policy.active);
}

#[test]
fn test_create_policy_invalid_premium() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &0,
        &10000,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_create_policy_invalid_coverage() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    let result = client.try_create_policy(
        &owner,
        &String::from_str(&env, "Bad"),
        &CoverageType::Health,
        &100,
        &0,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));
}

#[test]
fn test_pay_premium() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &100,
        &10000,
    &None);

// ── pay_premium ───────────────────────────────────────────────────────────────

#[test]
fn test_pay_premium_updates_date() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let before = client.get_policy(&id).unwrap().next_payment_date;
    set_ledger_time(&env, 1, env.ledger().timestamp() + 1000);
    client.pay_premium(&owner, &id);
    let after = client.get_policy(&id).unwrap().next_payment_date;
    assert!(after > before);
}

#[test]
fn test_pay_premium_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);
    let other = Address::generate(&env);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &100,
        &10000,
    &None);

    // unauthorized payer
    client.pay_premium(&other, &policy_id);
    let result = client.try_pay_premium(&other, &policy_id);
    assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));}

#[test]
fn test_deactivate_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy"),
        &CoverageType::Health,
        &100,
        &10000,
    &None);

// ── deactivate_policy ─────────────────────────────────────────────────────────

#[test]
fn test_deactivate_policy() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    assert!(client.deactivate_policy(&owner, &id));
    assert!(!client.get_policy(&id).unwrap().active);
}

#[test]
fn test_get_active_policies() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    // Create 3 policies
    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    let p2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &CoverageType::Property,
        &300,
        &3000,
    &None);

// ── get_active_policies / get_total_monthly_premium ───────────────────────────

    let active = client.get_active_policies(&owner, &0, &100).items;
    assert_eq!(active.len(), 2);

#[test]
fn test_get_total_monthly_premium() {
    let (env, client, owner) = setup();
    client.create_policy(&owner, &String::from_str(&env, "P1"), &CoverageType::Health, &100, &1000);
    client.create_policy(&owner, &String::from_str(&env, "P2"), &CoverageType::Health, &200, &2000);
    assert_eq!(client.get_total_monthly_premium(&owner), 300);
}

#[test]
fn test_get_active_policies_excludes_deactivated() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

// ── add_tag: authorization ────────────────────────────────────────────────────

    // Create policy 1 and policy 2 for the same owner
    let policy_id1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);

    // Deactivate policy 1
    client.deactivate_policy(&owner, &policy_id1);

    // get_active_policies must return only the still-active policy
    let active = client.get_active_policies(&owner, &0, &100).items;
    assert_eq!(
        active.len(),
        1,
        "get_active_policies must return exactly one policy"
    );
    let only = active.get(0).unwrap();
    assert_eq!(
        only.id, policy_id2,
        "the returned policy must be the active one (policy_id2)"
    );
    assert!(only.active, "returned policy must have active == true");
}

/// Missing auth must fail.
#[test]
fn test_get_total_monthly_premium() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 300);
}

/// Tags on one policy must not appear on another.
#[test]
fn test_get_total_monthly_premium_zero_policies() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

// ── add_tag: events ───────────────────────────────────────────────────────────

/// add_tag must emit a tag_added event.
#[test]
fn test_add_tag_emits_event() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let before = env.events().all().len();
    client.add_tag(&owner, &id, &String::from_str(&env, "vip"));
    assert!(env.events().all().len() > before);
}

/// Duplicate add must NOT emit a tag_added event (nothing changed).
#[test]
fn test_get_total_monthly_premium_one_policy() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

// ── remove_tag: happy path ────────────────────────────────────────────────────

    // Create one policy with monthly_premium = 500
    client.create_policy(
        &owner,
        &String::from_str(&env, "Single Policy"),
        &CoverageType::Health,
        &500,
        &10000,
    &None);

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 500);
}

/// Removing all tags results in an empty list.
#[test]
fn test_get_total_monthly_premium_multiple_active_policies() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

// ── remove_tag: graceful on missing ──────────────────────────────────────────

    // Create three policies with premiums 100, 200, 300
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);
    client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 3"),
        &CoverageType::Auto,
        &300,
        &3000,
    &None);

    let total = client.get_total_monthly_premium(&owner);
    assert_eq!(total, 600); // 100 + 200 + 300
}

/// Removing a missing tag emits a "tag_no_tag" (Tag Not Found) event.
#[test]
fn test_get_total_monthly_premium_deactivated_policy_excluded() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();

    // Create two policies with premiums 100 and 200
    let policy1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    let policy2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Policy 2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);

    // Verify total includes both policies initially
    let total_initial = client.get_total_monthly_premium(&owner);
    assert_eq!(total_initial, 300); // 100 + 200

// ── remove_tag: authorization ─────────────────────────────────────────────────

/// A stranger cannot remove tags.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_remove_tag_by_stranger_panics() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    client.add_tag(&owner, &id, &String::from_str(&env, "vip"));
    let stranger = Address::generate(&env);
    client.remove_tag(&stranger, &id, &String::from_str(&env, "vip"));
}

/// Admin can remove tags from any policy.
#[test]
fn test_remove_tag_by_admin_succeeds() {
    let (env, client, owner) = setup();
    let admin = Address::generate(&env);
    client.set_admin(&admin, &admin);
    let id = make_policy(&env, &client, &owner);
    client.add_tag(&owner, &id, &String::from_str(&env, "vip"));
    client.remove_tag(&admin, &id, &String::from_str(&env, "vip"));
    assert_eq!(client.get_policy(&id).unwrap().tags.len(), 0);
}

// ── remove_tag: events ────────────────────────────────────────────────────────

    // Create policies for owner_a
    client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A1"),
        &CoverageType::Health,
        &100,
        &1000,
    &None);
    client.create_policy(
        &owner_a,
        &String::from_str(&env, "Policy A2"),
        &CoverageType::Life,
        &200,
        &2000,
    &None);

    // Create policies for owner_b
    client.create_policy(
        &owner_b,
        &String::from_str(&env, "Policy B1"),
        &CoverageType::Liability,
        &300,
        &3000,
    &None);

// ── 1. Unauthorized Access ────────────────────────────────────────────────────

/// A random address that is neither the policy owner nor the admin must cause
/// add_tag to panic with "unauthorized". State must be unchanged.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_qa_unauthorized_stranger_cannot_add_tag() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let random = Address::generate(&env);
    // random is not owner, no admin set — must panic
    client.add_tag(&random, &id, &String::from_str(&env, "ACTIVE"));
}

/// A random address must also be blocked from remove_tag.
#[test]
#[should_panic(expected = "unauthorized")]
fn test_qa_unauthorized_stranger_cannot_remove_tag() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    client.add_tag(&owner, &id, &String::from_str(&env, "ACTIVE"));
    let random = Address::generate(&env);
    client.remove_tag(&random, &id, &String::from_str(&env, "ACTIVE"));
}

/// After a failed unauthorized add_tag, the policy tags must remain empty —
/// no partial state mutation.
#[test]
fn test_multiple_premium_payments() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);

    // attempt unauthorized add — ignore the panic via try_
    let _ = client.try_add_tag(&random, &id, &String::from_str(&env, "ACTIVE"));

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "LongTerm"),
        &CoverageType::Life,
        &100,
        &10000,
    &None);

// ── 2. The Double-Tag ─────────────────────────────────────────────────────────

/// Adding "ACTIVE" twice must leave exactly one "ACTIVE" tag in storage.
#[test]
fn test_qa_double_tag_active_stored_once() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let active = String::from_str(&env, "ACTIVE");

    client.add_tag(&owner, &id, &active);
    client.add_tag(&owner, &id, &active); // duplicate

    let tags = client.get_policy(&id).unwrap().tags;
    assert_eq!(tags.len(), 1, "duplicate tag must not be stored twice");
    assert_eq!(
        tags.get(0).unwrap(),
        String::from_str(&env, "ACTIVE"),
        "the stored tag must be ACTIVE"
    );
}

/// The second (duplicate) add_tag call must emit NO new event — the contract
/// returns early before publishing.
#[test]
fn test_create_premium_schedule_succeeds() {
    setup_test_env!(env, Insurance, InsuranceClient, client, owner);
    client.initialize(&owner);
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
     &None);

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();

    assert_eq!(schedule.next_due, 3000);
    assert_eq!(schedule.interval, 2592000);
    assert!(schedule.active);
}

/// Adding "ACTIVE" then a different tag then "ACTIVE" again must still result
/// in exactly two unique tags.
#[test]
fn test_modify_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    client.add_tag(&owner, &id, &String::from_str(&env, "ACTIVE"));
    client.add_tag(&owner, &id, &String::from_str(&env, "VIP"));
    client.add_tag(&owner, &id, &String::from_str(&env, "ACTIVE")); // dup

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

// ── 3. The Ghost Remove ───────────────────────────────────────────────────────

/// Removing a tag that was never added must not crash.
#[test]
fn test_qa_ghost_remove_does_not_panic() {
    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    // no tags — removing "GHOST" must be graceful
    client.remove_tag(&owner, &id, &String::from_str(&env, "GHOST"));
}

/// After a ghost remove the tag list must still be empty.
#[test]
fn test_cancel_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

    let schedule_id = client.create_premium_schedule(&owner, &policy_id, &3000, &2592000);
    client.cancel_premium_schedule(&owner, &schedule_id);

    let schedule = client.get_premium_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

/// Ghost remove on a policy that already has other tags must not disturb them.
#[test]
fn test_execute_due_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

/// add_tag must publish exactly one event with topic ("insure", "tag_added")
/// and data (policy_id, tag).
#[test]
fn test_qa_add_tag_event_topics_and_data() {
    use soroban_sdk::{symbol_short, IntoVal};

    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let tag = String::from_str(&env, "ACTIVE");

    assert_eq!(executed.len(), 1);
    assert_eq!(executed.get(0), Some(schedule_id));

    let all = env.events().all();
    assert_eq!(
        all.len(),
        events_before + 1,
        "add_tag must emit exactly one event"
    );

    let (contract_id, topics, data) = all.last().unwrap();
    let _ = contract_id; // emitted by our contract

    // Verify topics: ("insure", "tag_added")
    let expected_topics = soroban_sdk::vec![
        &env,
        symbol_short!("insure").into_val(&env),
        symbol_short!("tag_added").into_val(&env),
    ];
    assert_eq!(topics, expected_topics, "tag_added event topics mismatch");

    // Verify data: (policy_id, tag)
    let (emitted_id, emitted_tag): (u32, String) =
        soroban_sdk::FromVal::from_val(&env, &data);
    assert_eq!(emitted_id, id, "tag_added event must carry the correct policy_id");
    assert_eq!(emitted_tag, tag, "tag_added event must carry the correct tag");
}

/// remove_tag on an existing tag must publish exactly one event with topic
/// ("insure", "tag_rmvd") and data (policy_id, tag).
#[test]
fn test_execute_recurring_premium_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let tag = String::from_str(&env, "ACTIVE");
    client.add_tag(&owner, &id, &tag);

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

    let (_, topics, data) = all.last().unwrap();

    let expected_topics = soroban_sdk::vec![
        &env,
        symbol_short!("insure").into_val(&env),
        symbol_short!("tag_rmvd").into_val(&env),
    ];
    assert_eq!(topics, expected_topics, "tag_rmvd event topics mismatch");

    let (emitted_id, emitted_tag): (u32, String) =
        soroban_sdk::FromVal::from_val(&env, &data);
    assert_eq!(emitted_id, id);
    assert_eq!(emitted_tag, tag);
}

/// Ghost remove must publish exactly one event with topic ("insure", "tag_miss")
/// and data (policy_id, tag) — the "Tag Not Found" signal.
#[test]
fn test_execute_missed_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    let (env, client, owner) = setup();
    let id = make_policy(&env, &client, &owner);
    let tag = String::from_str(&env, "GHOST");

    let policy_id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

    let (_, topics, data) = all.last().unwrap();

    set_ledger_time(&env, 1, 3000 + 2592000 * 3 + 100);
    client.execute_due_premium_schedules();

    let (emitted_id, emitted_tag): (u32, String) =
        soroban_sdk::FromVal::from_val(&env, &data);
    assert_eq!(emitted_id, id, "tag_miss event must carry the correct policy_id");
    assert_eq!(emitted_tag, tag, "tag_miss event must carry the correct tag");
}

/// Full lifecycle: add "ACTIVE", add "ACTIVE" again (dup), remove "ACTIVE",
/// remove "ACTIVE" again (ghost). Verify the exact event sequence.
#[test]
fn test_get_premium_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    client.initialize(&owner);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    let policy_id1 = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &500,
        &50000,
    &None);

    let policy_id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Insurance"),
        &CoverageType::Life,
        &300,
        &100000,
    &None);

    client.create_premium_schedule(&owner, &policy_id1, &3000, &2592000);
    client.create_premium_schedule(&owner, &policy_id2, &4000, &2592000);

    let schedules = client.get_premium_schedules(&owner);
    assert_eq!(schedules.len(), 2);
}

// -----------------------------------------------------------------------
// 3. create_policy — boundary conditions
// -----------------------------------------------------------------------

// --- Health min/max boundaries ---

#[test]
fn test_health_premium_at_minimum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // min_premium for Health = 1_000_000
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &1_000_000i128,
        &10_000_000i128, // min coverage
        &None,
    );
}

#[test]
fn test_health_premium_at_maximum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // max_premium = 500_000_000; need coverage ≤ 500M * 12 * 500 = 3T (within 100B limit)
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &500_000_000i128,
        &100_000_000_000i128, // max coverage for Health
        &None,
    );
}

#[test]
fn test_health_coverage_at_minimum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &10_000_000i128, // exactly min_coverage
        &None,
    );
}

#[test]
fn test_health_coverage_at_maximum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // max_coverage = 100_000_000_000; need premium ≥ 100B / (12*500) ≈ 16_666_667
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &500_000_000i128,       // max premium to allow max coverage via ratio
        &100_000_000_000i128,   // exactly max_coverage
        &None,
    );
}

// --- Life boundaries ---

#[test]
fn test_life_premium_at_minimum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    client.create_policy(
        &caller,
        &String::from_str(&env, "Life Min"),
        &CoverageType::Life,
        &500_000i128,     // min_premium
        &50_000_000i128,  // min_coverage
        &None,
    );
}

#[test]
fn test_liability_premium_at_minimum_boundary() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    client.create_policy(
        &caller,
        &String::from_str(&env, "Liability Min"),
        &CoverageType::Liability,
        &800_000i128,     // min_premium
        &5_000_000i128,   // min_coverage
        &None,
    );
}

// -----------------------------------------------------------------------
// 4. create_policy — name validation
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_empty_name_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, ""),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidName)));}

#[test]
fn test_create_policy_name_exceeds_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // 65 character name — exceeds MAX_NAME_LEN (64)
    let long_name = String::from_str(
        &env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
    );
    let result = client.try_create_policy(
        &caller,
        &long_name,
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidName)));}

#[test]
fn test_create_policy_name_at_max_length_succeeds() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Exactly 64 characters
    let max_name = String::from_str(
        &env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    client.create_policy(
        &caller,
        &max_name,
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
}

// -----------------------------------------------------------------------
// 5. create_policy — premium validation failures
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_zero_premium_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &0i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_policy_negative_premium_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &-1i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_health_policy_premium_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Health min_premium = 1_000_000; supply 999_999
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &999_999i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_health_policy_premium_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Health max_premium = 500_000_000; supply 500_000_001
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &500_000_001i128,
        &10_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_life_policy_premium_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Life min_premium = 500_000; supply 499_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Life"),
        &CoverageType::Life,
        &499_999i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_property_policy_premium_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Property min_premium = 2_000_000; supply 1_999_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Property"),
        &CoverageType::Property,
        &1_999_999i128,
        &100_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_auto_policy_premium_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Auto min_premium = 1_500_000; supply 1_499_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Auto"),
        &CoverageType::Auto,
        &1_499_999i128,
        &20_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_liability_policy_premium_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Liability min_premium = 800_000; supply 799_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Liability"),
        &CoverageType::Liability,
        &799_999i128,
        &5_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

// -----------------------------------------------------------------------
// 6. create_policy — coverage amount validation failures
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_zero_coverage_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &0i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_policy_negative_coverage_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &-1i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_health_policy_coverage_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Health min_coverage = 10_000_000; supply 9_999_999
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &9_999_999i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_health_policy_coverage_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Health max_coverage = 100_000_000_000; supply 100_000_000_001
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &500_000_000i128,
        &100_000_000_001i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_life_policy_coverage_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Life min_coverage = 50_000_000; supply 49_999_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Life"),
        &CoverageType::Life,
        &1_000_000i128,
        &49_999_999i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_property_policy_coverage_below_min_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Property min_coverage = 100_000_000; supply 99_999_999
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Property"),
        &CoverageType::Property,
        &5_000_000i128,
        &99_999_999i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

// -----------------------------------------------------------------------
// 7. create_policy — ratio guard (unsupported combination)
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_coverage_too_high_for_premium_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // premium = 1_000_000 → annual = 12_000_000 → max_coverage = 6_000_000_000
    // supply coverage = 6_000_000_001 (just over the ratio limit, but within Health's hard max)
    // Need premium high enough so health range isn't hit, but ratio is
    // Health max_coverage = 100_000_000_000
    // Use premium = 1_000_000, coverage = 7_000_000_000 → over ratio (6B), under hard cap (100B)
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &1_000_000i128,
        &7_000_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_create_policy_coverage_exactly_at_ratio_limit_succeeds() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // premium = 1_000_000 → ratio limit = 1M * 12 * 500 = 6_000_000_000
    // Health max_coverage = 100B, so 6B is fine
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &1_000_000i128,
        &6_000_000_000i128,
        &None,
    );
}

// -----------------------------------------------------------------------
// 8. External ref validation
// -----------------------------------------------------------------------

#[test]
fn test_create_policy_ext_ref_too_long_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // 129 character external ref — exceeds MAX_EXT_REF_LEN (128)
    let long_ref = String::from_str(
        &env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
    );
    let result = client.try_create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &Some(long_ref),
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidName)));}

#[test]
fn test_create_policy_ext_ref_at_max_length_succeeds() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Exactly 128 characters
    let max_ref = String::from_str(
        &env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &Some(max_ref),
    );
}

// -----------------------------------------------------------------------
// 9. pay_premium — happy path
// -----------------------------------------------------------------------

#[test]
fn test_pay_premium_success() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.pay_premium(&caller, &policy_id);

}

#[test]
fn test_pay_premium_updates_next_payment_date() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    env.ledger().set_timestamp(1_000_000u64);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    env.ledger().set_timestamp(2_000_000u64);
    client.pay_premium(&caller, &policy_id);
    let policy = client.get_policy(&policy_id).unwrap();
    // next_payment_date should be 2_000_000 + 30 days
    assert_eq!(policy.next_payment_date, 2_000_000 + 30 * 24 * 60 * 60);
}

// -----------------------------------------------------------------------
// 10. pay_premium — failure cases
// -----------------------------------------------------------------------

#[test]
fn test_pay_premium_nonexistent_policy_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let result = client.try_pay_premium(&caller, &999u32);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyNotFound)));}

#[test]
fn test_pay_premium_wrong_amount_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.pay_premium(&caller, &policy_id);}

#[test]
fn test_pay_premium_on_inactive_policy_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.deactivate_policy(&owner, &policy_id);
    client.pay_premium(&caller, &policy_id);}

// -----------------------------------------------------------------------
// 11. deactivate_policy — happy path
// -----------------------------------------------------------------------

#[test]
fn test_deactivate_policy_success() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    let result = client.deactivate_policy(&owner, &policy_id);


    let policy = client.get_policy(&policy_id).unwrap();
    assert!(!policy.active);
}

#[test]
fn test_deactivate_removes_from_active_list() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    assert_eq!(client.get_active_policies(&owner, &0, &100).items.len(), 1);
    client.deactivate_policy(&owner, &policy_id);
    assert_eq!(client.get_active_policies(&owner, &0, &100).items.len(), 0);
}

// -----------------------------------------------------------------------
// 12. deactivate_policy — failure cases
// -----------------------------------------------------------------------

#[test]
fn test_deactivate_policy_non_owner_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    let non_owner = Address::generate(&env);
    client.deactivate_policy(&non_owner, &policy_id);}

#[test]
fn test_deactivate_nonexistent_policy_panics() {
    let (env, client, owner) = setup();
    let result = client.try_deactivate_policy(&owner, &999u32);
    assert_eq!(result, Err(Ok(InsuranceError::PolicyNotFound)));}

#[test]
fn test_deactivate_already_inactive_policy_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.deactivate_policy(&owner, &policy_id);
    // Second deactivation must panic
    client.deactivate_policy(&owner, &policy_id);}

// -----------------------------------------------------------------------
// 13. set_external_ref
// -----------------------------------------------------------------------

#[test]
fn test_set_external_ref_success() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    let new_ref = String::from_str(&env, "NEW-REF-001");
    client.set_external_ref(&owner, &policy_id, &Some(new_ref));
    let policy = client.get_policy(&policy_id).unwrap();
}

#[test]
fn test_set_external_ref_clear() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let ext_ref = String::from_str(&env, "INITIAL-REF");
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &Some(ext_ref),
    );
    // Clear the ref
    client.set_external_ref(&owner, &policy_id, &None);
    let policy = client.get_policy(&policy_id).unwrap();
    assert!(policy.external_ref.is_none());
}

#[test]
fn test_set_external_ref_non_owner_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    let non_owner = Address::generate(&env);
    let new_ref = String::from_str(&env, "HACK");
    client.set_external_ref(&non_owner, &policy_id, &Some(new_ref));}

#[test]
fn test_set_external_ref_too_long_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    let long_ref = String::from_str(
        &env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
    );
    client.set_external_ref(&owner, &policy_id, &Some(long_ref));}

// -----------------------------------------------------------------------
// 14. Queries
// -----------------------------------------------------------------------

#[test]
fn test_get_active_policies_empty_initially() {
    let (env, client, owner) = setup();
    assert_eq!(client.get_active_policies(&owner, &0, &100).items.len(), 0);
}

#[test]
fn test_get_active_policies_reflects_creates_and_deactivations() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id1 = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.create_policy(
        &caller,
        &String::from_str(&env, "Second Policy"),
        &CoverageType::Life,
        &1_000_000i128,
        &60_000_000i128,
        &None,
    );
    assert_eq!(client.get_active_policies(&owner, &0, &100).items.len(), 2);
    client.deactivate_policy(&owner, &policy_id1);
    assert_eq!(client.get_active_policies(&owner, &0, &100).items.len(), 1);
}

#[test]
fn test_get_total_monthly_premium_sums_active_only() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    let policy_id1 = client.create_policy(
        &caller,
        &short_name(&env).unwrap(),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
    client.create_policy(
        &caller,
        &String::from_str(&env, "Second"),
        &CoverageType::Life,
        &1_000_000i128,
        &60_000_000i128,
        &None,
    );
    assert_eq!(client.get_total_monthly_premium(&caller), 6_000_000i128);
    client.deactivate_policy(&owner, &policy_id1);
    assert_eq!(client.get_total_monthly_premium(&caller), 1_000_000i128);
}

#[test]
fn test_get_total_monthly_premium_zero_when_no_policies() {
    let (env, client, owner) = setup();
    assert_eq!(client.get_total_monthly_premium(&owner), 0i128);
}

#[test]
fn test_get_policy_nonexistent_panics() {
    let (env, client, owner) = setup();
    client.get_policy(&999u32).unwrap();
}

// -----------------------------------------------------------------------
// 15. Uninitialized contract guard
// -----------------------------------------------------------------------

#[test]
#[should_panic(expected = "not initialized")]
fn test_create_policy_without_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    client.create_policy(
        &caller,
        &String::from_str(&env, "Test"),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    );
}

#[test]
#[should_panic(expected = "not initialized")]
fn test_get_active_policies_without_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    client.initialize(&owner);
    client.get_active_policies(&owner, &0, &100).items;
}

// -----------------------------------------------------------------------
// 16. Policy data integrity
// -----------------------------------------------------------------------

#[test]
fn test_policy_fields_stored_correctly() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    env.ledger().set_timestamp(1_700_000_000u64);
    let policy_id = client.create_policy(
        &caller,
        &String::from_str(&env, "My Health Plan"),
        &CoverageType::Health,
        &10_000_000i128,
        &100_000_000i128,
        &Some(String::from_str(&env, "EXT-001")),
    );
    let policy = client.get_policy(&policy_id).unwrap();
    assert_eq!(policy.id, 1u32);
    assert_eq!(policy.monthly_premium, 10_000_000i128);
    assert_eq!(policy.coverage_amount, 100_000_000i128);
    assert!(policy.active);
    assert_eq!(
        policy.next_payment_date,
        1_700_000_000u64 + 30 * 24 * 60 * 60
    );
}

// -----------------------------------------------------------------------
// 17. Cross-coverage-type boundary checks
// -----------------------------------------------------------------------

#[test]
fn test_property_premium_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Property max_premium = 2_000_000_000; supply 2_000_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Property"),
        &CoverageType::Property,
        &2_000_000_001i128,
        &100_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_auto_premium_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Auto max_premium = 750_000_000; supply 750_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Auto"),
        &CoverageType::Auto,
        &750_000_001i128,
        &20_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_liability_premium_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Liability max_premium = 400_000_000; supply 400_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Liability"),
        &CoverageType::Liability,
        &400_000_001i128,
        &5_000_000i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_life_coverage_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Life max_coverage = 500_000_000_000; supply 500_000_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Life"),
        &CoverageType::Life,
        &1_000_000_000i128, // max premium for Life
        &500_000_000_001i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_auto_coverage_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Auto max_coverage = 200_000_000_000; supply 200_000_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Auto"),
        &CoverageType::Auto,
        &750_000_000i128,
        &200_000_000_001i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}

#[test]
fn test_liability_coverage_above_max_panics() {
    let (env, client, owner) = setup();
    let caller = Address::generate(&env);
    // Liability max_coverage = 50_000_000_000; supply 50_000_000_001
    let result = client.try_create_policy(
        &caller,
        &String::from_str(&env, "Liability"),
        &CoverageType::Liability,
        &400_000_000i128,
        &50_000_000_001i128,
        &None,
    );
    assert_eq!(result, Err(Ok(InsuranceError::InvalidAmount)));}
