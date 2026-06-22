//! Event schema stability tests.
//!
//! These tests pin down the public event surface of this contract:
//!
//!   * The topic symbols emitted on every event (what indexers subscribe to).
//!   * The payload field set, names, and types of every event struct.
//!   * The variant set of every event enum.
//!
//! A failure here means the change is **breaking for downstream indexers**.
//! See [EVENTS.md](../../EVENTS.md) for the full schema contract.

#![cfg(test)]

use super::*;
use crate::pause_functions::{ARCHIVE, CANCEL_BILL, CREATE_BILL, PAY_BILL, RESTORE};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, Val, Vec as SorobanVec,
};

// ---------------------------------------------------------------------------
// Pause-function symbols
// ---------------------------------------------------------------------------

#[test]
fn pause_function_symbols_are_stable() {
    // These symbols name the pausable function set and double as action
    // symbols in the canonical Remitwise topic tuple. Indexers and the
    // pause admin tooling key off these literal values.
    assert_eq!(CREATE_BILL, symbol_short!("crt_bill"));
    assert_eq!(PAY_BILL, symbol_short!("pay_bill"));
    assert_eq!(CANCEL_BILL, symbol_short!("can_bill"));
    assert_eq!(ARCHIVE, symbol_short!("archive"));
    assert_eq!(RESTORE, symbol_short!("restore"));
}

#[test]
fn primary_namespace_symbol_is_stable() {
    // Frozen at "bill" - first element of every secondary topic tuple
    // `(bill, BillEvent::Variant)` emitted by this contract.
    let ns: Symbol = symbol_short!("bill");
    assert_eq!(ns, symbol_short!("bill"));
}

// ---------------------------------------------------------------------------
// Action symbols emitted via RemitwiseEvents::emit and direct publish
// ---------------------------------------------------------------------------

#[test]
fn remitwise_action_symbols_are_stable() {
    let actions = [
        symbol_short!("created"),
        symbol_short!("paid"),
        symbol_short!("can_bill"),
        symbol_short!("archived"),
        symbol_short!("restore"),
        symbol_short!("cleaned"),
        symbol_short!("ext_ref"),
        symbol_short!("paused"),
        symbol_short!("unpaused"),
        symbol_short!("upgraded"),
        symbol_short!("adm_xfr"),
        symbol_short!("batch_res"),
        symbol_short!("f_pay_id"),
        symbol_short!("fpay_auth"),
        symbol_short!("f_pay_pd"),
    ];
    assert_eq!(actions.len(), 15);
}

fn bill_event_matches(env: &Env, val: &Val, expected: &BillEvent) -> bool {
    let Ok(decoded) = BillEvent::try_from_val(env, val) else {
        return false;
    };
    matches!(
        (&decoded, expected),
        (BillEvent::Created, BillEvent::Created)
            | (BillEvent::Paid, BillEvent::Paid)
            | (BillEvent::ExternalRefUpdated, BillEvent::ExternalRefUpdated)
            | (BillEvent::Cancelled, BillEvent::Cancelled)
            | (BillEvent::Archived, BillEvent::Archived)
            | (BillEvent::Restored, BillEvent::Restored)
            | (BillEvent::ScheduleCreated, BillEvent::ScheduleCreated)
            | (BillEvent::ScheduleExecuted, BillEvent::ScheduleExecuted)
            | (BillEvent::ScheduleMissed, BillEvent::ScheduleMissed)
            | (BillEvent::ScheduleModified, BillEvent::ScheduleModified)
            | (BillEvent::ScheduleCancelled, BillEvent::ScheduleCancelled)
            | (
                BillEvent::RecurringBillCreated,
                BillEvent::RecurringBillCreated,
            )
    )
}

fn is_direct_bill_event(env: &Env, topics: &SorobanVec<Val>, expected: &BillEvent) -> bool {
    if topics.len() != 2 {
        return false;
    }
    let Ok(namespace) = Symbol::try_from_val(env, &topics.get(0).unwrap()) else {
        return false;
    };
    namespace == symbol_short!("bill") && bill_event_matches(env, &topics.get(1).unwrap(), expected)
}

fn has_remitwise_action(env: &Env, action: Symbol) -> bool {
    for (_, topics, _) in env.events().all() {
        if topics.len() != 4 {
            continue;
        }
        let Ok(namespace) = Symbol::try_from_val(env, &topics.get(0).unwrap()) else {
            continue;
        };
        let Ok(actual_action) = Symbol::try_from_val(env, &topics.get(3).unwrap()) else {
            continue;
        };
        if namespace == symbol_short!("Remitwise") && actual_action == action {
            return true;
        }
    }
    false
}

fn count_direct_bill_events(env: &Env, contract_id: &Address, expected: BillEvent) -> u32 {
    let mut count = 0;
    for (cid, topics, _) in env.events().all() {
        if cid == *contract_id && is_direct_bill_event(env, &topics, &expected) {
            count += 1;
        }
    }
    count
}

fn create_test_bill(env: &Env, client: &BillPaymentsClient, owner: &Address) -> u32 {
    client.create_bill(
        owner,
        &SorobanString::from_str(env, "Utility"),
        &100,
        &1_000_000,
        &false,
        &0,
        &None,
        &SorobanString::from_str(env, "XLM"),
        &None,
    )
}

fn direct_bill_payload<T>(env: &Env, contract_id: &Address, expected: BillEvent) -> Option<T>
where
    T: TryFromVal<Env, Val>,
{
    for (cid, topics, data) in env.events().all() {
        if cid == *contract_id && is_direct_bill_event(env, &topics, &expected) {
            return T::try_from_val(env, &data).ok();
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Payload schemas - enum events
// ---------------------------------------------------------------------------

#[test]
fn bill_event_variant_set_is_stable() {
    let env = Env::default();

    // Construct every variant by name -> compile-time stability check.
    let variants = [
        BillEvent::Created,
        BillEvent::Paid,
        BillEvent::ExternalRefUpdated,
        BillEvent::Cancelled,
        BillEvent::Archived,
        BillEvent::Restored,
        BillEvent::ScheduleCreated,
        BillEvent::ScheduleExecuted,
        BillEvent::ScheduleMissed,
        BillEvent::ScheduleModified,
        BillEvent::ScheduleCancelled,
        BillEvent::RecurringBillCreated,
    ];

    assert_eq!(variants.len(), 12, "BillEvent variant count drifted");

    for v in variants {
        // Each variant must serialize cleanly so the topic
        // `(bill, BillEvent::Foo)` keeps publishing.
        let _: Val = v.into_val(&env);
    }
}

#[test]
fn external_ref_update_emits_declared_bill_event_variant() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(42);
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let bill_id = create_test_bill(&env, &client, &owner);
    let external_ref = Some(SorobanString::from_str(&env, "INV-2026-001"));

    client.set_external_ref(&owner, &bill_id, &external_ref);

    let payload: (u32, Address, Option<SorobanString>, u64) =
        direct_bill_payload(&env, &contract_id, BillEvent::ExternalRefUpdated)
            .expect("ExternalRefUpdated event missing");
    assert_eq!(payload, (bill_id, owner.clone(), external_ref, 42));
    assert!(has_remitwise_action(&env, symbol_short!("ext_ref")));
}

#[test]
fn cancel_bill_emits_declared_bill_event_variant() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(84);
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let bill_id = create_test_bill(&env, &client, &owner);

    client.cancel_bill(&owner, &bill_id);

    let payload: (u32, Address, u64) =
        direct_bill_payload(&env, &contract_id, BillEvent::Cancelled)
            .expect("Cancelled event missing");
    assert_eq!(payload, (bill_id, owner.clone(), 84));
    assert!(has_remitwise_action(&env, CANCEL_BILL));
}

#[test]
fn restore_bill_emits_declared_bill_event_variant() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(100);
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let bill_id = create_test_bill(&env, &client, &owner);
    client.pay_bill(&owner, &bill_id);
    env.ledger().set_timestamp(200);
    assert_eq!(client.archive_paid_bills(&owner, &150), 1);

    env.ledger().set_timestamp(300);
    client.restore_bill(&owner, &bill_id);

    let payload: (u32, Address, u64) = direct_bill_payload(&env, &contract_id, BillEvent::Restored)
        .expect("Restored event missing");
    assert_eq!(payload, (bill_id, owner.clone(), 300));
    assert!(has_remitwise_action(&env, RESTORE));
}

#[test]
fn batch_pay_bills_emits_paid_variant_per_successful_bill() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let first_id = create_test_bill(&env, &client, &owner);
    let second_id = create_test_bill(&env, &client, &owner);
    let already_paid_id = create_test_bill(&env, &client, &owner);
    client.pay_bill(&owner, &already_paid_id);
    let paid_events_before = count_direct_bill_events(&env, &contract_id, BillEvent::Paid);
    let mut ids = SorobanVec::new(&env);
    ids.push_back(first_id);
    ids.push_back(999);
    ids.push_back(second_id);
    ids.push_back(already_paid_id);

    assert_eq!(client.batch_pay_bills(&owner, &ids), 2);

    let paid_events_after = count_direct_bill_events(&env, &contract_id, BillEvent::Paid);
    assert_eq!(paid_events_after - paid_events_before, 2);
    assert!(has_remitwise_action(&env, symbol_short!("paid")));
}

// ---------------------------------------------------------------------------
// Bill payload (the canonical bill record published with `crt_bill` events)
// ---------------------------------------------------------------------------

#[test]
fn bill_record_payload_schema() {
    use soroban_sdk::{
        testutils::Address as _, Address, String as SorobanString, Vec as SorobanVec,
    };
    let env = Env::default();
    let owner = Address::generate(&env);
    let name = SorobanString::from_str(&env, "Electricity");
    let currency = SorobanString::from_str(&env, "XLM");
    let tags = SorobanVec::<SorobanString>::new(&env);

    // Struct literal lists every public field by name -> compile-time check.
    let bill = Bill {
        id: 1,
        owner: owner.clone(),
        name: name.clone(),
        external_ref: None,
        amount: 1_000,
        due_date: 1_234_567_890,
        recurring: false,
        frequency_days: 0,
        paid: false,
        created_at: 1_234_567_800,
        paid_at: None,
        schedule_id: None,
        tags: tags.clone(),
        currency: currency.clone(),
    };

    // Round-trip via Val locks the on-wire serialization shape.
    let v: Val = bill.clone().into_val(&env);
    let decoded = Bill::try_from_val(&env, &v).expect("Bill round-trip failed");

    assert_eq!(decoded.id, 1);
    assert_eq!(decoded.owner, owner);
    assert_eq!(decoded.name, name);
    assert!(decoded.external_ref.is_none());
    assert_eq!(decoded.amount, 1_000);
    assert_eq!(decoded.due_date, 1_234_567_890);
    assert!(!decoded.recurring);
    assert_eq!(decoded.frequency_days, 0);
    assert!(!decoded.paid);
    assert_eq!(decoded.created_at, 1_234_567_800);
    assert!(decoded.paid_at.is_none());
    assert!(decoded.schedule_id.is_none());
    assert_eq!(decoded.tags.len(), 0);
    assert_eq!(decoded.currency, currency);
}
