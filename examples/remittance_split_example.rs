use soroban_sdk::{Env, Address, testutils::Address as _};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the RemittanceSplit contract
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);

    // 3. Generate a mock owner address
    let owner = Address::generate(&env);

    println!("--- Remitwise: Remittance Split Example ---");

    // 4. [Write] Initialize the split configuration
    // Percentages: 50% Spending, 30% Savings, 15% Bills, 5% Insurance
    println!("Initializing split configuration for owner: {:?}", owner);
    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // 5. [Read] Verify the configuration
    let config = client.get_config().unwrap();
    println!("Configuration verified:");
    println!("  Spending: {}%", config.spending_percent);
    println!("  Savings: {}%", config.savings_percent);
    println!("  Bills: {}%", config.bills_percent);
    println!("  Insurance: {}%", config.insurance_percent);

    // 6. [Write] Simulate a remittance distribution
    let total_amount = 1000i128;
    println!("\nCalculating allocation for total amount: {}", total_amount);
    let allocations = client.calculate_split(&total_amount);

    println!("Allocations:");
    println!("  Spending: {}", allocations.get(0).unwrap());
    println!("  Savings: {}", allocations.get(1).unwrap());
    println!("  Bills: {}", allocations.get(2).unwrap());
    println!("  Insurance: {}", allocations.get(3).unwrap());

    println!("\nExample completed successfully!");
}
