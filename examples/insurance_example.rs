use soroban_sdk::{Env, Address, String, testutils::Address as _};
use insurance::{Insurance, InsuranceClient};

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the Insurance contract
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    // 3. Generate a mock owner address
    let owner = Address::generate(&env);

    println!("--- Remitwise: Insurance Example ---");

    // 4. [Write] Create a new insurance policy
    let policy_name = String::from_str(&env, "Health Insurance");
    let coverage_type = String::from_str(&env, "HMO");
    let monthly_premium = 200i128;
    let coverage_amount = 50000i128;

    println!("Creating policy: '{}' with premium: {} and coverage: {}", policy_name, monthly_premium, coverage_amount);
    let policy_id = client.create_policy(&owner, &policy_name, &coverage_type, &monthly_premium, &coverage_amount).unwrap();
    println!("Policy created successfully with ID: {}", policy_id);

    // 5. [Read] List active policies
    let policy_page = client.get_active_policies(&owner, &0, &5);
    println!("\nActive Policies for {:?}:", owner);
    for policy in policy_page.items.iter() {
        println!("  ID: {}, Name: {}, Premium: {}, Coverage: {}", policy.id, policy.name, policy.monthly_premium, policy.coverage_amount);
    }

    // 6. [Write] Pay a premium
    println!("\nPaying premium for policy ID: {}...", policy_id);
    client.pay_premium(&owner, &policy_id).unwrap();
    println!("Premium paid successfully!");

    // 7. [Read] Verify policy status (next payment date updated)
    let policy = client.get_policy(&policy_id).unwrap();
    println!("Next Payment Date (Timestamp): {}", policy.next_payment_date);

    println!("\nExample completed successfully!");
}
