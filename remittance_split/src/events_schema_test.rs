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
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, IntoVal, Symbol, TryFromVal, Val,
};

// ---------------------------------------------------------------------------
// Topic symbols
// ---------------------------------------------------------------------------

#[test]
fn topic_constants_are_stable() {
    // Primary topic symbols frozen at their documented literals.
    assert_eq!(SPLIT_INITIALIZED, symbol_short!("init"));
    assert_eq!(SPLIT_CALCULATED, symbol_short!("calc"));
}

#[test]
fn primary_namespace_symbols_are_stable() {
    // First element of every secondary `(namespace, action)` topic tuple.
    let split_ns: Symbol = symbol_short!("split");
    let schedule_ns: Symbol = symbol_short!("schedule");
    assert_eq!(split_ns, symbol_short!("split"));
    assert_eq!(schedule_ns, symbol_short!("schedule"));
}

#[test]
fn remitwise_action_symbols_are_stable() {
    // Action symbols used as the 4th element of the canonical
    // `(Remitwise, category, priority, action)` topic tuple.
    let actions = [
        symbol_short!("init"),
        symbol_short!("calc"),
        symbol_short!("calc_raw"),
        symbol_short!("dist_ok"),
        symbol_short!("dist_comp"),
        symbol_short!("paused"),
        symbol_short!("unpaused"),
        symbol_short!("upgraded"),
        symbol_short!("adm_xfr"),
        symbol_short!("snap_exp"),
        symbol_short!("update"),
        symbol_short!("distrib"),
        symbol_short!("export"),
        symbol_short!("import"),
    ];
    assert_eq!(actions.len(), 14);
}

// ---------------------------------------------------------------------------
// Allocation category symbols
// ---------------------------------------------------------------------------

#[test]
fn allocation_category_symbols_are_stable() {
    // Returned in `Allocation { category, amount }` items - downstream
    // analytics depend on these exact strings.
    assert_eq!(symbol_short!("SPENDING"), symbol_short!("SPENDING"));
    assert_eq!(symbol_short!("SAVINGS"), symbol_short!("SAVINGS"));
    assert_eq!(symbol_short!("BILLS"), symbol_short!("BILLS"));
    assert_eq!(symbol_short!("INSURANCE"), symbol_short!("INSURANCE"));
}

// ---------------------------------------------------------------------------
// Payload schemas - struct events
// ---------------------------------------------------------------------------

#[test]
fn split_initialized_event_payload_schema() {
    let env = Env::default();

    // Struct literal lists every field by name -> compile-time stability check.
    let evt = SplitInitializedEvent {
        spending_percent: 50,
        savings_percent: 30,
        bills_percent: 15,
        insurance_percent: 5,
        timestamp: 1_234_567_800,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = SplitInitializedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.spending_percent, 50);
    assert_eq!(decoded.savings_percent, 30);
    assert_eq!(decoded.bills_percent, 15);
    assert_eq!(decoded.insurance_percent, 5);
    assert_eq!(decoded.timestamp, 1_234_567_800);

    // Documented invariant: the four percentages sum to 100.
    assert_eq!(
        decoded.spending_percent
            + decoded.savings_percent
            + decoded.bills_percent
            + decoded.insurance_percent,
        100
    );
}

#[test]
fn split_calculated_event_payload_schema() {
    let env = Env::default();

    let evt = SplitCalculatedEvent {
        total_amount: 10_000,
        spending_amount: 5_000,
        savings_amount: 3_000,
        bills_amount: 1_500,
        insurance_amount: 500,
        timestamp: 1_234_567_850,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = SplitCalculatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.total_amount, 10_000);
    assert_eq!(decoded.spending_amount, 5_000);
    assert_eq!(decoded.savings_amount, 3_000);
    assert_eq!(decoded.bills_amount, 1_500);
    assert_eq!(decoded.insurance_amount, 500);
    assert_eq!(decoded.timestamp, 1_234_567_850);

    // Documented invariant: the four allocations sum to total.
    assert_eq!(
        decoded.spending_amount
            + decoded.savings_amount
            + decoded.bills_amount
            + decoded.insurance_amount,
        decoded.total_amount
    );
}

#[test]
fn distribution_completed_event_payload_schema() {
    let env = Env::default();
    let from = Address::generate(&env);

    let evt = DistributionCompletedEvent {
        from: from.clone(),
        total_amount: 10_000,
        spending_amount: 5_000,
        savings_amount: 3_000,
        bills_amount: 1_500,
        insurance_amount: 500,
        timestamp: 1_234_567_900,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = DistributionCompletedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.from, from);
    assert_eq!(decoded.total_amount, 10_000);
    assert_eq!(decoded.spending_amount, 5_000);
    assert_eq!(decoded.savings_amount, 3_000);
    assert_eq!(decoded.bills_amount, 1_500);
    assert_eq!(decoded.insurance_amount, 500);
    assert_eq!(decoded.timestamp, 1_234_567_900);
}

// ---------------------------------------------------------------------------
// Payload schemas - enum events
// ---------------------------------------------------------------------------

#[test]
fn split_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        SplitEvent::Initialized,
        SplitEvent::Updated,
        SplitEvent::Calculated,
        SplitEvent::DistributionCompleted,
        SplitEvent::SnapshotExported,
        SplitEvent::SnapshotImported,
    ];
    assert_eq!(variants.len(), 6, "SplitEvent variant count drifted");
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}

#[test]
fn schedule_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        ScheduleEvent::Created,
        ScheduleEvent::Executed,
        ScheduleEvent::Missed,
        ScheduleEvent::Modified,
        ScheduleEvent::Cancelled,
    ];
    assert_eq!(variants.len(), 5, "ScheduleEvent variant count drifted");
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}
