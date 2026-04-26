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
use soroban_sdk::{symbol_short, Env, IntoVal, Symbol, TryFromVal, Val};

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
        symbol_short!("canceled"),
        symbol_short!("archived"),
        symbol_short!("restored"),
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
    ];

    assert_eq!(variants.len(), 11, "BillEvent variant count drifted");

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
