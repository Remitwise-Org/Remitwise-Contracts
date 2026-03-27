use soroban_sdk::{Env, Address, testutils::Address as _};
use family_wallet::{FamilyWallet, FamilyWalletClient};
use remitwise_common::FamilyRole;

fn main() {
    // 1. Setup the Soroban environment
    let env = Env::default();
    env.mock_all_auths();

    // 2. Register the FamilyWallet contract
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    // 3. Generate mock addresses
    let owner = Address::generate(&env);
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);

    println!("--- Remitwise: Family Wallet Example ---");

    // 4. [Write] Initialize the wallet. Do not include `owner` in `initial_members`
    // (the contract rejects that to preserve FamilyRole::Owner).
    println!("Initializing wallet with owner: {:?}", owner);
    let initial_members = soroban_sdk::vec![&env, member1.clone()];

    match client.try_init(&owner, &initial_members) {
        Ok(Ok(true)) => println!("Wallet initialized successfully!"),
        Ok(Ok(_)) => eprintln!("init returned unexpected success value"),
        Ok(Err(e)) => panic!("init failed: {:?}", e),
        Err(e) => panic!("host error on init: {:?}", e),
    }

    // 5. [Read] Check roles of members
    let owner_member = client.get_member(&owner).unwrap();
    println!("\nOwner Role: {:?}", owner_member.role);
    
    let m1_member = client.get_member(&member1).unwrap();
    println!("Member 1 Role: {:?}", m1_member.role);

    // 6. [Write] Add a new family member with a specific role and spending limit
    println!("\nAdding new member: {:?}", member2);
    let spending_limit = 500_0000000; // 500 tokens
    client.add_member(&owner, &member2, &FamilyRole::Member, &spending_limit);
    println!("Member added successfully.");

    // 7. [Read] Verify the new member
    let m2_member = client.get_member(&member2).unwrap();
    println!("Member 2 Details:");
    println!("  Role: {:?}", m2_member.role);
    println!("  Spending Limit: {}", m2_member.spending_limit);

    println!("\nExample completed successfully!");
}
