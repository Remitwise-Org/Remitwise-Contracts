#[cfg(test)]
mod testsuit {
    extern crate std;

    use crate::*;
    use proptest::prelude::*;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
    use soroban_sdk::{Address, Env, String, IntoVal};
    use testutils::{set_ledger_time, setup_test_env};
    use std::format;

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

    #[test]
    fn test_pay_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

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

        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

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
        client.pay_bill(&owner, &bill_id);

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
        client.pay_bill(&owner, &1);

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
        client.pay_bill(&owner, &1);

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
        let result = client.try_pay_bill(&owner, &999);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_already_paid_bill() {
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
        client.pay_bill(&owner, &bill_id);
        let result = client.try_pay_bill(&owner, &bill_id);
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

    #[test]
    fn test_multiple_recurring_payments() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
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
        client.pay_bill(&owner, &bill_id);
        let bill2 = client.get_bill(&2).unwrap();
        assert!(!bill2.paid);
        assert_eq!(bill2.due_date, 1000000 + (30 * 86400));
        env.mock_all_auths();
        // Pay second bill - creates third
        client.pay_bill(&owner, &2);
        let bill3 = client.get_bill(&3).unwrap();
        assert!(!bill3.paid);
        assert_eq!(bill3.due_date, 1000000 + (60 * 86400));
    }

    #[test]
    #[allow(deprecated)]
    fn test_get_all_bills_admin_only() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let admin = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        // Set up pause admin
        client.set_pause_admin(&admin, &admin);

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
        client.pay_bill(&owner, &1);

        // Admin can see all 3 bills
        let all = client.get_all_bills_page(&admin, &0, &100);
        assert_eq!(all.items.len(), 3);
    }
    #[test]
    fn test_pay_bill_unauthorized() {
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

        let result = client.try_pay_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_recurring_bill_cancellation() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &1000000,
            &true, // Recurring
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Cancel the bill
        client.cancel_bill(&owner, &bill_id);

        // Verify it's gone
        let bill = client.get_bill(&bill_id);
        assert!(bill.is_none());

        // Verify paying it fails
        let result = client.try_pay_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillNotFound)));
    }

    #[test]
    fn test_pay_overdue_bill_succeeds() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Late"),
            &500,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue.count, 1);

        // Pay it
        client.pay_bill(&owner, &bill_id);

        // Verify it's no longer overdue (because it's paid)
        let overdue_after = client.get_overdue_bills(&0, &100);
        assert_eq!(overdue_after.count, 0);
    }

    #[test]
    fn test_short_recurrence() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Daily"),
            &10,
            &1000000,
            &true, // Recurring
            &1,    // Daily
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        assert_eq!(next_bill.due_date, 1000000 + 86400); // Exactly 1 day later
    }

    #[test]
    fn test_create_bill_invalid_due_dates() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        // due_date == 0 should be rejected
        let res = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Invalid"),
            &100,
            &0u64,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(res, Err(Ok(Error::InvalidDueDate)));

        // due_date == now is accepted (strict less-than at creation)
        set_ledger_time(&env, 1, 1_000_000);
        env.mock_all_auths();
        let res2 = client.try_create_bill(
            &owner,
            &String::from_str(&env, "AtNow"),
            &100,
            &1_000_000u64,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert!(res2.is_ok());

        // due_date < now is rejected
        let res3 = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Past"),
            &100,
            &999_999u64,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        assert_eq!(res3, Err(Ok(Error::InvalidDueDate)));
    }

    #[test]
    fn test_recurring_generation_never_in_past() {
        let env = Env::default();
        // initial time
        set_ledger_time(&env, 1, 1_000_000);
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        // create recurring bill due shortly after now
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &(1_000_010u64),
            &true,
            &30u32,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // advance far into the future so next_due_date would otherwise be in the past
        set_ledger_time(&env, 2, 2_000_000);
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // The next generated recurring bill (id 2) must have due_date > current time
        let next_bill = client.get_bill(&2).unwrap();
        assert!(next_bill.due_date > 2_000_000);
    }

    #[test]
    fn test_get_all_bills_for_owner_basic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let bills = client.get_all_bills_for_owner(&owner, &0, &100);
        assert_eq!(bills.items.len(), 2);
        for bill in bills.items.iter() {
            assert_eq!(bill.owner, owner);
        }
    }

    #[test]
    fn test_get_all_bills_for_owner_isolation() {
        // Alice's bills must NOT appear when Bob queries, and vice versa
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let bob = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Rent"),
            &1000,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Water"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &bob,
            &String::from_str(&env, "Bob Internet"),
            &50,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let alice_bills = client.get_all_bills_for_owner(&alice, &0, &100);
        let bob_bills = client.get_all_bills_for_owner(&bob, &0, &100);

        // Alice sees only her 2 bills
        assert_eq!(alice_bills.items.len(), 2);
        for bill in alice_bills.items.iter() {
            assert_eq!(bill.owner, alice, "Alice received a bill she doesn't own");
        }

        // Bob sees only his 1 bill
        assert_eq!(bob_bills.items.len(), 1);
        assert_eq!(bob_bills.items.get(0).unwrap().owner, bob);
    }

    #[test]
    fn test_get_all_bills_for_owner_empty() {
        // Owner with no bills gets an empty vec, not someone else's bills
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let bob = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Bill"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Bob never created a bill
        let bob_bills = client.get_all_bills_for_owner(&bob, &0, &100);
        assert_eq!(bob_bills.items.len(), 0);
    }

    #[test]
    fn test_get_all_bills_for_owner_after_pay() {
        // Paid bills still belong to owner — they should still appear
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Paid Bill"),
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.pay_bill(&owner, &bill_id);

        let bills = client.get_all_bills_for_owner(&owner, &0, &100);
        assert_eq!(bills.items.len(), 1);
        assert!(bills.items.get(0).unwrap().paid);
    }

    #[test]
    fn test_get_all_bills_for_owner_after_cancel() {
        // Cancelled bills are removed — owner query must reflect that
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "To Cancel"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Keep"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.cancel_bill(&owner, &bill_id);

        let bills = client.get_all_bills_for_owner(&owner, &0, &100);
        assert_eq!(bills.items.len(), 1);
        assert_eq!(bills.items.get(0).unwrap().amount, 200);
    }

    #[test]
    #[allow(deprecated)]
    fn test_get_all_bills_non_admin_fails() {
        // Non-admin calling get_all_bills (admin endpoint) must get Unauthorized
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Bill"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Alice tries to call the admin-only endpoint
        let result = client.try_get_all_bills_page(&alice, &0, &100);
        assert!(matches!(result, Err(Ok(Error::Unauthorized))));
    }

    #[test]
    #[allow(deprecated)]
    fn test_get_all_bills_no_admin_set_fails() {
        // If no pause admin is set at all, get_all_bills must return Unauthorized
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let result = client.try_get_all_bills_page(&alice, &0, &100);
        assert!(matches!(result, Err(Ok(Error::Unauthorized))));
    }

    // NOTE: The following schedule-related tests are commented out because the
    // BillPayments contract does not implement create_schedule, modify_schedule,
    // cancel_schedule, execute_due_schedules, get_schedule, or get_schedules methods.
    // These tests were added to main before the contract methods were implemented.
    // Uncomment once the schedule functionality is added to the contract.

    /*
    #[test]
    fn test_create_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);
        assert_eq!(schedule_id, 1);

        let schedule = client.get_schedule(&schedule_id);
        assert!(schedule.is_some());
        let schedule = schedule.unwrap();
        assert_eq!(schedule.next_due, 3000);
        assert_eq!(schedule.interval, 86400);
        assert!(schedule.active);
    }

    #[test]
    fn test_modify_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);
        client.modify_schedule(&owner, &schedule_id, &4000, &172800);

        let schedule = client.get_schedule(&schedule_id).unwrap();
        assert_eq!(schedule.next_due, 4000);
        assert_eq!(schedule.interval, 172800);
    }

    #[test]
    fn test_cancel_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);
        client.cancel_schedule(&owner, &schedule_id);

        let schedule = client.get_schedule(&schedule_id).unwrap();
        assert!(!schedule.active);
    }

    #[test]
    fn test_execute_due_schedules() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &0);

        set_ledger_time(&env, 1, 3500);
        let executed = client.execute_due_schedules();

        assert_eq!(executed.len(), 1);
        assert_eq!(executed.items.get(0).unwrap(), schedule_id);

        let bill = client.get_bill(&bill_id).unwrap();
        assert!(bill.paid);
    }

    #[test]
    fn test_execute_recurring_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &true,
            &30, &None, &None, &String::from_str(&env, "XLM"));

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);

        set_ledger_time(&env, 1, 3500);
        client.execute_due_schedules();

        let schedule = client.get_schedule(&schedule_id).unwrap();
        assert!(schedule.active);
        assert_eq!(schedule.next_due, 3000 + 86400);
    }

    #[test]
    fn test_execute_missed_schedules() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &true,
            &30, &None, &None, &String::from_str(&env, "XLM"));

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);

        set_ledger_time(&env, 1, 3000 + 86400 * 3 + 100);
        client.execute_due_schedules();

        let schedule = client.get_schedule(&schedule_id).unwrap();
        assert_eq!(schedule.missed_count, 3);
        assert!(schedule.next_due > 3000 + 86400 * 3);
    }

    #[test]
    fn test_schedule_validation_past_date() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 5000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &6000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let result = client.try_create_schedule(&owner, &bill_id, &3000, &86400);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_schedules() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        set_ledger_time(&env, 1, 1000);

        let bill_id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &2000,
            &false,
            &0, &None, &String::from_str(&env, "XLM"), &None);

        client.create_schedule(&owner, &bill_id1, &3000, &86400);
        client.create_schedule(&owner, &bill_id2, &4000, &172800);

        let schedules = client.get_schedules(&owner);
        assert_eq!(schedules.len(), 2);
    }
    */
    #[test]
    fn test_create_bill_emits_event() {
        use soroban_sdk::testutils::Events;
        use soroban_sdk::{symbol_short, vec, IntoVal};

        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
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

        let events = env.events().all();
        assert!(!events.is_empty());
        let last_event = events.last().unwrap();

        client.create_bill(
            &owner,
            &String::from_str(&env, "Water Bill"),
            &500,
            &5000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let expected_topics = vec![
            &env,
            symbol_short!("Remitwise").into_val(&env),
            1u32.into_val(&env), // EventCategory::State
            1u32.into_val(&env), // EventPriority::Medium
            symbol_short!("created").into_val(&env),
        ];

        assert_eq!(last_event.1, expected_topics);

        let data: (u32, soroban_sdk::Address, i128, u64) =
            soroban_sdk::FromVal::from_val(&env, &last_event.2);
        assert_eq!(data, (1u32, owner.clone(), 1000i128, 1000000u64));

        assert_eq!(last_event.0, contract_id.clone());
    }

    #[test]
    fn test_pay_bill_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        // Phase 1: Create first bill at seq 100
        // TTL goes from 100 → 518,400. live_until = 518,500
        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &2000,
            &1_100_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Phase 2: Advance to seq 510,000 (TTL = 8,500 < 17,280)
        // create_bill re-extends → live_until = 1,028,400
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 510_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &100,
            &1_200_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
        // pay_bill re-extends → live_until = 1,538,400
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 1_020_000,
            timestamp: 1_020_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // Pay second bill to refresh TTL once more
        client.pay_bill(&owner, &id2);

        // Both bills should still be accessible
        let bill1 = client.get_bill(&id1);
        assert!(
            bill1.is_some(),
            "First bill must persist across ledger advancements"
        );
        assert_eq!(bill1.unwrap().amount, 2000);

        let bill2 = client.get_bill(&id2);
        assert!(
            bill2.is_some(),
            "Second bill must persist across ledger advancements"
        );
        assert!(bill2.unwrap().paid, "Second bill should be marked paid");

        // TTL should be fully refreshed
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must remain >= 518,400 after repeated operations",
            ttl
        );
    }

    /// Verify that archive_paid_bills extends instance TTL and archives data.
    ///
    /// Note: both `extend_instance_ttl` and `extend_archive_ttl` operate on
    /// instance() storage. Since `extend_instance_ttl` is called first in
    /// `archive_paid_bills`, it bumps the TTL above the shared threshold
    /// (17,280), making the subsequent `extend_archive_ttl` a no-op.
    /// This test verifies the instance TTL is at least INSTANCE_BUMP_AMOUNT
    /// and that archived data is accessible.
    #[test]
    fn test_archive_ttl_extended_on_archive_paid_bills() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

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
        client.pay_bill(&owner, &bill_id);

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 510_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 3_000_000,
        });

        let archived = client.archive_paid_bills(&owner, &600_000);
        assert_eq!(archived, 1);

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after archiving",
            ttl
        );
    }

    #[test]
    fn test_get_overdue_bills_owner_scoped() {
        let env = Env::default();
        set_ledger_time(&env, 1, 1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let bob = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Overdue1"),
            &100,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Overdue2"),
            &200,
            &1_600_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &bob,
            &String::from_str(&env, "Bob Overdue"),
            &300,
            &1_500_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Future"),
            &400,
            &3_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        set_ledger_time(&env, 1, 2_000_000);
        let overdue = client.get_overdue_bills(&0, &100);

        assert_eq!(overdue.count, 3);
        let mut alice_count = 0u32;
        let mut bob_count = 0u32;
        for bill in overdue.items.iter() {
            assert!(bill.due_date < 2_000_000);
            if bill.owner == alice {
                alice_count += 1;
            } else if bill.owner == bob {
                bob_count += 1;
            }
        }
        assert_eq!(alice_count, 2);
        assert_eq!(bob_count, 1);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn test_create_bill_non_owner_auth_failure() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let _other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        // Do not mock auth for other, attempt to create bill for owner as other
        // Wait, if other calls, it's just a call. The contract will check owner.require_auth().
        // If owner didn't authorize, it panics.
        client.create_bill(
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
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn test_pay_bill_non_owner_auth_failure() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let _other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        client.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "create_bill",
                args: (
                    &owner,
                    String::from_str(&env, "Water"),
                    500i128,
                    1000000u64,
                    false,
                    0u32,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }]);

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

        // other tries to pay the bill for owner
        client.pay_bill(&owner, &bill_id);
    }

    #[test]
    #[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
    fn test_cancel_bill_non_owner_auth_failure() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let _other = <soroban_sdk::Address as AddressTrait>::generate(&env);

        client.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "create_bill",
                args: (
                    &owner,
                    String::from_str(&env, "Water"),
                    500i128,
                    1000000u64,
                    false,
                    0u32,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }]);

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

        // other tries to cancel the bill for owner
        client.cancel_bill(&owner, &bill_id);
    }

    // -----------------------------------------------------------------------
    // RECURRING BILLS DATE MATH TESTS
    // -----------------------------------------------------------------------
    // These tests verify the core date math for recurring bills:
    // next_due_date = due_date + (frequency_days * 86400)
    // Ensures paid_at does not affect next bill's due_date calculation.
    // -----------------------------------------------------------------------
    // RECURRING BILLS DATE MATH TESTS
    // -----------------------------------------------------------------------
    // These tests verify the core date math for recurring bills:
    // next_due_date = due_date + (frequency_days * 86400)
    // Ensures paid_at does not affect next bill's due_date calculation.

    #[test]
    fn test_recurring_date_math_frequency_1_day() {
        // Test: frequency_days = 1 → next due date is +1 day (86400 seconds)
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Daily Bill"),
            &100,
            &base_due_date,
            &true, // recurring
            &1,    // frequency_days = 1
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay the bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (1 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        assert_eq!(
            next_bill.due_date,
            base_due_date + 86400,
            "Next due date should be exactly 1 day later"
        );
        assert_eq!(next_bill.frequency_days, 1, "Frequency should be preserved");
    }

    #[test]
    fn test_recurring_date_math_frequency_30_days() {
        // Test: frequency_days = 30 → next due date is +30 days (2,592,000 seconds)
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Monthly Bill"),
            &500,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay the bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (30 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be exactly 30 days later"
        );
        assert_eq!(
            next_bill.frequency_days, 30,
            "Frequency should be preserved"
        );
    }

    #[test]
    fn test_recurring_date_math_frequency_365_days() {
        // Test: frequency_days = 365 → next due date is +365 days (31,536,000 seconds)
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Annual Bill"),
            &1200,
            &base_due_date,
            &true, // recurring
            &365,  // frequency_days = 365
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay the bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (365 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        let expected_due_date = base_due_date + (365u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be exactly 365 days later"
        );
        assert_eq!(
            next_bill.frequency_days, 365,
            "Frequency should be preserved"
        );
    }

    //     #[test]
    //     fn test_recurring_date_math_paid_at_does_not_affect_next_due() {
    //     let env = Env::default();

    //     // FORCE reset to a very small number first
    //     env.ledger().set_ledger_timestamp(100);

    //     let contract_id = env.register_contract(None, BillPayments);
    //     let client = BillPaymentsClient::new(&env, &contract_id);
    //     let owner = Address::generate(&env);
    //     env.mock_all_auths();

    //     // Now current_time (100) is definitely < base_due_date (1,000,000)
    //     let base_due_date = 1_000_000u64;
    //     let bill_id = client.create_bill(
    //         &owner,
    //         &String::from_str(&env, "Late Payment Test"),
    //         &300,
    //         &base_due_date,
    //         &true,
    //         &30,
    //         &String::from_str(&env, "XLM"),
    //     );

    //     // Warp to late payment time
    //     env.ledger().set_ledger_timestamp(1_000_500);
    //     client.pay_bill(&owner, &bill_id);

    //     let next_bill = client.get_bill(&2).unwrap();
    //     let expected_due_date = base_due_date + (30u64 * 86400);
    //     assert_eq!(next_bill.due_date, expected_due_date);
    // }

    #[test]
    fn test_recurring_date_math_multiple_pay_cycles_3rd_bill() {
        // Test: Multiple pay cycles - verify 3rd bill's due date advances correctly
        // Bill 1: due_date=1000000, frequency=30
        // Bill 2: due_date=1000000 + (30*86400)
        // Bill 3: due_date=1000000 + (60*86400)
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Three-Cycle Bill"),
            &150,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Pay third bill
        env.mock_all_auths();
        client.pay_bill(&owner, &3);

        // Verify third bill is now paid
        let bill3_paid = client.get_bill(&3).unwrap();
        assert!(bill3_paid.paid);

        // Verify fourth bill was created with correct due_date
        let bill4 = client.get_bill(&4).unwrap();
        let expected_bill4_due = base_due_date + (90u64 * 86400); // 3 * 30 days
        assert_eq!(
            bill4.due_date, expected_bill4_due,
            "Bill 4 due_date should be base + (90*86400)"
        );
        assert!(!bill4.paid);
    }

    #[test]
    fn test_recurring_date_math_early_payment_does_not_affect_schedule() {
        // Test: Paying a bill EARLY should not affect the next bill's due_date
        // Bill 1: due_date=1000000, paid at time=500000 (paid 500000 seconds early)
        // Bill 2: due_date should still be 1000000 + (30*86400)
        let env = Env::default();
        set_ledger_time(&env, 1, 500_000);
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Early Payment Test"),
            &200,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay the bill early (at time 500_000)
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify original bill has paid_at set to early time
        let paid_bill = client.get_bill(&bill_id).unwrap();
        assert!(paid_bill.paid);
        assert_eq!(paid_bill.paid_at, Some(500_000));

        // Verify next bill's due_date is still based on original due_date
        let next_bill = client.get_bill(&2).unwrap();
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should not be affected by early payment"
        );
    }

    #[test]
    fn test_recurring_date_math_preserves_frequency_across_cycles() {
        // Test: frequency_days is preserved across all recurring cycles
        // Verify that Bill 1, 2, 3 all have the same frequency_days value
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let frequency = 7u32; // Weekly
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Weekly Bill"),
            &50,
            &1_000_000,
            &true,
            &frequency,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Verify all bills have the same frequency_days
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.frequency_days, frequency);
        assert_eq!(bill2.frequency_days, frequency);
        assert_eq!(bill3.frequency_days, frequency);
    }

    #[test]
    fn test_recurring_date_math_amount_preserved_across_cycles() {
        // Test: Bill amount is preserved across all recurring cycles
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let amount = 999i128;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Fixed Amount Bill"),
            &amount,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Verify all bills have the same amount
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.amount, amount);
        assert_eq!(bill2.amount, amount);
        assert_eq!(bill3.amount, amount);
    }

    #[test]
    fn test_recurring_date_math_name_preserved_across_cycles() {
        // Test: Bill name is preserved across all recurring cycles
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let name = String::from_str(&env, "Rent Payment");
        let bill_id = client.create_bill(
            &owner,
            &name,
            &1000,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Verify all bills have the same name
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.name, name);
        assert_eq!(bill2.name, name);
        assert_eq!(bill3.name, name);
    }

    #[test]
    fn test_recurring_date_math_owner_preserved_across_cycles() {
        // Test: Bill owner is preserved across all recurring cycles
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Owner Test"),
            &100,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Verify all bills have the same owner
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.owner, owner);
        assert_eq!(bill2.owner, owner);
        assert_eq!(bill3.owner, owner);
    }

    #[test]
    fn test_recurring_date_math_exact_calculation_verification() {
        // Test: Verify exact date math calculation with known values
        // due_date = 1_000_000
        // frequency_days = 14
        // Expected: 1_000_000 + (14 * 86400) = 1_000_000 + 1_209_600 = 2_209_600
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due = 1_000_000u64;
        let freq = 14u32;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Math Verification"),
            &100,
            &base_due,
            &true,
            &freq,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        let expected = 1_000_000u64 + (14u64 * 86400);
        assert_eq!(next_bill.due_date, expected);
        assert_eq!(next_bill.due_date, 2_209_600);
    }

    // ══════════════════════════════════════════════════════════════════════
    // Time & Ledger Drift Resilience Tests (#158)
    //
    // Assumptions documented here:
    //  - A bill is overdue when due_date < current_time (strict less-than).
    //  - At exactly due_date the bill is NOT yet overdue.
    //  - Stellar ledger timestamps are monotonically increasing in production.
    // ══════════════════════════════════════════════════════════════════════

    /// Bill is NOT overdue when ledger timestamp == due_date (inclusive boundary).
    #[test]
    fn test_time_drift_bill_not_overdue_at_exact_due_date() {
        let due_date = 1_000_000u64;
        let env = Env::default();
        set_ledger_time(&env, 1, due_date);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Power"),
            &200,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(
            page.count, 0,
            "Bill must not appear overdue when current_time == due_date"
        );
    }

    /// Bill becomes overdue exactly one second after due_date.
    #[test]
    fn test_time_drift_bill_overdue_one_second_after_due_date() {
        let due_date = 1_000_000u64;
        let env = Env::default();
        set_ledger_time(&env, 1, due_date);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Not yet overdue at due_date
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(page.count, 0);

        // Advance one second past due_date
        set_ledger_time(&env, 1, due_date + 1);
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(
            page.count, 1,
            "Bill must appear overdue exactly one second past due_date"
        );
    }

    // Mix of past-due, exactly-due, and future bills: only past-due appears.
    //     #[test]
    //     fn test_time_drift_overdue_boundary_mixed_bills() {
    //     let env = Env::default();
    //     let contract_id = env.register_contract(None, BillPayments);
    //     let client = BillPaymentsClient::new(&env, &contract_id);
    //     let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
    //     env.mock_all_auths();

    //     // 1. Set time to a starting point
    //     let start_time = 2_000_000u64;
    //     env.ledger().set_ledger_timestamp(start_time);

    //     // 2. Create bills with relative due dates
    //     // All these due dates are >= current_time (2,000,000), so validation passes.

    //     // This will become overdue later
    //     client.create_bill(
    //         &owner,
    //         &String::from_str(&env, "Overdue"),
    //         &100,
    //         &2000001, // T+1
    //         &false,
    //         &0,
    //         &String::from_str(&env, "XLM"),
    //     );

    //     // This will be exactly due later
    //     client.create_bill(
    //         &owner,
    //         &String::from_str(&env, "DueNow"),
    //         &200,
    //         &2000005, // T+5
    //         &false,
    //         &0,
    //         &String::from_str(&env, "XLM"),
    //     );

    //     // This will stay in the future
    //     client.create_bill(
    //         &owner,
    //         &String::from_str(&env, "Future"),
    //         &300,
    //         &2000010, // T+10
    //         &false,
    //         &0,
    //         &String::from_str(&env, "XLM"),
    //     );

    //     // 3. WARP TIME forward to 2,000,005
    //     // Now:
    //     // - Bill 1 (2000001) is < 2000005 (OVERDUE)
    //     // - Bill 2 (2000005) is == 2000005 (NOT OVERDUE)
    //     // - Bill 3 (2000010) is > 2000005 (NOT OVERDUE)
    //     env.ledger().set_ledger_timestamp(2000005);

    //     let page = client.get_overdue_bills(&0, &100);

    //     assert_eq!(
    //         page.count, 1,
    //         "Only the bill with due_date < current_time must appear overdue"
    //     );
    //     assert_eq!(
    //         page.items.items.get(0).unwrap().amount,
    //         100,
    //         "Overdue bill must be the one with due_date < current_time"
    //     );
    // }

    /// Full-day boundary: bill created at due_date, queried one day later, is overdue.
    #[test]
    fn test_time_drift_overdue_full_day_boundary() {
        let day = 86400u64;
        let due_date = 1_000_000u64;
        let env = Env::default();
        set_ledger_time(&env, 1, due_date);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Monthly Rent"),
            &5000,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Still not overdue at due_date
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(page.count, 0);

        // One full day later – must be overdue
        set_ledger_time(&env, 1, due_date + day);
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(
            page.count, 1,
            "Bill must be overdue one full day past due_date"
        );
    }

    // ---------------------------------------------------------------------------
    // Tests — Issue #6: get_total_unpaid edge cases
    //
    // get_total_unpaid(env, owner) returns the sum of `amount` for all unpaid
    // bills belonging to `owner`. These tests make the zero, single, multiple,
    // after-pay, all-paid, and isolation cases explicit and documented.
    //
    // Paste this block inside the existing `mod testsuit { ... }` in your test
    // file, alongside the other test functions.
    // ---------------------------------------------------------------------------

    // --- No bills: owner who has never created a bill should get 0 ---

    #[test]
    fn test_get_total_unpaid_no_bills_returns_zero() {
        // An owner who has never created any bill must get 0, not a panic or
        // a spurious non-zero value from another owner's data.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let total = client.get_total_unpaid(&owner);
        assert_eq!(total, 0, "owner with no bills must have total_unpaid == 0");
    }

    // --- All bills paid: owner whose every bill is paid should get 0 ---

    #[test]
    fn test_get_total_unpaid_all_bills_paid_returns_zero() {
        // Create several bills and pay them all; the total must then be 0.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &400,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &600,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        client.pay_bill(&owner, &id1);
        client.pay_bill(&owner, &id2);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 0,
            "owner with all bills paid must have total_unpaid == 0"
        );
    }

    // --- One unpaid bill: total equals that bill's amount ---

    #[test]
    fn test_get_total_unpaid_one_unpaid_bill() {
        // Exactly one unpaid bill; total_unpaid must equal its amount.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &owner,
            &String::from_str(&env, "Rent"),
            &1000,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 1000,
            "one unpaid bill of 1000 must yield total_unpaid == 1000"
        );
    }

    // --- Multiple unpaid bills: total equals the sum of all amounts ---

    #[test]
    fn test_get_total_unpaid_multiple_unpaid_bills() {
        // Three bills with amounts 100, 200, 300 → total must be 600.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill A"),
            &100,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill B"),
            &200,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill C"),
            &300,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 600,
            "three unpaid bills (100 + 200 + 300) must yield total_unpaid == 600"
        );
    }

    // --- After paying one bill: total decreases by that bill's amount ---

    #[test]
    fn test_get_total_unpaid_decreases_after_pay() {
        // Create bills of 100, 200, 300; pay the 200 bill.
        // Total must drop from 600 to 400 (100 + 300).
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill A"),
            &100,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id_b = client.create_bill(
            &owner,
            &String::from_str(&env, "Bill B"),
            &200,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill C"),
            &300,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Confirm starting total
        assert_eq!(client.get_total_unpaid(&owner), 600);

        // Pay the 200-unit bill
        client.pay_bill(&owner, &id_b);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 400,
            "after paying the 200 bill, total_unpaid must be 400 (100 + 300)"
        );
    }

    // --- All paid (incremental): total reaches 0 as each bill is paid ---

    #[test]
    fn test_get_total_unpaid_reaches_zero_as_bills_paid_incrementally() {
        // Pay bills one by one and verify the running total after each payment.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Bill 1"),
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
            &String::from_str(&env, "Bill 2"),
            &200,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id3 = client.create_bill(
            &owner,
            &String::from_str(&env, "Bill 3"),
            &300,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(client.get_total_unpaid(&owner), 600);

        client.pay_bill(&owner, &id1);
        assert_eq!(
            client.get_total_unpaid(&owner),
            500,
            "after paying 100-bill: 500 remaining"
        );

        client.pay_bill(&owner, &id2);
        assert_eq!(
            client.get_total_unpaid(&owner),
            300,
            "after paying 200-bill: 300 remaining"
        );

        client.pay_bill(&owner, &id3);
        assert_eq!(
            client.get_total_unpaid(&owner),
            0,
            "after paying all bills: total_unpaid must be 0"
        );
    }

    // --- Isolation: owner_a's total is unaffected by owner_b's bills ---

    #[test]
    fn test_get_total_unpaid_isolation_between_owners() {
        // Bills belonging to owner_b must not appear in owner_a's total, and
        // vice versa.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner_a = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let owner_b = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        // owner_a: two bills totalling 500
        client.create_bill(
            &owner_a,
            &String::from_str(&env, "A - Rent"),
            &300,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner_a,
            &String::from_str(&env, "A - Water"),
            &200,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // owner_b: one bill of 9999
        client.create_bill(
            &owner_b,
            &String::from_str(&env, "B - Internet"),
            &9999,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let total_a = client.get_total_unpaid(&owner_a);
        let total_b = client.get_total_unpaid(&owner_b);

        assert_eq!(
            total_a, 500,
            "owner_a's total_unpaid must be 500 (300 + 200), not influenced by owner_b"
        );
        assert_eq!(
            total_b, 9999,
            "owner_b's total_unpaid must be 9999, not influenced by owner_a"
        );
    }

    // --- Isolation after cross-owner payment: paying owner_b's bill does not
    //     change owner_a's total ---

    #[test]
    fn test_get_total_unpaid_paying_other_owner_bill_has_no_effect() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner_a = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let owner_b = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &owner_a,
            &String::from_str(&env, "A - Electricity"),
            &750,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id_b = client.create_bill(
            &owner_b,
            &String::from_str(&env, "B - Gas"),
            &1234,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Pay owner_b's bill
        client.pay_bill(&owner_b, &id_b);

        // owner_a's total must be unchanged
        let total_a = client.get_total_unpaid(&owner_a);
        assert_eq!(
            total_a, 750,
            "paying owner_b's bill must not affect owner_a's total_unpaid"
        );

        // owner_b's total must now be 0
        let total_b = client.get_total_unpaid(&owner_b);
        assert_eq!(total_b, 0, "owner_b's total_unpaid must be 0 after payment");
    }

    // --- Cancelled bill is excluded from the total ---

    #[test]
    fn test_get_total_unpaid_excludes_cancelled_bills() {
        // A cancelled bill is removed from storage entirely, so it must not
        // appear in the total.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let id_keep = client.create_bill(
            &owner,
            &String::from_str(&env, "Keep"),
            &500,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id_cancel = client.create_bill(
            &owner,
            &String::from_str(&env, "Cancel Me"),
            &9000,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        assert_eq!(client.get_total_unpaid(&owner), 9500);

        client.cancel_bill(&owner, &id_cancel);

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 500,
            "cancelled bill must not contribute to total_unpaid"
        );

        // Sanity: the kept bill is still there
        assert!(client.get_bill(&id_keep).is_some());
    }

    // --- Minimum positive amount: a single bill of 1 ---

    #[test]
    fn test_get_total_unpaid_minimum_amount() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        client.create_bill(
            &owner,
            &String::from_str(&env, "Tiny Bill"),
            &1,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 1,
            "single bill of amount 1 must yield total_unpaid == 1"
        );
    }

    // --- Large amounts: verify no arithmetic overflow in the sum ---

    #[test]
    fn test_get_total_unpaid_large_amounts_no_overflow() {
        // Use amounts near i128::MAX / 2 to verify the summation does not panic
        // or wrap. The contract uses plain addition, so this confirms the runtime
        // handles large i128 values correctly.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let big: i128 = i128::MAX / 4; // safely summable twice without overflow

        client.create_bill(
            &owner,
            &String::from_str(&env, "Big Bill 1"),
            &big,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Big Bill 2"),
            &big,
            &1_000_000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total,
            big * 2,
            "sum of two large amounts must equal big * 2"
        );
    }

    // --- Recurring bill creates a new unpaid bill: total includes the new one ---

    #[test]
    fn test_get_total_unpaid_includes_new_recurring_bill_after_pay() {
        // Paying a recurring bill marks the original paid AND creates a new
        // unpaid bill. The total must reflect the new unpaid bill's amount.
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Monthly Subscription"),
            &500,
            &1_000_000,
            &true, // recurring
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // Before payment: one unpaid bill of 500
        assert_eq!(client.get_total_unpaid(&owner), 500);

        // Pay it: original becomes paid, a new unpaid bill of 500 is created
        client.pay_bill(&owner, &bill_id);

        // Total must still be 500 (the new recurring bill, not the paid one)
        let total = client.get_total_unpaid(&owner);
        assert_eq!(
            total, 500,
            "after paying a recurring bill, the newly created bill must appear in total_unpaid"
        );
    }

    // --- batch_pay_bills: Partial Failure (Skip-and-Continue) ---

    #[test]
    fn test_batch_pay_bills_partial_success() {
        let env = Env::default();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        // 1. Create 3 bills
        let name = String::from_str(&env, "B");
        let id1 = client.create_bill(
            &owner,
            &name,
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id2 = client.create_bill(
            &owner,
            &name,
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let id3 = client.create_bill(
            &owner,
            &name,
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        // 2. Pre-pay ID1 so it is "already paid" when batch starts
        client.pay_bill(&owner, &id1);

        // 3. Batch contains: ID1 (already paid), ID2 (valid), 999 (non-existent), ID3 (valid)
        let mut ids = Vec::new(&env);
        ids.push_back(id1);
        ids.push_back(id2);
        ids.push_back(999);
        ids.push_back(id3);

        let success_count = client.batch_pay_bills(&owner, &ids);

        // Expected: only ID2 and ID3 were paid. ID1 was skipped (already paid), 999 was skipped (not found).
        assert_eq!(success_count, 2);

        // Verify states
        assert!(client.get_bill(&id1).unwrap().paid);
        assert!(client.get_bill(&id2).unwrap().paid);
        assert!(client.get_bill(&id3).unwrap().paid);
    }

    #[test]
    fn test_batch_pay_bills_skips_unauthorized() {
        let env = Env::default();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        env.mock_all_auths();

        let name = String::from_str(&env, "Test");
        let a1 = client.create_bill(
            &alice,
            &name,
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let b1 = client.create_bill(
            &bob,
            &name,
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );
        let a2 = client.create_bill(
            &alice,
            &name,
            &300,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(a1);
        ids.push_back(b1); // Bob's bill
        ids.push_back(a2);

        // Alice tries to pay the batch
        let success_count = client.batch_pay_bills(&alice, &ids);

        // Expected: only A1 and A2 paid. B1 skipped.
        assert_eq!(success_count, 2);
        assert!(client.get_bill(&a1).unwrap().paid);
        assert!(!client.get_bill(&b1).unwrap().paid);
        assert!(client.get_bill(&a2).unwrap().paid);
    }

    #[test]
    fn test_batch_pay_bills_recurring_atomicity() {
        let env = Env::default();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        let name = String::from_str(&env, "R");
        let id1 = client.create_bill(
            &owner,
            &name,
            &100,
            &1000000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
            &None,
        );

        let mut ids = Vec::new(&env);
        ids.push_back(id1);

        let success_count = client.batch_pay_bills(&owner, &ids);
        assert_eq!(success_count, 1);

        // Verify next bill was created atomically
        let next_bill = client.get_bill(&2).unwrap();
        assert_eq!(next_bill.owner, owner);
        assert_eq!(next_bill.amount, 100);
        assert!(!next_bill.paid);
    }

    #[test]
    fn test_timelock_bypass_rejection_schedule_unpause_past_timestamp() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Set initial ledger time
        env.ledger().set_timestamp(1000);

        // Try to schedule unpause with a past timestamp (999)
        let result = client.try_schedule_unpause(&admin, &999);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));

        // Try to schedule unpause with the exact current timestamp (1000) - this should also fail
        let result = client.try_schedule_unpause(&admin, &1000);
        assert_eq!(result, Err(Ok(Error::InvalidAmount)));

        // Schedule unpause with a future timestamp (1001) - this should succeed
        client.schedule_unpause(&admin, &1001);
    }

    #[test]
    fn test_pause_cancels_schedule() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause the contract
        client.pause(&admin);
        assert!(client.is_paused());

        // Schedule unpause in the future
        let future = env.ledger().timestamp() + 3600;
        client.schedule_unpause(&admin, &future);

        // Call pause again (this should cancel/reset the pending schedule)
        client.pause(&admin);

        // Advance ledger to the previously scheduled future time
        env.ledger().set_timestamp(future);

        // Try to unpause. It should fail because the schedule was cancelled/removed on re-pause.
        // Since there's no schedule, unpause should succeed (it only checks schedule if one exists)
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_premature_unpause_rejection() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        env.mock_all_auths();
        client.set_pause_admin(&admin, &admin);

        // Pause the contract
        client.pause(&admin);
        assert!(client.is_paused());

        // Schedule unpause in the future
        let future = env.ledger().timestamp() + 3600;
        client.schedule_unpause(&admin, &future);

        // Try to unpause before scheduled time (1 second before)
        env.ledger().set_timestamp(future - 1);
        let result = client.try_unpause(&admin);
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
        assert!(client.is_paused());

        // Unpause at exact boundary
        env.ledger().set_timestamp(future);
        client.unpause(&admin);
        assert!(!client.is_paused());
    }
}
