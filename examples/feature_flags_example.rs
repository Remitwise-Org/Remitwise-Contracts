use soroban_sdk::{testutils::Address as _, Address, Env, String};

// Import the feature flags contract
use feature_flags::{FeatureFlagsContract, FeatureFlagsContractClient};

fn main() {
    println!("=== Feature Flags Example ===\n");

    // Create test environment
    let env = Env::default();
    env.mock_all_auths();

    // Deploy feature flags contract
    let flags_contract_id = env.register_contract(None, FeatureFlagsContract);
    let flags_client = FeatureFlagsContractClient::new(&env, &flags_contract_id);

    // Create admin address
    let admin = Address::generate(&env);

    println!("1. Initializing feature flags contract...");
    flags_client.initialize(&admin);
    println!("   ✓ Initialized with admin: {:?}\n", admin);

    // Set up some feature flags
    println!("2. Setting up feature flags...");
    
    let strict_dates_key = String::from_str(&env, "strict_goal_dates");
    let strict_dates_desc = String::from_str(
        &env,
        "Enforce future dates for savings goals"
    );
    flags_client.set_flag(&admin, &strict_dates_key, &true, &strict_dates_desc);
    println!("   ✓ Set 'strict_goal_dates' = true");

    let enhanced_validation_key = String::from_str(&env, "enhanced_validation");
    let enhanced_validation_desc = String::from_str(
        &env,
        "Enable additional input validation"
    );
    flags_client.set_flag(&admin, &enhanced_validation_key, &false, &enhanced_validation_desc);
    println!("   ✓ Set 'enhanced_validation' = false");

    let batch_ops_key = String::from_str(&env, "batch_operations");
    let batch_ops_desc = String::from_str(
        &env,
        "Enable batch operation endpoints"
    );
    flags_client.set_flag(&admin, &batch_ops_key, &true, &batch_ops_desc);
    println!("   ✓ Set 'batch_operations' = true\n");

    // Query individual flags
    println!("3. Querying individual flags...");
    println!("   strict_goal_dates: {}", flags_client.is_enabled(&strict_dates_key));
    println!("   enhanced_validation: {}", flags_client.is_enabled(&enhanced_validation_key));
    println!("   batch_operations: {}\n", flags_client.is_enabled(&batch_ops_key));

    // Query non-existent flag (should return false)
    let nonexistent_key = String::from_str(&env, "nonexistent_feature");
    println!("4. Querying non-existent flag...");
    println!("   nonexistent_feature: {} (defaults to false)\n", 
        flags_client.is_enabled(&nonexistent_key));

    // Get detailed flag information
    println!("5. Getting detailed flag information...");
    let flag_details = flags_client.get_flag(&strict_dates_key).unwrap();
    println!("   Key: {}", flag_details.key);
    println!("   Enabled: {}", flag_details.enabled);
    println!("   Description: {}", flag_details.description);
    println!("   Updated at: {}", flag_details.updated_at);
    println!("   Updated by: {:?}\n", flag_details.updated_by);

    // Toggle a flag
    println!("6. Toggling 'enhanced_validation' flag...");
    println!("   Before: {}", flags_client.is_enabled(&enhanced_validation_key));
    flags_client.set_flag(&admin, &enhanced_validation_key, &true, &enhanced_validation_desc);
    println!("   After: {}\n", flags_client.is_enabled(&enhanced_validation_key));

    // Get all flags
    println!("7. Getting all flags...");
    let all_flags = flags_client.get_all_flags();
    println!("   Total flags: {}", all_flags.len());
    for key in all_flags.keys() {
        let flag = all_flags.get(key.clone()).unwrap();
        println!("   - {}: {}", flag.key, flag.enabled);
    }
    println!();

    // Remove a flag
    println!("8. Removing 'batch_operations' flag...");
    flags_client.remove_flag(&admin, &batch_ops_key);
    println!("   ✓ Flag removed");
    println!("   Is enabled: {} (returns false after removal)\n", 
        flags_client.is_enabled(&batch_ops_key));

    // Transfer admin
    println!("9. Transferring admin role...");
    let new_admin = Address::generate(&env);
    flags_client.transfer_admin(&admin, &new_admin);
    println!("   ✓ Admin transferred to: {:?}", new_admin);
    println!("   Current admin: {:?}\n", flags_client.get_admin());

    // Demonstrate usage in application logic
    println!("10. Example: Using flags in application logic...");
    simulate_create_goal(&env, &flags_client, 1000, env.ledger().timestamp() + 86400);
    simulate_create_goal(&env, &flags_client, 2000, env.ledger().timestamp() - 86400);

    println!("\n=== Example Complete ===");
}

// Simulated function showing how to use feature flags in contract logic
fn simulate_create_goal(
    env: &Env,
    flags_client: &FeatureFlagsContractClient,
    target_amount: i128,
    target_date: u64,
) {
    let strict_dates_key = String::from_str(env, "strict_goal_dates");
    let strict_dates_enabled = flags_client.is_enabled(&strict_dates_key);

    println!("   Creating goal with target_date: {}", target_date);
    println!("   Current time: {}", env.ledger().timestamp());
    println!("   strict_goal_dates flag: {}", strict_dates_enabled);

    if strict_dates_enabled {
        let current_time = env.ledger().timestamp();
        if target_date <= current_time {
            println!("   ✗ Rejected: Target date must be in the future (flag enabled)");
            return;
        }
    }

    println!("   ✓ Goal creation would proceed");
}
