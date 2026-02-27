use soroban_sdk::{Env, Address, String, testutils::Address as _};
use bill_payments::{BillPayments, BillPaymentsClient};

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the BillPayments contract
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    // 3. Generate a mock owner address
    let owner = Address::generate(&env);

    println!("--- Remitwise: Bill Payments Example ---");

    // 4. [Write] Create a new bill
    let bill_name = String::from_str(&env, "Electricity Bill");
    let amount = 1500i128;
    let due_date = env.ledger().timestamp() + 604800; // 1 week from now
    let currency = String::from_str(&env, "USD");

    println!("Creating bill: '{}' for {} {}", bill_name, amount, currency);
    let bill_id = client.create_bill(&owner, &bill_name, &amount, &due_date, &false, &0, &currency).unwrap();
    println!("Bill created successfully with ID: {}", bill_id);

    // 5. [Read] List unpaid bills
    let bill_page = client.get_unpaid_bills(&owner, &0, &5);
    println!("\nUnpaid Bills for {:?}:", owner);
    for bill in bill_page.items.iter() {
        println!("  ID: {}, Name: {}, Amount: {} {}", bill.id, bill.name, bill.amount, bill.currency);
    }

    // 6. [Write] Pay the bill
    println!("\nPaying bill with ID: {}...", bill_id);
    client.pay_bill(&owner, &bill_id).unwrap();
    println!("Bill paid successfully!");

    // 7. [Read] Verify bill is no longer in unpaid list
    let updated_page = client.get_unpaid_bills(&owner, &0, &5);
    println!("Number of unpaid bills remaining: {}", updated_page.count);

    println!("\nExample completed successfully!");
}
