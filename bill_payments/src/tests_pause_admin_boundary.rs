#![cfg(test)]

//! Deterministic failure-boundary coverage for `BillPayments::get_pause_admin_public`
//! and its grant-aware companion `get_pause_admin_grant` (issue #1833).
//!
//! # What is under test
//!
//! The pause-admin role is the root of every emergency control on this
//! contract (`pause`, `unpause`, `schedule_unpause`, `pause_function`,
//! `unpause_function`, `emergency_pause_all`, `refresh_admin_grant`,
//! `set_pause_admin`). `get_pause_admin_public` is the *only* unauthenticated
//! way for a monitor, an indexer, or an incident responder to learn who holds
//! that role, so its boundaries are the contract between on-chain state and
//! the humans/robots that respond to incidents.
//!
//! The role is stored as two independent instance entries:
//!
//! - `PAUSE_ADM` — the address. Read by `get_pause_admin_public`.
//! - `PADM_GT` — the timestamp at which the grant was last issued, bounded by
//!   [`ADMIN_GRANT_TTL`] and enforced *only* on the write paths, via
//!   `require_admin_grant_valid`.
//!
//! # Invariants pinned here
//!
//! 1. **Total read** — every reachable state returns a value. `None` means
//!    "not bootstrapped yet", never "error", and the getter never panics.
//! 2. **Purity** — reads create no `PAUSE_ADM`/`PADM_GT` entry, rewrite
//!    nothing, and emit no events, so polling the getter cannot start an
//!    admin's grant clock.
//! 3. **TTL independence** — the address is reported even after the grant
//!    lapses; expiry is a *write-path* rejection, not a read-path erasure.
//! 4. **Reader/writer agreement** — `get_pause_admin_grant().expired` is
//!    exactly `require_admin_grant_valid().is_err()` for every grant
//!    timestamp (property-tested across the grid), and
//!    `get_pause_admin_grant().admin` is exactly what `get_pause_admin_public`
//!    returns.
//! 5. **No drift under retry / partial failure** — a rejected
//!    `set_pause_admin` (unauthorised, or after a lapsed grant) leaves the
//!    reader's answer untouched; a repeated read is idempotent; a retried
//!    `restore_from_snapshot` fails cleanly and disturbs nothing.
//! 6. **Incident-safe** — the global pause, the per-function pause flags, and
//!    the kill switch gate writes only. The read path stays available while
//!    writes are halted.
//! 7. **Monotonic expiry** — once `expired` is true it can never return to
//!    false, so an operator's "is this admin still in charge?" poll is stable.
//! 8. **Snapshot fidelity** — `pre_upgrade` / `restore_from_snapshot` round
//!    trip the address exactly, including restoring `None` (revoking it).

extern crate std;

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError, ADMIN_GRANT_TTL};
use proptest::prelude::*;
use remitwise_common::{activate_kill_switch, require_no_active_kill_switch};
use soroban_sdk::{
    symbol_short,
    testutils::{EnvTestConfig, Events, Ledger},
    Address, Env,
};
use testutils::generate_test_address;

/// Base ledger timestamp for the TTL tests. Any non-zero value works; a
/// realistic one keeps the arithmetic readable.
const T0: u64 = 1_700_000_000;

fn setup() -> (Env, Address, BillPaymentsClient<'static>) {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.budget().reset_unlimited();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    (env, contract_id, client)
}

/// Raw `PAUSE_ADM` instance entry, bypassing the public getters. `None` means
/// the key is *absent* (not merely "holds null"), which is what lets the
/// purity assertions distinguish absent from set.
fn raw_pause_admin_key(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    })
}

/// Raw `PADM_GT` instance entry. `None` is the legacy / never-issued state.
fn raw_grant_timestamp(env: &Env, contract_id: &Address) -> Option<u64> {
    env.as_contract(contract_id, || -> Option<u64> {
        env.storage().instance().get(&symbol_short!("PADM_GT"))
    })
}

/// Write `PAUSE_ADM` / `PADM_GT` directly, modelling state produced by an older
/// contract version, a snapshot restore, or a corrupt ledger entry.
fn seed_pause_state(
    env: &Env,
    contract_id: &Address,
    admin: Option<Address>,
    granted: Option<u64>,
) {
    env.as_contract(contract_id, || {
        match admin {
            Some(addr) => env
                .storage()
                .instance()
                .set(&symbol_short!("PAUSE_ADM"), &addr),
            None => env.storage().instance().remove(&symbol_short!("PAUSE_ADM")),
        }
        match granted {
            Some(ts) => env.storage().instance().set(&symbol_short!("PADM_GT"), &ts),
            None => env.storage().instance().remove(&symbol_short!("PADM_GT")),
        }
    });
}

/// The private write-path predicate that actually gates every pause/refresh
/// call, used as the oracle the public grant view is compared against. A future
/// divergence between the reader and the writer fails here rather than in
/// production.
fn write_path_grant_valid(env: &Env, contract_id: &Address) -> bool {
    env.as_contract(contract_id, || {
        BillPayments::require_admin_grant_valid(env).is_ok()
    })
}

// ---------------------------------------------------------------------------
// 1. Total read: the "no admin yet" and "admin cleared" states
// ---------------------------------------------------------------------------

/// Before bootstrap the getter must return `None` and must **not** create the
/// `PAUSE_ADM` key. A reader that materialised the key would turn a
/// permissionless read into a state mutation, and would make the bootstrap
/// branch of `set_pause_admin` behave differently on a fresh contract than on
/// one a monitor had already polled.
#[test]
fn test_get_pause_admin_public_returns_none_before_bootstrap() {
    let (env, contract_id, client) = setup();

    assert_eq!(client.get_pause_admin_public(), None);
    assert_eq!(client.get_pause_admin_public(), None, "read is repeatable");
    assert_eq!(
        raw_pause_admin_key(&env, &contract_id),
        None,
        "a pure read must not materialise the PAUSE_ADM entry"
    );

    let grant = client.get_pause_admin_grant();
    assert_eq!(grant.admin, None);
    assert_eq!(grant.granted_at, None);
    assert_eq!(grant.expires_at, None);
    assert!(!grant.expired);
    assert!(
        !grant.usable,
        "with no admin configured there is nobody who can act"
    );
}

/// `restore_from_snapshot` must be able to *clear* the pause admin, and the
/// reader must report the cleared state as a plain `None` — not a stale cached
/// address, and not an error.
#[test]
fn test_get_pause_admin_public_returns_none_after_snapshot_clears_admin() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);

    let upgrade_admin = generate_test_address(&env);
    let pause_admin = generate_test_address(&env);

    // Snapshot *before* any pause admin exists.
    let _ = client.set_upgrade_admin(&upgrade_admin, &upgrade_admin);
    let _ = client.pre_upgrade(&upgrade_admin);

    // Then bootstrap a pause admin, which the snapshot must not contain.
    let _ = client.set_pause_admin(&pause_admin, &pause_admin);
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin.clone()));

    let _ = client.restore_from_snapshot(&upgrade_admin);

    assert_eq!(
        client.get_pause_admin_public(),
        None,
        "restoring a snapshot with no pause admin must revoke the role"
    );
    assert_eq!(raw_pause_admin_key(&env, &contract_id), None);
    assert!(!client.get_pause_admin_grant().usable);
}

// ---------------------------------------------------------------------------
// 2. Success path: rotation is reflected, and never with a stale value
// ---------------------------------------------------------------------------

/// Every authorised rotation must be visible immediately, and the reader must
/// never surface a previous holder — the property an operator relies on to
/// stop trusting a compromised key.
#[test]
fn test_get_pause_admin_public_tracks_every_authorised_rotation() {
    let (env, _contract_id, client) = setup();
    env.ledger().set_timestamp(T0);

    let first = generate_test_address(&env);
    let second = generate_test_address(&env);
    let third = generate_test_address(&env);

    let _ = client.set_pause_admin(&first, &first);
    assert_eq!(client.get_pause_admin_public(), Some(first.clone()));

    let _ = client.set_pause_admin(&first, &second);
    assert_eq!(
        client.get_pause_admin_public(),
        Some(second.clone()),
        "the outgoing admin must not remain readable after rotation"
    );

    let _ = client.set_pause_admin(&second, &third);
    assert_eq!(client.get_pause_admin_public(), Some(third.clone()));
    assert!(client.get_pause_admin_grant().usable);
}

/// A long chain of rotations must round-trip every distinct address with zero
/// transformation, so an operator is never handed a "close enough" address.
#[test]
fn test_many_distinct_admin_addresses_round_trip_unchanged() {
    let (env, _contract_id, client) = setup();
    env.ledger().set_timestamp(T0);

    let mut current = generate_test_address(&env);
    let _ = client.set_pause_admin(&current, &current);
    assert_eq!(client.get_pause_admin_public(), Some(current.clone()));

    for _ in 0..16 {
        let next = generate_test_address(&env);
        let _ = client.set_pause_admin(&current, &next);
        assert_eq!(client.get_pause_admin_public(), Some(next.clone()));
        current = next;
    }
}

/// The grant view's `admin` field and the legacy getter must be the same value
/// in every reachable state — they read the same entry and must never drift.
#[test]
fn test_grant_view_admin_matches_legacy_getter_in_every_state() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);

    let admin = generate_test_address(&env);
    let states: [(Option<Address>, Option<u64>); 4] = [
        (None, None),
        (None, Some(T0)),
        (Some(admin.clone()), None),
        (Some(admin.clone()), Some(T0)),
    ];

    for (seeded_admin, granted) in states {
        seed_pause_state(&env, &contract_id, seeded_admin.clone(), granted);
        let legacy = client.get_pause_admin_public();
        assert_eq!(
            client.get_pause_admin_grant().admin,
            legacy,
            "grant view and legacy getter must agree for seeded state \
             (admin set: {}, grant set: {})",
            seeded_admin.is_some(),
            granted.is_some()
        );
        // The legacy getter is a thin wrapper over the private reader.
        assert_eq!(legacy, raw_pause_admin_key(&env, &contract_id));
    }
}

// ---------------------------------------------------------------------------
// 3. Purity / retry: repeated reads change nothing
// ---------------------------------------------------------------------------

/// Reading is idempotent, silent, and non-mutating. This is the retry-safety
/// property: a monitor polling in a loop (or a retrying client) must not be
/// able to perturb the state it observes, and must not spam the event stream
/// that incident tooling tails.
#[test]
fn test_repeated_reads_are_idempotent_silent_and_non_mutating() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);

    let admin = generate_test_address(&env);
    let _ = client.set_pause_admin(&admin, &admin);

    let admin_before = raw_pause_admin_key(&env, &contract_id);
    let grant_before = raw_grant_timestamp(&env, &contract_id);
    let events_before = env.events().all().len();

    for _ in 0..25 {
        assert_eq!(client.get_pause_admin_public(), Some(admin.clone()));
    }
    let first_grant = client.get_pause_admin_grant();
    for _ in 0..25 {
        assert_eq!(client.get_pause_admin_grant(), first_grant);
    }

    assert_eq!(
        raw_pause_admin_key(&env, &contract_id),
        admin_before,
        "reads must not rewrite PAUSE_ADM"
    );
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        grant_before,
        "reads must not create or refresh PADM_GT"
    );
    assert_eq!(
        env.events().all().len(),
        events_before,
        "reads must not emit events"
    );
}

/// A grant that has already lapsed is dead forever: advancing the clock can
/// only keep it dead. This is what makes "poll until the admin is back in
/// charge" a safe operator loop — the poll cannot oscillate.
#[test]
fn test_expiry_is_monotonic_across_the_ttl_boundary() {
    let (env, contract_id, client) = setup();

    let admin = generate_test_address(&env);
    let granted_at = 1_000u64;
    seed_pause_state(&env, &contract_id, Some(admin.clone()), Some(granted_at));
    let deadline = granted_at + ADMIN_GRANT_TTL;

    let mut seen_expired = false;
    for now in (deadline - 5)..=(deadline + 5) {
        env.ledger().set_timestamp(now);
        let grant = client.get_pause_admin_grant();
        if seen_expired {
            assert!(
                grant.expired,
                "expiry regressed at t={now}: a lapsed grant must stay lapsed"
            );
            assert!(
                !grant.usable,
                "a lapsed grant must stay unusable at t={now}"
            );
        }
        if now < deadline {
            assert!(!grant.expired, "t={now} must still be live");
            assert!(grant.usable, "t={now} must still be usable");
        }
        seen_expired |= grant.expired;
    }
    assert!(seen_expired, "the grant must have lapsed inside the sweep");
    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin),
        "the address survives the grant's death"
    );
}

// ---------------------------------------------------------------------------
// 4. The TTL boundary — the core failure boundary
// ---------------------------------------------------------------------------

/// `now == granted_at + TTL` is already expired (the write-path comparison is
/// inclusive) and the second before it is still live. Both sides of the
/// boundary are asserted so an off-by-one in either direction fails here.
#[test]
fn test_grant_expires_exactly_on_the_ttl_second() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);

    let granted_at = raw_grant_timestamp(&env, &contract_id).expect("grant issued");
    assert_eq!(
        granted_at, T0,
        "set_pause_admin must stamp PADM_GT with the current ledger time"
    );

    // Last live second.
    env.ledger().set_timestamp(granted_at + ADMIN_GRANT_TTL - 1);
    let grant = client.get_pause_admin_grant();
    assert!(
        !grant.expired,
        "t = granted_at + TTL - 1 must still be live"
    );
    assert!(grant.usable);
    assert_eq!(grant.expires_at, Some(granted_at + ADMIN_GRANT_TTL));

    // First dead second.
    env.ledger().set_timestamp(granted_at + ADMIN_GRANT_TTL);
    let grant = client.get_pause_admin_grant();
    assert!(
        grant.expired,
        "t = granted_at + TTL must already be expired (inclusive bound)"
    );
    assert!(!grant.usable);
    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin),
        "the address stays reportable after the grant lapses — that is how an \
         operator finds the stuck key"
    );
}

/// The reader and the write path must reach the *same* verdict at the
/// boundary, in the legacy state, and for a saturating (near-`u64::MAX`)
/// grant timestamp. Any divergence would let a lapsed admin act, or would
/// lock out a live one.
#[test]
fn test_reader_expiry_always_agrees_with_the_write_path() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    let ttl = ADMIN_GRANT_TTL;

    for (granted_at, now) in [
        (0u64, 0u64),
        (0, 1),
        (1_000, 1_000),
        (1_000, 1_000 + ttl - 1),
        (1_000, 1_000 + ttl),
        (1_000, 1_000 + ttl + 1),
        (u64::MAX - ttl, u64::MAX - ttl),
        // Saturating deadline: `granted_at + TTL` must clamp to u64::MAX and
        // stay in the future, never wrap around to a date in the past.
        (u64::MAX - 1, u64::MAX - 1),
    ] {
        seed_pause_state(&env, &contract_id, Some(admin.clone()), Some(granted_at));
        env.ledger().set_timestamp(now);

        let grant = client.get_pause_admin_grant();
        let write_path_ok = write_path_grant_valid(&env, &contract_id);
        assert_eq!(
            grant.expired, !write_path_ok,
            "reader/writer expiry disagree for granted_at={granted_at} now={now}"
        );
        assert_eq!(
            grant.usable, write_path_ok,
            "reader/writer usability disagree for granted_at={granted_at} now={now}"
        );
        assert_eq!(grant.admin, Some(admin.clone()));
        assert_eq!(
            client.get_pause_admin_public(),
            Some(admin.clone()),
            "lapsing the grant must never hide the address"
        );
    }
}

/// Near-`u64::MAX` grant timestamps must saturate, not wrap. A wrapped
/// deadline would land in the distant past and permanently lock out a live
/// admin — an unrecoverable lockout that no public view would explain.
#[test]
fn test_grant_deadline_saturates_instead_of_wrapping() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    let granted_at = u64::MAX - 1;
    seed_pause_state(&env, &contract_id, Some(admin.clone()), Some(granted_at));
    env.ledger().set_timestamp(1_000);

    let grant = client.get_pause_admin_grant();
    assert_eq!(grant.granted_at, Some(granted_at));
    assert_eq!(grant.expires_at, Some(u64::MAX));
    assert!(
        !grant.expired,
        "a wrapped deadline would wrongly declare a live grant expired"
    );
    assert!(grant.usable);
    assert!(write_path_grant_valid(&env, &contract_id));
}

/// A lapsed grant must block the write path while the read path keeps
/// reporting the stuck admin — the "incident" boundary where a half-broken
/// control plane must still be diagnosable.
#[test]
fn test_lapsed_grant_blocks_writes_but_reader_still_reports_admin() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);

    let granted_at = raw_grant_timestamp(&env, &contract_id).expect("grant issued");
    env.ledger().set_timestamp(granted_at + ADMIN_GRANT_TTL);

    assert_eq!(
        client.try_pause(&admin),
        Err(Ok(BillPaymentsError::AdminGrantExpired)),
        "a lapsed grant must reject the write with a typed, diagnosable error"
    );

    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin),
        "the rejected write must not erase the admin from the reader"
    );
    let grant = client.get_pause_admin_grant();
    assert!(grant.expired);
    assert!(!grant.usable);
    assert!(
        !client.is_paused(),
        "the rejected write must not take effect"
    );
}

/// The only supported way out of a lapsed grant is the recorded admin acting.
/// Polling the reader must not be a back door into reviving it.
#[test]
fn test_refresh_admin_grant_reinstates_a_lapsed_grant_but_polling_does_not() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);

    let granted_at = raw_grant_timestamp(&env, &contract_id).expect("grant issued");
    let revive_at = granted_at + ADMIN_GRANT_TTL + 1_000;
    env.ledger().set_timestamp(revive_at);
    assert!(client.get_pause_admin_grant().expired);

    // 50 polls by an unauthenticated observer: still dead, grant stamp intact.
    for _ in 0..50 {
        assert!(client.get_pause_admin_grant().expired);
    }
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(granted_at),
        "polling the reader must not start or restart the grant clock"
    );

    // The recorded admin can still refresh its own grant — the address is
    // reported even though the grant lapsed, which is the recovery path.
    let _ = client.refresh_admin_grant(&admin);
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(revive_at),
        "refresh must restamp PADM_GT with the current ledger time"
    );

    let grant = client.get_pause_admin_grant();
    assert!(!grant.expired);
    assert!(grant.usable);
    assert_eq!(grant.admin, Some(admin));
    let _ = client.pause(&admin);
    assert!(client.is_paused(), "the revived grant must be able to act");
}

// ---------------------------------------------------------------------------
// 5. Legacy / stale state: the lazy-migration boundary
// ---------------------------------------------------------------------------

/// Pre-TTL deployments have `PAUSE_ADM` but no `PADM_GT`. The reader must
/// still answer, must report the grant as "unknown but usable" (matching the
/// write path, which allows the legacy state and starts the clock there), and
/// must **not** perform the migration itself.
#[test]
fn test_legacy_admin_without_grant_timestamp_is_readable_and_polling_is_inert() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    seed_pause_state(&env, &contract_id, Some(admin.clone()), None);

    for _ in 0..10 {
        let grant = client.get_pause_admin_grant();
        assert_eq!(grant.admin, Some(admin.clone()));
        assert_eq!(grant.granted_at, None);
        assert_eq!(grant.expires_at, None);
        assert!(
            !grant.expired,
            "no grant timestamp means no expiry to report"
        );
        assert!(grant.usable, "legacy admins are allowed to act");
        assert_eq!(client.get_pause_admin_public(), Some(admin.clone()));
    }

    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        None,
        "the unauthenticated read path must not run the legacy migration"
    );
}

/// The lazy migration belongs to the authenticated write path, and it stamps
/// the current ledger time so the TTL starts from the first write rather than
/// from whenever the contract was originally deployed.
#[test]
fn test_legacy_migration_is_performed_by_the_write_path_only() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    seed_pause_state(&env, &contract_id, Some(admin.clone()), None);

    let now = 5_000u64;
    env.ledger().set_timestamp(now);

    let _ = client.pause(&admin);

    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(now),
        "the first guarded write must start the TTL clock at its own timestamp"
    );
    let grant = client.get_pause_admin_grant();
    assert!(!grant.expired);
    assert!(grant.usable);
    assert!(client.is_paused());
}

// ---------------------------------------------------------------------------
// 6. Permission states: rejected writes leave the reader untouched
// ---------------------------------------------------------------------------

/// Bootstrap is self-service only: naming somebody else must be rejected and
/// must not write anything.
#[test]
fn test_rejected_bootstrap_leaves_reader_unset() {
    let (env, contract_id, client) = setup();
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(
        client.try_set_pause_admin(&alice, &bob),
        Err(Ok(BillPaymentsError::UnauthorizedPause))
    );

    assert_eq!(client.get_pause_admin_public(), None);
    assert_eq!(
        raw_pause_admin_key(&env, &contract_id),
        None,
        "a rejected bootstrap must not create PAUSE_ADM"
    );
    assert_eq!(raw_grant_timestamp(&env, &contract_id), None);
    assert!(!client.get_pause_admin_grant().usable);
}

/// A third party must not be able to rotate the role away from its holder, and
/// the failed attempt must leave the incumbent readable.
#[test]
fn test_rejected_unauthorised_rotation_leaves_reader_unchanged() {
    let (env, _contract_id, client) = setup();
    let admin = generate_test_address(&env);
    let mallory = generate_test_address(&env);
    let _ = client.set_pause_admin(&admin, &admin);

    assert_eq!(
        client.try_set_pause_admin(&mallory, &mallory),
        Err(Ok(BillPaymentsError::UnauthorizedPause))
    );
    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin),
        "a rejected rotation must not clear or replace the incumbent"
    );
    assert!(client.get_pause_admin_grant().usable);
}

/// A rotation attempted after the grant lapsed is rejected *before* it can
/// write, and the reader keeps reporting the now-stuck admin. This closes the
/// obvious bypass: "my grant expired, so let me rotate to myself".
#[test]
fn test_rotation_after_expiry_is_rejected_and_leaves_reader_unchanged() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    let successor = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);

    let granted_at = raw_grant_timestamp(&env, &contract_id).expect("grant issued");
    env.ledger().set_timestamp(granted_at + ADMIN_GRANT_TTL);

    assert_eq!(
        client.try_set_pause_admin(&admin, &successor),
        Err(Ok(BillPaymentsError::AdminGrantExpired)),
        "the TTL must not be bypassable by routing through set_pause_admin"
    );
    assert_eq!(client.get_pause_admin_public(), Some(admin));
    assert_eq!(
        raw_pause_admin_key(&env, &contract_id),
        Some(admin),
        "a rejected rotation must not partially apply"
    );
}

/// `set_pause_admin(admin, admin)` is an allowed no-op rotation that doubles as
/// a grant refresh. The reader's answer must be unchanged while the grant
/// clock moves forward — the "duplicate input" boundary.
#[test]
fn test_duplicate_same_admin_rotation_is_idempotent_and_refreshes_grant() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);
    let first_stamp = raw_grant_timestamp(&env, &contract_id).expect("grant issued");

    let later = first_stamp + 1_000;
    env.ledger().set_timestamp(later);
    let _ = client.set_pause_admin(&admin, &admin);

    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin.clone()),
        "a same-admin rotation must not change the reported address"
    );
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(later),
        "a same-admin rotation is the documented way to renew a grant"
    );
    let grant = client.get_pause_admin_grant();
    assert!(grant.usable);
    assert_eq!(grant.granted_at, Some(later));
}

/// Rotating the role requires the caller's signature: `set_pause_admin` is not
/// reachable without `require_auth`, not even by a would-be new admin.
#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_set_pause_admin_requires_caller_auth() {
    // Deliberately no `mock_all_auths`: the bootstrap below must fail at
    // `require_auth` before touching storage.
    let env = Env::default();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let alice = generate_test_address(&env);

    let _ = client.set_pause_admin(&alice, &alice);
}

// ---------------------------------------------------------------------------
// 7. Operational states: pause and kill switch must not blind the reader
// ---------------------------------------------------------------------------

/// While the contract is paused — the state an incident responder is
/// diagnosing — the reader must still say who is able to lift the pause.
#[test]
fn test_reader_unaffected_while_contract_is_paused() {
    let (env, _contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);
    let before = client.get_pause_admin_grant();

    let _ = client.pause(&admin);
    assert!(client.is_paused());

    assert_eq!(client.get_pause_admin_public(), Some(admin));
    let after = client.get_pause_admin_grant();
    assert_eq!(after, before, "pausing must not perturb the grant view");
    assert!(
        after.usable,
        "a paused contract must not imply a dead grant"
    );
}

/// The kill switch halts writes only. Monitoring must keep working while it is
/// engaged, otherwise the responder is blind exactly when they need to be.
#[test]
fn test_reader_unaffected_while_kill_switch_is_engaged() {
    let (env, contract_id, client) = setup();
    let admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);
    let _ = client.set_pause_admin(&admin, &admin);

    env.as_contract(&contract_id, || {
        activate_kill_switch(&env);
    });
    // Sanity: the guard really is engaged for the write paths.
    let blocked = env.as_contract(&contract_id, || require_no_active_kill_switch(&env));
    assert!(blocked.is_err(), "writes must be blocked once engaged");

    assert_eq!(
        client.get_pause_admin_public(),
        Some(admin),
        "the pause admin must stay discoverable during a kill-switch incident"
    );
    let grant = client.get_pause_admin_grant();
    assert!(
        grant.usable,
        "the grant itself is unaffected by the kill switch"
    );
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// 8. Snapshot fidelity and retry safety
// ---------------------------------------------------------------------------

/// A snapshot/restore round trip is value-preserving: restoring must never
/// corrupt the pause admin, and must never leave a dangling reader value.
#[test]
fn test_snapshot_restore_round_trip_preserves_pause_admin() {
    let (env, _contract_id, client) = setup();
    let upgrade_admin = generate_test_address(&env);
    let pause_admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);

    let _ = client.set_upgrade_admin(&upgrade_admin, &upgrade_admin);
    let _ = client.set_pause_admin(&pause_admin, &pause_admin);
    let original = client.get_pause_admin_grant();

    let _ = client.pre_upgrade(&upgrade_admin);
    let _ = client.restore_from_snapshot(&upgrade_admin);

    let restored = client.get_pause_admin_grant();
    assert_eq!(restored.admin, Some(pause_admin));
    assert_eq!(restored.usable, original.usable);
    assert_eq!(restored.expired, original.expired);
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin));
}

/// Restoring a snapshot that captured no pause admin must revoke the role, so
/// a rollback cannot silently leave a half-restored admin in place.
#[test]
fn test_snapshot_without_pause_admin_revokes_the_role_on_restore() {
    let (env, _contract_id, client) = setup();
    let upgrade_admin = generate_test_address(&env);
    let pause_admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);

    let _ = client.set_upgrade_admin(&upgrade_admin, &upgrade_admin);
    let _ = client.pre_upgrade(&upgrade_admin);
    let _ = client.set_pause_admin(&pause_admin, &pause_admin);
    assert_eq!(client.get_pause_admin_public(), Some(pause_admin));

    let _ = client.restore_from_snapshot(&upgrade_admin);
    assert_eq!(client.get_pause_admin_public(), None);
    assert!(!client.get_pause_admin_grant().usable);
}

/// A retried `restore_from_snapshot` (a client resubmitting after a timeout,
/// say) must fail cleanly and must leave the reader's answer stable — both when
/// the snapshot was already consumed and when it has gone stale.
#[test]
fn test_retried_snapshot_restore_fails_cleanly_and_reader_is_stable() {
    let (env, _contract_id, client) = setup();
    let upgrade_admin = generate_test_address(&env);
    let pause_admin = generate_test_address(&env);
    env.ledger().set_timestamp(T0);

    let _ = client.set_upgrade_admin(&upgrade_admin, &upgrade_admin);
    let _ = client.set_pause_admin(&pause_admin, &pause_admin);
    let _ = client.pre_upgrade(&upgrade_admin);

    let _ = client.restore_from_snapshot(&upgrade_admin);
    let after_first = client.get_pause_admin_public();
    assert_eq!(after_first, Some(pause_admin.clone()));

    // The snapshot was consumed by the first restore: the retry must fail.
    assert_eq!(
        client.try_restore_from_snapshot(&upgrade_admin),
        Err(Ok(BillPaymentsError::SnapshotNotFound))
    );
    assert_eq!(
        client.get_pause_admin_public(),
        after_first,
        "a failed retry must not disturb the restored state"
    );

    // A stale snapshot is rejected too, and again leaves the reader alone.
    let _ = client.pre_upgrade(&upgrade_admin);
    env.ledger().set_timestamp(T0 + 31 * 24 * 3_600);
    assert_eq!(
        client.try_restore_from_snapshot(&upgrade_admin),
        Err(Ok(BillPaymentsError::SnapshotTooOld))
    );
    assert_eq!(client.get_pause_admin_public(), after_first);
}

// ---------------------------------------------------------------------------
// 9. Property tests over the grant grid
// ---------------------------------------------------------------------------

/// Across the (grant, now) grid the public grant view must agree with the
/// private write-path predicate, and the reported admin must never disappear.
proptest! {
    #[test]
    fn prop_grant_view_matches_write_path(
        granted_at in 0u64..(1u64 << 40),
        skew in 0u64..(2 * ADMIN_GRANT_TTL + 2),
    ) {
        let (env, contract_id, client) = setup();
        let admin = generate_test_address(&env);
        seed_pause_state(&env, &contract_id, Some(admin.clone()), Some(granted_at));
        let now = granted_at.saturating_add(skew);
        env.ledger().set_timestamp(now);

        let grant = client.get_pause_admin_grant();
        let write_path_ok = write_path_grant_valid(&env, &contract_id);

        prop_assert_eq!(grant.granted_at, Some(granted_at));
        prop_assert_eq!(
            grant.expires_at,
            Some(granted_at.saturating_add(ADMIN_GRANT_TTL))
        );
        prop_assert_eq!(grant.expired, !write_path_ok);
        prop_assert_eq!(grant.usable, write_path_ok);
        prop_assert_eq!(grant.admin, Some(admin.clone()));
        // TTL independence: the address survives the grant's death.
        prop_assert_eq!(client.get_pause_admin_public(), Some(admin));
    }
}

/// With no grant timestamp at all, the view is stable and inert however far
/// the clock is advanced: no expiry, no mutation, still usable.
proptest! {
    #[test]
    fn prop_legacy_state_is_stable_at_any_time(now in 0u64..(1u64 << 50)) {
        let (env, contract_id, client) = setup();
        let admin = generate_test_address(&env);
        seed_pause_state(&env, &contract_id, Some(admin.clone()), None);
        env.ledger().set_timestamp(now);

        let grant = client.get_pause_admin_grant();
        prop_assert_eq!(grant.granted_at, None);
        prop_assert_eq!(grant.expires_at, None);
        prop_assert!(!grant.expired);
        prop_assert!(grant.usable);
        prop_assert_eq!(client.get_pause_admin_public(), Some(admin.clone()));
        prop_assert_eq!(raw_grant_timestamp(&env, &contract_id), None);
    }
}
