#![cfg(test)]

use super::*;
use remitwise_common::{EventCategory, EventPriority};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, String, TryFromVal, Val, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, InsuranceClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_pause_admin(&admin, &admin);
    (env, client, admin)
}

fn create_health_policy(env: &Env, client: &InsuranceClient, owner: &Address) -> u32 {
    client.create_policy(
        owner,
        &String::from_str(env, "Health Plan"),
        &CoverageType::Health,
        &1_000i128,
        &10_000i128,
        &None,
    )
}

/// Return all events whose namespace topic is "Remitwise" and action topic matches `action`.
fn insurance_events_for(
    env: &Env,
    action: soroban_sdk::Symbol,
) -> SorobanVec<(Address, SorobanVec<Val>, Val)> {
    let mut result = SorobanVec::new(env);
    for event in env.events().all().iter() {
        let topics = &event.1;
        if topics.len() >= 4 {
            if let Ok(ns) = soroban_sdk::Symbol::try_from_val(env, &topics.get(0).unwrap()) {
                if let Ok(act) = soroban_sdk::Symbol::try_from_val(env, &topics.get(3).unwrap()) {
                    if ns == symbol_short!("Remitwise") && act == action {
                        result.push_back(event);
                    }
                }
            }
        }
    }
    result
}

/// Decode topic[i] as a Symbol and assert it equals `expected`.
fn assert_topic_sym(
    env: &Env,
    topics: &SorobanVec<Val>,
    i: u32,
    expected: soroban_sdk::Symbol,
    label: &str,
) {
    let actual = soroban_sdk::Symbol::try_from_val(env, &topics.get(i).unwrap())
        .unwrap_or_else(|_| panic!("{label}: topic[{i}] is not a Symbol"));
    assert_eq!(actual, expected, "{label}: topic[{i}] value mismatch");
}

/// Decode topic[i] as a u32 and assert it equals `expected`.
fn assert_topic_u32(env: &Env, topics: &SorobanVec<Val>, i: u32, expected: u32, label: &str) {
    let actual = u32::try_from_val(env, &topics.get(i).unwrap())
        .unwrap_or_else(|_| panic!("{label}: topic[{i}] is not a u32"));
    assert_eq!(actual, expected, "{label}: topic[{i}] value mismatch");
}

// ---------------------------------------------------------------------------
// create_policy — functional tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_policy_returns_id_starting_at_one() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert_eq!(id, 1);
}

#[test]
fn test_create_policy_increments_id() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id1 = create_health_policy(&env, &client, &owner);
    let id2 = create_health_policy(&env, &client, &owner);
    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
}

#[test]
fn test_create_policy_stores_fields_correctly() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);

    let ext_ref = String::from_str(&env, "EXT-001");
    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "Life Cover"),
        &CoverageType::Life,
        &500i128,
        &5_000i128,
        &Some(ext_ref.clone()),
    );

    let policy = client.get_policy(&id).unwrap();
    assert_eq!(policy.id, id);
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.coverage_type, CoverageType::Life);
    assert_eq!(policy.monthly_premium, 500i128);
    assert_eq!(policy.coverage_amount, 5_000i128);
    assert!(policy.active);
    assert_eq!(policy.external_ref, Some(ext_ref));
    assert_eq!(policy.next_payment_date, 1_000_000 + 30 * 86_400);
}

#[test]
fn test_create_policy_without_external_ref() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    let policy = client.get_policy(&id).unwrap();
    assert!(policy.external_ref.is_none());
}

#[test]
fn test_get_policy_returns_none_for_unknown_id() {
    let (_env, client, _) = setup();
    assert!(client.get_policy(&999u32).is_none());
}

// ---------------------------------------------------------------------------
// pay_premium — functional tests
// ---------------------------------------------------------------------------

#[test]
fn test_pay_premium_returns_true_on_success() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert!(client.pay_premium(&owner, &id));
}

#[test]
fn test_pay_premium_advances_next_payment_date() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 1_000_000);
    let id = create_health_policy(&env, &client, &owner);

    env.ledger().with_mut(|li| li.timestamp = 2_000_000);
    client.pay_premium(&owner, &id);

    let policy = client.get_policy(&id).unwrap();
    assert_eq!(policy.next_payment_date, 2_000_000 + 30 * 86_400);
}

#[test]
fn test_pay_premium_returns_false_for_unknown_policy() {
    let (_env, client, _) = setup();
    let owner = Address::generate(&_env);
    assert!(!client.pay_premium(&owner, &999u32));
}

#[test]
fn test_pay_premium_returns_false_for_inactive_policy() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    client.deactivate_policy(&owner, &id);
    assert!(!client.pay_premium(&owner, &id));
}

#[test]
fn test_pay_premium_returns_false_for_wrong_caller() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert!(!client.pay_premium(&other, &id));
}

// ---------------------------------------------------------------------------
// deactivate_policy — functional tests
// ---------------------------------------------------------------------------

#[test]
fn test_deactivate_policy_sets_active_false() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    client.deactivate_policy(&owner, &id);
    let policy = client.get_policy(&id).unwrap();
    assert!(!policy.active);
}

#[test]
fn test_deactivate_policy_returns_true_on_success() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert!(client.deactivate_policy(&owner, &id));
}

#[test]
fn test_deactivate_policy_returns_false_for_unknown_policy() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    assert!(!client.deactivate_policy(&owner, &999u32));
}

#[test]
fn test_deactivate_policy_returns_false_for_wrong_caller() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert!(!client.deactivate_policy(&other, &id));
}

#[test]
fn test_deactivate_policy_removes_from_active_page() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert_eq!(client.get_active_policies(&owner, &0, &50).count, 1);
    client.deactivate_policy(&owner, &id);
    assert_eq!(client.get_active_policies(&owner, &0, &50).count, 0);
}

// ---------------------------------------------------------------------------
// set_external_ref — functional tests
// ---------------------------------------------------------------------------

#[test]
fn test_set_external_ref_updates_value() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);

    let new_ref = String::from_str(&env, "INSURER-XYZ-007");
    assert!(client.set_external_ref(&owner, &id, &Some(new_ref.clone())));

    let policy = client.get_policy(&id).unwrap();
    assert_eq!(policy.external_ref, Some(new_ref));
}

#[test]
fn test_set_external_ref_clears_value() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let ext_ref = String::from_str(&env, "INITIAL-REF");
    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Plan"),
        &CoverageType::Health,
        &1_000i128,
        &10_000i128,
        &Some(ext_ref),
    );

    client.set_external_ref(&owner, &id, &None);
    let policy = client.get_policy(&id).unwrap();
    assert!(policy.external_ref.is_none());
}

#[test]
fn test_set_external_ref_returns_false_for_unknown_policy() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let r = String::from_str(&env, "REF");
    assert!(!client.set_external_ref(&owner, &999u32, &Some(r)));
}

#[test]
fn test_set_external_ref_returns_false_for_wrong_caller() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    let r = String::from_str(&env, "HACK");
    assert!(!client.set_external_ref(&other, &id, &Some(r)));
}

// ---------------------------------------------------------------------------
// batch_pay_premiums — functional tests
// ---------------------------------------------------------------------------

#[test]
fn test_batch_pay_premiums_pays_all_active_owned() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id1 = create_health_policy(&env, &client, &owner);
    let id2 = create_health_policy(&env, &client, &owner);
    let ids = soroban_sdk::vec![&env, id1, id2];
    assert_eq!(client.batch_pay_premiums(&owner, &ids), 2);
}

#[test]
fn test_batch_pay_premiums_skips_inactive() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id1 = create_health_policy(&env, &client, &owner);
    let id2 = create_health_policy(&env, &client, &owner);
    client.deactivate_policy(&owner, &id2);
    let ids = soroban_sdk::vec![&env, id1, id2];
    assert_eq!(client.batch_pay_premiums(&owner, &ids), 1);
}

// ---------------------------------------------------------------------------
// get_active_policies — pagination tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_active_policies_empty_initially() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let page = client.get_active_policies(&owner, &0, &10);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_get_active_policies_returns_single_policy() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    let page = client.get_active_policies(&owner, &0, &10);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().id, id);
}

#[test]
fn test_get_active_policies_isolates_by_owner() {
    let (env, client, _) = setup();
    let owner1 = Address::generate(&env);
    let owner2 = Address::generate(&env);
    create_health_policy(&env, &client, &owner1);
    create_health_policy(&env, &client, &owner2);
    assert_eq!(client.get_active_policies(&owner1, &0, &50).count, 1);
    assert_eq!(client.get_active_policies(&owner2, &0, &50).count, 1);
}

#[test]
fn test_get_active_policies_pagination_cursor() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    for _ in 0..5 {
        create_health_policy(&env, &client, &owner);
    }
    let page1 = client.get_active_policies(&owner, &0, &3);
    assert_eq!(page1.count, 3);
    assert_ne!(page1.next_cursor, 0);

    let page2 = client.get_active_policies(&owner, &page1.next_cursor, &3);
    assert_eq!(page2.count, 2);
    assert_eq!(page2.next_cursor, 0);
}

#[test]
fn test_get_active_policies_zero_limit_uses_default() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    create_health_policy(&env, &client, &owner);
    // limit=0 should use DEFAULT_PAGE_LIMIT, not crash
    let page = client.get_active_policies(&owner, &0, &0);
    assert_eq!(page.count, 1);
}

// ---------------------------------------------------------------------------
// get_total_monthly_premium — tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_total_monthly_premium_sums_active_policies() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    client.create_policy(
        &owner,
        &String::from_str(&env, "A"),
        &CoverageType::Health,
        &300i128,
        &3_000i128,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "B"),
        &CoverageType::Life,
        &700i128,
        &7_000i128,
        &None,
    );
    assert_eq!(client.get_total_monthly_premium(&owner), 1_000i128);
}

#[test]
fn test_get_total_monthly_premium_excludes_inactive() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id1 = client.create_policy(
        &owner,
        &String::from_str(&env, "A"),
        &CoverageType::Health,
        &300i128,
        &3_000i128,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "B"),
        &CoverageType::Life,
        &700i128,
        &7_000i128,
        &None,
    );
    client.deactivate_policy(&owner, &id1);
    assert_eq!(client.get_total_monthly_premium(&owner), 700i128);
}

#[test]
fn test_get_total_monthly_premium_zero_with_no_policies() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    assert_eq!(client.get_total_monthly_premium(&owner), 0i128);
}

// ---------------------------------------------------------------------------
// Event schema stability tests
//
// These tests lock the topic schema and payload struct shapes.
// A change to any topic value or payload field name/type MUST break these tests,
// ensuring indexers are never silently broken by a contract update.
// ---------------------------------------------------------------------------

/// Event category/priority numeric values must not change.
#[test]
fn test_event_category_priority_discriminants_are_stable() {
    assert_eq!(
        EventCategory::Transaction as u32,
        0,
        "Transaction category moved"
    );
    assert_eq!(EventCategory::State as u32, 1, "State category moved");
    assert_eq!(EventPriority::Low as u32, 0, "Low priority moved");
    assert_eq!(EventPriority::Medium as u32, 1, "Medium priority moved");
}

/// The action symbols used as topic[3] must not be renamed.
#[test]
fn test_event_action_symbols_are_stable() {
    assert_eq!(EVT_POLICY_CREATED, symbol_short!("created"));
    assert_eq!(EVT_PREMIUM_PAID, symbol_short!("paid"));
    assert_eq!(EVT_POLICY_DEACTIVATED, symbol_short!("deactive"));
    assert_eq!(EVT_EXT_REF_UPDATED, symbol_short!("ext_ref"));
}

/// PolicyCreatedEvent: verify exact 4-part topic schema and all payload fields.
#[test]
fn test_policy_created_event_schema() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 500_000u64);

    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "Health Plan"),
        &CoverageType::Health,
        &1_000i128,
        &10_000i128,
        &None,
    );

    let events = insurance_events_for(&env, EVT_POLICY_CREATED);
    assert_eq!(events.len(), 1, "expected exactly one PolicyCreated event");
    let event = events.get(0).unwrap();
    let topics = event.1.clone();

    // Topic schema: (Remitwise, Transaction=0, Medium=1, "created")
    assert_topic_sym(
        &env,
        &topics,
        0,
        symbol_short!("Remitwise"),
        "PolicyCreated",
    );
    assert_topic_u32(
        &env,
        &topics,
        1,
        EventCategory::Transaction as u32,
        "PolicyCreated",
    );
    assert_topic_u32(
        &env,
        &topics,
        2,
        EventPriority::Medium as u32,
        "PolicyCreated",
    );
    assert_topic_sym(&env, &topics, 3, symbol_short!("created"), "PolicyCreated");

    // Payload: decode as PolicyCreatedEvent and verify every field
    let data: PolicyCreatedEvent = PolicyCreatedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.policy_id, id, "payload.policy_id mismatch");
    assert_eq!(data.owner, owner, "payload.owner mismatch");
    assert_eq!(
        data.coverage_type,
        CoverageType::Health,
        "payload.coverage_type mismatch"
    );
    assert_eq!(
        data.monthly_premium, 1_000i128,
        "payload.monthly_premium mismatch"
    );
    assert_eq!(
        data.coverage_amount, 10_000i128,
        "payload.coverage_amount mismatch"
    );
    assert_eq!(data.timestamp, 500_000u64, "payload.timestamp mismatch");
}

/// PremiumPaidEvent: verify exact 4-part topic schema and all payload fields.
#[test]
fn test_premium_paid_event_schema() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 1_000_000u64);
    let id = create_health_policy(&env, &client, &owner);

    env.ledger().with_mut(|li| li.timestamp = 2_000_000u64);
    client.pay_premium(&owner, &id);

    let events = insurance_events_for(&env, EVT_PREMIUM_PAID);
    assert_eq!(events.len(), 1, "expected exactly one PremiumPaid event");
    let event = events.get(0).unwrap();
    let topics = event.1.clone();

    // Topic schema: (Remitwise, Transaction=0, Low=0, "paid")
    assert_topic_sym(&env, &topics, 0, symbol_short!("Remitwise"), "PremiumPaid");
    assert_topic_u32(
        &env,
        &topics,
        1,
        EventCategory::Transaction as u32,
        "PremiumPaid",
    );
    assert_topic_u32(&env, &topics, 2, EventPriority::Low as u32, "PremiumPaid");
    assert_topic_sym(&env, &topics, 3, symbol_short!("paid"), "PremiumPaid");

    // Payload: decode and verify all fields
    let data: PremiumPaidEvent = PremiumPaidEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.policy_id, id, "payload.policy_id mismatch");
    assert_eq!(data.owner, owner, "payload.owner mismatch");
    assert_eq!(data.amount, 1_000i128, "payload.amount mismatch");
    assert_eq!(
        data.next_payment_date,
        2_000_000 + 30 * 86_400,
        "payload.next_payment_date mismatch"
    );
    assert_eq!(data.timestamp, 2_000_000u64, "payload.timestamp mismatch");
}

/// PolicyDeactivatedEvent: verify exact 4-part topic schema and all payload fields.
#[test]
fn test_policy_deactivated_event_schema() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 3_000_000u64);
    let id = create_health_policy(&env, &client, &owner);

    env.ledger().with_mut(|li| li.timestamp = 4_000_000u64);
    client.deactivate_policy(&owner, &id);

    let events = insurance_events_for(&env, EVT_POLICY_DEACTIVATED);
    assert_eq!(
        events.len(),
        1,
        "expected exactly one PolicyDeactivated event"
    );
    let event = events.get(0).unwrap();
    let topics = event.1.clone();

    // Topic schema: (Remitwise, State=1, Medium=1, "deactive")
    assert_topic_sym(
        &env,
        &topics,
        0,
        symbol_short!("Remitwise"),
        "PolicyDeactivated",
    );
    assert_topic_u32(
        &env,
        &topics,
        1,
        EventCategory::State as u32,
        "PolicyDeactivated",
    );
    assert_topic_u32(
        &env,
        &topics,
        2,
        EventPriority::Medium as u32,
        "PolicyDeactivated",
    );
    assert_topic_sym(
        &env,
        &topics,
        3,
        symbol_short!("deactive"),
        "PolicyDeactivated",
    );

    // Payload: decode and verify all fields
    let data: PolicyDeactivatedEvent =
        PolicyDeactivatedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.policy_id, id, "payload.policy_id mismatch");
    assert_eq!(data.owner, owner, "payload.owner mismatch");
    assert_eq!(data.timestamp, 4_000_000u64, "payload.timestamp mismatch");
}

/// ExternalRefUpdatedEvent: verify exact 4-part topic schema and all payload fields.
#[test]
fn test_external_ref_updated_event_schema() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 5_000_000u64);
    let id = create_health_policy(&env, &client, &owner);

    let new_ref = String::from_str(&env, "INSURER-XYZ-007");
    env.ledger().with_mut(|li| li.timestamp = 6_000_000u64);
    client.set_external_ref(&owner, &id, &Some(new_ref.clone()));

    let events = insurance_events_for(&env, EVT_EXT_REF_UPDATED);
    assert_eq!(
        events.len(),
        1,
        "expected exactly one ExternalRefUpdated event"
    );
    let event = events.get(0).unwrap();
    let topics = event.1.clone();

    // Topic schema: (Remitwise, State=1, Low=0, "ext_ref")
    assert_topic_sym(
        &env,
        &topics,
        0,
        symbol_short!("Remitwise"),
        "ExternalRefUpdated",
    );
    assert_topic_u32(
        &env,
        &topics,
        1,
        EventCategory::State as u32,
        "ExternalRefUpdated",
    );
    assert_topic_u32(
        &env,
        &topics,
        2,
        EventPriority::Low as u32,
        "ExternalRefUpdated",
    );
    assert_topic_sym(
        &env,
        &topics,
        3,
        symbol_short!("ext_ref"),
        "ExternalRefUpdated",
    );

    // Payload: decode and verify all fields
    let data: ExternalRefUpdatedEvent =
        ExternalRefUpdatedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.policy_id, id, "payload.policy_id mismatch");
    assert_eq!(data.owner, owner, "payload.owner mismatch");
    assert_eq!(
        data.external_ref,
        Some(new_ref),
        "payload.external_ref mismatch"
    );
    assert_eq!(data.timestamp, 6_000_000u64, "payload.timestamp mismatch");
}

/// ExternalRefUpdated with None: verify payload carries None correctly.
#[test]
fn test_external_ref_updated_event_schema_none_value() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let ext_ref = String::from_str(&env, "INITIAL");
    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "Plan"),
        &CoverageType::Health,
        &1_000i128,
        &10_000i128,
        &Some(ext_ref),
    );

    client.set_external_ref(&owner, &id, &None);

    let events = insurance_events_for(&env, EVT_EXT_REF_UPDATED);
    assert_eq!(events.len(), 1);
    let data: ExternalRefUpdatedEvent =
        ExternalRefUpdatedEvent::try_from_val(&env, &events.get(0).unwrap().2).unwrap();
    assert!(
        data.external_ref.is_none(),
        "clearing must emit None in payload"
    );
}

/// Each lifecycle operation emits exactly one Remitwise-namespaced event.
#[test]
fn test_each_lifecycle_emits_exactly_one_remitwise_event() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);
    assert_eq!(
        insurance_events_for(&env, EVT_POLICY_CREATED).len(),
        1,
        "create_policy must emit exactly one event"
    );

    client.pay_premium(&owner, &id);
    assert_eq!(
        insurance_events_for(&env, EVT_PREMIUM_PAID).len(),
        1,
        "pay_premium must emit exactly one event"
    );

    client.set_external_ref(&owner, &id, &Some(String::from_str(&env, "REF")));
    assert_eq!(
        insurance_events_for(&env, EVT_EXT_REF_UPDATED).len(),
        1,
        "set_external_ref must emit exactly one event"
    );

    client.deactivate_policy(&owner, &id);
    assert_eq!(
        insurance_events_for(&env, EVT_POLICY_DEACTIVATED).len(),
        1,
        "deactivate_policy must emit exactly one event"
    );
}

/// No event is emitted when create_policy, pay_premium, deactivate, or set_external_ref
/// return false (guard conditions met — wrong owner, missing policy, etc.).
#[test]
fn test_no_event_emitted_on_failed_operations() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let id = create_health_policy(&env, &client, &owner);

    // pay_premium by wrong caller — should return false, no PremiumPaid event
    client.pay_premium(&other, &id);
    assert_eq!(insurance_events_for(&env, EVT_PREMIUM_PAID).len(), 0);

    // deactivate by wrong caller — no PolicyDeactivated event
    client.deactivate_policy(&other, &id);
    assert_eq!(insurance_events_for(&env, EVT_POLICY_DEACTIVATED).len(), 0);

    // set_external_ref by wrong caller — no ExternalRefUpdated event
    client.set_external_ref(&other, &id, &Some(String::from_str(&env, "X")));
    assert_eq!(insurance_events_for(&env, EVT_EXT_REF_UPDATED).len(), 0);
}

/// batch_pay_premiums emits one PremiumPaid event per successfully paid policy.
#[test]
fn test_batch_pay_premiums_event_per_policy() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    let id1 = create_health_policy(&env, &client, &owner);
    let id2 = create_health_policy(&env, &client, &owner);
    let id3 = create_health_policy(&env, &client, &owner);

    // Deactivate id3 — should not get an event
    client.deactivate_policy(&owner, &id3);

    let ids = soroban_sdk::vec![&env, id1, id2, id3];
    client.batch_pay_premiums(&owner, &ids);

    let paid_events = insurance_events_for(&env, EVT_PREMIUM_PAID);
    assert_eq!(
        paid_events.len(),
        2,
        "batch must emit one event per paid policy only"
    );
}

/// PayloadSchema: PremiumPaidEvent from batch carries correct per-policy data.
#[test]
fn test_batch_premium_paid_event_payload_schema() {
    let (env, client, _) = setup();
    let owner = Address::generate(&env);
    env.ledger().with_mut(|li| li.timestamp = 1_000_000u64);
    let id = create_health_policy(&env, &client, &owner);

    env.ledger().with_mut(|li| li.timestamp = 2_000_000u64);
    let ids = soroban_sdk::vec![&env, id];
    client.batch_pay_premiums(&owner, &ids);

    let events = insurance_events_for(&env, EVT_PREMIUM_PAID);
    assert_eq!(events.len(), 1);
    let data: PremiumPaidEvent =
        PremiumPaidEvent::try_from_val(&env, &events.get(0).unwrap().2).unwrap();
    assert_eq!(data.policy_id, id);
    assert_eq!(data.owner, owner);
    assert_eq!(data.amount, 1_000i128);
    assert_eq!(data.next_payment_date, 2_000_000 + 30 * 86_400);
    assert_eq!(data.timestamp, 2_000_000u64);
}
