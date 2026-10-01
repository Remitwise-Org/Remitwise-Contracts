//! Deterministic failure-boundary coverage for `BillPayments::get_version`.
//!
//! The read path must return the compile-time `CONTRACT_VERSION` default when
//! no version has been stored, reflect every successful `set_version`, and
//! preserve the full `u32` range without truncation at the ends of the
//! domain. It is a pure read and must never require authorization.

#[cfg(test)]
mod version_read_boundary_tests {
    use crate::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env};
    use testutils::setup_test_env;

    #[test]
    fn returns_the_contract_version_default_before_any_write() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, _owner);
        assert_eq!(client.get_version(), crate::CONTRACT_VERSION);
    }

    #[test]
    fn reflects_the_value_written_by_the_upgrade_admin() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &7);
        assert_eq!(client.get_version(), 7);
    }

    #[test]
    fn accepts_the_lower_boundary_version_of_zero() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &0);
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn preserves_the_upper_boundary_version_without_truncation() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        client.set_version(&owner, &u32::MAX);
        assert_eq!(client.get_version(), u32::MAX);
    }

    #[test]
    fn returns_only_the_most_recently_written_version() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);

        for version in [1u32, 42, 3, 100] {
            client.set_version(&owner, &version);
            assert_eq!(client.get_version(), version);
        }
    }

    #[test]
    fn repeated_reads_are_stable_and_side_effect_free() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);
        client.set_version(&owner, &11);

        let first = client.get_version();
        let second = client.get_version();
        let third = client.get_version();
        assert_eq!(first, 11);
        assert_eq!(second, first);
        assert_eq!(third, first);
    }

    #[test]
    fn read_does_not_require_authorization() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);

        // No mocked auths at all: reads must still succeed.
        env.set_auths(&[]);
        assert_eq!(client.get_version(), crate::CONTRACT_VERSION);
    }

    #[test]
    fn a_failed_write_leaves_the_readable_version_unchanged() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_upgrade_admin(&owner, &owner);
        client.set_version(&owner, &4);

        let stranger = Address::generate(&env);
        assert_eq!(
            client.try_set_version(&stranger, &99),
            Err(Ok(Error::Unauthorized))
        );
        assert_eq!(client.get_version(), 4);
    }
}
