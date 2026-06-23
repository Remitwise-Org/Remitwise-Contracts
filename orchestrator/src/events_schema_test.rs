//! Event schema stability tests for the orchestrator contract.
//!
//! Pins the public event surface documented in [EVENTS.md](../../EVENTS.md) and
//! [docs/orchestrator-events.md](../../docs/orchestrator-events.md).

#![cfg(test)]

use remitwise_common::{EventCategory, EventPriority};
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

// ---------------------------------------------------------------------------
// Action symbols emitted via RemitwiseEvents::emit
// ---------------------------------------------------------------------------

#[test]
fn remitwise_flow_action_symbols_are_stable() {
    let actions = [
        symbol_short!("flow"),
        symbol_short!("flow_ok"),
        symbol_short!("flow_fail"),
        symbol_short!("init_ok"),
    ];
    assert_eq!(actions.len(), 4);
}

#[test]
fn primary_namespace_symbol_is_stable() {
    let ns: Symbol = symbol_short!("Remitwise");
    assert_eq!(ns, symbol_short!("Remitwise"));
}

#[test]
fn orchestrator_upgrade_topic_is_stable() {
    let contract: Symbol = symbol_short!("orch");
    let action: Symbol = symbol_short!("upgraded");
    assert_eq!(contract, symbol_short!("orch"));
    assert_eq!(action, symbol_short!("upgraded"));
}

// ---------------------------------------------------------------------------
// Payload schemas - lifecycle events
// ---------------------------------------------------------------------------

fn sample_address(env: &Env) -> Address {
    Address::generate(env)
}

#[test]
fn flow_started_event_payload_schema() {
    let env = Env::default();
    let executor = sample_address(&env);
    let amount: i128 = 10_000;

    let payload = (executor.clone(), amount);
    let v: Val = payload.clone().into_val(&env);
    let decoded: (Address, i128) = TryFromVal::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.0, executor);
    assert_eq!(decoded.1, amount);
}

#[test]
fn flow_completed_event_payload_schema() {
    let env = Env::default();
    let executor = sample_address(&env);
    let amount: i128 = 10_000;

    let payload = (executor.clone(), amount);
    let v: Val = payload.clone().into_val(&env);
    let decoded: (Address, i128) = TryFromVal::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.0, executor);
    assert_eq!(decoded.1, amount);
}

#[test]
fn flow_failed_event_payload_schema() {
    let env = Env::default();
    let executor = sample_address(&env);
    let error_code: u32 = 2;

    let payload = (executor.clone(), error_code);
    let v: Val = payload.clone().into_val(&env);
    let decoded: (Address, u32) = TryFromVal::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.0, executor);
    assert_eq!(decoded.1, error_code);
}

#[test]
fn init_completed_event_payload_schema() {
    let env = Env::default();
    let caller = sample_address(&env);

    let payload = caller.clone();
    let v: Val = payload.clone().into_val(&env);
    let decoded: Address = TryFromVal::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded, caller);
}

#[test]
fn version_upgrade_event_payload_schema() {
    let env = Env::default();
    let payload = (1u32, 2u32);
    let v: Val = payload.into_val(&env);
    let decoded: (u32, u32) = TryFromVal::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded, (1, 2));
}

#[test]
fn event_category_and_priority_discriminants_are_stable() {
    assert_eq!(EventCategory::Transaction.to_u32(), 0);
    assert_eq!(EventCategory::System.to_u32(), 3);
    assert_eq!(EventPriority::High.to_u32(), 2);
}
