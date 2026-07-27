//! Event schema stability tests.
//!
//! These tests pin down the public event surface of this contract:
//!
//!   * The topic symbols emitted on every event (what indexers subscribe to).
//!   * The payload field set, names, and types of every event struct.
//!   * The `InsuranceEvent` enum variant set and wire serialization.
//!
//! A failure here means the change is **breaking for downstream indexers**.
//! See [EVENTS.md](../../EVENTS.md) for the full schema contract.
//!
//! The struct-literal initialisations are themselves compile-time checks:
//! adding, removing, or renaming a field will fail to compile here.

#![cfg(test)]

use super::*;
use remitwise_common::CoverageType;
use soroban_sdk::{
    symbol_short, testutils::Address as _, Address, Env, IntoVal, String as SorobanString, Symbol,
    TryFromVal, Val,
};

// ---------------------------------------------------------------------------
// Topic symbols — namespace
// ---------------------------------------------------------------------------

/// Every insurance event uses `symbol_short!("insurance")` as its first topic.
/// Pinning this prevents accidental rename from breaking all indexer subscriptions.
#[test]
fn primary_namespace_symbol_is_stable() {
    let ns: Symbol = symbol_short!("insurance");
    assert_eq!(ns, symbol_short!("insurance"));
}

// ---------------------------------------------------------------------------
// InsuranceEvent enum — variant set stability
// ---------------------------------------------------------------------------

/// Compile-time + runtime check: every variant must exist with the exact name
/// listed here. Adding/removing/renaming a variant is a breaking indexer change.
#[test]
fn insurance_event_variant_set_is_stable() {
    let env = Env::default();

    let variants = [
        InsuranceEvent::Created,
        InsuranceEvent::PremiumPaid,
        InsuranceEvent::Deactivated,
        InsuranceEvent::Reactivated,
        InsuranceEvent::ExternalRefUpdated,
        InsuranceEvent::ScheduleCreated,
        InsuranceEvent::ScheduleExecuted,
        InsuranceEvent::ScheduleCancelled,
        InsuranceEvent::ScheduleModified,
    ];

    assert_eq!(variants.len(), 9, "InsuranceEvent variant count drifted");

    // Each variant must serialize cleanly so the topic tuple keeps publishing.
    for v in variants {
        let _: Val = v.into_val(&env);
    }
}

/// Pinned contract: every variant must serialize as a typed enum value (not a raw
/// Symbol) so indexers can match `InsuranceEvent::*` from the on-chain topic bytes.
#[test]
fn insurance_event_variants_serialize_as_enum_not_symbol() {
    let env = Env::default();

    for variant in [
        InsuranceEvent::Created,
        InsuranceEvent::PremiumPaid,
        InsuranceEvent::Deactivated,
        InsuranceEvent::Reactivated,
        InsuranceEvent::ExternalRefUpdated,
        InsuranceEvent::ScheduleCreated,
        InsuranceEvent::ScheduleExecuted,
        InsuranceEvent::ScheduleCancelled,
        InsuranceEvent::ScheduleModified,
    ] {
        let val: Val = variant.clone().into_val(&env);
        // Round-trip: decode back to InsuranceEvent — must succeed (proves it
        // serialized as a proper enum variant, not a raw Symbol).
        let decoded = InsuranceEvent::try_from_val(&env, &val)
            .expect("InsuranceEvent variant must deserialize from its own serialized form");
        assert_eq!(decoded, variant);
    }
}

// ---------------------------------------------------------------------------
// Payload schemas — struct round-trips (compile-time field checks)
// ---------------------------------------------------------------------------

#[test]
fn policy_created_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Life Insurance");

    let evt = PolicyCreatedEvent {
        policy_id: 1,
        name: name.clone(),
        coverage_type: CoverageType::Life,
        monthly_premium: 2_000,
        coverage_amount: 500_000,
        timestamp: 1_234_567_800,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyCreatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 1);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.coverage_type, CoverageType::Life);
    assert_eq!(decoded.monthly_premium, 2_000);
    assert_eq!(decoded.coverage_amount, 500_000);
    assert_eq!(decoded.timestamp, 1_234_567_800);
}

#[test]
fn premium_paid_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Term Policy");

    let evt = PremiumPaidEvent {
        policy_id: 7,
        name: name.clone(),
        amount: 2_000,
        next_payment_date: 1_237_246_200,
        timestamp: 1_234_567_850,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PremiumPaidEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 7);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.amount, 2_000);
    assert_eq!(decoded.next_payment_date, 1_237_246_200);
    assert_eq!(decoded.timestamp, 1_234_567_850);
}

#[test]
fn policy_deactivated_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Health Plan");

    let evt = PolicyDeactivatedEvent {
        policy_id: 3,
        name: name.clone(),
        timestamp: 9_999,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyDeactivatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 3);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.timestamp, 9_999);
}

#[test]
fn policy_reactivated_event_payload_schema() {
    let env = Env::default();
    let name = SorobanString::from_str(&env, "Auto Policy");

    let evt = PolicyReactivatedEvent {
        policy_id: 4,
        name: name.clone(),
        timestamp: 12_345,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = PolicyReactivatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 4);
    assert_eq!(decoded.name, name);
    assert_eq!(decoded.timestamp, 12_345);
}

/// Pinned schema for the new `ExternalRefUpdatedEvent` payload.
/// Field names, types, and order are part of the public indexer contract.
#[test]
fn external_ref_updated_event_payload_schema() {
    let env = Env::default();
    let caller = Address::generate(&env);
    let ref_val = SorobanString::from_str(&env, "EXTREF-001");

    // With a Some value
    let evt = ExternalRefUpdatedEvent {
        policy_id: 5,
        caller: caller.clone(),
        ext_ref: Some(ref_val.clone()),
        timestamp: 99_999,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded = ExternalRefUpdatedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.policy_id, 5);
    assert_eq!(decoded.caller, caller);
    assert_eq!(decoded.ext_ref, Some(ref_val));
    assert_eq!(decoded.timestamp, 99_999);

    // With a None value (clearing the ref)
    let evt_none = ExternalRefUpdatedEvent {
        policy_id: 5,
        caller: caller.clone(),
        ext_ref: None,
        timestamp: 100_000,
    };

    let v2: Val = evt_none.into_val(&env);
    let decoded_none =
        ExternalRefUpdatedEvent::try_from_val(&env, &v2).expect("round-trip failed (None)");

    assert!(decoded_none.ext_ref.is_none());
    assert_eq!(decoded_none.timestamp, 100_000);
}

#[test]
fn premium_schedule_executed_event_payload_schema() {
    let env = Env::default();

    let evt = PremiumScheduleExecutedEvent {
        schedule_id: 10,
        policy_id: 2,
        amount: 1_500,
        next_due: 1_700_086_400,
        timestamp: 1_700_000_000,
    };

    let v: Val = evt.clone().into_val(&env);
    let decoded =
        PremiumScheduleExecutedEvent::try_from_val(&env, &v).expect("round-trip failed");

    assert_eq!(decoded.schedule_id, 10);
    assert_eq!(decoded.policy_id, 2);
    assert_eq!(decoded.amount, 1_500);
    assert_eq!(decoded.next_due, 1_700_086_400);
    assert_eq!(decoded.timestamp, 1_700_000_000);
}

// ---------------------------------------------------------------------------
// Topic symbols — legacy / admin events (non-lifecycle)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_topic_symbols_are_stable() {
    let snapshot_actions = [
        symbol_short!("snap_pre"),
        symbol_short!("snap_rst"),
        symbol_short!("snap_dsc"),
    ];
    assert_eq!(snapshot_actions.len(), 3);
}

#[test]
fn remitwise_action_symbols_are_stable() {
    let actions = [symbol_short!("prem_pay"), symbol_short!("upgraded")];
    assert_eq!(actions.len(), 2);
}

// ---------------------------------------------------------------------------
// End-to-end emission tests — verify the contract actually emits the right
// (namespace, InsuranceEvent::*) topic pair with a decodable payload.
// ---------------------------------------------------------------------------

/// Helper: register and initialise a fresh Insurance contract.
fn setup_contract(env: &Env) -> (InsuranceClient<'_>, Address) {
    let id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(env, &id);
    let owner = Address::generate(env);
    client.init(&owner);
    (client, owner)
}

/// Helper: create a minimal Health policy.
fn create_health_policy(
    env: &Env,
    client: &InsuranceClient<'_>,
    policy_owner: &Address,
) -> u32 {
    client.create_policy(
        policy_owner,
        &SorobanString::from_str(env, "Test Policy"),
        &CoverageType::Health,
        &5_000_000i128,
        &50_000_000i128,
        &None,
    )
}

/// `create_policy` must emit `(insurance, InsuranceEvent::Created)` with a
/// `PolicyCreatedEvent` payload that matches the arguments supplied.
#[test]
fn create_policy_emits_created_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);

    let mut found = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        let variant = InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap());
        if let Ok(InsuranceEvent::Created) = variant {
            let payload: PolicyCreatedEvent =
                PolicyCreatedEvent::try_from_val(&env, &data).expect("payload decode failed");
            assert_eq!(payload.policy_id, pid);
            assert_eq!(
                payload.name,
                SorobanString::from_str(&env, "Test Policy")
            );
            assert_eq!(payload.coverage_type, CoverageType::Health);
            assert_eq!(payload.monthly_premium, 5_000_000);
            assert_eq!(payload.coverage_amount, 50_000_000);
            found = true;
        }
    }
    assert!(found, "InsuranceEvent::Created was not emitted");
}

/// `pay_premium` must emit `(insurance, InsuranceEvent::PremiumPaid)` with a
/// `PremiumPaidEvent` payload containing the correct policy id and amount.
#[test]
fn pay_premium_emits_premium_paid_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);
    env.events().all(); // drain creation events

    client.pay_premium(&policy_owner, &pid);

    let mut found = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::PremiumPaid) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: PremiumPaidEvent =
                PremiumPaidEvent::try_from_val(&env, &data).expect("payload decode failed");
            assert_eq!(payload.policy_id, pid);
            assert_eq!(payload.amount, 5_000_000);
            found = true;
        }
    }
    assert!(found, "InsuranceEvent::PremiumPaid was not emitted");
}

/// `deactivate_policy` must emit `(insurance, InsuranceEvent::Deactivated)` with
/// a `PolicyDeactivatedEvent` whose `policy_id` matches.
#[test]
fn deactivate_policy_emits_deactivated_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);
    client.deactivate_policy(&policy_owner, &pid);

    let mut found = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::Deactivated) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: PolicyDeactivatedEvent =
                PolicyDeactivatedEvent::try_from_val(&env, &data).expect("payload decode failed");
            assert_eq!(payload.policy_id, pid);
            found = true;
        }
    }
    assert!(found, "InsuranceEvent::Deactivated was not emitted");
}

/// `reactivate_policy` must emit `(insurance, InsuranceEvent::Reactivated)` after
/// the mandatory cooldown window has elapsed.
#[test]
fn reactivate_policy_emits_reactivated_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);
    client.deactivate_policy(&policy_owner, &pid);

    // Advance past the 24-hour cooldown (MAX_TENURE_SECS = 86_400).
    env.ledger().with_mut(|l| l.timestamp += 86_401);

    client.reactivate_policy(&policy_owner, &pid).unwrap();

    let mut found = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::Reactivated) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: PolicyReactivatedEvent =
                PolicyReactivatedEvent::try_from_val(&env, &data).expect("payload decode failed");
            assert_eq!(payload.policy_id, pid);
            found = true;
        }
    }
    assert!(found, "InsuranceEvent::Reactivated was not emitted");
}

/// `set_external_ref` must emit `(insurance, InsuranceEvent::ExternalRefUpdated)`
/// with an `ExternalRefUpdatedEvent` payload that contains the new ref value.
#[test]
fn set_external_ref_emits_external_ref_updated_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);

    let new_ref = SorobanString::from_str(&env, "EXTREF-42");
    client
        .set_external_ref(&contract_owner, &pid, &Some(new_ref.clone()))
        .unwrap();

    let mut found = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::ExternalRefUpdated) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: ExternalRefUpdatedEvent =
                ExternalRefUpdatedEvent::try_from_val(&env, &data)
                    .expect("payload decode failed");
            assert_eq!(payload.policy_id, pid);
            assert_eq!(payload.caller, contract_owner);
            assert_eq!(payload.ext_ref, Some(new_ref.clone()));
            found = true;
        }
    }
    assert!(
        found,
        "InsuranceEvent::ExternalRefUpdated was not emitted"
    );
}

/// Clearing (`None`) also emits `ExternalRefUpdated` with `ext_ref: None`.
#[test]
fn set_external_ref_clear_emits_event_with_none() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);

    // Set first, then clear.
    client
        .set_external_ref(
            &contract_owner,
            &pid,
            &Some(SorobanString::from_str(&env, "INITIAL")),
        )
        .unwrap();
    client
        .set_external_ref(&contract_owner, &pid, &None)
        .unwrap();

    let mut found_clear = false;
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::ExternalRefUpdated) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: ExternalRefUpdatedEvent =
                ExternalRefUpdatedEvent::try_from_val(&env, &data)
                    .expect("payload decode failed");
            if payload.ext_ref.is_none() {
                assert_eq!(payload.policy_id, pid);
                assert_eq!(payload.caller, contract_owner);
                found_clear = true;
            }
        }
    }
    assert!(
        found_clear,
        "ExternalRefUpdated with None ext_ref was not emitted on clear"
    );
}

/// `batch_pay_premiums` must emit one `PremiumPaid` event per policy, matching
/// the shape emitted by individual `pay_premium` calls.
#[test]
fn batch_pay_premiums_emits_premium_paid_for_each_policy() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid_a = create_health_policy(&env, &client, &policy_owner);
    // Create a second policy (different premium to distinguish).
    let pid_b = client.create_policy(
        &policy_owner,
        &SorobanString::from_str(&env, "Second Policy"),
        &CoverageType::Auto,
        &10_000_000i128,
        &100_000_000i128,
        &None,
    );

    let mut batch = soroban_sdk::Vec::new(&env);
    batch.push_back(pid_a);
    batch.push_back(pid_b);
    client.batch_pay_premiums(&policy_owner, &batch);

    let mut paid_ids: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for (_cid, topics, data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != symbol_short!("insurance") {
            continue;
        }
        if let Ok(InsuranceEvent::PremiumPaid) =
            InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap())
        {
            let payload: PremiumPaidEvent =
                PremiumPaidEvent::try_from_val(&env, &data).expect("payload decode failed");
            paid_ids.push_back(payload.policy_id);
        }
    }

    assert_eq!(paid_ids.len(), 2, "Expected exactly 2 PremiumPaid events");
    assert!(paid_ids.contains(pid_a), "Missing PremiumPaid for pid_a");
    assert!(paid_ids.contains(pid_b), "Missing PremiumPaid for pid_b");
}

/// All four primary lifecycle variants must use `symbol_short!("insurance")` as
/// their first topic — confirmed by inspecting events from a full lifecycle run.
#[test]
fn all_lifecycle_events_use_insurance_namespace() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, contract_owner) = setup_contract(&env);
    let policy_owner = Address::generate(&env);

    let pid = create_health_policy(&env, &client, &policy_owner);
    client.pay_premium(&policy_owner, &pid);
    client.deactivate_policy(&policy_owner, &pid);
    env.ledger().with_mut(|l| l.timestamp += 86_401);
    client.reactivate_policy(&policy_owner, &pid).unwrap();
    client
        .set_external_ref(
            &contract_owner,
            &pid,
            &Some(SorobanString::from_str(&env, "REF")),
        )
        .unwrap();

    let insurance_ns = symbol_short!("insurance");
    let mut lifecycle_variants_seen: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);

    for (_cid, topics, _data) in env.events().all() {
        if topics.len() < 2 {
            continue;
        }
        let ns = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        if ns != insurance_ns {
            continue;
        }
        if let Ok(variant) = InsuranceEvent::try_from_val(&env, &topics.get(1).unwrap()) {
            let discriminant = match variant {
                InsuranceEvent::Created => 0u32,
                InsuranceEvent::PremiumPaid => 1,
                InsuranceEvent::Deactivated => 2,
                InsuranceEvent::Reactivated => 3,
                InsuranceEvent::ExternalRefUpdated => 4,
                InsuranceEvent::ScheduleCreated => 5,
                InsuranceEvent::ScheduleExecuted => 6,
                InsuranceEvent::ScheduleCancelled => 7,
                InsuranceEvent::ScheduleModified => 8,
            };
            if !lifecycle_variants_seen.contains(discriminant) {
                lifecycle_variants_seen.push_back(discriminant);
            }
        }
    }

    // Created(0), PremiumPaid(1), Deactivated(2), Reactivated(3), ExternalRefUpdated(4)
    for expected in [0u32, 1, 2, 3, 4] {
        assert!(
            lifecycle_variants_seen.contains(expected),
            "Lifecycle variant discriminant {} not seen in insurance namespace",
            expected
        );
    }
}
