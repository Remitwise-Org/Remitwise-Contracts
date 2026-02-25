#[cfg(test)]
mod testsuit {
    use crate::*;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as AddressTrait, Ledger, LedgerInfo};
    use soroban_sdk::Env;

    fn set_time(env: &Env, timestamp: u64) {
        let proto = env.ledger().protocol_version();

        env.ledger().set(LedgerInfo {
            protocol_version: proto,
            sequence_number: 1,
            timestamp,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 100000,
        });
    }

    #[test]
    fn test_create_bill() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        assert_eq!(bill_id, 1);

        let bill = client.get_bill(&1);
        assert!(bill.is_some());
        let bill = bill.unwrap();
        assert_eq!(bill.amount, 1000);
        assert!(!bill.paid);
    }

    #[test]
    fn test_create_bill_invalid_amount() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let result = client.try_create_bill(
            &owner,
            &String::from_str(&env, "Invalid"),
            &0,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.pay_bill(&owner, &1);

        let unpaid = client.get_unpaid_bills(&owner);
        assert_eq!(unpaid.len(), 2);
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
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);
        let result = client.try_pay_bill(&owner, &bill_id);
        assert_eq!(result, Err(Ok(Error::BillAlreadyPaid)));
    }

    #[test]
    fn test_get_overdue_bills() {
        let env = Env::default();
        set_time(&env, 2_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);
        env.mock_all_auths();
        // Create bills with different due dates
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue1"),
            &100,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue2"),
            &200,
            &1500000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &3000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let overdue = client.get_overdue_bills(&owner);
        assert_eq!(overdue.len(), 2); // Only first two are overdue
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
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);
        
        // Verify cancelled bill is completely removed from storage
        assert!(client.get_bill(&bill_id).is_none(), "cancelled bill should return None");
        
        // Create another bill and verify its ID is distinct and cancelled bill still returns None
        env.mock_all_auths();
        let new_bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "New Bill"),
            &200,
            &2000000,
            &false,
            &0,
        );
        assert_ne!(bill_id, new_bill_id, "new bill should have different ID");
        assert!(client.get_bill(&new_bill_id).is_some(), "new bill should exist");
        assert!(client.get_bill(&bill_id).is_none(), "cancelled bill should still return None");
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
                    &String::from_str(&env, "XLM"),
        );
        env.mock_all_auths();
        client.cancel_bill(&owner, &bill_id);
        
        // Verify owner can successfully cancel their own bill and it's removed
        assert!(client.get_bill(&bill_id).is_none(), "bill should be removed after owner cancellation");
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill2"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Bill3"),
            &300,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.pay_bill(&owner, &1);

        // Admin can see all 3 bills
        let all = client.get_all_bills(&admin);
        assert_eq!(all.len(), 3);
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
                    &String::from_str(&env, "XLM"),
        );

        let result = client.try_pay_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_pay_bill_unauthorized_strict() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);

        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner_a,
            &String::from_str(&env, "Fraud Test"),
            &1000,
            &1000000,
            &false,
            &0,
        );

        let result = client.try_pay_bill(&owner_b, &bill_id);

        assert_eq!(result, Err(Ok(Error::Unauthorized)));

        let bill = client.get_bill(&bill_id).unwrap();
        assert!(!bill.paid);
        assert!(bill.paid_at.is_none());
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
                    &String::from_str(&env, "XLM"),
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
    fn test_pay_overdue_bill() {
        let env = Env::default();
        set_time(&env, 2_000_000); // Set time past due date
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Late"),
            &500,
            &1000000, // Due in past
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        // Verify it shows up in overdue
        let overdue = client.get_overdue_bills(&owner);
        assert_eq!(overdue.len(), 1);

        // Pay it
        client.pay_bill(&owner, &bill_id);

        // Verify it's no longer overdue (because it's paid)
        let overdue_after = client.get_overdue_bills(&owner);
        assert_eq!(overdue_after.len(), 0);
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
                    &String::from_str(&env, "XLM"),
        );

        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        assert_eq!(next_bill.due_date, 1000000 + 86400); // Exactly 1 day later
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
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let bills = client.get_all_bills_for_owner(&owner);
        assert_eq!(bills.len(), 2);
        for bill in bills.iter() {
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
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Water"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &bob,
            &String::from_str(&env, "Bob Internet"),
            &50,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let alice_bills = client.get_all_bills_for_owner(&alice);
        let bob_bills = client.get_all_bills_for_owner(&bob);

        // Alice sees only her 2 bills
        assert_eq!(alice_bills.len(), 2);
        for bill in alice_bills.iter() {
            assert_eq!(bill.owner, alice, "Alice received a bill she doesn't own");
        }

        // Bob sees only his 1 bill
        assert_eq!(bob_bills.len(), 1);
        assert_eq!(bob_bills.get(0).unwrap().owner, bob);
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
                    &String::from_str(&env, "XLM"),
        );

        // Bob never created a bill
        let bob_bills = client.get_all_bills_for_owner(&bob);
        assert_eq!(bob_bills.len(), 0);
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
                    &String::from_str(&env, "XLM"),
        );
        client.pay_bill(&owner, &bill_id);

        let bills = client.get_all_bills_for_owner(&owner);
        assert_eq!(bills.len(), 1);
        assert!(bills.get(0).unwrap().paid);
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
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "Keep"),
            &200,
            &1000000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.cancel_bill(&owner, &bill_id);

        let bills = client.get_all_bills_for_owner(&owner);
        assert_eq!(bills.len(), 1);
        assert_eq!(bills.get(0).unwrap().amount, 200);
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
                    &String::from_str(&env, "XLM"),
        );

        // Alice tries to call the admin-only endpoint
        let result = client.try_get_all_bills(&alice);
        assert_eq!(result.unwrap_err().unwrap(), Error::Unauthorized);
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

        let result = client.try_get_all_bills(&alice);
        assert_eq!(result.unwrap_err().unwrap(), Error::Unauthorized);
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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &0);

        set_time(&env, 3500);
        let executed = client.execute_due_schedules();

        assert_eq!(executed.len(), 1);
        assert_eq!(executed.get(0).unwrap(), schedule_id);

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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &true,
            &30,
                    &String::from_str(&env, "XLM"),
        );

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);

        set_time(&env, 3500);
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
        set_time(&env, 1000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &true,
            &30,
                    &String::from_str(&env, "XLM"),
        );

        let schedule_id = client.create_schedule(&owner, &bill_id, &3000, &86400);

        set_time(&env, 3000 + 86400 * 3 + 100);
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
        set_time(&env, 5000);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &6000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

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
        set_time(&env, 1000);

        let bill_id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let bill_id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        client.create_schedule(&owner, &bill_id1, &3000, &86400);
        client.create_schedule(&owner, &bill_id2, &4000, &172800);

        let schedules = client.get_schedules(&owner);
        assert_eq!(schedules.len(), 2);
    }
    */

    // ========================================================================
    // Storage TTL Extension Tests
    //
    // Verify that instance storage TTL is properly extended on state-changing
    // operations, preventing unexpected data expiration.
    //
    // Contract TTL configuration:
    //   INSTANCE_LIFETIME_THRESHOLD  = 17,280 ledgers (~1 day)
    //   INSTANCE_BUMP_AMOUNT         = 518,400 ledgers (~30 days)
    //   ARCHIVE_LIFETIME_THRESHOLD   = 17,280 ledgers (~1 day)
    //   ARCHIVE_BUMP_AMOUNT          = 2,592,000 ledgers (~180 days)
    //
    // Operations extending instance TTL:
    //   create_bill, pay_bill, archive_paid_bills, restore_bill,
    //   bulk_cleanup_bills, batch_pay_bills
    //
    // Operations extending archive TTL:
    //   archive_paid_bills
    // ========================================================================

    /// Verify that create_bill extends instance storage TTL.
    #[test]
    fn test_instance_ttl_extended_on_create_bill() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        // create_bill calls extend_instance_ttl internally
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Electricity"),
            &1000,
            &2000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        assert_eq!(bill_id, 1);

        // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT (518,400)
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after create_bill",
            ttl
        );
    }

    /// Verify that pay_bill refreshes instance TTL after ledger advancement.
    ///
    /// extend_ttl(threshold, extend_to) only extends when TTL <= threshold.
    /// After create_bill at seq 100 sets TTL to 518,400 (live_until = 518,500),
    /// we must advance past seq 501,220 so TTL drops below 17,280.
    #[test]
    fn test_instance_ttl_refreshed_on_pay_bill() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        client.create_bill(
            &owner,
            &String::from_str(&env, "Water Bill"),
            &500,
            &5000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        // Advance ledger far enough that TTL drops below threshold (17,280).
        // After create_bill: live_until = 100 + 518,400 = 518,500
        // At seq 510,000: TTL = 518,500 - 510,000 = 8,500 < 17,280 ✓
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 500_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // pay_bill calls extend_instance_ttl → re-extends TTL to 518,400
        client.pay_bill(&owner, &1);

        // TTL should be refreshed relative to the new sequence number
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= 518,400 after pay_bill refreshes it",
            ttl
        );
    }

    /// Verify that data remains accessible across repeated operations
    /// spanning multiple ledger advancements, proving TTL is continuously renewed.
    ///
    /// Each phase advances the ledger past the TTL threshold so every
    /// state-changing call actually re-extends the TTL.
    #[test]
    fn test_data_persists_across_repeated_operations() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 3_000_000,
        });

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        // Create and pay a bill so it can be archived
        client.create_bill(
            &owner,
            &String::from_str(&env, "Old Electric"),
            &800,
            &500,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.pay_bill(&owner, &1);

        // Advance ledger so TTL drops below threshold
        // After pay_bill at seq 100: live_until = 518,500
        // At seq 510,000: TTL = 8,500 < 17,280 → archive will re-extend
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

        // archive_paid_bills calls extend_instance_ttl then extend_archive_ttl
        let archived = client.archive_paid_bills(&owner, &600_000);
        assert_eq!(archived, 1);

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after archiving",
            ttl
        );

        // Archived bill should be retrievable
        let archived_bill = client.get_archived_bill(&1);
        assert!(archived_bill.is_some(), "Archived bill must be accessible");
    }

    /// Verify that batch_pay_bills extends instance TTL.
    #[test]
    fn test_instance_ttl_extended_on_batch_pay_bills() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        let id1 = client.create_bill(
            &owner,
            &String::from_str(&env, "Gas"),
            &300,
            &600_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        let id2 = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &200,
            &600_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        // Advance ledger past threshold so extend_ttl has observable effect
        // After create_bill at seq 100: live_until = 518,500
        // At seq 510,000: TTL = 8,500 < 17,280
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

        let ids = soroban_sdk::vec![&env, id1, id2];
        let paid_count = client.batch_pay_bills(&owner, &ids);
        assert_eq!(paid_count, 2);

        // TTL should be fully refreshed
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= 518,400 after batch_pay_bills",
            ttl
        );
    }

    #[test]
    fn test_get_overdue_bills_owner_scoped() {
        let env = Env::default();
        set_time(&env, 2_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let alice = <soroban_sdk::Address as AddressTrait>::generate(&env);
        let bob = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();

        // Alice has 2 overdue bills
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Overdue1"),
            &100,
            &1_000_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Overdue2"),
            &200,
            &1_500_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        // Bob has 1 overdue bill
        client.create_bill(
            &bob,
            &String::from_str(&env, "Bob Overdue"),
            &300,
            &1_000_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        // Alice has 1 future bill (not overdue)
        client.create_bill(
            &alice,
            &String::from_str(&env, "Alice Future"),
            &400,
            &3_000_000,
            &false,
            &0,
                    &String::from_str(&env, "XLM"),
        );

        let alice_overdue = client.get_overdue_bills(&alice);
        let bob_overdue = client.get_overdue_bills(&bob);

        // Alice sees only her 2 overdue bills, not Bob's
        assert_eq!(alice_overdue.len(), 2);
        for bill in alice_overdue.iter() {
            assert_eq!(bill.owner, alice);
        }

        // Bob sees only his 1 overdue bill, not Alice's
        assert_eq!(bob_overdue.len(), 1);
        assert_eq!(bob_overdue.get(0).unwrap().owner, bob);
    }

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
            &true,  // recurring
            &1,     // frequency_days = 1
                    &String::from_str(&env, "XLM"),
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
            &true,  // recurring
            &30,    // frequency_days = 30
                    &String::from_str(&env, "XLM"),
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
        assert_eq!(next_bill.frequency_days, 30, "Frequency should be preserved");
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
            &true,   // recurring
            &365,    // frequency_days = 365
                    &String::from_str(&env, "XLM"),
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
        assert_eq!(next_bill.frequency_days, 365, "Frequency should be preserved");
    }

    #[test]
    fn test_recurring_date_math_paid_at_does_not_affect_next_due() {
        // Test: paid_at timestamp does NOT affect next bill's due_date calculation
        // Bill 1: due_date=1000000, paid_at=1000500 (paid 500 seconds late)
        // Bill 2: due_date should be 1000000 + (30*86400), NOT 1000500 + (30*86400)
        let env = Env::default();
        set_time(&env, 1_000_500); // Set current time to 500 seconds after due date
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Late Payment Test"),
            &300,
            &base_due_date,
            &true,  // recurring
            &30,    // frequency_days = 30
                    &String::from_str(&env, "XLM"),
        );

        // Pay the bill (at time 1_000_500, which is 500 seconds after due_date)
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify original bill has paid_at set
        let paid_bill = client.get_bill(&bill_id).unwrap();
        assert!(paid_bill.paid, "Bill should be marked as paid");
        assert_eq!(
            paid_bill.paid_at,
            Some(1_000_500),
            "paid_at should be set to current time"
        );

        // Verify next bill's due_date is based on original due_date, NOT paid_at
        let next_bill = client.get_bill(&2).unwrap();
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be based on original due_date, not paid_at"
        );
        assert!(!next_bill.paid, "Next bill should be unpaid");
    }

    #[test]
    fn test_recurring_date_math_multiple_pay_cycles_2nd_bill() {
        // Test: Multiple pay cycles - verify 2nd bill's due date advances correctly
        // Bill 1: due_date=1000000, frequency=30
        // Bill 2: due_date=1000000 + (30*86400)
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Multi-Cycle Bill"),
            &250,
            &base_due_date,
            &true,  // recurring
            &30,    // frequency_days = 30
                    &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        env.mock_all_auths();
        client.pay_bill(&owner, &bill_id);

        // Verify second bill
        let bill2 = client.get_bill(&2).unwrap();
        let expected_bill2_due = base_due_date + (30u64 * 86400);
        assert_eq!(bill2.due_date, expected_bill2_due);
        assert!(!bill2.paid);

        // Pay second bill
        env.mock_all_auths();
        client.pay_bill(&owner, &2);

        // Verify second bill is now paid
        let bill2_paid = client.get_bill(&2).unwrap();
        assert!(bill2_paid.paid);

        // Verify third bill was created with correct due_date
        let bill3 = client.get_bill(&3).unwrap();
        let expected_bill3_due = expected_bill2_due + (30u64 * 86400);
        assert_eq!(
            bill3.due_date, expected_bill3_due,
            "Bill 3 due_date should be Bill 2 due_date + (30*86400)"
        );
        assert!(!bill3.paid);
    }

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
            &true,  // recurring
            &30,    // frequency_days = 30
                    &String::from_str(&env, "XLM"),
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
        set_time(&env, 500_000); // Set time BEFORE due date
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
            &true,  // recurring
            &30,    // frequency_days = 30
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
                    &String::from_str(&env, "XLM"),
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
        set_time(&env, due_date);

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
        set_time(&env, due_date);

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
        );

        // Not yet overdue at due_date
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(page.count, 0);

        // Advance one second past due_date
        set_time(&env, due_date + 1);
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(
            page.count, 1,
            "Bill must appear overdue exactly one second past due_date"
        );
    }

    /// Mix of past-due, exactly-due, and future bills: only past-due appears.
    #[test]
    fn test_time_drift_overdue_boundary_mixed_bills() {
        let current_time = 2_000_000u64;
        let env = Env::default();
        set_time(&env, current_time);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

        env.mock_all_auths();
        // Past-due (due_date < current_time)
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue"),
            &100,
            &(current_time - 1),
            &false,
            &0,
        );
        env.mock_all_auths();
        // Exactly-due (due_date == current_time) – NOT overdue
        client.create_bill(
            &owner,
            &String::from_str(&env, "DueNow"),
            &200,
            &current_time,
            &false,
            &0,
        );
        env.mock_all_auths();
        // Future (due_date > current_time) – NOT overdue
        client.create_bill(
            &owner,
            &String::from_str(&env, "Future"),
            &300,
            &(current_time + 1),
            &false,
            &0,
        );

        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(
            page.count, 1,
            "Only the bill with due_date < current_time must appear overdue"
        );
        assert_eq!(
            page.items.get(0).unwrap().amount,
            100,
            "Overdue bill must be the one with due_date < current_time"
        );
    }

    /// Full-day boundary: bill created at due_date, queried one day later, is overdue.
    #[test]
    fn test_time_drift_overdue_full_day_boundary() {
        let day = 86400u64;
        let due_date = 1_000_000u64;
        let env = Env::default();
        set_time(&env, due_date);

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
        );

        // Still not overdue at due_date
        let page = client.get_overdue_bills(&0, &100);
        assert_eq!(page.count, 0);

        // One full day later – must be overdue
        set_time(&env, due_date + day);
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
    );
    let id2 = client.create_bill(
        &owner,
        &String::from_str(&env, "Water"),
        &600,
        &1_000_000,
        &false,
        &0,
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
    );
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill B"),
        &200,
        &1_000_000,
        &false,
        &0,
    );
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill C"),
        &300,
        &1_000_000,
        &false,
        &0,
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
    );
    let id_b = client.create_bill(
        &owner,
        &String::from_str(&env, "Bill B"),
        &200,
        &1_000_000,
        &false,
        &0,
    );
    client.create_bill(
        &owner,
        &String::from_str(&env, "Bill C"),
        &300,
        &1_000_000,
        &false,
        &0,
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
    );
    let id2 = client.create_bill(
        &owner,
        &String::from_str(&env, "Bill 2"),
        &200,
        &1_000_000,
        &false,
        &0,
    );
    let id3 = client.create_bill(
        &owner,
        &String::from_str(&env, "Bill 3"),
        &300,
        &1_000_000,
        &false,
        &0,
    );

    assert_eq!(client.get_total_unpaid(&owner), 600);

    client.pay_bill(&owner, &id1);
    assert_eq!(client.get_total_unpaid(&owner), 500, "after paying 100-bill: 500 remaining");

    client.pay_bill(&owner, &id2);
    assert_eq!(client.get_total_unpaid(&owner), 300, "after paying 200-bill: 300 remaining");

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
    );
    client.create_bill(
        &owner_a,
        &String::from_str(&env, "A - Water"),
        &200,
        &1_000_000,
        &false,
        &0,
    );

    // owner_b: one bill of 9999
    client.create_bill(
        &owner_b,
        &String::from_str(&env, "B - Internet"),
        &9999,
        &1_000_000,
        &false,
        &0,
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
    );
    let id_b = client.create_bill(
        &owner_b,
        &String::from_str(&env, "B - Gas"),
        &1234,
        &1_000_000,
        &false,
        &0,
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
    );
    let id_cancel = client.create_bill(
        &owner,
        &String::from_str(&env, "Cancel Me"),
        &9000,
        &1_000_000,
        &false,
        &0,
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
    );

    let total = client.get_total_unpaid(&owner);
    assert_eq!(total, 1, "single bill of amount 1 must yield total_unpaid == 1");
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
    );
    client.create_bill(
        &owner,
        &String::from_str(&env, "Big Bill 2"),
        &big,
        &1_000_000,
        &false,
        &0,
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
}
