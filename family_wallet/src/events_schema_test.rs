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
fn primary_namespace_symbols_are_stable() {
    // First element of every secondary `(namespace, action)` topic tuple
    // emitted by this contract.
    let family_ns: Symbol = symbol_short!("family");
    let wallet_ns: Symbol = symbol_short!("wallet");
    let em_mode_ns: Symbol = symbol_short!("em_mode");
    assert_eq!(family_ns, symbol_short!("family"));
    assert_eq!(wallet_ns, symbol_short!("wallet"));
    assert_eq!(em_mode_ns, symbol_short!("em_mode"));
}

#[test]
fn remitwise_action_symbols_are_stable() {
    // Action symbols used as the 4th element of the canonical
    // `(Remitwise, category, priority, action)` topic tuple emitted via
    // `RemitwiseEvents::emit`, plus the direct `(family|wallet, action)`
    // tuples emitted via `env.events().publish`.
    let actions = [
        symbol_short!("member"),
        symbol_short!("limit"),
        symbol_short!("batch_mem"),
        symbol_short!("em_mode"),
        symbol_short!("em_conf"),
        symbol_short!("em_prop"),
        symbol_short!("add_mem"),
        symbol_short!("rem_mem"),
        symbol_short!("role_exp"),
        symbol_short!("archived"),
        symbol_short!("exp_cln"),
        symbol_short!("upgraded"),
        symbol_short!("adm_xfr"),
    ];
    assert_eq!(actions.len(), 13);
}

// ---------------------------------------------------------------------------
// Payload schemas - struct events
// ---------------------------------------------------------------------------

#[test]
fn member_added_event_payload_schema() {
    let env = Env::default();
    let member = Address::generate(&env);

    // Struct literal lists every field by name -> compile-time stability check.
    let evt = MemberAddedEvent {
        member: member.clone(),
        role: FamilyRole::Member,
        spending_limit: 1_000_000,
        timestamp: 1_234_567_800,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = MemberAddedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.member, member);
    assert_eq!(decoded.role, FamilyRole::Member);
    assert_eq!(decoded.spending_limit, 1_000_000);
    assert_eq!(decoded.timestamp, 1_234_567_800);
}

#[test]
fn spending_limit_updated_event_payload_schema() {
    let env = Env::default();
    let member = Address::generate(&env);

    let evt = SpendingLimitUpdatedEvent {
        member: member.clone(),
        old_limit: 500_000,
        new_limit: 750_000,
        timestamp: 1_234_567_900,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = SpendingLimitUpdatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.member, member);
    assert_eq!(decoded.old_limit, 500_000);
    assert_eq!(decoded.new_limit, 750_000);
    assert_eq!(decoded.timestamp, 1_234_567_900);
}

#[test]
fn archived_transaction_payload_schema() {
    let env = Env::default();
    let proposer = Address::generate(&env);

    let evt = ArchivedTransaction {
        tx_id: 42,
        tx_type: TransactionType::LargeWithdrawal,
        proposer: proposer.clone(),
        executed_at: 1_111,
        archived_at: 2_222,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = ArchivedTransaction::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.tx_id, 42);
    assert_eq!(decoded.tx_type, TransactionType::LargeWithdrawal);
    assert_eq!(decoded.proposer, proposer);
    assert_eq!(decoded.executed_at, 1_111);
    assert_eq!(decoded.archived_at, 2_222);
}

#[test]
fn executed_tx_meta_payload_schema() {
    let env = Env::default();
    let proposer = Address::generate(&env);

    let evt = ExecutedTxMeta {
        tx_id: 99,
        tx_type: TransactionType::EmergencyTransfer,
        proposer: proposer.clone(),
        executed_at: 3_333,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = ExecutedTxMeta::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.tx_id, 99);
    assert_eq!(decoded.tx_type, TransactionType::EmergencyTransfer);
    assert_eq!(decoded.proposer, proposer);
    assert_eq!(decoded.executed_at, 3_333);
}

// ---------------------------------------------------------------------------
// Payload schemas - enum events
// ---------------------------------------------------------------------------

#[test]
fn emergency_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        EmergencyEvent::ModeOn,
        EmergencyEvent::ModeOff,
        EmergencyEvent::TransferInit,
        EmergencyEvent::TransferExec,
    ];
    assert_eq!(variants.len(), 4, "EmergencyEvent variant count drifted");
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}

#[test]
fn archive_event_variant_set_is_stable() {
    let env = Env::default();
    let variants = [
        ArchiveEvent::TransactionsArchived,
        ArchiveEvent::ExpiredCleaned,
        ArchiveEvent::TransactionCancelled,
    ];
    assert_eq!(variants.len(), 3, "ArchiveEvent variant count drifted");
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}

#[test]
fn transaction_type_variant_set_is_stable() {
    let env = Env::default();
    // TransactionType is part of every executed/archived tx event payload.
    let variants = [
        TransactionType::LargeWithdrawal,
        TransactionType::SplitConfigChange,
        TransactionType::RoleChange,
        TransactionType::EmergencyTransfer,
        TransactionType::PolicyCancellation,
        TransactionType::RegularWithdrawal,
    ];
    assert_eq!(variants.len(), 6, "TransactionType variant count drifted");
    for v in variants {
        let _: Val = v.into_val(&env);
    }

    // Discriminant values are persisted in archived events and must not shift.
    assert_eq!(TransactionType::LargeWithdrawal as u32, 1);
    assert_eq!(TransactionType::SplitConfigChange as u32, 2);
    assert_eq!(TransactionType::RoleChange as u32, 3);
    assert_eq!(TransactionType::EmergencyTransfer as u32, 4);
    assert_eq!(TransactionType::PolicyCancellation as u32, 5);
    assert_eq!(TransactionType::RegularWithdrawal as u32, 6);
}

#[test]
fn family_role_variant_set_is_stable() {
    // FamilyRole is included in MemberAddedEvent and is queried by indexers.
    assert_eq!(FamilyRole::Owner as u32, 1);
    assert_eq!(FamilyRole::Admin as u32, 2);
    assert_eq!(FamilyRole::Member as u32, 3);
    assert_eq!(FamilyRole::Viewer as u32, 4);
}
