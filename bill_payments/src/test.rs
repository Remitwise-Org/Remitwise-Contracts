#[cfg(test)]
mod testsuit {
    extern crate std;

    use crate::pause_functions;
    use crate::*;
    use proptest::prelude::*;
    use remitwise_common;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env, IntoVal, String};
    use std::format;
    use testutils::{set_ledger_time, setup_test_env};

    fn set_time(env: &Env, timestamp: u64) {
        let proto = env.ledger().protocol_version();
        env.ledger().set(LedgerInfo {
            protocol_version: proto,
            sequence_number: env.ledger().sequence(),
            timestamp,
            network_id: env.ledger().network_id().into(),
            base_reserve: 0,
            min_temp_entry_ttl: 0,
            min_persistent_entry_ttl: 0,
            max_entry_ttl: 6315840,
        });
    }

    proptest! {
        #[test]
        fn prop_overdue_bills_all_due_dates_less_than_now(
            now in 1_000_000u64..10_000_000u64,
            n_overdue in 1usize..10,
            n_future in 0usize..10
        ) {
            let env = Env::default();
            set_ledger_time(&env, 1, now);
            let contract_id = env.register_contract(None, BillPayments);
            let client = BillPaymentsClient::new(&env, &contract_id);
            let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
            env.mock_all_auths();

            // Create bills that will become overdue after advancing time.
            for i in 0..n_overdue {
                client.create_bill(
                    &owner,
                    &String::from_str(&env, &format!("Overdue{}", i)),
                    &100,
                    &(now + 1 + i as u64),
                    &false,
                    &0, &None, &String::from_str(&env, "XLM"), &None);
                env.mock_all_auths();
            }

            // Create future bills
            for i in 0..n_future {
                client.create_bill(
                    &owner,
                    &String::from_str(&env, &format!("Future{}", i)),
                    &100,
                    &(now + 10_000 + i as u64),
                    &false,
                    &0, &None, &String::from_str(&env, "XLM"), &None);
                env.mock_all_auths();
            }

            let advanced = now + 5_000;
            set_ledger_time(&env, 1, advanced);

            let overdue = client.get_overdue_bills(&0, &100);
            // All overdue bills should have due_date < current ledger time
            for bill in overdue.items.iter() {
                assert!(
                    bill.due_date < advanced,
                    "Bill due_date {} not less than ledger time {}",
                    bill.due_date,
                    advanced
                );
            }
            // The number of overdue bills should match n_overdue
            assert_eq!(overdue.count, n_overdue as u32);
        }
    }

    #[test]
    fn test_create_bill_succeeds() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(bill_id, 1);

        let bill = client.get_bill(&1);
        assert!(bill.is_some());
        let bill = bill.unwrap();
        assert_eq!(bill.amount, 1000);
        assert!(!bill.paid);
        assert!(bill.external_ref.is_none());
    }

    #[test]
    fn test_create_bill_invalid_amount_fails() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Invalid"),
            &0,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    #[test]
    fn test_create_bill_empty_name_fails() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, ""),
            &1000,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(result, Err(Ok(Error::InvalidName)));
    }

    #[test]
    fn test_create_bill_name_too_long_fails() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        // Build a string longer than MAX_NAME_LEN (64)
        let long_name = String::from_str(&env, &"x".repeat(65));
        let result = client.try_create_bill(
            &owner,
            &long_name,
            &1000,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(result, Err(Ok(Error::InvalidName)));
    }

    #[test]
    fn test_create_bill_name_at_max_length_succeeds() {
        setup_test_env!(env, BillPayments, BillPaymentsClient, client, owner);
        // A name exactly MAX_NAME_LEN (64) bytes should be accepted
        let valid_name = String::from_str(&env, &"x".repeat(64));
        let bill_id = client.create_bill(
            &owner,
            &valid_name,
            &1000,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(bill_id, 1);
    }

    #[test]
    fn test_create_recurring_bill_invalid_frequency() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Monthly"),
            &500,
            &1000000,
            &true,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(result, Err(Ok(Error::InvalidFrequency)));
    }

    #[test]
    fn test_create_bill_negative_amount() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Invalid"),
            &-100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(result, Err(Ok(Error::InvalidAmount)));
    }

    // -----------------------------------------------------------------------
    // Currency validation tests (SC-015)
    // -----------------------------------------------------------------------

    #[test]
    fn test_currency_valid_xlm() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.currency, String::from_str(&env, "XLM"));
    }

    #[test]
    fn test_currency_empty_defaults_to_xlm() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "EmptyCurrency"),
            &100,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, ""),
            &None,
        );
        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.currency, String::from_str(&env, "XLM"));
    }

    #[test]
    fn test_currency_lowercase_normalized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Lowercase"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "xlm"),
            &None,
        );
        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.currency, String::from_str(&env, "XLM"));
    }

    #[test]
    fn test_currency_invalid_with_numbers() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "InvalidNumber"),
            &100,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM1"),
            &None,
        );
        assert_eq!(result, Err(Ok(Error::InvalidCurrency)));
    }

    #[test]
    fn test_currency_invalid_too_long() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "TooLong"),
            &100,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "VERYLONGCURRENCYCODE"),
            &None,
        );
        assert_eq!(result, Err(Ok(Error::InvalidCurrency)));
    }

    #[test]
    fn test_currency_unsupported_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Unsupported"),
            &100,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "NGN"),
            &None,
        );
        assert_eq!(result, Err(Ok(Error::UnsupportedCurrency)));
    }

    #[test]
    fn test_pay_bill_settlement_window_expired() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        let creation_time = 1_000_000;
        set_ledger_time(&env, 1, creation_time);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &creation_time,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Advance time well beyond MAX_SETTLEMENT_WINDOW_SECS (30 days = 2_592_000 seconds)
        set_ledger_time(&env, 2, creation_time + 3_000_000);

        env.mock_all_auths();
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::SettlementWindowExpired)));
    }

    #[test]
    fn test_pay_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);

        assert!(bill.paid_at.is_some());
    }

    #[test]
    fn test_recurring_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &10000,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);

        // Check original bill is paid
        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);

        // Check next recurring bill was created
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);

        assert_eq!(bill2.amount, 10000);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_without_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_without(&owner, &1);
        assert_eq!(result, Ok(Ok(())));
    }

    #[test]
    fn test_without_invalid_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_without(&owner, &0);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_without_unauthorized_id() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_without(&owner, &999);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    #[allow(deprecated)]
    fn test_get_all_bills_admin_only() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        // Create recurring bill
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Subscription"),
            &999,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        // Pay first bill - creates second
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&orch, &0, &owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    fn test_get_unpaid_bills() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let unpaid = client.get_unpaid_bills(&owner, &0, &100);
        assert_eq!(unpaid.items.len(), 2);
    }

    #[test]
    fn test_get_total_unpaid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &1);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 500); // 200 + 300
    }

    #[test]
    fn test_pay_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let result = client.try_pay_bill(&orch, &0, &owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.pay_bill(&orch, &0, &owner, &bill_id);
        let result = client.try_pay_bill(&orch, &0, &owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1_800_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 2);
    }

    #[test]
    fn test_cancel_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify cancelled bill is completely removed from storage
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should return None"
        );

        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(
            client.get_bill(&new_bill_id).is_some(),
            "new bill should exist"
        );
        assert!(
            client.get_bill(&bill_id).is_none(),
            "cancelled bill should still return None"
        );

        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    /// Issue #1591: a paid bill is a terminal, audited record. `cancel_bill`
    /// must not be usable to delete it -- that would silently destroy the
    /// payment record (and paid_at trail) instead of going through
    /// `reverse_payment`, the dedicated typed reversal path.
    #[test]
    fn test_cancel_bill_rejects_already_paid_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let orch = Address::generate(&env);
        client.init_admin(&owner, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&owner, &orch);
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&orch, &0, &owner, &bill_id);

        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));

        // The bill record must survive the rejected cancellation attempt.
        let bill = client
            .get_bill(&bill_id)
            .expect("paid bill must still exist");
        assert!(bill.paid);
    }

    #[test]
    fn test_cancel_bill_owner_succeeds() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Test"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);

        // Verify owner can successfully cancel their own bill and it's removed
        assert!(
            client.get_bill(&bill_id).is_none(),
            "bill should be removed after owner cancellation"
        );
        let result = client.try_cancel_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_cancel_bill_unauthorized_fails() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_cancel_nonexistent_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        let result = client.try_cancel_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let ref_id = Some(String::from_str(&env, "BILL-EXT-123"));
        env.mock_all_auths();
        client.set_external_ref(&owner, &bill_id, &ref_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert_eq!(bill.external_ref, ref_id);
    }

    #[test]
    fn test_set_external_ref_unauthorized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        let result = client.try_set_external_ref(
            &other,
            &bill_id,
            &Some(String::from_str(&env, "BILL-EXT-123")),
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// Tests the complete external reference index lifecycle:
    /// Register -> Verify uniqueness -> Revoke -> Re-verify/Re-register.
    #[test]
    fn test_external_ref_register_verify_revoke_reverify() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let ref_1 = Some(String::from_str(&env, "REF-001"));
        let ref_2 = Some(String::from_str(&env, "REF-002"));

        // 1. REGISTER: Create bill 1 with ref_1 and bill 2 with ref_2
        let bill1_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electric"),
            &100,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill2_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &50,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. VERIFY: Duplicate external_ref registration is rejected
        let dup_res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &75,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Attempting to set bill 1's ref to ref_2 must fail with DuplicateExternalRef
        let set_dup_res = client.try_set_external_ref(&owner, &bill1_id, &ref_2);
        assert_eq!(set_dup_res, Err(Ok(Error::DuplicateExternalRef)));

        // Verify index integrity after failed update: ref_1 must NOT have been prematurely released!
        let dup_res_after_failed_update = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Solar"),
            &80,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(
            dup_res_after_failed_update,
            Err(Ok(Error::DuplicateExternalRef)),
            "Failed set_external_ref must not prematurely release original reference"
        );

        // 3. REVOKE: Revoke ref_1 from bill 1 by setting external_ref to None
        client.set_external_ref(&owner, &bill1_id, &None);
        let bill1 = client.get_bill(&bill1_id).unwrap();
        assert_eq!(bill1.external_ref, None);

        // 4. RE-VERIFY / RE-REGISTER: ref_1 can now be registered to a new bill
        let bill3_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &120,
            &1000000,
            &false,
            &0,
            &ref_1,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill3 = client.get_bill(&bill3_id).unwrap();
        assert_eq!(bill3.external_ref, ref_1);

        // Revoke ref_2 via cancel_bill
        client.cancel_bill(&owner, &bill2_id);

        // Re-verify ref_2 can now be registered to another bill
        let bill4_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Trash"),
            &30,
            &1000000,
            &false,
            &0,
            &ref_2,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let bill4 = client.get_bill(&bill4_id).unwrap();
        assert_eq!(bill4.external_ref, ref_2);
    }

    #[test]
    fn test_schedule_unpause_requires_active_pause_state() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        let future = env.ledger().timestamp() + 3600;
        let result = client.try_schedule_unpause(&admin, &future);
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    #[test]
    fn test_paused_since_tracks_successful_global_pause_lifecycle() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);
        env.ledger().set_timestamp(0);

        assert_eq!(client.get_paused_since(), None);
        client.pause(&admin);
        assert_eq!(client.get_paused_since(), Some(0));

        env.ledger().set_timestamp(2000);
        client.pause(&admin);
        assert_eq!(client.get_paused_since(), Some(0));

        client.schedule_unpause(&admin, &3000);
        env.ledger().set_timestamp(2999);
        assert_eq!(client.try_unpause(&admin), Err(Ok(Error::ContractPaused)));
        assert!(client.is_paused());
        assert_eq!(client.get_paused_since(), Some(0));

        env.ledger().set_timestamp(3000);
        client.unpause(&admin);
        assert!(!client.is_paused());
        assert_eq!(client.get_paused_since(), None);
    }

    #[test]
    fn test_unauthorized_pause_does_not_set_paused_since() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        assert_eq!(client.try_pause(&other), Err(Ok(Error::UnauthorizedPause)));
        assert!(!client.is_paused());
        assert_eq!(client.get_paused_since(), None);
    }

    #[test]
    fn test_schedule_unpause_replaces_existing_pending_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);
        client.pause(&admin);

        let first = env.ledger().timestamp() + 3600;
        let second = env.ledger().timestamp() + 7200;
        client.schedule_unpause(&admin, &first);
        client.schedule_unpause(&admin, &second);

        env.ledger().set_timestamp(first + 1);
        let result = client.try_unpause(&admin);
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        env.ledger().set_timestamp(second);
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_schedule_unpause_is_rejected_when_contract_is_not_paused() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        let future = env.ledger().timestamp() + 3600;
        let result = client.try_schedule_unpause(&admin, &future);
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        client.pause(&admin);
        client.schedule_unpause(&admin, &(env.ledger().timestamp() + 60));
        env.ledger().set_timestamp(env.ledger().timestamp() + 61);
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    /// Verify batch_pay_bills with a mix of valid and invalid bill IDs.
    /// Invalid IDs are skipped; valid ones are processed. No partial
    /// state is left from invalid entries.
    #[test]
    fn test_batch_pay_bills_mixed_valid_invalid() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();

        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Bill1"),
            &100,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Include invalid IDs (non-existent, already paid, wrong owner)
        let bill_ids = soroban_sdk::vec![&env, id1, 999, id2, 888];
        let result = client.batch_pay_bills(&owner, &bill_ids);
        assert!(result.is_ok(), "batch must succeed, skipping invalid IDs");

        // Both valid bills are paid
        assert!(client.get_bill(&id1).unwrap().paid);
        assert!(client.get_bill(&id2).unwrap().paid);

        // No phantom bills created for invalid IDs
        assert!(client.get_bill(&999).is_none());
        assert!(client.get_bill(&888).is_none());
    }

    /// Verify batch_pay_bills is fully atomic when recurring child creation
    /// fails after earlier bills have been staged.
    #[test]
    fn test_batch_pay_bills_atomic_rollback_on_overflow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();

        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Safe"),
            &100,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Recurring"),
            &200,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Both should succeed in batch (no overflow)
        let
