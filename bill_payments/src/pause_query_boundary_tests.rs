//! Deterministic failure-boundary coverage for
//! `BillPayments::is_function_paused_public`.
//!
//! The query must default to `false`, reflect only the function-level pause
//! map (never the global pause flag), stay isolated between functions, and be
//! a pure read that cannot be mutated by a rejected pause attempt.

#[cfg(test)]
mod pause_query_boundary_tests {
    use crate::pause_functions::{ARCHIVE, CREATE_BILL, PAY_BILL};
    use crate::*;
    use soroban_sdk::symbol_short;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Address;
    use testutils::setup_test_env;

    #[test]
    fn reports_false_for_known_functions_before_any_pause() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, _owner);

        assert!(!client.is_function_paused_public(&PAY_BILL));
        assert!(!client.is_function_paused_public(&CREATE_BILL));
        assert!(!client.is_function_paused_public(&ARCHIVE));
    }

    #[test]
    fn reports_false_for_an_arbitrary_symbol() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, _owner);

        let unknown = symbol_short!("nope");
        assert!(!client.is_function_paused_public(&unknown));
    }

    #[test]
    fn pause_function_marks_only_the_targeted_function() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);

        client.pause_function(&owner, &PAY_BILL);

        assert!(client.is_function_paused_public(&PAY_BILL));
        assert!(!client.is_function_paused_public(&CREATE_BILL));
        assert!(!client.is_function_paused_public(&ARCHIVE));
    }

    #[test]
    fn unpause_function_clears_only_the_targeted_function() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);

        client.pause_function(&owner, &PAY_BILL);
        client.pause_function(&owner, &ARCHIVE);
        client.unpause_function(&owner, &PAY_BILL);

        assert!(!client.is_function_paused_public(&PAY_BILL));
        assert!(client.is_function_paused_public(&ARCHIVE));
    }

    #[test]
    fn functions_are_paused_independently() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);

        client.pause_function(&owner, &CREATE_BILL);
        assert!(client.is_function_paused_public(&CREATE_BILL));
        assert!(!client.is_function_paused_public(&PAY_BILL));

        client.pause_function(&owner, &PAY_BILL);
        assert!(client.is_function_paused_public(&CREATE_BILL));
        assert!(client.is_function_paused_public(&PAY_BILL));
    }

    #[test]
    fn a_global_pause_does_not_set_function_level_flags() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);

        client.pause(&owner);

        // Global pause is a separate concern from per-function flags.
        assert!(client.is_paused());
        assert!(!client.is_function_paused_public(&PAY_BILL));
        assert!(!client.is_function_paused_public(&CREATE_BILL));
    }

    #[test]
    fn a_rejected_pause_attempt_does_not_set_the_flag() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);

        let stranger = Address::generate(&env);
        assert_eq!(
            client.try_pause_function(&stranger, &PAY_BILL),
            Err(Ok(Error::UnauthorizedPause))
        );
        assert!(!client.is_function_paused_public(&PAY_BILL));
    }

    #[test]
    fn reads_are_stable_and_deterministic() {
        setup_test_env!(_env, BillPayments, BillPaymentsClient, client, owner);
        client.set_pause_admin(&owner, &owner);
        client.pause_function(&owner, &PAY_BILL);

        for _ in 0..3 {
            assert!(client.is_function_paused_public(&PAY_BILL));
            assert!(!client.is_function_paused_public(&ARCHIVE));
        }
    }
}
