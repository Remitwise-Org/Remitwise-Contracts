#![cfg(test)]
#![allow(clippy::all)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env};

use crate::{ReportScope, ReportingContract, ReportingContractClient, ReportingError};

fn setup() -> (
    Env,
    ReportingContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let id = env.register_contract(None, ReportingContract);
    let client = ReportingContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let owner = Address::generate(&env);
    let viewer = Address::generate(&env);
    client.init(&admin);
    (env, client, owner, viewer, admin)
}

#[test]
fn grant_is_scoped_to_owner_viewer_and_data_class() {
    let (env, client, owner, viewer, _admin) = setup();
    client.grant_viewer(&owner, &viewer, &ReportScope::Stored, &0);
    assert_eq!(
        client.get_viewer_grant(&viewer, &owner, &viewer, &ReportScope::Stored),
        Some(0)
    );
    assert_eq!(
        client.try_get_stored_report_for(&viewer, &owner, &7),
        Ok(Ok(None))
    );
    assert_eq!(
        client.try_get_archived_reports_page_for(&viewer, &owner, &0, &20),
        Err(Ok(ReportingError::Unauthorized))
    );
    let other_owner = Address::generate(&env);
    assert_eq!(
        client.try_get_stored_report_for(&viewer, &other_owner, &7),
        Err(Ok(ReportingError::Unauthorized))
    );
}

#[test]
fn revoke_blocks_new_queries_immediately() {
    let (_env, client, owner, viewer, _admin) = setup();
    client.grant_viewer(&owner, &viewer, &ReportScope::Stored, &0);
    client.revoke_viewer(&owner, &viewer, &ReportScope::Stored);
    assert_eq!(
        client.try_get_stored_report_for(&viewer, &owner, &7),
        Err(Ok(ReportingError::Unauthorized))
    );
}

#[test]
fn expired_grant_is_not_authorized() {
    let (env, client, owner, viewer, _admin) = setup();
    let expiry = env.ledger().timestamp() + 10;
    client.grant_viewer(&owner, &viewer, &ReportScope::Stored, &expiry);
    env.ledger().set_timestamp(expiry);
    assert_eq!(
        client.try_get_stored_report_for(&viewer, &owner, &7),
        Err(Ok(ReportingError::Unauthorized))
    );
}

#[test]
fn invalid_expiry_and_self_viewer_are_rejected() {
    let (env, client, owner, viewer, _admin) = setup();
    env.ledger().set_timestamp(100);
    let now = env.ledger().timestamp();
    assert_eq!(
        client.try_grant_viewer(&owner, &viewer, &ReportScope::Stored, &now),
        Err(Ok(ReportingError::InvalidGrantExpiry))
    );
    assert_eq!(
        client.try_grant_viewer(&owner, &owner, &ReportScope::Stored, &0),
        Err(Ok(ReportingError::InvalidViewer))
    );
}

#[test]
fn owner_can_read_without_a_grant() {
    let (_env, client, owner, _viewer, _admin) = setup();
    assert_eq!(
        client.try_get_stored_report_for(&owner, &owner, &7),
        Ok(Ok(None))
    );
    let page = client.get_archived_reports_page_for(&owner, &owner, &0, &20);
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.count, 0);
}

#[test]
fn unrelated_caller_cannot_inspect_grant_state() {
    let (env, client, owner, viewer, _admin) = setup();
    let stranger = Address::generate(&env);
    client.grant_viewer(&owner, &viewer, &ReportScope::Stored, &0);
    assert_eq!(
        client.try_get_viewer_grant(&stranger, &owner, &viewer, &ReportScope::Stored),
        Err(Ok(ReportingError::Unauthorized))
    );
}
