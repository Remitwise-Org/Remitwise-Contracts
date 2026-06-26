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
