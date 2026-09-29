#![cfg(test)]

use bill_payments::{
    BillPayments, BillPaymentsClient, BillPaymentsError, DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS,
};
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

        // Configure the trusted orchestrator so the guarded pay_bill branch
        // reaches the paused check (the epoch guard runs first).
        let orch = Address::generate(&env);
        client.init_admin(&admin, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_trusted_orchestrator(&admin, &orch);

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
                client.try_pay_bill(&orch, &0, &caller, &1)
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
        assert_eq!(
            exploit_result,
            Err(Ok(BillPaymentsError::AdminGrantExpired))
        );

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

#[cfg(test)]
mod unpause_failure_boundary_tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger, LedgerInfo};

    /// Deterministic helper: register a fresh contract and return
    /// (env, client, admin, pause_admin).
    fn setup() -> (Env, BillPaymentsClient<'static>, Address, Address) {
        let env = Env::default();
        env.budget().reset_unlimited();
        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pause_admin = Address::generate(&env);
        env.mock_all_auths();
        client.init_admin(&admin, &DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        client.set_pause_admin(&admin, &pause_admin);
        (env, client, admin, pause_admin)
    }

    /// Success path: pause then unpause restores normal operation.
    #[test]
    fn test_unpause_success_restores_operation() {
        let (env, client, _admin, pause_admin) = setup();

        client.pause(&pause_admin);
        // While paused, a writable entrypoint must be rejected.
        let caller = Address::generate(&env);
        let dummy = String::from_str(&env, "dummy");
        let paused_result = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_eq!(paused_result, Err(BillPaymentsError::ContractPaused));

        // Unpause must succeed and restore normal operation.
        let unpause_result = client.try_unpause(&pause_admin);
        assert!(unpause_result.is_ok());

        // After unpause, the same entrypoint must no longer be rejected
        // with ContractPaused (it may fail for other reasons, but not pause).
        let resumed_result = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_ne!(resumed_result, Err(BillPaymentsError::ContractPaused));
    }

    /// Rejection: unpause by a non-pause-admin must be rejected.
    #[test]
    fn test_unpause_rejects_unauthorized_caller() {
        let (env, client, _admin, pause_admin) = setup();
        let attacker = Address::generate(&env);

        client.pause(&pause_admin);

        let result = client.try_unpause(&attacker);
        assert!(result.is_err(), "unauthorized unpause must be rejected");

        // Contract must remain paused after the rejected attempt.
        let caller = Address::generate(&env);
        let dummy = String::from_str(&env, "dummy");
        let still_paused = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_eq!(still_paused, Err(BillPaymentsError::ContractPaused));
    }

    /// Boundary: unpause when the contract is not paused.
    /// Must be deterministic and must not corrupt state.
    #[test]
    fn test_unpause_when_not_paused_is_deterministic() {
        let (_env, client, _admin, pause_admin) = setup();

        // Contract is not paused. Unpause behavior must be deterministic.
        let first = client.try_unpause(&pause_admin);
        let second = client.try_unpause(&pause_admin);

        // Both calls must return the same result (idempotent / deterministic).
        assert_eq!(first, second);

        // Regardless of outcome, the contract must still be operable
        // (i.e. not left in a paused state by a failed unpause).
        let caller = Address::generate(&_env);
        let dummy = String::from_str(&_env, "dummy");
        let result = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_ne!(result, Err(BillPaymentsError::ContractPaused));
    }

    /// Boundary: unpause is idempotent when already unpaused.
    #[test]
    fn test_unpause_idempotent_after_success() {
        let (_env, client, _admin, pause_admin) = setup();

        client.pause(&pause_admin);
        assert!(client.try_unpause(&pause_admin).is_ok());

        // Second unpause must be deterministic and not corrupt state.
        let second = client.try_unpause(&pause_admin);
        let third = client.try_unpause(&pause_admin);
        assert_eq!(second, third);
    }

    /// Regression: pause -> unpause -> pause -> unpause cycle must be stable.
    #[test]
    fn test_unpause_pause_cycle_stable() {
        let (env, client, _admin, pause_admin) = setup();
        let caller = Address::generate(&env);
        let dummy = String::from_str(&env, "dummy");

        for _ in 0..3 {
            client.pause(&pause_admin);
            let paused = client
                .try_create_bill(
                    &caller,
                    &dummy,
                    &100,
                    &2000000000,
                    &false,
                    &0,
                    &None,
                    &dummy,
                    &None,
                )
                .map(|_| ())
                .map_err(|e| e.unwrap());
            assert_eq!(paused, Err(BillPaymentsError::ContractPaused));

            assert!(client.try_unpause(&pause_admin).is_ok());

            let resumed = client
                .try_create_bill(
                    &caller,
                    &dummy,
                    &100,
                    &2000000000,
                    &false,
                    &0,
                    &None,
                    &dummy,
                    &None,
                )
                .map(|_| ())
                .map_err(|e| e.unwrap());
            assert_ne!(resumed, Err(BillPaymentsError::ContractPaused));
        }
    }

    /// Boundary: unpause after pause-admin grant TTL has expired must be rejected.
    #[test]
    fn test_unpause_rejected_after_admin_grant_expires() {
        let (env, client, _admin, pause_admin) = setup();

        client.pause(&pause_admin);

        // Advance beyond ADMIN_GRANT_TTL (30 days).
        let ttl_seconds: u64 = 30 * 24 * 60 * 60;
        env.ledger().with_mut(|li| {
            li.timestamp = li.timestamp.saturating_add(ttl_seconds + 1);
        });

        let result = client.try_unpause(&pause_admin);
        assert_eq!(result, Err(Ok(BillPaymentsError::AdminGrantExpired)));

        // Contract must remain paused after the rejected attempt.
        let caller = Address::generate(&env);
        let dummy = String::from_str(&env, "dummy");
        let still_paused = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_eq!(still_paused, Err(BillPaymentsError::ContractPaused));
    }

    /// Failure recovery: after a rejected unpause, a valid unpause still works.
    #[test]
    fn test_unpause_recovers_after_rejected_attempt() {
        let (env, client, _admin, pause_admin) = setup();
        let attacker = Address::generate(&env);

        client.pause(&pause_admin);

        // Rejected attempt must not corrupt state.
        assert!(client.try_unpause(&attacker).is_err());

        // Valid unpause must still succeed.
        assert!(client.try_unpause(&pause_admin).is_ok());

        // Contract must be operable again.
        let caller = Address::generate(&env);
        let dummy = String::from_str(&env, "dummy");
        let resumed = client
            .try_create_bill(
                &caller,
                &dummy,
                &100,
                &2000000000,
                &false,
                &0,
                &None,
                &dummy,
                &None,
            )
            .map(|_| ())
            .map_err(|e| e.unwrap());
        assert_ne!(resumed, Err(BillPaymentsError::ContractPaused));
    }

    /// Regression: unpause must not be reachable via the emergency-pause
    /// admin path when the caller is not the configured pause admin.
    #[test]
    fn test_unpause_requires_pause_admin_not_general_admin() {
        let (env, client, admin, pause_admin) = setup();
        let other = Address::generate(&env);

        client.pause(&pause_admin);

        // The general admin (init_admin) is not the pause admin here.
        let admin_attempt = client.try_unpause(&admin);
        let other_attempt = client.try_unpause(&other);

        // Both must be rejected; only the pause admin can unpause.
        assert!(admin_attempt.is_err());
        assert!(other_attempt.is_err());

        // Pause admin can still unpause.
        assert!(client.try_unpause(&pause_admin).is_ok());
    }
}
