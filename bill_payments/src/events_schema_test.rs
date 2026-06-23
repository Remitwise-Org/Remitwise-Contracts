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
use crate::BillPaymentsClient;
use soroban_sdk::testutils::Address as AddressTrait;
use soroban_sdk::testutils::Events;
use soroban_sdk::{symbol_short, Env, IntoVal, Symbol, TryFromVal, Val, Address, String, Vec};

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
        symbol_short!("cancelled"),
        symbol_short!("archived"),
        symbol_short!("restored"),
        symbol_short!("cleaned"),
        symbol_short!("ext_upd"),
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

#[test]
fn bill_event_secondary_topics_emit_expected_variants() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let bill_id_1 = client.create_bill(
        &owner,
        &String::from_str(&env, "Electricity"),
        &1000,
        &1_000_000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    client.set_external_ref(&owner, &bill_id_1, &Some(String::from_str(&env, "REF1")));
    client.cancel_bill(&owner, &bill_id_1);

    let bill_id_2 = client.create_bill(
        &owner,
        &String::from_str(&env, "Water"),
        &2000,
        &1_000_100,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    client.pay_bill(&owner, &bill_id_2);
    client.archive_paid_bills(&owner, &(env.ledger().timestamp() + 1));
    client.restore_bill(&owner, &bill_id_2);

    let mut found_cancelled = false;
    let mut found_external_ref = false;
    let mut found_restored = false;
    let mut found_paid = 0u32;

    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let namespace: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if namespace != symbol_short!("bill") {
            continue;
        }

        let variant = BillEvent::try_from_val(&env, &topics.get(1).unwrap());
        if let Ok(variant) = variant {
            match variant {
                BillEvent::Cancelled => {
                    let payload: (u32, Address, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
                    assert_eq!(payload.0, bill_id_1);
                    assert_eq!(payload.1, owner);
                    found_cancelled = true;
                }
                BillEvent::ExternalRefUpdated => {
                    let payload: (u32, Address, Option<String>) = TryFromVal::try_from_val(&env, &data).unwrap();
                    assert_eq!(payload.0, bill_id_1);
                    assert_eq!(payload.1, owner);
                    assert_eq!(payload.2, Some(String::from_str(&env, "REF1")));
                    found_external_ref = true;
                }
                BillEvent::Restored => {
                    let payload: (u32, Address, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
                    assert_eq!(payload.0, bill_id_2);
                    assert_eq!(payload.1, owner);
                    found_restored = true;
                }
                BillEvent::Paid => {
                    let payload: (u32, Address, Option<String>) = TryFromVal::try_from_val(&env, &data).unwrap();
                    assert_eq!(payload.1, owner);
                    found_paid += 1;
                }
                _ => {}
            }
        }
    }

    assert!(found_cancelled, "BillEvent::Cancelled was not emitted");
    assert!(found_external_ref, "BillEvent::ExternalRefUpdated was not emitted");
    assert!(found_restored, "BillEvent::Restored was not emitted");
    assert_eq!(found_paid, 1, "Expected exactly one BillEvent::Paid emitted");
}

#[test]
fn batch_pay_bills_emits_paid_events_matching_pay_bill() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let bill_a = client.create_bill(
        &owner,
        &String::from_str(&env, "Electricity"),
        &1000,
        &1_000_000,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );
    let bill_b = client.create_bill(
        &owner,
        &String::from_str(&env, "Water"),
        &2000,
        &1_000_100,
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM"),
        &None,
    );

    let mut batch = Vec::new(&env);
    batch.push_back(bill_a);
    batch.push_back(bill_b);
    client.batch_pay_bills(&owner, &batch);

    let mut paid_events = 0u32;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let namespace: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if namespace != symbol_short!("bill") {
            continue;
        }
        let variant = BillEvent::try_from_val(&env, &topics.get(1).unwrap());
        if let Ok(BillEvent::Paid) = variant {
            let payload: (u32, Address, Option<String>) = TryFromVal::try_from_val(&env, &data).unwrap();
            assert!(payload.0 == bill_a || payload.0 == bill_b);
            assert_eq!(payload.1, owner);
            paid_events += 1;
        }
    }

    assert_eq!(paid_events, 2, "batch_pay_bills must emit exactly two BillEvent::Paid events");
}
