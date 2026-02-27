use soroban_sdk::{Env, Address, testutils::Address as _};
use orchestrator::{Orchestrator, OrchestratorClient};

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the Orchestrator contract
    let contract_id = env.register_contract(None, Orchestrator);
    let client = OrchestratorClient::new(&env, &contract_id);

    // 3. Generate mock addresses for all participants and contracts
    let caller = Address::generate(&env);
    
    // Contract addresses
    let family_wallet_addr = Address::generate(&env);
    let remittance_split_addr = Address::generate(&env);
    let savings_addr = Address::generate(&env);
    let bills_addr = Address::generate(&env);
    let insurance_addr = Address::generate(&env);

    // Resource IDs
    let goal_id = 1u32;
    let bill_id = 1u32;
    let policy_id = 1u32;

    println!("--- Remitwise: Orchestrator Example ---");

    // 4. [Write] Execute a complete remittance flow
    // This coordinates splitting the amount and paying into downstream contracts
    let total_amount = 5000i128;
    println!("Executing complete remittance flow for amount: {}", total_amount);
    println!("Orchestrating across:");
    println!("  - Savings Goal ID: {}", goal_id);
    println!("  - Bill ID: {}", bill_id);
    println!("  - Insurance Policy ID: {}", policy_id);

    // In this dry-run example, we show the call signature.
    // In a full test environment, you would first set up the state in the dependent contracts.
    
    /*
    client.execute_remittance_flow(
        &caller,
        &total_amount,
        &family_wallet_addr,
        &remittance_split_addr,
        &savings_addr,
        &bills_addr,
        &insurance_addr,
        &goal_id,
        &bill_id,
        &policy_id
    ).unwrap();
    */

    println!("\nOrchestrator is designed to handle complex cross-contract workflows atomically.");
    println!("Example setup completed successfully!");
}
