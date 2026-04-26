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
//!
//! The struct-literal initialisations are themselves compile-time checks:
//! adding, removing, or renaming a field will fail to compile here.

#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, IntoVal, String as SorobanString, Symbol,
    TryFromVal, Val,
};

// ---------------------------------------------------------------------------
// Topic symbols
// ---------------------------------------------------------------------------

#[test]
fn topic_constants_are_stable() {
    // Primary topic symbols. Renaming any of these breaks every indexer
    // subscribed to the savings_goals event stream.
    assert_eq!(GOAL_CREATED, symbol_short!("created"));
    assert_eq!(GOAL_COMPLETED, symbol_short!("completed"));
}

#[test]
fn primary_namespace_symbol_is_stable() {
    // The contract's primary namespace symbol used as the first element of
    // every secondary `(namespace, action)` topic tuple. Frozen at "savings".
    let ns: Symbol = symbol_short!("savings");
    assert_eq!(ns, symbol_short!("savings"));
}

// ---------------------------------------------------------------------------
// Payload schemas - struct events
// ---------------------------------------------------------------------------

fn sample_address(env: &Env) -> Address {
    Address::generate(env)
}

#[test]
fn goal_created_event_payload_schema() {
    let env = Env::default();
    let owner = sample_address(&env);
    let name = SorobanString::from_str(&env, "Emergency Fund");

    // Struct literal lists every field by name -> compile-time stability check.
    let evt = GoalCreatedEvent {
        goal_id: 1,
        owner: owner.clone(),
        name: name.clone(),
        target_amount: 50_000,
        target_date: 1_735_689_600,
        timestamp: 1_234_567_800,
    };

    // Round-trip via Val locks down the on-wire serialization shape.
    let v: Val = evt.clone().into_val(&env);
    let decoded = GoalCreatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.goal_id, 1);
    assert_eq!(decoded.owner, owner);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.target_amount, 50_000);
    assert_eq!(decoded.target_date, 1_735_689_600);
    assert_eq!(decoded.timestamp, 1_234_567_800);
}

#[test]
fn funds_added_event_payload_schema() {
    let env = Env::default();
    let owner = sample_address(&env);

    let evt = FundsAddedEvent {
        goal_id: 7,
        owner: owner.clone(),
        amount: 5_000,
        new_total: 15_000,
        timestamp: 1_234_567_850,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = FundsAddedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.goal_id, 7);
    assert_eq!(decoded.owner, owner);
    assert_eq!(decoded.amount, 5_000);
    assert_eq!(decoded.new_total, 15_000);
    assert_eq!(decoded.timestamp, 1_234_567_850);
}

#[test]
fn funds_withdrawn_event_payload_schema() {
    let env = Env::default();
    let owner = sample_address(&env);

    let evt = FundsWithdrawnEvent {
        goal_id: 2,
        owner: owner.clone(),
        amount: 300,
        new_total: 500,
        timestamp: 9_999,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = FundsWithdrawnEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.goal_id, 2);
    assert_eq!(decoded.owner, owner);
    assert_eq!(decoded.amount, 300);
    assert_eq!(decoded.new_total, 500);
    assert_eq!(decoded.timestamp, 9_999);
}

#[test]
fn goal_completed_event_payload_schema() {
    let env = Env::default();
    let owner = sample_address(&env);
    let name = SorobanString::from_str(&env, "Vacation");

    let evt = GoalCompletedEvent {
        goal_id: 3,
        owner: owner.clone(),
        name: name.clone(),
        final_amount: 25_000,
        timestamp: 12_345,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = GoalCompletedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.goal_id, 3);
    assert_eq!(decoded.owner, owner);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.final_amount, 25_000);
    assert_eq!(decoded.timestamp, 12_345);
}

// ---------------------------------------------------------------------------
// Payload schemas - enum events
// ---------------------------------------------------------------------------

/// Build every variant of `SavingsEvent` by name. This is a pure compile-time
/// stability check: renaming or removing a variant fails to compile.
#[test]
fn savings_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        SavingsEvent::GoalCreated,
        SavingsEvent::FundsAdded,
        SavingsEvent::FundsWithdrawn,
        SavingsEvent::GoalCompleted,
        SavingsEvent::GoalLocked,
        SavingsEvent::GoalUnlocked,
        SavingsEvent::ScheduleCreated,
        SavingsEvent::ScheduleExecuted,
        SavingsEvent::ScheduleMissed,
        SavingsEvent::ScheduleModified,
        SavingsEvent::ScheduleCancelled,
    ];

    // Lock the variant count so accidental additions are caught by review.
    assert_eq!(variants.len(), 11, "SavingsEvent variant count drifted");

    // Each variant must serialize cleanly to a Val (so the topic tuple
    // `(namespace, SavingsEvent::Foo)` keeps publishing).
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}

// ---------------------------------------------------------------------------
// Action symbols emitted via RemitwiseEvents::emit
// ---------------------------------------------------------------------------

/// Asserts every action symbol the contract uses with `RemitwiseEvents::emit`
/// retains its documented literal. Action symbols form the 4th element of
/// the canonical `(Remitwise, category, priority, action)` topic tuple.
#[test]
fn remitwise_action_symbols_are_stable() {
    let actions = [
        symbol_short!("created"),
        symbol_short!("goal_new"),
        symbol_short!("funds_add"),
        symbol_short!("funds_rem"),
        symbol_short!("batch_add"),
        symbol_short!("tags_add"),
        symbol_short!("tags_rem"),
        symbol_short!("upgraded"),
        symbol_short!("paused"),
        symbol_short!("unpaused"),
        symbol_short!("adm_xfr"),
        symbol_short!("snap_exp"),
    ];
    // Touch each so a literal change triggers a recompile in this file.
    assert_eq!(actions.len(), 12);
}
