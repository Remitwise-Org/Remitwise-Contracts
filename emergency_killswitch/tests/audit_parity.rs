//! Issue #1761 — emergency & administrator controls: events and audit parity.
//!
//! These integration tests drive the *real* contract entrypoints end-to-end
//! and decode the `("emergency", "control")` audit events Soroban actually
//! recorded. They prove four invariants at the actual integration boundary:
//!
//! 1. **Every committed transition** publishes a versioned, complete
//!    control/audit record (schema version, monotonic correlation `seq`,
//!    actor, timestamp, and operation `kind`) that matches the final state.
//! 2. **Strict ordering & correlation** — `seq` is strictly increasing across
//!    a sequence of transitions and matches the observable `get_event_seq`
//!    counter, so downstream indexers can deterministically order and
//!    correlate the whole emergency audit stream.
//! 3. **Committed-only emission** — rejected, stale, repeated, and failed
//!    operations emit *no* control event, advance `seq` by zero, and leave no
//!    partial state behind.
//! 4. **Consensus-driven transitions** (threshold activation / recovery)
//!    record a `None` actor (no single authorizing principal), while
//!    admin-driven transitions record the authorizing admin address.

#![cfg(test)]

use emergency_killswitch::{
    ControlEvent, EmergencyKillswitch, EmergencyKillswitchClient, Error, PauseScope, EVENT_VERSION,
    RECOVERY_DELAY,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as AddressTrait, Events as EventsTrait, Ledger as LedgerTrait},
    vec, Address, Env, Symbol, TryFromVal,
};

fn setup(env: &Env) -> (EmergencyKillswitchClient<'_>, Address) {
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(env, &contract_id);
    let admin = Address::generate(env);
    (client, admin)
}

/// Decodes every `("emergency", "control")` audit event in emission order.
fn control_events(env: &Env) -> Vec<ControlEvent> {
    env.events()
        .all()
        .iter()
        .filter_map(|(_cid, topics, data)| {
            let t0: Option<Symbol> = topics
                .get(0)
                .and_then(|t| Symbol::try_from_val(env, &t).ok());
            let t1: Option<Symbol> = topics
                .get(1)
                .and_then(|t| Symbol::try_from_val(env, &t).ok());
            if t0 == Some(symbol_short!("emergency")) && t1 == Some(symbol_short!("control")) {
                ControlEvent::try_from_val(env, &data).ok()
            } else {
                None
            }
        })
        .collect()
}

fn last_control_event(env: &Env) -> ControlEvent {
    control_events(env)
        .into_iter()
        .last()
        .expect("expected at least one control event")
}

fn count_control_events(env: &Env) -> usize {
    control_events(env).len()
}

// ─── Invariant 1 & 2: parity, versioning, correlation, ordering ─────────────

/// A pause → schedule-unpause → unpause cycle emits one versioned, complete
/// control event per committed transition, with `seq` starting at 1 and
/// strictly increasing, and every event carrying the right actor + timestamp
/// that matches the observable contract state.
#[test]
fn lifecycle_emits_versioned_ordered_control_events_matching_state() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    env.ledger().set_timestamp(5_000);
    client.initialize(&admin);
    assert_eq!(client.get_event_seq(), 1);

    env.ledger().set_timestamp(5_100);
    client.pause();
    assert!(client.is_paused());
    // Snapshot the observable state right after the transition so we can
    // prove the event payload matches the committed state (get_paused_since
    // is cleared again on unpause below).
    let paused_since_at_pause = client.get_paused_since();
    assert_eq!(paused_since_at_pause, Some(5_100));

    env.ledger().set_timestamp(6_000);
    client.schedule_unpause(&7_000);
    assert_eq!(client.get_unpause_schedule(), Some(7_000));

    env.ledger().set_timestamp(7_000);
    client.unpause();
    assert!(!client.is_paused());

    let evts = control_events(&env);
    assert_eq!(evts.len(), 4, "one control event per committed transition");

    let kinds: Vec<Symbol> = evts.iter().map(|e| e.kind.clone()).collect();
    assert_eq!(
        kinds,
        std::vec![
            symbol_short!("init"),
            symbol_short!("pause"),
            symbol_short!("schedule"),
            symbol_short!("unpause"),
        ]
    );

    // Correlation / ordering: seq is 1..=4 strictly increasing.
    let seqs: Vec<u64> = evts.iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        std::vec![1u64, 2, 3, 4],
        "seq must be strictly monotonic from 1"
    );
    assert_eq!(
        client.get_event_seq(),
        4,
        "observable counter matches last seq"
    );

    // Versioning: every record is the current schema version.
    assert!(
        evts.iter().all(|e| e.version == EVENT_VERSION),
        "every control event must be versioned"
    );

    // Completeness / parity: the pause record matches committed state fields.
    let pause = &evts[1];
    assert_eq!(pause.actor.as_ref(), Some(&admin));
    assert_eq!(pause.timestamp, 5_100);
    // The audit record's timestamp equals the paused_at the state reported at
    // the moment of commit.
    assert_eq!(pause.timestamp, paused_since_at_pause.unwrap());

    let unpause = &evts[3];
    assert_eq!(unpause.actor.as_ref(), Some(&admin));
    assert_eq!(unpause.timestamp, 7_000);
}

// ─── Invariant 3: committed-only emission (rejections emit nothing) ─────────

/// Rejected, stale, and repeated operations must emit no control event and
/// must not advance the correlation counter or leave partial state.
#[test]
fn rejected_operations_emit_no_control_event_and_leave_no_state() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    // Give the ledger a nonzero base time so "in the past" scheduling is
    // genuinely in the past (the default Env starts at timestamp 0, where a
    // schedule of 0 is not rejected as being in the past).
    env.ledger().set_timestamp(1_000);
    client.initialize(&admin);
    assert_eq!(count_control_events(&env), 1);

    let before = count_control_events(&env);

    // Double init rejected.
    assert_eq!(
        client.try_initialize(&admin),
        Err(Ok(Error::AlreadyInitialized))
    );

    // Unpause with no pending schedule -> InvalidSchedule.
    assert_eq!(client.try_unpause(), Err(Ok(Error::InvalidSchedule)));

    // Transfer to the current admin is rejected.
    assert_eq!(
        client.try_transfer_admin(&admin, &0),
        Err(Ok(Error::InvalidAdmin))
    );

    // Scheduling an unpause in the past is rejected.
    assert_eq!(
        client.try_schedule_unpause(&0),
        Err(Ok(Error::InvalidSchedule))
    );

    // None of the rejected calls may advance the counter or emit a record.
    assert_eq!(count_control_events(&env), before);
    assert_eq!(
        client.get_event_seq(),
        1,
        "seq must not advance on rejection"
    );

    // No partial state leaked by any rejected transition.
    assert!(!client.is_paused());
    assert_eq!(client.get_paused_since(), None);
    assert_eq!(client.get_unpause_schedule(), None);
}

/// Re-pausing an already-paused function is a genuine no-op: it must neither
/// emit a control event nor advance the counter, matching the idempotent
/// pause_function path.
#[test]
fn repeated_function_pause_is_idempotent_and_emits_nothing() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let module = symbol_short!("bill");
    let func = symbol_short!("pay");

    client.initialize(&admin); // seq 1
    client.pause_function(&module, &func); // seq 2 — real transition
    assert_eq!(count_control_events(&env), 2);
    assert_eq!(client.list_paused_functions(&module).len(), 1);

    // Repeat: already paused, no event.
    client.pause_function(&module, &func);
    assert_eq!(
        count_control_events(&env),
        2,
        "repeated pause must not re-emit"
    );
    assert_eq!(client.get_event_seq(), 2);
    assert_eq!(client.list_paused_functions(&module).len(), 1);

    // Unpause is a real transition.
    client.unpause_function(&module, &func); // seq 3
    assert_eq!(count_control_events(&env), 3);

    // Unpause of a non-paused function is a no-op: no event.
    client.unpause_function(&module, &func);
    assert_eq!(count_control_events(&env), 3);
    assert_eq!(client.get_event_seq(), 3);
    assert!(client.list_paused_functions(&module).is_empty());
}

// ─── Invariant 4: consensus-driven transitions carry no single actor ────────

/// Threshold activation / recovery are quorum-driven, so their control events
/// must record `actor = None`. Wrong-epoch and too-early rejections emit
/// nothing and leave no partial activation state.
#[test]
fn activation_recovery_emit_actor_none_and_rejections_emit_nothing() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let signer1 = Address::generate(&env);
    let signer2 = Address::generate(&env);

    client.initialize(&admin); // seq 1
    let signers = vec![&env, signer1.clone(), signer2.clone()];
    let epoch = client.configure_signers(&admin, &signers, &2); // seq 2
    let approvals = vec![&env, signer1, signer2];

    // Activate at a known base time so recovery readiness (base + delay) is
    // predictable. `set_timestamp` is used instead of relative `with_mut`
    // deltas, which are unreliable on this soroban-env-host test harness.
    env.ledger().set_timestamp(1_000);
    client.activate(&epoch, &approvals, &PauseScope::Global); // seq 3
    assert!(client.is_paused(), "activation must commit global pause");
    // RecoveryReadyAt is now 1_000 + RECOVERY_DELAY.
    let ready_at = 1_000 + RECOVERY_DELAY;

    let act = last_control_event(&env);
    assert_eq!(act.kind, symbol_short!("activated"));
    assert_eq!(
        act.actor, None,
        "consensus-driven activation has no single actor"
    );
    assert_eq!(act.version, EVENT_VERSION);

    let before = count_control_events(&env);

    // Wrong-epoch activation rejected -> nothing emitted.
    assert_eq!(
        client.try_activate(&(epoch + 1), &approvals, &PauseScope::Global),
        Err(Ok(Error::EpochMismatch))
    );
    // Recovery right before the mandatory delay rejected -> nothing emitted.
    env.ledger().set_timestamp(ready_at - 1);
    assert_eq!(
        client.try_recover(&epoch, &approvals),
        Err(Ok(Error::RecoveryTooEarly))
    );

    assert_eq!(
        count_control_events(&env),
        before,
        "rejections must not emit"
    );
    assert_eq!(client.get_event_seq(), 3);
    // Partial activation state must not have leaked from any failed call.
    assert!(client.is_paused());

    // Successful recover after the delay.
    env.ledger().set_timestamp(ready_at + 1);
    client.recover(&epoch, &approvals); // seq 4
    let rec = last_control_event(&env);
    assert_eq!(rec.kind, symbol_short!("recovered"));
    assert_eq!(rec.actor, None);
    assert!(!client.is_paused());
    assert_eq!(client.get_event_seq(), 4);
}

// ─── Administrator rotation & remaining emergency controls ──────────────────

/// transfer_admin records the outgoing (authorizing) admin as the actor, and
/// the newly transferred admin is the actor for subsequent privileged ops.
#[test]
fn transfer_admin_records_old_admin_then_new_admin_acts() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);

    client.initialize(&admin); // seq 1

    env.ledger().with_mut(|l| l.timestamp = 9_000);
    client.transfer_admin(&new_admin, &0); // seq 2

    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("admn_xfer"));
    assert_eq!(
        evt.actor.as_ref(),
        Some(&admin),
        "actor is the outgoing admin"
    );
    assert_eq!(evt.timestamp, 9_000);
    assert_eq!(client.get_event_seq(), 2);

    // The new admin is now the one authorizing operations.
    client.pause(); // seq 3
    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("pause"));
    assert_eq!(evt.actor.as_ref(), Some(&new_admin));
    assert_eq!(client.get_event_seq(), 3);
}

/// Module-level pause/unpause and the versioned recovery entrypoints all emit
/// control events carrying the admin actor.
#[test]
fn module_pause_and_clear_emit_admin_actor_control_events() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let module = symbol_short!("bill");

    client.initialize(&admin); // seq 1
    client.pause_module(&module); // seq 2
    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("mpause"));
    assert_eq!(evt.actor.as_ref(), Some(&admin));
    assert!(client.is_module_paused(&module));

    client.unpause_module(&module); // seq 3
    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("munpause"));
    assert_eq!(evt.actor.as_ref(), Some(&admin));
    assert!(!client.is_module_paused(&module));

    client.clear_emergency_state(); // seq 4
    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("cleared"));
    assert_eq!(evt.actor.as_ref(), Some(&admin));

    assert_eq!(client.get_event_seq(), 4);
}

/// The epoch bump, pre-upgrade snapshot, restore, discard, and migration
/// controls are all committed transitions that must emit control events.
#[test]
fn epoch_bump_snapshot_and_migration_controls_emit_control_events() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    client.initialize(&admin); // seq 1
    let old_epoch = client.get_kill_switch_epoch();
    client.bump_kill_switch_epoch(&admin); // seq 2
    let evt = last_control_event(&env);
    assert_eq!(evt.kind, symbol_short!("epch_bump"));
    assert_eq!(
        client.get_kill_switch_epoch(),
        old_epoch + 1,
        "epoch transition must be committed"
    );

    client.pre_upgrade(&admin); // seq 3
    assert_eq!(last_control_event(&env).kind, symbol_short!("snap_pre"));

    client.restore_from_snapshot(&admin); // seq 4
    assert_eq!(last_control_event(&env).kind, symbol_short!("snap_rst"));

    client.discard_snapshot(&admin); // seq 5
    assert_eq!(last_control_event(&env).kind, symbol_short!("snap_dsc"));

    assert_eq!(client.get_event_seq(), 5);
}
