//! Deterministic failure-boundary coverage for
//! `BillPayments::set_upgrade_admin`.
//!
//! Exercises the two authorization regimes the entry point implements —
//! first-time bootstrap (caller must equal the new admin) and transfer
//! (only the incumbent, and only to a genuinely different address) — plus
//! the `SameAdmin` no-op guard and the rotation handover of authority.

#[cfg(test)]
mod upgrade_admin_boundary_tests {
    use crate::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use testutils::setup_test_env;

    #[test]
    fn upgrade_admin_is_unset_before_bootstrap() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, _owner);
        assert_eq!(client.get_upgrade_admin_public(), None);
    }

    #[test]
    fn bootstrap_requires_caller_to_equal_new_admin() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        let other = Address::generate(&env);

        assert_eq!(
            client.try_set_upgrade_admin(&owner, &other),
            Err(Ok(Error::Unauthorized))
        );
        // The failed bootstrap must not have written anything.
        assert_eq!(client.get_upgrade_admin_public(), None);
    }

    #[test]
    fn bootstrap_installs_the_caller_as_admin() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);

        client.set_upgrade_admin(&owner, &owner);
        assert_eq!(client.get_upgrade_admin_public(), Some(owner.clone()));
    }

    #[test]
    fn transfer_rejects_a_non_admin_caller() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let stranger = Address::generate(&env);
        let target = Address::generate(&env);
        assert_eq!(
            client.try_set_upgrade_admin(&stranger, &target),
            Err(Ok(Error::Unauthorized))
        );
        assert_eq!(client.get_upgrade_admin_public(), Some(owner.clone()));
    }

    #[test]
    fn transferring_to_the_current_admin_is_rejected_as_same_admin() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        assert_eq!(
            client.try_set_upgrade_admin(&owner, &owner),
            Err(Ok(Error::SameAdmin))
        );
    }

    #[test]
    fn successful_transfer_updates_authority_and_locks_out_the_previous_admin() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let new_admin = Address::generate(&env);
        client.set_upgrade_admin(&owner, &new_admin);
        assert_eq!(client.get_upgrade_admin_public(), Some(new_admin.clone()));

        // The rotated-out admin can no longer transfer.
        let third = Address::generate(&env);
        assert_eq!(
            client.try_set_upgrade_admin(&owner, &third),
            Err(Ok(Error::Unauthorized))
        );

        // ...but the new admin can.
        client.set_upgrade_admin(&new_admin, &third);
        assert_eq!(client.get_upgrade_admin_public(), Some(third));
    }

    #[test]
    fn authority_can_be_rotated_back_to_the_original_admin() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        let other = Address::generate(&env);
        client.set_upgrade_admin(&owner, &other);
        client.set_upgrade_admin(&other, &owner);

        assert_eq!(client.get_upgrade_admin_public(), Some(owner));
    }

    #[test]
    fn repeated_same_admin_attempts_stay_rejected_and_do_not_mutate_state() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        for _ in 0..3 {
            assert_eq!(
                client.try_set_upgrade_admin(&owner, &owner),
                Err(Ok(Error::SameAdmin))
            );
        }
        assert_eq!(client.get_upgrade_admin_public(), Some(owner));
    }

    #[test]
    fn set_upgrade_admin_requires_authorization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        env.mock_all_auths();
        client.set_upgrade_admin(&owner, &owner);

        env.set_auths(&[]);
        assert!(client.try_set_upgrade_admin(&owner, &owner).is_err());
    }
}
