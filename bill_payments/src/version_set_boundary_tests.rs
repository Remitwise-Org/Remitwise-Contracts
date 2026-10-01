//! Deterministic failure-boundary coverage for `BillPayments::set_version`.
//!
//! Covers the authorization gate (an upgrade admin must exist and be the
//! caller), the full `u32` input domain, last-write-wins determinism, and the
//! failure-recovery invariant that a rejected write never mutates the stored
//! version.

#[cfg(test)]
mod version_set_boundary_tests {
    use crate::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use testutils::setup_test_env;

    #[test]
    fn set_version_without_an_upgrade_admin_is_unauthorized() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);

        assert_eq!(
            client.try_set_version(&owner, &2),
            Err(Ok(Error::Unauthorized))
        );
    }

    #[test]
    fn set_version_rejects_a_non_admin_caller() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let stranger = Address::generate(&env);
        assert_eq!(
            client.try_set_version(&stranger, &2),
            Err(Ok(Error::Unauthorized))
        );
        // Default version must be untouched by the rejected write.
        assert_eq!(client.get_version(), crate::CONTRACT_VERSION);
    }

    #[test]
    fn set_version_requires_authorization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        env.mock_all_auths();
        client.set_upgrade_admin(&owner, &owner);

        env.set_auths(&[]);
        assert!(client.try_set_version(&owner, &2).is_err());
    }

    #[test]
    fn the_upgrade_admin_can_set_the_version() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        assert!(client.try_set_version(&owner, &13).is_ok());
        assert_eq!(client.get_version(), 13);
    }

    #[test]
    fn accepts_both_ends_of_the_u32_domain() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &0);
        assert_eq!(client.get_version(), 0);

        client.set_version(&owner, &u32::MAX);
        assert_eq!(client.get_version(), u32::MAX);
    }

    #[test]
    fn writes_are_deterministic_last_write_wins() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &9);
        client.set_version(&owner, &4);
        // No monotonicity is enforced: the most recent accepted write is
        // authoritative (deterministic, no partial application).
        assert_eq!(client.get_version(), 4);
    }

    #[test]
    fn writing_the_same_version_repeatedly_is_idempotent() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        for _ in 0..3 {
            assert!(client.try_set_version(&owner, &6).is_ok());
        }
        assert_eq!(client.get_version(), 6);
    }

    #[test]
    fn a_rejected_write_never_mutates_the_stored_version() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);
        client.set_version(&owner, &1);

        let stranger = Address::generate(&env);
        let _ = client.try_set_version(&stranger, &12345);
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn the_rotated_out_admin_can_no_longer_set_the_version() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let new_admin = Address::generate(&env);
        client.set_upgrade_admin(&owner, &new_admin);

        assert_eq!(
            client.try_set_version(&owner, &77),
            Err(Ok(Error::Unauthorized))
        );
        assert!(client.try_set_version(&new_admin, &77).is_ok());
        assert_eq!(client.get_version(), 77);
    }
}
