#![cfg(test)]

use bill_payments::{BillPayments, BillPaymentsClient, BillPaymentsError};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

#[derive(Clone, Debug)]
enum WritableEntrypoint {
    CreateBillSchedule,
    ModifyBillSchedule,
    CancelBillSchedule,
    CreateBill,
    PayBill,
    CancelBill,
    ArchivePaidBills,
    RestoreBill,
    BulkCleanupBills,
    BatchPayBills,
    AddTagsToBill,
    RemoveTagsFromBill,
    SetExternalRef,
}

fn any_writable_entrypoint() -> impl Strategy<Value = WritableEntrypoint> {
    prop_oneof![
        Just(WritableEntrypoint::CreateBillSchedule),
        Just(WritableEntrypoint::ModifyBillSchedule),
        Just(WritableEntrypoint::CancelBillSchedule),
        Just(WritableEntrypoint::CreateBill),
        Just(WritableEntrypoint::PayBill),
        Just(WritableEntrypoint::CancelBill),
        Just(WritableEntrypoint::ArchivePaidBills),
        Just(WritableEntrypoint::RestoreBill),
        Just(WritableEntrypoint::BulkCleanupBills),
        Just(WritableEntrypoint::BatchPayBills),
        Just(WritableEntrypoint::AddTagsToBill),
        Just(WritableEntrypoint::RemoveTagsFromBill),
        Just(WritableEntrypoint::SetExternalRef),
    ]
}

proptest! {
    #[test]
    fn test_emergency_pause_all_rejects_every_entrypoint(entrypoint in any_writable_entrypoint()) {
        let env = Env::default();
        env.budget().reset_unlimited();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        env.mock_all_auths();

        // Setup pause admin and trigger emergency pause
        client.set_pause_admin(&admin, &admin);
        client.emergency_pause_all(&admin);

        let dummy_string = String::from_str(&env, "dummy");
        let dummy_vec_u32 = Vec::new(&env);
        let dummy_vec_string = Vec::new(&env);

        let result = match entrypoint {
            WritableEntrypoint::CreateBillSchedule => {
                client.try_create_bill_schedule(
                    &caller,
                    &dummy_string,
                    &100,
                    &dummy_string,
                    &2000000000,
                    &1,
                ).map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::ModifyBillSchedule => {
                client.try_modify_bill_schedule(&caller, &1, &100, &2000000000, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::CancelBillSchedule => {
                client.try_cancel_bill_schedule(&caller, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::CreateBill => {
                client.try_create_bill(
                    &caller,
                    &dummy_string,
                    &100,
                    &2000000000,
                    &false,
                    &0,
                    &None,
                    &dummy_string,
                    &None,
                ).map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::PayBill => {
                client.try_pay_bill(&caller, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::CancelBill => {
                client.try_cancel_bill(&caller, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::ArchivePaidBills => {
                client.try_archive_paid_bills(&caller, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::RestoreBill => {
                client.try_restore_bill(&caller, &1)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::BulkCleanupBills => {
                client.try_bulk_cleanup_bills(&caller, &10)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::BatchPayBills => {
                client.try_batch_pay_bills(&caller, &dummy_vec_u32)
                    .map(|_| ()).map_err(|e| e.unwrap())
            }
            WritableEntrypoint::AddTagsToBill => {
                client.try_add_tags_to_bill(&caller, &1, &dummy_vec_string)
                    .map(|_| ())
                    .map_err(|e| BillPaymentsError::try_from(e.unwrap()).unwrap())
            }
            WritableEntrypoint::RemoveTagsFromBill => {
                client.try_remove_tags_from_bill(&caller, &1, &dummy_vec_string)
                    .map(|_| ())
                    .map_err(|e| BillPaymentsError::try_from(e.unwrap()).unwrap())
            }
            WritableEntrypoint::SetExternalRef => {
                client.try_set_external_ref(&caller, &1, &None)
                    .map(|_| ())
                    .map_err(|e| e.unwrap())
            }
        };

        assert_eq!(
            result,
            Err(BillPaymentsError::ContractPaused),
            "Expected entrypoint to be rejected with ContractPaused"
        );
    }
}
#[cfg(test)]
mod admin_grant_ttl_tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    /// Test that demonstrates and verifies the fix for admin grant TTL bypass vulnerability.
    /// 
    /// **Threat being mitigated**: T-ADMIN-01: Admin Grant TTL Bypass
    /// 
    /// **Attack scenario without the fix**:
    /// 1. Admin sets up pause admin with 30-day TTL
    /// 2. Time advances beyond the 30-day TTL (admin grant expires)  
    /// 3. Attacker calls `set_pause_admin` which lacks TTL validation
    /// 4. Attacker successfully changes admin despite expired grant
    /// 5. Attacker now controls pause functionality indefinitely
    /// 
    /// **Defense applied**:
    /// Added `require_admin_grant_valid` check to `set_pause_admin` when an admin
    /// already exists, preventing bypass of the 30-day expiration mechanism.
    #[test]
    fn test_set_pause_admin_respects_ttl_expiration() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);

        let initial_admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let new_admin = Address::generate(&env);

        // Set initial ledger time 
        env.ledger().with_mut(|li| {
            li.timestamp = 1_000_000;
        });

        env.mock_all_auths();

        // Step 1: Initial admin sets themselves as pause admin
        client.set_pause_admin(&initial_admin, &initial_admin);

        // Step 2: Advance time beyond ADMIN_GRANT_TTL (30 days = 2,592,000 seconds)  
        let ttl_seconds = 30 * 24 * 60 * 60; // 2,592,000 seconds
        env.ledger().with_mut(|li| {
            li.timestamp = 1_000_000 + ttl_seconds + 1; // 1 second past expiration
        });

        // Step 3: Verify that other admin functions correctly reject expired grants
        // This proves the TTL mechanism works for other functions
        let pause_result = client.try_pause(&initial_admin);
        assert_eq!(pause_result, Err(Ok(BillPaymentsError::AdminGrantExpired)));

        // Step 4: Attempt to exploit the vulnerability
        // Before the fix: this would succeed, bypassing TTL validation
        // After the fix: this should fail with AdminGrantExpired
        let exploit_result = client.try_set_pause_admin(&initial_admin, &new_admin);
        
        // The fix ensures this attack is blocked
        assert_eq!(exploit_result, Err(Ok(BillPaymentsError::AdminGrantExpired)));

        // Step 5: Verify the attacker cannot gain control
        // Even if somehow the previous call succeeded, the attacker shouldn't be able to pause
        let attacker_pause_result = client.try_pause(&new_admin);
        assert!(attacker_pause_result.is_err()); // Should fail regardless
        
        // Step 6: Verify legitimate admin can still regain control through proper channels  
        // Reset to fresh time and set up admin properly
        env.ledger().with_mut(|li| {
            li.timestamp = 2_000_000; // Fresh timestamp
        });
        
        // Initial admin can still set themselves (self-assignment allowed)
        let self_reassign_result = client.try_set_pause_admin(&initial_admin, &initial_admin);
        assert!(self_reassign_result.is_ok());
        
        // Now pause should work with fresh grant
        let pause_after_refresh = client.try_pause(&initial_admin);
        assert!(pause_after_refresh.is_ok());
    }

    /// Additional test: Verify first-time admin setup is not affected by the fix
    #[test]
    fn test_initial_admin_setup_unaffected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        
        env.mock_all_auths();

        // First-time setup should work (no existing admin means no TTL check)
        let result = client.try_set_pause_admin(&admin, &admin);
        assert!(result.is_ok());
        
        // Admin should be able to pause immediately after setup
        let pause_result = client.try_pause(&admin);
        assert!(pause_result.is_ok());
    }
}