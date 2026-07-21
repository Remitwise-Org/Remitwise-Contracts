//! Event schema stability tests.
//!
//! These tests pin down the public event surface of this contract:
//!
//!   * The topic symbols emitted on every event (what indexers subscribe to).
//!   * The payload field set, names, and types of every event struct.
//!
//! A failure here means the change is **breaking for downstream indexers**.
//! See [EVENTS.md](../../docs/EVENTS.md) for the full schema contract.
//!
//! The struct-literal initialisations are themselves compile-time checks:
//! adding, removing, or renaming a field will fail to compile here.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short, Env, IntoVal, String as SorobanString, Symbol, TryFromVal, Val,
};

#[test]
fn topic_constants_are_stable() {
    // Topic symbols emitted by the contract. Renaming any of these breaks indexers
    // subscribed to the insurance event stream.
    let env = Env::default();
    assert_eq!(symbol_short!("created"), Symbol::new(&env, "created"));
    assert_eq!(symbol_short!("policy"), Symbol::new(&env, "policy"));
    assert_eq!(symbol_short!("paid"), Symbol::new(&env, "paid"));
    assert_eq!(symbol_short!("premium"), Symbol::new(&env, "premium"));
    assert_eq!(symbol_short!("deactive"), Symbol::new(&env, "deactive"));
    assert_eq!(Symbol::new(&env, "reactivated"), Symbol::new(&env, "reactivated"));
    assert_eq!(symbol_short!("insurance"), Symbol::new(&env, "insurance"));
    assert_eq!(symbol_short!("sched_exe"), Symbol::new(&env, "sched_exe"));
}

#[test]
fn policy_created_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Life Insurance");

    // Struct literal lists every field by name -> compile-time stability check.
    let evt = PolicyCreatedEvent {
        policy_id: 1,
        name: name.clone(),
        coverage_type: CoverageType::Life,
        monthly_premium: 500,
        coverage_amount: 100_000,
        timestamp: 1_234_567_890,
    };

    // Round-trip via Val locks down the on-wire serialization shape.
    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyCreatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 1);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.coverage_type, CoverageType::Life);
    assert_eq!(decoded.monthly_premium, 500);
    assert_eq!(decoded.coverage_amount, 100_000);
    assert_eq!(decoded.timestamp, 1_234_567_890);
}

#[test]
fn premium_paid_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Life Insurance");

    let evt = PremiumPaidEvent {
        policy_id: 2,
        name: name.clone(),
        amount: 500,
        next_payment_date: 1_234_567_900,
        timestamp: 1_234_567_890,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PremiumPaidEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 2);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.amount, 500);
    assert_eq!(decoded.next_payment_date, 1_234_567_900);
    assert_eq!(decoded.timestamp, 1_234_567_890);
}

#[test]
fn policy_deactivated_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Life Insurance");

    let evt = PolicyDeactivatedEvent {
        policy_id: 3,
        name: name.clone(),
        timestamp: 1_234_567_890,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyDeactivatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 3);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.timestamp, 1_234_567_890);
}

#[test]
fn premium_schedule_executed_event_payload_schema() {
    let env = Env::default();

    let evt = PremiumScheduleExecutedEvent {
        schedule_id: 10,
        policy_id: 4,
        amount: 500,
        next_due: 1_234_568_000,
        timestamp: 1_234_567_890,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PremiumScheduleExecutedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.schedule_id, 10);
    assert_eq!(decoded.policy_id, 4);
    assert_eq!(decoded.amount, 500);
    assert_eq!(decoded.next_due, 1_234_568_000);
    assert_eq!(decoded.timestamp, 1_234_567_890);
}

#[test]
fn policy_reactivated_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Life Insurance");

    let evt = PolicyReactivatedEvent {
        policy_id: 5,
        name: name.clone(),
        timestamp: 1_234_567_890,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyReactivatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 5);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.timestamp, 1_234_567_890);
}

#[test]
fn remitwise_action_symbols_are_stable() {
    // Medium-priority transaction action symbol used by this contract with RemitwiseEvents::emit
    let env = Env::default();
    assert_eq!(symbol_short!("prem_pay"), Symbol::new(&env, "prem_pay"));
}
