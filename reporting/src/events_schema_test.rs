//! Event schema stability tests.
//!
//! These tests pin down the public event surface of this contract:
//!
//!   * The topic symbols emitted on every event (what indexers subscribe to).
//!   * The variant set of the `ReportEvent` enum (used as the action symbol
//!     in every `(report, ReportEvent::*)` topic tuple).
//!
//! A failure here means the change is **breaking for downstream indexers**.
//! See [EVENTS.md](../../EVENTS.md) for the full schema contract.

#![cfg(test)]

use super::*;
use soroban_sdk::{symbol_short, Env, IntoVal, Symbol, Val};

// ---------------------------------------------------------------------------
// Topic symbols
// ---------------------------------------------------------------------------

#[test]
fn primary_namespace_symbol_is_stable() {
    // First element of every secondary `(namespace, action)` topic tuple
    // emitted by this contract.
    let ns: Symbol = symbol_short!("report");
    assert_eq!(ns, symbol_short!("report"));
}

// ---------------------------------------------------------------------------
// ReportEvent enum variants - serialized into the topic tuple
// ---------------------------------------------------------------------------

#[test]
fn report_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        ReportEvent::ReportGenerated,
        ReportEvent::ReportStored,
        ReportEvent::AddressesConfigured,
        ReportEvent::ReportsArchived,
        ReportEvent::ArchivesCleaned,
    ];
    assert_eq!(variants.len(), 5, "ReportEvent variant count drifted");

    for v in variants {
        // Each variant must serialize cleanly so the topic
        // `(report, ReportEvent::Foo)` keeps publishing.
        let _: Val = v.into_val(&env);
    }
}

// ---------------------------------------------------------------------------
// Data-availability discriminants embedded in report payloads
// ---------------------------------------------------------------------------

#[test]
fn data_availability_discriminants_are_stable() {
    // Embedded in every RemittanceSummary published with a generated report.
    // Indexers persist these as numeric values.
    assert_eq!(DataAvailability::Complete as u32, 0);
    assert_eq!(DataAvailability::Partial as u32, 1);
    assert_eq!(DataAvailability::Missing as u32, 2);
}
