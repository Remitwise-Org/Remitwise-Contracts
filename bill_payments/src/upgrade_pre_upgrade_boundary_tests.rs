//! Deterministic failure-boundary coverage for `BillPayments::pre_upgrade`.
//!
//! Focuses on the snapshot entry point's authorization model, its
//! idempotency/overwrite semantics, and the recovery guarantee that a
//! snapshot taken before an upgrade can restore the captured contract
//! version afterwards. Only the public contract client is exercised, so the
//! tests stay behavioural rather than reaching into storage layout.

#[cfg(test)]
mod upgrade_pre_upgrade_boundary_tests {
    use crate::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use testutils::setup_test_env;

    #[test]
    fn pre_upgrade_without_an_upgrade_admin_is_unauthorized() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);

        // No `set_upgrade_admin` bootstrap has run, so there is no admin to
        // authorize against.
        assert_eq!(client.try_pre_upgrade(&owner), Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn pre_upgrade_rejects_a_non_admin_caller() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let stranger = Address::generate(&env);
        assert_eq!(
            client.try_pre_upgrade(&stranger),
            Err(Ok(Error::Unauthorized))
        );
    }

    #[test]
    fn pre_upgrade_requires_authorization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        env.mock_all_auths();
        client.set_upgrade_admin(&owner, &owner);

        // Strip mocked auths: the `require_auth()` guard must reject the call.
        env.set_auths(&[]);
        assert!(client.try_pre_upgrade(&owner).is_err());
    }

    #[test]
    fn pre_upgrade_snapshot_restores_the_captured_version() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &3);
        client.pre_upgrade(&owner);

        // Mutate state after the snapshot — restore must roll the version back.
        client.set_version(&owner, &9);
        assert_eq!(client.get_version(), 9);

        client.restore_from_snapshot(&owner);
        assert_eq!(client.get_version(), 3);
    }

    #[test]
    fn pre_upgrade_overwrites_a_previous_snapshot() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &2);
        client.pre_upgrade(&owner);

        client.set_version(&owner, &5);
        client.pre_upgrade(&owner);

        // A later mutation must be discarded in favour of the *latest*
        // snapshot, not the first one.
        client.set_version(&owner, &8);
        client.restore_from_snapshot(&owner);
        assert_eq!(client.get_version(), 5);
    }

    #[test]
    fn pre_upgrade_is_repeatable_and_produces_a_restorable_snapshot() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        // Repeated calls must not error (idempotent overwrite, no accumulation).
        assert!(client.try_pre_upgrade(&owner).is_ok());
        assert!(client.try_pre_upgrade(&owner).is_ok());

        assert!(client.try_restore_from_snapshot(&owner).is_ok());
    }

    #[test]
    fn restore_without_a_snapshot_fails_closed() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        assert_eq!(
            client.try_restore_from_snapshot(&owner),
            Err(Ok(Error::SnapshotNotFound))
        );
    }

    #[test]
    fn restore_rejects_a_non_admin_caller() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);
        client.pre_upgrade(&owner);

        let stranger = Address::generate(&env);
        assert_eq!(
            client.try_restore_from_snapshot(&stranger),
            Err(Ok(Error::Unauthorized))
        );
    }

    #[test]
    fn snapshot_authority_follows_the_current_upgrade_admin() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let new_admin = Address::generate(&env);
        client.set_upgrade_admin(&owner, &new_admin);

        // The rotated-out admin must no longer be able to snapshot; the
        // current admin must be able to.
        assert_eq!(
            client.try_pre_upgrade(&owner),
            Err(Ok(Error::Unauthorized))
        );
        assert!(client.try_pre_upgrade(&new_admin).is_ok());
    }
}
