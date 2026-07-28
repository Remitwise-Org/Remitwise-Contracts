//! Integration tests for the registered-operator lifecycle.
//!
//! The "operator" is a family-member address registered in `FamilyWallet`
//! with a `FamilyRole` (Owner / Admin / Member / Viewer). The contract must
//! distinguish three states on every privileged operation:
//!
//! - **Registered** — the operator has a `FamilyMember` row in storage under
//!   the given role. Admin / Owner-privileged paths accept the address.
//! - **Not-registered** — the address was never added. Storage returns `None`,
//!   privileged paths refuse the caller.
//! - **Revoked** — the address was added and then either removed, or had its
//!   role expiry pass. Privileged paths must treat it as Not-registered.
//!
//! These tests pin the boundary between those three states. They are
//! intentionally short and assertive so a regression in `add_family_member`,
//! `remove_family_member`, `set_role_expiry`, or the internal
//! `role_has_expired` helper produces a single, named compile failure rather
//! than a behaviour change slipping past review.
//!
//! All time advancement is deterministic: `testutils::set_ledger_time` is the
//! only mechanism used to move the ledger clock; no OS-clock reads.

#![cfg(test)]

use family_wallet::{Client as FamilyWalletClient, FamilyWallet};
use remitwise_common::FamilyRole;
use soroban_sdk::{testutils::Address as _, vec, Address, Env};
use testutils::set_ledger_time;

/// `init` creates an Owner with no members. After that, a freshly generated
/// address is in the **Not-registered** state: storage has no `FamilyMember`
/// for it, and admin-only entrypoints must refuse it.
#[test]
fn get_family_member_returns_none_for_never_registered_address() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    // No members passed — only the Owner is in storage.
    let initial_members = vec![&env];
    client.init(&owner, &initial_members);

    let never_added = Address::generate(&env);

    // Storage boundary: address was never inserted.
    assert!(
        client.get_family_member(&never_added).is_none(),
        "an address that was never added must not have a FamilyMember record"
    );
    // No expiry was ever set for it.
    assert_eq!(
        client.get_role_expiry_public(&never_added),
        None,
        "an address that was never added must have no role expiry recorded"
    );
}

/// A missing (Not-registered) address cannot add new members — adding is a
/// privileged action gated by `is_owner_or_admin`. This is the sad-path
/// counterpart to the happy-path registration test below.
#[test]
#[should_panic(expected = "Only Owner or Admin can add family members")]
fn add_family_member_rejects_unregistered_caller_as_not_authorized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let unregistered = Address::generate(&env);
    let target = Address::generate(&env);

    client.init(&owner, &vec![&env]);

    // `unregistered` is in the Not-registered state. Privileged entrypoints
    // must refuse it, even if mock_all_auths() is set.
    client.add_family_member(&unregistered, &target, &FamilyRole::Member);
}

/// After `add_family_member`, the address moves into the **Registered** state.
/// `get_family_member` returns `Some(_)` with the exact role passed.
#[test]
fn get_family_member_returns_registered_role_after_add_family_member() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    assert!(
        client.add_family_member(&owner, &admin, &FamilyRole::Admin),
        "owner must be allowed to add an Admin operator"
    );

    let registered = client.get_family_member(&admin);
    assert!(
        registered.is_some(),
        "address added via add_family_member must be in the Registered state"
    );
    let operator = registered.unwrap();
    assert_eq!(
        operator.role,
        FamilyRole::Admin,
        "the stored role must equal the role passed to add_family_member"
    );
}

/// Removing an operator transitions it from Registered → **Revoked**:
/// subsequent reads of `get_family_member` and `get_role_expiry_public`
/// must both return empty for that address.
#[test]
fn get_family_member_returns_none_after_remove_family_member() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let operator = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &operator, &FamilyRole::Admin);
    // Sanity check: before the boundary transition, we are Registered.
    assert!(client.get_family_member(&operator).is_some());

    // Revoke via removal.
    assert!(client.remove_family_member(&owner, &operator));

    // Post-removal, storage must show the operator as Not-registered: no
    // FamilyMember record and no dangling role expiry.
    assert!(
        client.get_family_member(&operator).is_none(),
        "removed operator must not retain a FamilyMember record"
    );
    assert_eq!(
        client.get_role_expiry_public(&operator),
        None,
        "removed operator must not retain a stale role expiry"
    );
}

/// An operator with a configured role expiry passes from Registered into
/// **Revoked** at the boundary timestamp. Privileged actions gated on
/// `role_has_expired` must refuse the address exactly at `t == expiry`
/// (inclusive boundary, matching the contract's documented semantics).
#[test]
#[should_panic(expected = "Only Owner or Admin can configure emergency settings")]
fn treat_role_expired_at_boundary_timestamp_as_revoked() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let expiry = 1_010u64;
    assert!(client.set_role_expiry(&owner, &admin, &Some(expiry)));
    assert_eq!(client.get_role_expiry_public(&admin), Some(expiry));

    // Advance the ledger *exactly* to the boundary timestamp `expiry`.
    // role_has_expired is checked inclusively at this point, so the operator
    // must transition from Registered → Revoked.
    set_ledger_time(&env, 101, expiry);

    // The contract rejects privileged entrypoints from an expired operator.
    client.configure_emergency(&admin, &1000_0000000, &3600, &0, &10000_0000000);
}

/// Sad-path counterpart to the expiry test: at `t == expiry - 1` (one second
/// *before* the boundary) the role is still **Registered**, so the same
/// privileged entrypoint must succeed. This pins the inclusive-vs-exclusive
/// direction of the boundary so a future off-by-one flip in `role_has_expired`
/// turns into a failing build.
#[test]
fn allow_privileged_action_one_second_before_role_expiry_boundary() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);

    let expiry = 1_010u64;
    client.set_role_expiry(&owner, &admin, &Some(expiry));

    // Just before the boundary — Admin is still Registered.
    set_ledger_time(&env, 101, expiry - 1);
    assert!(
        client.configure_emergency(&admin, &1000_0000000, &3600, &0, &10000_0000000),
        "an Admin whose role expiry is one second in the future must still be Registered"
    );
}

/// Lifecycle round-trip: an address that was **Revoked** (expiry passed) can
/// be re-registered as an operator and regain access. This pins the contract
/// invariant that revocation is not a permanent state — a renewal by an
/// Owner transitions Revoked → Registered again.
#[test]
fn re_register_revoked_operator_restores_registered_state() {
    let env = Env::default();
    env.mock_all_auths();

    set_ledger_time(&env, 100, 1_000);

    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let admin = Address::generate(&env);

    client.init(&owner, &vec![&env]);
    client.add_family_member(&owner, &admin, &FamilyRole::Admin);
    client.set_role_expiry(&owner, &admin, &Some(1_010));

    // Move into Revoked.
    set_ledger_time(&env, 101, 1_010);
    assert_eq!(
        client.get_role_expiry_public(&admin),
        Some(1_010),
        "expiry value must be exactly the boundary timestamp during Revoked state"
    );

    // Owner renews → Re-Registered. The boundary timestamp is unchanged;
    // renewal only resets the expiry.
    let renewed_to = 1_010u64 + 500;
    assert!(client.set_role_expiry(&owner, &admin, &Some(renewed_to)));
    assert_eq!(client.get_role_expiry_public(&admin), Some(renewed_to));

    // Privileged action succeeds again.
    assert!(client.configure_emergency(
        &admin,
        &1000_0000000,
        &3600,
        &0,
        &10000_0000000
    ));
}