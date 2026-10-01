#[cfg(test)]
mod testsuit {
    extern crate std;

    use crate::*;
    use crate::pause_functions;
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

    /// Verify batch_pay_bills is fully atomic: if one recurring bill
    /// computation overflows, the entire batch reverts with no changes.
    #[test]
    fn test_batch_pay_bills_atomic_rollback_on_overflow() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();

        client.set_upgrade_admin(&admin, &admin);
        let result = client.try_pre_upgrade(&stranger);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    // -----------------------------------------------------------------------
    // pause_function failure-boundary tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_pause_function_unauthorized_no_admin() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let caller = Address::generate(&env);

        env.mock_all_auths();
        // No pause admin set
        let result = client.try_pause_function(&caller, &pause_functions::CREATE_BILL);
        assert_eq!(result, Err(Ok(Error::UnauthorizedPause)));
    }

    #[test]
    fn test_pause_function_unauthorized_wrong_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        env.mock_all_auths();
        let result = client.try_pause_function(&attacker, &pause_functions::CREATE_BILL);
        assert_eq!(result, Err(Ok(Error::UnauthorizedPause)));
    }

    #[test]
    fn test_pause_function_success() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        env.mock_all_auths();
        let result = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result.is_ok());

        // Verify function is paused
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_idempotent() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        env.mock_all_auths();
        // Pause once
        let result1 = client.pause_function(&admin, &pause_functions::PAY_BILL);
        assert!(result1.is_ok());

        env.mock_all_auths();
        // Pause again - should succeed (idempotent)
        let result2 = client.pause_function(&admin, &pause_functions::PAY_BILL);
        assert!(result2.is_ok());

        // Function should still be paused
        assert!(client.is_function_paused_public(&pause_functions::PAY_BILL));
    }

    #[test]
    fn test_pause_function_blocks_operation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        env.mock_all_auths();
        // Pause create_bill
        client.pause_function(&admin, &pause_functions::CREATE_BILL);

        // Try to create a bill - should fail with FunctionPaused
        env.mock_all_auths();
        let result = client.try_create_bill(
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
        assert_eq!(result, Err(Ok(Error::FunctionPaused)));
    }

    #[test]
    fn test_pause_function_multiple_functions() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        env.mock_all_auths();
        // Pause multiple functions
        client.pause_function(&admin, &pause_functions::CREATE_BILL);
        client.pause_function(&admin, &pause_functions::PAY_BILL);
        client.pause_function(&admin, &pause_functions::CANCEL_BILL);

        // Verify all are paused
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
        assert!(client.is_function_paused_public(&pause_functions::PAY_BILL));
        assert!(client.is_function_paused_public(&pause_functions::CANCEL_BILL));

        // Verify unrelated function is not paused
        assert!(!client.is_function_paused_public(&pause_functions::ARCHIVE));
    }

    #[test]
    fn test_pause_function_with_global_pause() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause globally
        env.mock_all_auths();
        client.pause(&admin);
        assert!(client.is_paused());

        // Can still pause individual functions while globally paused
        env.mock_all_auths();
        let result = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result.is_ok());

        // Function should be paused
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_concurrent_safety() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Simulate concurrent pause operations
        env.mock_all_auths();
        let result1 = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result1.is_ok());

        env.mock_all_auths();
        let result2 = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result2.is_ok());

        // Both should succeed and result in the same state
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_unpause_cycle() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));

        // Unpause
        env.mock_all_auths();
        client.unpause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(!client.is_function_paused_public(&pause_functions::CREATE_BILL));

        // Pause again
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_preserves_other_paused_functions() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause first function
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);

        // Pause second function
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::PAY_BILL);

        // Unpause first function
        env.mock_all_auths();
        client.unpause_function(&admin, &pause_functions::CREATE_BILL);

        // Second function should still be paused
        assert!(!client.is_function_paused_public(&pause_functions::CREATE_BILL));
        assert!(client.is_function_paused_public(&pause_functions::PAY_BILL));
    }

    #[test]
    fn test_pause_function_stale_state_handling() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause function
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);

        // Simulate storage being in a stale state by directly checking
        // The pause state should be persistent
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));

        // Even after multiple reads, state should remain consistent
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_unknown_symbol() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Try to pause an unknown function symbol
        // The contract allows pausing any symbol, but it won't affect operations
        // that don't check that specific symbol
        env.mock_all_auths();
        let unknown_symbol = soroban_sdk::symbol_short!("unknown");
        let result = client.pause_function(&admin, &unknown_symbol);
        assert!(result.is_ok());

        // The unknown symbol should be marked as paused
        assert!(client.is_function_paused_public(&unknown_symbol));
    }

    #[test]
    fn test_pause_function_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause function and check for event
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);

        let events = env.events().all();
        let fn_paused_found = events.iter().any(|event| {
            if let Ok((topics, _)) = event {
                if topics.len() >= 3 {
                    let action = topics.get(2).unwrap();
                    *action == soroban_sdk::symbol_short!("fn_paused").into_val(&env)
                } else {
                    false
                }
            } else {
                false
            }
        });
        assert!(fn_paused_found, "fn_paused event should be emitted");
    }

    #[test]
    fn test_unpause_function_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause first
        env.mock_all_auths();
        client.pause_function(&admin, &pause_functions::CREATE_BILL);

        // Clear events
        env.events().all();

        // Unpause and check for event
        env.mock_all_auths();
        client.unpause_function(&admin, &pause_functions::CREATE_BILL);

        let events = env.events().all();
        let fn_unpaused_found = events.iter().any(|event| {
            if let Ok((topics, _)) = event {
                if topics.len() >= 3 {
                    let action = topics.get(2).unwrap();
                    *action == soroban_sdk::symbol_short!("fn_unpaused").into_val(&env)
                } else {
                    false
                }
            } else {
                false
            }
        });
        assert!(fn_unpaused_found, "fn_unpaused event should be emitted");
    }

    #[test]
    fn test_pause_function_admin_grant_expired() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        // Set initial time
        let initial_time = 1_000_000;
        set_ledger_time(&env, 1, initial_time);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Advance time beyond ADMIN_GRANT_TTL (30 days = 2_592_000 seconds)
        let expired_time = initial_time + 2_592_001;
        set_ledger_time(&env, 2, expired_time);

        env.mock_all_auths();
        // Attempt to pause function with expired admin grant
        let result = client.try_pause_function(&admin, &pause_functions::CREATE_BILL);
        assert_eq!(result, Err(Ok(Error::AdminGrantExpired)));
    }

    #[test]
    fn test_pause_function_kill_switch_active() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Activate kill switch
        env.mock_all_auths();
        remitwise_common::activate_kill_switch(&env, &admin);

        // Attempt to pause function with kill switch active
        env.mock_all_auths();
        let result = client.try_pause_function(&admin, &pause_functions::CREATE_BILL);
        // Kill switch causes panic_with_error, which manifests as a contract error
        assert!(result.is_err());
    }

    #[test]
    fn test_pause_function_admin_grant_ttl_boundary() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        // Set initial time
        let initial_time = 1_000_000;
        set_ledger_time(&env, 1, initial_time);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Test at exact expiry boundary (just before expiry)
        let just_before_expiry = initial_time + 2_591_999; // 30 days - 1 second
        set_ledger_time(&env, 2, just_before_expiry);

        env.mock_all_auths();
        let result_before = client.try_pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result_before.is_ok(), "Should succeed just before expiry");

        // Test at exact expiry boundary (at expiry)
        let at_expiry = initial_time + 2_592_000; // exactly 30 days
        set_ledger_time(&env, 3, at_expiry);

        env.mock_all_auths();
        let result_at = client.try_pause_function(&admin, &pause_functions::PAY_BILL);
        assert_eq!(result_at, Err(Ok(Error::AdminGrantExpired)), "Should fail at exact expiry");

        // Test just after expiry
        let just_after_expiry = initial_time + 2_592_001; // 30 days + 1 second
        set_ledger_time(&env, 4, just_after_expiry);

        env.mock_all_auths();
        let result_after = client.try_pause_function(&admin, &pause_functions::CANCEL_BILL);
        assert_eq!(result_after, Err(Ok(Error::AdminGrantExpired)), "Should fail just after expiry");
    }

    #[test]
    fn test_pause_function_retry_safety() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // First pause attempt
        env.mock_all_auths();
        let result1 = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result1.is_ok());
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));

        // Simulate retry (second attempt with same state)
        env.mock_all_auths();
        let result2 = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result2.is_ok());

        // State should remain consistent
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));

        // Verify no duplicate entries or corruption
        let paused_map: soroban_sdk::Map<soroban_sdk::Symbol, bool> = env
            .storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("PAUSED_FN"))
            .unwrap();
        assert_eq!(paused_map.len(), 1, "Should have exactly one paused function");
    }

    #[test]
    fn test_pause_function_legacy_admin_grant_migration() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        // Set pause admin without grant timestamp (legacy state)
        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Manually remove the grant timestamp to simulate legacy state
        env.storage()
            .instance()
            .remove(&soroban_sdk::symbol_short!("PADM_GT"));

        // Verify grant timestamp is not present
        let grant_timestamp: Option<u64> = env
            .storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("PADM_GT"));
        assert!(grant_timestamp.is_none(), "Grant timestamp should be absent in legacy state");

        // Pause function should succeed and migrate the grant timestamp
        env.mock_all_auths();
        let result = client.pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result.is_ok());

        // Verify grant timestamp was set (migration occurred)
        let grant_timestamp_after: Option<u64> = env
            .storage()
            .instance()
            .get(&soroban_sdk::symbol_short!("PADM_GT"));
        assert!(grant_timestamp_after.is_some(), "Grant timestamp should be set after migration");

        // Function should be paused
        assert!(client.is_function_paused_public(&pause_functions::CREATE_BILL));
    }

    #[test]
    fn test_pause_function_with_refreshed_grant() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        let initial_time = 1_000_000;
        set_ledger_time(&env, 1, initial_time);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Advance time close to expiry
        let near_expiry = initial_time + 2_000_000;
        set_ledger_time(&env, 2, near_expiry);

        // Refresh admin grant
        env.mock_all_auths();
        client.refresh_admin_grant(&admin);

        // Advance time past original expiry but within refreshed window
        let past_original_expiry = initial_time + 2_600_000;
        set_ledger_time(&env, 3, past_original_expiry);

        // Pause should succeed with refreshed grant
        env.mock_all_auths();
        let result = client.try_pause_function(&admin, &pause_functions::CREATE_BILL);
        assert!(result.is_ok(), "Should succeed with refreshed grant");
    }

    // =========================================================================
    // restore_from_snapshot — deterministic failure-boundary coverage
    //
    // Covers all error paths, permission guards, stale-state rejection,
    // schema-version mismatch, idempotency, and full state-reconstruction
    // fidelity.  Each test names the invariant it asserts so reviewers can
    // map code → requirement without reading both files.
    // =========================================================================

    /// INVARIANT: restore_from_snapshot returns SnapshotNotFound when no
    /// snapshot has been written (no prior pre_upgrade call).
    #[test]
    fn test_restore_no_snapshot_returns_snapshot_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::SnapshotNotFound)),
            "restore must fail with SnapshotNotFound when no snapshot exists"
        );
    }

    /// INVARIANT: restore_from_snapshot returns Unauthorized when called by
    /// an address that is not the current upgrade admin.
    #[test]
    fn test_restore_by_non_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let interloper = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);
        // Take a valid snapshot so we are sure the auth check fires before the
        // snapshot-not-found check.
        client.pre_upgrade(&admin);

        let result = client.try_restore_from_snapshot(&interloper);
        assert_eq!(
            result,
            Err(Ok(Error::Unauthorized)),
            "restore must reject callers that are not the upgrade admin"
        );
    }

    /// INVARIANT: restore_from_snapshot returns Unauthorized when no upgrade
    /// admin has been set at all (contract freshly deployed).
    #[test]
    fn test_restore_with_no_upgrade_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        // No set_upgrade_admin call — admin slot is empty.
        let stranger = Address::generate(&env);

        let result = client.try_restore_from_snapshot(&stranger);
        assert_eq!(
            result,
            Err(Ok(Error::Unauthorized)),
            "restore must fail with Unauthorized when upgrade admin is not configured"
        );
    }

    /// INVARIANT: restore_from_snapshot returns SnapshotTooOld when the
    /// snapshot timestamp is older than SNAPSHOT_MAX_AGE_SECS (30 days).
    ///
    /// The ledger is advanced to just beyond the staleness threshold so the
    /// age check fires deterministically without flakiness.
    #[test]
    fn test_restore_stale_snapshot_returns_snapshot_too_old() {
        use remitwise_common::SNAPSHOT_MAX_AGE_SECS;

        let env = Env::default();
        env.mock_all_auths();
        // Start at a non-zero timestamp so subtraction stays valid.
        let start_ts: u64 = 1_000_000;
        set_time(&env, start_ts);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        // Take snapshot at start_ts.
        client.pre_upgrade(&admin);

        // Advance the ledger beyond the 30-day freshness window.
        // +1 ensures we are strictly past the boundary.
        let stale_ts = start_ts + SNAPSHOT_MAX_AGE_SECS + 1;
        set_time(&env, stale_ts);

        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::SnapshotTooOld)),
            "restore must reject a snapshot older than SNAPSHOT_MAX_AGE_SECS"
        );
    }

    /// INVARIANT: restore_from_snapshot returns InvalidLimit (the current
    /// error code for schema-version mismatch — see SNAPSHOT_VERSION comment
    /// in lib.rs) when the stored snapshot carries a schema_version that does
    /// not match SNAPSHOT_VERSION.
    ///
    /// The mismatch is injected directly into persistent storage using
    /// env.as_contract so no production code path is used to write the
    /// bogus snapshot.
    #[test]
    fn test_restore_schema_version_mismatch_returns_invalid_limit() {
        use remitwise_common::SNAPSHOT_KEY;
        use soroban_sdk::symbol_short;

        let env = Env::default();
        env.mock_all_auths();
        let start_ts: u64 = 1_000_000;
        set_time(&env, start_ts);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        // Inject a snapshot with a schema_version that will never match
        // SNAPSHOT_VERSION (which is 1).
        let bad_snapshot = PreUpgradeSnapshot {
            schema_version: 0xDEAD_BEEF, // intentionally wrong
            next_id: 42,
            version: 1,
            upgrade_admin: Some(admin.clone()),
            paused: false,
            pause_admin: None,
        };
        env.as_contract(&contract_id, || {
            env.storage().persistent().set(&SNAPSHOT_KEY, &bad_snapshot);
            // Also write a fresh SNAP_TS so the freshness check is not the
            // first thing to fail.
            env.storage()
                .persistent()
                .set(&symbol_short!("SNAP_TS"), &start_ts);
        });

        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::InvalidLimit)),
            "restore must return InvalidLimit for a schema_version != SNAPSHOT_VERSION"
        );
    }

    /// INVARIANT: a successful pre_upgrade → restore_from_snapshot round-trip
    /// reconstructs all instance-storage fields exactly: next_id, version,
    /// upgrade_admin, paused (false), and pause_admin.
    #[test]
    fn test_restore_roundtrip_reconstructs_all_state_fields() {
        use soroban_sdk::symbol_short;

        let env = Env::default();
        env.mock_all_auths();
        let start_ts: u64 = 1_000_000;
        set_time(&env, start_ts);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pause_admin = Address::generate(&env);

        // --- Set up initial state ---
        client.set_upgrade_admin(&admin, &admin);

        // Set a custom version (so it differs from CONTRACT_VERSION default).
        client.set_version(&admin, &42u32);

        // Set a pause admin so Option<Address> is Some.
        client.set_pause_admin(&admin, &pause_admin);

        // Create a bill to advance next_id beyond 0.
        let _bill_id = client.create_bill(
            &admin,
            &String::from_str(&env, "Rent"),
            &1000,
            &2_000_000u64,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Record the state we expect to come back after restore.
        // next_id is a private field; read it via env.as_contract.
        let expected_next_id: u32 = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
        });
        let expected_version = client.get_version();
        let expected_upgrade_admin = client.get_upgrade_admin_public();
        let expected_pause_admin = client.get_pause_admin_public();

        // --- Overwrite version to simulate a (failed) upgrade ---
        client.set_version(&admin, &999u32);
        assert_eq!(client.get_version(), 999u32, "precondition: version was overwritten");

        // --- Snapshot & restore ---
        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        // --- Verify every field was restored exactly ---
        let restored_next_id: u32 = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
        });
        assert_eq!(
            restored_next_id, expected_next_id,
            "next_id must be restored exactly"
        );
        assert_eq!(
            client.get_version(),
            expected_version,
            "version must be restored exactly"
        );
        assert_eq!(
            client.get_upgrade_admin_public(),
            expected_upgrade_admin,
            "upgrade_admin must be restored exactly"
        );
        assert_eq!(
            client.get_pause_admin_public(),
            expected_pause_admin,
            "pause_admin must be restored exactly"
        );
        // paused was false at snapshot time — must remain false after restore
        assert!(
            !client.is_paused(),
            "paused must be false after restoring a snapshot taken while unpaused"
        );
    }

    /// INVARIANT: restore_from_snapshot is idempotent in the failure direction:
    /// a second call after a successful first call must return SnapshotNotFound
    /// because the first call consumed (removed) the snapshot.
    #[test]
    fn test_restore_consumes_snapshot_second_restore_returns_snapshot_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        client.pre_upgrade(&admin);

        // First restore — must succeed.
        client.restore_from_snapshot(&admin);

        // Second restore — snapshot has been consumed.
        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::SnapshotNotFound)),
            "second restore must fail with SnapshotNotFound (snapshot was consumed by first call)"
        );
    }

    /// INVARIANT: pre_upgrade + restore_from_snapshot preserves a `paused = true`
    /// state exactly — the contract must come back paused after a restore of a
    /// snapshot taken while paused.
    #[test]
    fn test_restore_roundtrip_preserves_paused_true() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pause_admin = Address::generate(&env);

        client.set_upgrade_admin(&admin, &admin);
        client.set_pause_admin(&admin, &pause_admin);

        // Pause the contract before taking the snapshot.
        client.pause(&pause_admin);
        assert!(client.is_paused(), "precondition: contract must be paused");

        client.pre_upgrade(&admin);

        // Unpause to simulate a (partial) upgrade that reversed pause state.
        client.unpause(&pause_admin);
        assert!(!client.is_paused(), "precondition: contract must be unpaused after unpause");

        // Restore should re-apply the paused=true state from the snapshot.
        client.restore_from_snapshot(&admin);

        assert!(
            client.is_paused(),
            "paused must be true after restoring a snapshot taken while paused"
        );
    }

    /// INVARIANT: pre_upgrade + restore_from_snapshot preserves pause_admin = None
    /// (an Option field that was never set must come back as None, not as a stale
    /// previous value).
    #[test]
    fn test_restore_roundtrip_preserves_pause_admin_none() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);
        // Explicitly do NOT call set_pause_admin — pause_admin should be None.

        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert!(
            client.get_pause_admin_public().is_none(),
            "pause_admin must be None after restoring a snapshot where pause_admin was not set"
        );
    }

    /// INVARIANT: discard_snapshot removes the snapshot so that a subsequent
    /// restore_from_snapshot returns SnapshotNotFound.
    #[test]
    fn test_discard_snapshot_makes_subsequent_restore_fail_with_snapshot_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        client.pre_upgrade(&admin);
        client.discard_snapshot(&admin);

        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::SnapshotNotFound)),
            "restore must fail with SnapshotNotFound after discard_snapshot"
        );
    }

    /// INVARIANT: pre_upgrade returns Unauthorized when called by an address
    /// that is not the upgrade admin.
    #[test]
    fn test_pre_upgrade_by_non_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let interloper = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        let result = client.try_pre_upgrade(&interloper);
        assert_eq!(
            result,
            Err(Ok(Error::Unauthorized)),
            "pre_upgrade must reject callers that are not the upgrade admin"
        );
    }

    /// INVARIANT: discard_snapshot returns Unauthorized when called by an
    /// address that is not the upgrade admin.
    #[test]
    fn test_discard_snapshot_by_non_admin_returns_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let interloper = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        client.pre_upgrade(&admin);

        let result = client.try_discard_snapshot(&interloper);
        assert_eq!(
            result,
            Err(Ok(Error::Unauthorized)),
            "discard_snapshot must reject callers that are not the upgrade admin"
        );
    }

    /// INVARIANT: the kill switch blocks restore_from_snapshot.
    ///
    /// When the kill switch is active all write entry points must be blocked.
    /// restore_from_snapshot is a write operation (it mutates instance storage)
    /// and must therefore be blocked.
    ///
    /// The kill switch is injected via env.as_contract so we are testing the
    /// guard that was added to the entry point, not a higher-level auth layer.
    #[test]
    fn test_kill_switch_blocks_restore_from_snapshot() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);
        client.pre_upgrade(&admin);

        // Activate kill switch directly in contract storage.
        env.as_contract(&contract_id, || {
            remitwise_common::activate_kill_switch(&env);
        });

        let result = client.try_restore_from_snapshot(&admin);
        assert!(
            result.is_err(),
            "restore_from_snapshot must be blocked when the kill switch is active"
        );
    }

    /// INVARIANT: the kill switch blocks pre_upgrade.
    #[test]
    fn test_kill_switch_blocks_pre_upgrade() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        env.as_contract(&contract_id, || {
            remitwise_common::activate_kill_switch(&env);
        });

        let result = client.try_pre_upgrade(&admin);
        assert!(
            result.is_err(),
            "pre_upgrade must be blocked when the kill switch is active"
        );
    }

    /// INVARIANT: the kill switch blocks discard_snapshot.
    #[test]
    fn test_kill_switch_blocks_discard_snapshot() {
        let env = Env::default();
        env.mock_all_auths();
        set_time(&env, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);
        client.pre_upgrade(&admin);

        env.as_contract(&contract_id, || {
            remitwise_common::activate_kill_switch(&env);
        });

        let result = client.try_discard_snapshot(&admin);
        assert!(
            result.is_err(),
            "discard_snapshot must be blocked when the kill switch is active"
        );
    }

    /// BOUNDARY: restore at exactly SNAPSHOT_MAX_AGE_SECS must succeed (the
    /// boundary is inclusive on the inside — age == MAX is still fresh).
    #[test]
    fn test_restore_at_exact_boundary_age_succeeds() {
        use remitwise_common::SNAPSHOT_MAX_AGE_SECS;

        let env = Env::default();
        env.mock_all_auths();
        let start_ts: u64 = 1_000_000;
        set_time(&env, start_ts);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        client.pre_upgrade(&admin);

        // Advance to exactly the boundary: age == SNAPSHOT_MAX_AGE_SECS.
        let boundary_ts = start_ts + SNAPSHOT_MAX_AGE_SECS;
        set_time(&env, boundary_ts);

        // Must succeed — boundary is inclusive.
        client.restore_from_snapshot(&admin);
    }

    /// BOUNDARY: restore at SNAPSHOT_MAX_AGE_SECS + 1 must fail (the boundary
    /// is exclusive on the outside — age > MAX is stale).
    #[test]
    fn test_restore_one_second_past_boundary_returns_snapshot_too_old() {
        use remitwise_common::SNAPSHOT_MAX_AGE_SECS;

        let env = Env::default();
        env.mock_all_auths();
        let start_ts: u64 = 1_000_000;
        set_time(&env, start_ts);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        client.pre_upgrade(&admin);

        // age == SNAPSHOT_MAX_AGE_SECS + 1 → must be rejected.
        let stale_ts = start_ts + SNAPSHOT_MAX_AGE_SECS + 1;
        set_time(&env, stale_ts);

        let result = client.try_restore_from_snapshot(&admin);
        assert_eq!(
            result,
            Err(Ok(Error::SnapshotTooOld)),
            "age one second past the boundary must be rejected as SnapshotTooOld"
        );
    }

    /// REGRESSION: discard_snapshot on a contract with no snapshot present
    /// must not panic — it is a no-op on persistent storage and must return Ok.
    #[test]
    fn test_discard_snapshot_without_prior_snapshot_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_upgrade_admin(&admin, &admin);

        // No pre_upgrade — discard on empty storage must succeed silently.
        client.discard_snapshot(&admin);
    }
}
