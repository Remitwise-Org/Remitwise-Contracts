#![cfg(test)]

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

#[test]
fn test_recurring_bill_lifecycle() {
    let e = Env::default();

    // Register the contract
    let contract_id = e.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&e, &contract_id);

    // Setup: Create a User
    let user = Address::generate(&e);

    // Mock authorization so 'require_auth' passes
    e.mock_all_auths();

    let current_time = e.ledger().timestamp();
    let due_date = current_time + 86400; // 1 day later
    let frequency_days = 30;

    // Create recurring bill
    let bill_id = client.create_bill(
        &user,
        &soroban_sdk::String::from_str(&e, "Monthly Rent"),
        &10000,
        &due_date,
        &true, // recurring
        &frequency_days,
        &None,
        &soroban_sdk::String::from_str(&e, "XLM"),
        &None,
    );

    // Verify the bill was created
    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.id, bill_id);
    assert_eq!(bill.recurring, true);
    assert_eq!(bill.frequency_days, frequency_days);
    assert_eq!(bill.schedule_id, None); // No explicit schedule id was provided on creation

    // Pay the bill
    client.pay_bill(&user, &bill_id);

    // Verify the original bill is paid
    let paid_bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(paid_bill.paid, true);
    assert!(paid_bill.paid_at.is_some());

    // Verify next bill was created
    let next_bill_id = bill_id + 1;
    let next_bill = client.get_bill(&next_bill_id).unwrap();
    assert_eq!(next_bill.id, next_bill_id);
    assert_eq!(next_bill.owner, user);
    assert_eq!(
        next_bill.name,
        soroban_sdk::String::from_str(&e, "Monthly Rent")
    );
    assert_eq!(next_bill.amount, 10000);
    assert_eq!(
        next_bill.due_date,
        due_date + (frequency_days as u64 * 86400)
    );
    assert_eq!(next_bill.recurring, true);
    assert_eq!(next_bill.frequency_days, frequency_days);
    assert_eq!(next_bill.paid, false);
    assert_eq!(next_bill.schedule_id, None); // Preserves the original schedule_id value

    // Attempt to pay the same bill again - should fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.pay_bill(&user, &bill_id);
    }));
    assert!(result.is_err(), "Second pay attempt should fail");

    // Verify no additional bills were created
    let next_next_bill_id = next_bill_id + 1;
    let extra_bill = client.get_bill(&next_next_bill_id);
    assert!(
        extra_bill.is_none(),
        "No extra bill should be created on second pay attempt"
    );
}
