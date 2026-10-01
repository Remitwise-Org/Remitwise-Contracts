#![cfg(test)]

//! Deterministic failure-boundary coverage for `BillPayments::set_pause_admin`
//! (issue #1816).
//!
//! # Subject
//!
//! `set_pause_admin(caller, new_admin)` is the **only** entry point that
//! installs, hands off, or renews the pause-admin role. Every emergency control
//! on this contract — `pause`, `unpause`, `schedule_unpause`, `pause_function`,
//! `unpause_function`, `emergency_pause_all`, `refresh_admin_grant` — is gated
//! on the `PAUSE_ADM` address and the `PADM_GT` grant that this function
//! writes, so a defect here compromises every other safeguard at once.
//!
//! # State model
//!
//! The role is stored as two independent instance-storage entries:
//!
//! | key         | meaning                                                       |
//! |-------------|---------------------------------------------------------------|
//! | `PAUSE_ADM` | the current admin address; absent == not bootstrapped         |
//! | `PADM_GT`   | ledger timestamp the grant was issued. `Some(..)` bounds the  |
//! |             | grant by [`ADMIN_GRANT_TTL`]; `None` is the legacy/un-stamped |
//! |             | state written before the TTL mechanism existed                |
//!
//! `set_pause_admin` reaches exactly two success transitions:
//!
//! * **bootstrap** — `PAUSE_ADM` absent, `caller == new_admin`; both keys are
//!   written and `PADM_GT = now`.
//! * **rotation** — `PAUSE_ADM` set to the live incumbent, `caller == incumbent`;
//!   both keys are (re)written and `PADM_GT = now`.
//!
//! and three deterministic rejections: a bootstrap that names somebody else
//! (typed `UnauthorizedPause`), a rotation by a non-incumbent (typed
//! `UnauthorizedPause`), and *any* call once the incumbent's grant has lapsed
//! (typed `AdminGrantExpired`). The TTL gate runs before the caller-identity
//! gate, so a lapsed grant is reported identically to every caller; that is
//! safe because the grant state is already a public read via
//! `get_pause_admin_grant`.
//!
//! # Invariants pinned by this module
//!
//! 1. **Exactness** — a successful call writes *both* keys: `PAUSE_ADM` is the
//!    requested address byte-for-byte and `PADM_GT` is the current ledger time.
//! 2. **Atomicity** — every rejection leaves *both* entries exactly as they
//!    were. There is no half-applied rotation, so a retry after a failure sees
//!    the same pre-state (no partial-failure data loss).
//! 3. **TTL boundary** — the grant is live through `granted_at + TTL - 1` and
//!    dead from `granted_at + TTL` (inclusive), for `set_pause_admin` exactly as
//!    for the other guarded writes.
//! 4. **Legacy migration** — a pre-TTL admin (address set, no `PADM_GT`) may
//!    rotate, and the first guarded write stamps the clock.
//! 5. **Lost-update safety** — once a rotation lands, the outgoing admin can no
//!    longer act; a stale second call from it is rejected and cannot undo the
//!    first (the deterministic model for two racing rotation attempts).
//! 6. **Retry / duplicate determinism** — repeating a successful call is an
//!    idempotent no-op on `PAUSE_ADM`; repeating a rejected call keeps
//!    rejecting with the same typed error.
//! 7. **No collateral mutation** — rotating the admin does not touch the global
//!    pause flag or any other operational state.
//!
//! Each rejection is a *typed* `BillPaymentsError` (never a bare panic), which
//! is what keeps failures diagnosable without exposing key material.

extern crate std;

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError, ADMIN_GRANT_TTL};
use proptest::prelude::*;
use soroban_sdk::{
    symbol_short,
    testutils::{EnvTestConfig, Ledger},
    Address, Env,
};
use testutils::generate_test_address;

/// Base ledger timestamp. Any non-zero value works; a realistic one keeps the
/// TTL arithmetic readable.
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
/// the key is *absent*, which lets the atomicity assertions distinguish absent
/// from set.
fn raw_pause_admin_key(env: &Env, contract_id: &Address) -> Option<Address> {
    env.as_contract(contract_id, || {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    })
}

/// Raw `PADM_GT` instance entry. `None` is the legacy / never-issued state.
fn raw_grant_timestamp(env: &Env, contract_id: &Address) -> Option<u64> {
    env.as_contract(contract_id, || {
        env.storage().instance().get(&symbol_short!("PADM_GT"))
    })
}

/// Write `PAUSE_ADM` / `PADM_GT` directly, modelling state produced by an older
/// contract version or a snapshot restore.
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

// ---------------------------------------------------------------------------
// Success path: bootstrap and rotation write both keys exactly
// ---------------------------------------------------------------------------

/// The bootstrap transition must install the address *and* start the grant
/// clock, and must return `Ok(())` so the caller can distinguish success from a
/// rejection.
#[test]
fn test_bootstrap_writes_both_keys_and_returns_ok() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    assert_eq!(
        raw_pause_admin_key(&env, &contract_id),
        Some(alice.clone()),
        "bootstrap must write PAUSE_ADM"
    );
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(T0),
        "bootstrap must stamp PADM_GT with the current ledger time"
    );
    assert_eq!(client.get_pause_admin_public(), Some(alice.clone()));

    let grant = client.get_pause_admin_grant();
    assert!(grant.usable);
    assert!(!grant.expired);
    assert_eq!(grant.granted_at, Some(T0));
}

/// A live incumbent may hand the role to a *different* address; the reader must
/// immediately report the new holder and the grant clock must advance.
#[test]
fn test_rotation_handoff_updates_both_keys() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));

    let handoff_at = T0 + 1_000;
    env.ledger().set_timestamp(handoff_at);
    assert_eq!(client.try_set_pause_admin(&alice, &bob), Ok(Ok(())));

    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(bob.clone()));
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(handoff_at));
    assert_eq!(client.get_pause_admin_public(), Some(bob.clone()));
    assert!(client.get_pause_admin_grant().usable);
}

// ---------------------------------------------------------------------------
// Permission boundaries: rejected callers leave the state byte-identical
// ---------------------------------------------------------------------------

/// Bootstrap is self-service only. Naming somebody else must be rejected with a
/// typed error and must write **nothing** — not even the grant timestamp.
#[test]
fn test_bootstrap_naming_another_address_is_rejected_and_writes_nothing() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(
        client.try_set_pause_admin(&alice, &bob),
        Err(Ok(BillPaymentsError::UnauthorizedPause))
    );
    assert_eq!(raw_pause_admin_key(&env, &contract_id), None);
    assert_eq!(raw_grant_timestamp(&env, &contract_id), None);
    assert_eq!(client.get_pause_admin_public(), None);
    assert!(!client.get_pause_admin_grant().usable);
}

/// A third party must not be able to rotate the role away from its holder, and
/// the rejected attempt must leave *both* entries untouched (atomicity).
#[test]
fn test_non_admin_rotation_is_rejected_and_state_is_byte_identical() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let mallory = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    let admin_before = raw_pause_admin_key(&env, &contract_id);
    let grant_before = raw_grant_timestamp(&env, &contract_id);

    assert_eq!(
        client.try_set_pause_admin(&mallory, &mallory),
        Err(Ok(BillPaymentsError::UnauthorizedPause))
    );
    assert_eq!(raw_pause_admin_key(&env, &contract_id), admin_before);
    assert_eq!(raw_grant_timestamp(&env, &contract_id), grant_before);
    assert_eq!(client.get_pause_admin_public(), Some(alice.clone()));
}

/// Once a rotation lands, the outgoing admin is no longer the incumbent. A
/// second, stale call from it loses the race and cannot undo the winner — the
/// deterministic model of two concurrent rotation attempts.
#[test]
fn test_outgoing_admin_cannot_undo_a_completed_rotation() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    env.ledger().set_timestamp(T0 + 10);
    assert_eq!(client.try_set_pause_admin(&alice, &bob), Ok(Ok(())));

    assert_eq!(
        client.try_set_pause_admin(&alice, &alice),
        Err(Ok(BillPaymentsError::UnauthorizedPause))
    );
    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(bob.clone()));
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(T0 + 10));
}

// ---------------------------------------------------------------------------
// TTL boundary: the core failure boundary
// ---------------------------------------------------------------------------

/// The second just before the TTL is still live: rotation succeeds and the
/// grant clock is restarted from the new ledger time.
#[test]
fn test_rotation_accepted_on_the_last_live_second() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));

    let last_live = T0 + ADMIN_GRANT_TTL - 1;
    env.ledger().set_timestamp(last_live);
    assert_eq!(client.try_set_pause_admin(&alice, &bob), Ok(Ok(())));
    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(bob.clone()));
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(last_live));
}

/// `now == granted_at + TTL` is already dead (inclusive comparison). The
/// rejected call must not restamp or refresh the grant.
#[test]
fn test_rotation_rejected_on_the_exact_expiry_second_without_restamping() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    env.ledger().set_timestamp(T0 + ADMIN_GRANT_TTL);

    assert_eq!(
        client.try_set_pause_admin(&alice, &bob),
        Err(Ok(BillPaymentsError::AdminGrantExpired))
    );
    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(alice.clone()));
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(T0),
        "a rejected rotation must not write PADM_GT"
    );
}

/// A lapsed admin cannot revive itself by re-running `set_pause_admin`; the
/// sanctioned recovery path is `refresh_admin_grant`. This closes the obvious
/// TTL bypass.
#[test]
fn test_lapsed_admin_cannot_self_refresh_through_set_pause_admin() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));

    env.ledger().set_timestamp(T0 + ADMIN_GRANT_TTL);
    assert_eq!(
        client.try_set_pause_admin(&alice, &alice),
        Err(Ok(BillPaymentsError::AdminGrantExpired)),
        "the TTL must not be bypassable by re-running set_pause_admin"
    );
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(T0));

    // Recovery is explicit and authenticated.
    assert_eq!(client.try_refresh_admin_grant(&alice), Ok(Ok(())));
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(T0 + ADMIN_GRANT_TTL));
}

/// With a lapsed grant the TTL gate runs before the caller-identity gate, so
/// every caller — incumbent or stranger — observes the same typed rejection and
/// neither can write. Grant state is already a public read via
/// `get_pause_admin_grant`, so this leaks nothing.
#[test]
fn test_lapsed_grant_rejects_every_caller_deterministically() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let stranger = generate_test_address(&env);
    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));

    env.ledger().set_timestamp(T0 + ADMIN_GRANT_TTL);
    for caller in [&alice, &stranger] {
        assert_eq!(
            client.try_set_pause_admin(caller, &stranger),
            Err(Ok(BillPaymentsError::AdminGrantExpired))
        );
    }
    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(alice));
    assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(T0));
}

// ---------------------------------------------------------------------------
// Stale / legacy state
// ---------------------------------------------------------------------------

/// Pre-TTL deployments have `PAUSE_ADM` but no `PADM_GT`. Such an admin may
/// still rotate, and the first guarded write stamps the grant clock at its own
/// ledger time.
#[test]
fn test_legacy_admin_without_grant_can_rotate_and_gets_a_stamped_grant() {
    let (env, contract_id, client) = setup();
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);
    seed_pause_state(&env, &contract_id, Some(alice.clone()), None);

    let now = 5_000u64;
    env.ledger().set_timestamp(now);
    assert_eq!(client.try_set_pause_admin(&alice, &bob), Ok(Ok(())));
    assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(bob.clone()));
    assert_eq!(
        raw_grant_timestamp(&env, &contract_id),
        Some(now),
        "the legacy migration must stamp the first guarded write's timestamp"
    );
}

// ---------------------------------------------------------------------------
// Retry / duplicate determinism
// ---------------------------------------------------------------------------

/// Repeating an identical successful call is idempotent on `PAUSE_ADM` and
/// deterministic on `PADM_GT` while the clock is frozen, so a retrying client
/// cannot oscillate the role.
#[test]
fn test_identical_call_is_idempotent_and_deterministic() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    let admin_after_first = raw_pause_admin_key(&env, &contract_id);
    let grant_after_first = raw_grant_timestamp(&env, &contract_id);

    for _ in 0..8 {
        assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
        assert_eq!(raw_pause_admin_key(&env, &contract_id), admin_after_first);
        assert_eq!(raw_grant_timestamp(&env, &contract_id), grant_after_first);
    }
}

// ---------------------------------------------------------------------------
// No collateral mutation
// ---------------------------------------------------------------------------

/// Rotating the pause admin must not disturb the global pause flag: an incident
/// handled by handing the role to a fresh responder must not silently lift the
/// halt.
#[test]
fn test_rotation_does_not_disturb_the_global_pause_state() {
    let (env, contract_id, client) = setup();
    env.ledger().set_timestamp(T0);
    let alice = generate_test_address(&env);
    let bob = generate_test_address(&env);

    assert_eq!(client.try_set_pause_admin(&alice, &alice), Ok(Ok(())));
    assert_eq!(client.try_pause(&alice), Ok(Ok(())));
    assert!(client.is_paused());

    env.ledger().set_timestamp(T0 + 60);
    assert_eq!(client.try_set_pause_admin(&alice, &bob), Ok(Ok(())));

    assert!(client.is_paused(), "admin rotation must not lift a pause");
    assert_eq!(client.get_pause_admin_public(), Some(bob));
}

// ---------------------------------------------------------------------------
// Property test over the TTL grid
// ---------------------------------------------------------------------------

/// Across the (grant, now) grid, `set_pause_admin` succeeds **iff** the grant is
/// live, and on success writes both keys exactly; on failure it writes nothing.
/// This is the single strongest statement of the boundary: no off-by-one, no
/// partial write, no saturation surprise.
proptest! {
    #[test]
    fn prop_set_pause_admin_is_gated_exactly_by_the_grant_ttl(
        granted_at in 0u64..(1u64 << 40),
        skew in 0u64..(2 * ADMIN_GRANT_TTL + 2),
        rotate_to_self in any::<bool>(),
    ) {
        let (env, contract_id, client) = setup();
        let admin = generate_test_address(&env);
        let other = generate_test_address(&env);
        seed_pause_state(&env, &contract_id, Some(admin.clone()), Some(granted_at));

        let now = granted_at.saturating_add(skew);
        env.ledger().set_timestamp(now);
        let new_admin = if rotate_to_self { admin.clone() } else { other.clone() };

        let result = client.try_set_pause_admin(&admin, &new_admin);
        let grant_live = now < granted_at.saturating_add(ADMIN_GRANT_TTL);

        if grant_live {
            prop_assert_eq!(result, Ok(Ok(())));
            prop_assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(new_admin.clone()));
            prop_assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(now));
        } else {
            prop_assert_eq!(result, Err(Ok(BillPaymentsError::AdminGrantExpired)));
            prop_assert_eq!(raw_pause_admin_key(&env, &contract_id), Some(admin.clone()));
            prop_assert_eq!(raw_grant_timestamp(&env, &contract_id), Some(granted_at));
        }
    }
}
