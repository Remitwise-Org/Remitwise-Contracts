use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, Address, Env};
// NOTE: initialize_split requires a USDC/token contract address in addition to percentages

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
    let mock_token = Address::generate(&env);
    println!("Initializing split configuration for owner: {:?}", owner);
    client.initialize_split(&owner, &0u64, &mock_token, &50u32, &30u32, &15u32, &5u32);

    // 5. [Read] Verify the configuration
    let config = client.get_config().unwrap();
    println!("Configuration verified:");
    println!("  Spending: {}%", config.spending_percent);
    println!("  Savings: {}%", config.savings_percent);
    println!("  Bills: {}%", config.bills_percent);
    println!("  Insurance: {}%", config.insurance_percent);

    // 6. [Write] Simulate a remittance distribution
    let total_amount = 1000i128;
    println!(
        "\nCalculating allocation for total amount: {}",
        total_amount
    );
    let allocations = client.calculate_split(&total_amount);

    println!("Allocations:");
    println!("  Spending: {}", allocations.get(0).unwrap());
    println!("  Savings: {}", allocations.get(1).unwrap());
    println!("  Bills: {}", allocations.get(2).unwrap());
    println!("  Insurance: {}", allocations.get(3).unwrap());

    println!("\nExample completed successfully!");
}
