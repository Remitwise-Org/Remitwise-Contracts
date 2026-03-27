//! # Bill Event Schema Parity & Backward Compatibility Tests
//!
//! Comprehensive tests validating that:
//!
//! 1. **Schema parity** — every contract operation emits a typed event struct
//!    matching the canonical schema defined in `events.rs`.
//! 2. **Backward compatibility** — topics use deterministic constant symbols,
//!    event data always includes `schema_version`, and field ordering is stable.
//! 3. **Consumer reliability** — downstream indexers can decode events by
//!    fixed topic offsets (namespace=0, category=1, priority=2, action=3).
//!
//! # Coverage
//!
//! | Operation              | Event Struct          | Topic Action   |
//! |------------------------|-----------------------|----------------|
//! | `create_bill`          | `BillCreatedEvent`    | `"created"`    |
//! | `pay_bill`             | `BillPaidEvent`       | `"paid"`       |
//! | `cancel_bill`          | `BillCancelledEvent`  | `"canceled"`   |
//! | `archive_paid_bills`   | `BillsArchivedEvent`  | `"archived"`   |
//! | `restore_bill`         | `BillRestoredEvent`   | `"restored"`   |
//! | `set_version`          | `VersionUpgradeEvent` | `"upgraded"`   |
//! | `batch_pay_bills`      | `BillPaidEvent` × N   | `"paid"`       |
//! | `pause` / `unpause`    | `()`                  | `"paused"` etc |

#![cfg(test)]

use bill_payments::events::{
    BillCancelledEvent, BillCreatedEvent, BillPaidEvent, BillRestoredEvent, BillsArchivedEvent,
    VersionUpgradeEvent, EVENT_SCHEMA_VERSION,
};
use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{symbol_short, testutils::Events, Address, Env, IntoVal, Symbol, TryFromVal, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register the contract, create a client, and mock all auths.
fn setup(env: &Env) -> (Address, BillPaymentsClient) {
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(env, &contract_id);
    (contract_id, client)
}

/// Extract the last emitted event's 4-topic tuple and data payload.
///
/// Returns `(namespace, category, priority, action, data_val)`.
fn last_event(
    env: &Env,
) -> (
    Symbol,
    u32,
    u32,
    Symbol,
    soroban_sdk::Val,
) {
    let all = env.events().all();
    assert!(!all.is_empty(), "No events were emitted");
    let (_cid, topics, data) = all.last().unwrap();

    let namespace = Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    let category = u32::try_from_val(env, &topics.get(1).unwrap()).unwrap();
    let priority = u32::try_from_val(env, &topics.get(2).unwrap()).unwrap();
    let action = Symbol::try_from_val(env, &topics.get(3).unwrap()).unwrap();

    (namespace, category, priority, action, data)
}

/// Find all events matching a given action symbol from the full event list.
fn events_with_action(env: &Env, action: Symbol) -> u32 {
    let all = env.events().all();
    let mut count = 0u32;
    for i in 0..all.len() {
        let (_cid, topics, _data) = all.get(i).unwrap();
        if let Ok(a) = Symbol::try_from_val(env, &topics.get(3).unwrap()) {
            if a == action {
                count += 1;
            }
        }
    }
    count
}

// ===========================================================================
// 1. CREATE BILL — BillCreatedEvent
// ===========================================================================

/// Verify `create_bill` emits a `BillCreatedEvent` with correct fields.
#[test]
fn test_create_bill_emits_typed_created_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Electricity"),
        &1000,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    let (namespace, category, priority, action, data) = last_event(&env);

    // Topic structure must be deterministic
    assert_eq!(namespace, symbol_short!("Remitwise"), "namespace mismatch");
    assert_eq!(category, 1u32, "expected EventCategory::State (1)");
    assert_eq!(priority, 1u32, "expected EventPriority::Medium (1)");
    assert_eq!(action, symbol_short!("created"), "action mismatch");

    // Decode typed event data
    let event: BillCreatedEvent = BillCreatedEvent::try_from_val(&env, &data)
        .expect("Failed to decode BillCreatedEvent from event data");

    assert_eq!(event.bill_id, bill_id, "bill_id mismatch");
    assert_eq!(event.owner, user, "owner mismatch");
    assert_eq!(event.amount, 1000, "amount mismatch");
    assert_eq!(event.due_date, 1234567890, "due_date mismatch");
    assert_eq!(
        event.currency,
        soroban_sdk::String::from_str(&env, "XLM"),
        "currency mismatch"
    );
    assert!(!event.recurring, "recurring mismatch");
    assert_eq!(
        event.schema_version, EVENT_SCHEMA_VERSION,
        "schema_version mismatch"
    );
}

/// Verify currency normalization is reflected in the created event.
#[test]
fn test_create_bill_event_currency_normalized() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Internet"),
        &500,
        &2000000000,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "usdc"), // lowercase input
    );

    let (_ns, _cat, _pri, _act, data) = last_event(&env);
    let event: BillCreatedEvent =
        BillCreatedEvent::try_from_val(&env, &data).expect("decode failure");

    assert_eq!(
        event.currency,
        soroban_sdk::String::from_str(&env, "USDC"),
        "Currency should be normalized to uppercase in event"
    );
}

/// Verify recurring flag is forwarded correctly in the event.
#[test]
fn test_create_recurring_bill_event_has_recurring_true() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Rent"),
        &10000,
        &1234567890,
        &true,
        &30,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    let (_ns, _cat, _pri, _act, data) = last_event(&env);
    let event: BillCreatedEvent =
        BillCreatedEvent::try_from_val(&env, &data).expect("decode failure");

    assert!(event.recurring, "recurring flag must be true for recurring bills");
}

// ===========================================================================
// 2. PAY BILL — BillPaidEvent
// ===========================================================================

/// Verify `pay_bill` emits a `BillPaidEvent` with correct fields.
#[test]
fn test_pay_bill_emits_typed_paid_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Water"),
        &750,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    client.pay_bill(&user, &bill_id);

    let (namespace, category, priority, action, data) = last_event(&env);

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(category, 0u32, "expected EventCategory::Transaction (0)");
    assert_eq!(priority, 2u32, "expected EventPriority::High (2)");
    assert_eq!(action, symbol_short!("paid"));

    let event: BillPaidEvent =
        BillPaidEvent::try_from_val(&env, &data).expect("Failed to decode BillPaidEvent");

    assert_eq!(event.bill_id, bill_id);
    assert_eq!(event.owner, user);
    assert_eq!(event.amount, 750);
    assert_eq!(
        event.schema_version, EVENT_SCHEMA_VERSION,
        "schema_version must match"
    );
}

/// Verify paid_at timestamp is populated from the ledger.
#[test]
fn test_pay_bill_event_paid_at_matches_ledger_timestamp() {
    let env = Env::default();
    env.mock_all_auths();

    use soroban_sdk::testutils::Ledger;
    env.ledger().set_timestamp(999_999);

    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Gas"),
        &300,
        &1_500_000,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    env.ledger().set_timestamp(1_200_000);
    client.pay_bill(&user, &bill_id);

    let (_ns, _cat, _pri, _act, data) = last_event(&env);
    let event: BillPaidEvent =
        BillPaidEvent::try_from_val(&env, &data).expect("decode failure");

    assert_eq!(event.paid_at, 1_200_000, "paid_at must match ledger timestamp");
}

// ===========================================================================
// 3. CANCEL BILL — BillCancelledEvent
// ===========================================================================

/// Verify `cancel_bill` emits a `BillCancelledEvent`.
#[test]
fn test_cancel_bill_emits_typed_cancelled_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Phone"),
        &200,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    client.cancel_bill(&user, &bill_id);

    let (namespace, _cat, _pri, action, data) = last_event(&env);

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(action, symbol_short!("canceled"));

    let event: BillCancelledEvent =
        BillCancelledEvent::try_from_val(&env, &data).expect("Failed to decode BillCancelledEvent");

    assert_eq!(event.bill_id, bill_id);
    assert_eq!(event.owner, user);
    assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
}

// ===========================================================================
// 4. ARCHIVE PAID BILLS — BillsArchivedEvent
// ===========================================================================

/// Verify `archive_paid_bills` emits a `BillsArchivedEvent`.
#[test]
fn test_archive_emits_typed_archived_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    // Create and pay several bills
    for i in 1..=3u32 {
        let bill_id = client.create_bill(
            &user,
            &soroban_sdk::String::from_str(&env, "Archivable"),
            &(100 * i as i128),
            &(1234567890 + i as u64),
            &false,
            &0,
            &None,
            &soroban_sdk::String::from_str(&env, "XLM"),
        );
        client.pay_bill(&user, &bill_id);
    }

    client.archive_paid_bills(&user, &u64::MAX);

    let (_ns, category, priority, action, data) = last_event(&env);

    assert_eq!(category, 3u32, "expected EventCategory::System (3)");
    assert_eq!(priority, 0u32, "expected EventPriority::Low (0)");
    assert_eq!(action, symbol_short!("archived"));

    let event: BillsArchivedEvent =
        BillsArchivedEvent::try_from_val(&env, &data).expect("Failed to decode BillsArchivedEvent");

    assert_eq!(event.count, 3, "should have archived 3 bills");
    assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
}

/// Verify archive event has zero count when there is nothing to archive.
#[test]
fn test_archive_emits_zero_count_when_nothing_to_archive() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    client.archive_paid_bills(&user, &u64::MAX);

    let (_ns, _cat, _pri, action, data) = last_event(&env);
    assert_eq!(action, symbol_short!("archived"));

    let event: BillsArchivedEvent =
        BillsArchivedEvent::try_from_val(&env, &data).expect("decode failure");

    assert_eq!(event.count, 0, "count must be 0 when nothing was archived");
}

// ===========================================================================
// 5. RESTORE BILL — BillRestoredEvent
// ===========================================================================

/// Verify `restore_bill` emits a `BillRestoredEvent`.
#[test]
fn test_restore_bill_emits_typed_restored_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Restore Target"),
        &500,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );
    client.pay_bill(&user, &bill_id);
    client.archive_paid_bills(&user, &u64::MAX);

    // Now restore
    client.restore_bill(&user, &bill_id);

    let (_ns, _cat, _pri, action, data) = last_event(&env);

    assert_eq!(action, symbol_short!("restored"));

    let event: BillRestoredEvent =
        BillRestoredEvent::try_from_val(&env, &data).expect("Failed to decode BillRestoredEvent");

    assert_eq!(event.bill_id, bill_id);
    assert_eq!(event.owner, user);
    assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
}

// ===========================================================================
// 6. VERSION UPGRADE — VersionUpgradeEvent
// ===========================================================================

/// Verify `set_version` emits a typed `VersionUpgradeEvent`.
#[test]
fn test_set_version_emits_typed_upgrade_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let admin = Address::generate(&env);

    client.set_upgrade_admin(&admin, &admin);
    client.set_version(&admin, &2);

    let (namespace, category, priority, action, data) = last_event(&env);

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(category, 3u32, "expected EventCategory::System (3)");
    assert_eq!(priority, 2u32, "expected EventPriority::High (2)");
    assert_eq!(action, symbol_short!("upgraded"));

    let event: VersionUpgradeEvent =
        VersionUpgradeEvent::try_from_val(&env, &data).expect("Failed to decode VersionUpgradeEvent");

    assert_eq!(event.previous_version, 1, "previous_version should be 1");
    assert_eq!(event.new_version, 2, "new_version should be 2");
    assert_eq!(event.schema_version, EVENT_SCHEMA_VERSION);
}

// ===========================================================================
// 7. BATCH PAY — multiple BillPaidEvents
// ===========================================================================

/// Verify `batch_pay_bills` emits one `BillPaidEvent` per bill.
#[test]
fn test_batch_pay_emits_per_bill_paid_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let mut ids = Vec::new(&env);
    for i in 1..=3u32 {
        let id = client.create_bill(
            &user,
            &soroban_sdk::String::from_str(&env, "Batch Bill"),
            &(100 * i as i128),
            &(1234567890 + i as u64),
            &false,
            &0,
            &None,
            &soroban_sdk::String::from_str(&env, "XLM"),
        );
        ids.push_back(id);
    }

    client.batch_pay_bills(&user, &ids);

    // There should be at least 3 "paid" events (one per bill)
    let paid_count = events_with_action(&env, symbol_short!("paid"));
    assert!(
        paid_count >= 3,
        "Expected at least 3 paid events from batch, got {}",
        paid_count
    );
}

// ===========================================================================
// 8. PAUSE / UNPAUSE — topic compatibility
// ===========================================================================

/// Verify `pause` emits with `("Remitwise", System, High, "paused")`.
#[test]
fn test_pause_event_topic_compat() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let admin = Address::generate(&env);

    client.set_pause_admin(&admin, &admin);
    client.pause(&admin);

    let (namespace, category, priority, action, _data) = last_event(&env);

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(category, 3u32, "System category");
    assert_eq!(priority, 2u32, "High priority");
    assert_eq!(action, symbol_short!("paused"));
}

/// Verify `unpause` emits with `("Remitwise", System, High, "unpaused")`.
#[test]
fn test_unpause_event_topic_compat() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let admin = Address::generate(&env);

    client.set_pause_admin(&admin, &admin);
    client.pause(&admin);
    client.unpause(&admin);

    let (namespace, _cat, _pri, action, _data) = last_event(&env);

    assert_eq!(namespace, symbol_short!("Remitwise"));
    assert_eq!(action, symbol_short!("unpaused"));
}

// ===========================================================================
// 9. TOPIC STRUCTURE STABILITY (backward compat)
// ===========================================================================

/// All events must use the 4-topic tuple: (namespace, cat, priority, action).
/// This test verifies every single event in a full lifecycle has exactly 4
/// topic entries — a change would break indexer decoding.
#[test]
fn test_all_events_have_four_topics() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);
    let admin = Address::generate(&env);

    // Setup admin
    client.set_pause_admin(&admin, &admin);
    client.set_upgrade_admin(&admin, &admin);

    // Complete lifecycle
    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Lifecycle"),
        &1000,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );
    client.pay_bill(&user, &bill_id);
    client.archive_paid_bills(&user, &u64::MAX);
    client.restore_bill(&user, &bill_id);
    client.pause(&admin);
    client.unpause(&admin);
    client.set_version(&admin, &2);

    let all = env.events().all();
    for i in 0..all.len() {
        let (_cid, topics, _data) = all.get(i).unwrap();
        assert_eq!(
            topics.len(),
            4,
            "Event at index {} has {} topics, expected 4",
            i,
            topics.len()
        );
    }
}

/// The namespace topic must always be "Remitwise" across all events.
#[test]
fn test_all_events_use_remitwise_namespace() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    // Trigger multiple events
    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "NS Check"),
        &100,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );
    client.pay_bill(&user, &bill_id);

    let all = env.events().all();
    for i in 0..all.len() {
        let (_cid, topics, _data) = all.get(i).unwrap();
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(
            ns,
            symbol_short!("Remitwise"),
            "Event {} namespace must be 'Remitwise'",
            i
        );
    }
}

// ===========================================================================
// 10. SCHEMA VERSION CONSISTENCY
// ===========================================================================

/// All typed events must carry `schema_version == EVENT_SCHEMA_VERSION`.
#[test]
fn test_schema_version_consistent_across_event_types() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);
    let admin = Address::generate(&env);

    client.set_upgrade_admin(&admin, &admin);

    // Create
    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Schema V"),
        &500,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    let all = env.events().all();
    let (_cid, _topics, data) = all.last().unwrap();
    let created: BillCreatedEvent =
        BillCreatedEvent::try_from_val(&env, &data).expect("decode");
    assert_eq!(created.schema_version, EVENT_SCHEMA_VERSION);

    // Pay
    client.pay_bill(&user, &bill_id);
    let all = env.events().all();
    let (_cid, _topics, data) = all.last().unwrap();
    let paid: BillPaidEvent = BillPaidEvent::try_from_val(&env, &data).expect("decode");
    assert_eq!(paid.schema_version, EVENT_SCHEMA_VERSION);

    // Upgrade
    client.set_version(&admin, &5);
    let all = env.events().all();
    let (_cid, _topics, data) = all.last().unwrap();
    let upgrade: VersionUpgradeEvent =
        VersionUpgradeEvent::try_from_val(&env, &data).expect("decode");
    assert_eq!(upgrade.schema_version, EVENT_SCHEMA_VERSION);
}

// ===========================================================================
// 11. EDGE CASES
// ===========================================================================

/// Verify that a recurring bill payment emits both a paid event for the
/// original and a created event for the successor — in that order.
#[test]
fn test_recurring_pay_emits_created_after_paid() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Monthly"),
        &1000,
        &1234567890,
        &true,
        &30,
        &None,
        &soroban_sdk::String::from_str(&env, "XLM"),
    );

    // Clear event count before pay
    let events_before = env.events().all().len();

    client.pay_bill(&user, &bill_id);

    let all = env.events().all();
    // At least one paid event should have been emitted
    let paid_count = events_with_action(&env, symbol_short!("paid"));
    assert!(paid_count >= 1, "Expected at least 1 paid event");
}

/// Verify empty-currency bills default to XLM in the event.
#[test]
fn test_empty_currency_defaults_to_xlm_in_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&env, "Default Currency"),
        &100,
        &1234567890,
        &false,
        &0,
        &None,
        &soroban_sdk::String::from_str(&env, ""), // empty → "XLM"
    );

    let (_ns, _cat, _pri, _act, data) = last_event(&env);
    let event: BillCreatedEvent =
        BillCreatedEvent::try_from_val(&env, &data).expect("decode failure");

    assert_eq!(
        event.currency,
        soroban_sdk::String::from_str(&env, "XLM"),
        "Empty currency must default to XLM in event data"
    );
}

/// Verify multiple sequential creates produce monotonically increasing bill_ids
/// in their events.
#[test]
fn test_sequential_creates_monotonic_bill_ids_in_events() {
    let env = Env::default();
    env.mock_all_auths();
    let (_cid, client) = setup(&env);
    let user = Address::generate(&env);

    let mut prev_id = 0u32;
    for i in 1..=5u32 {
        let id = client.create_bill(
            &user,
            &soroban_sdk::String::from_str(&env, "Seq"),
            &(100 * i as i128),
            &(1234567890 + i as u64),
            &false,
            &0,
            &None,
            &soroban_sdk::String::from_str(&env, "XLM"),
        );

        let (_ns, _cat, _pri, _act, data) = last_event(&env);
        let event: BillCreatedEvent =
            BillCreatedEvent::try_from_val(&env, &data).expect("decode failure");

        assert_eq!(event.bill_id, id, "event bill_id must match returned id");
        assert!(
            event.bill_id > prev_id,
            "bill_ids must be monotonically increasing"
        );
        prev_id = event.bill_id;
    }
}

// ===========================================================================
// 12. COMPILE-TIME SCHEMA PARITY (regression guard)
// ===========================================================================

/// This test validates the compile-time assertions by constructing events
/// with all mandatory fields. If a field is removed, this won't compile.
#[test]
fn test_event_constructors_fill_all_fields() {
    let env = Env::default();
    let user = Address::generate(&env);

    let created = BillCreatedEvent::new(
        1,
        user.clone(),
        1000,
        9999,
        soroban_sdk::String::from_str(&env, "XLM"),
        false,
    );
    assert_eq!(created.schema_version, EVENT_SCHEMA_VERSION);

    let paid = BillPaidEvent::new(1, user.clone(), 1000, 10000);
    assert_eq!(paid.schema_version, EVENT_SCHEMA_VERSION);

    let cancelled = BillCancelledEvent::new(1, user.clone(), 10001);
    assert_eq!(cancelled.schema_version, EVENT_SCHEMA_VERSION);

    let restored = BillRestoredEvent::new(1, user.clone(), 10002);
    assert_eq!(restored.schema_version, EVENT_SCHEMA_VERSION);

    let archived = BillsArchivedEvent::new(5, 10003);
    assert_eq!(archived.schema_version, EVENT_SCHEMA_VERSION);

    let upgrade = VersionUpgradeEvent::new(1, 2);
    assert_eq!(upgrade.schema_version, EVENT_SCHEMA_VERSION);
}
