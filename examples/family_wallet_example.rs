use soroban_sdk::{Env, Address, Vec, testutils::Address as _};
use family_wallet::{FamilyWallet, FamilyWalletClient, FamilyRole};

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

    // 4. [Write] Initialize the wallet with an owner and some initial members
    println!("Initializing wallet with owner: {:?}", owner);
    let mut initial_members = Vec::new(&env);
    initial_members.push_back(owner.clone());
    initial_members.push_back(member1.clone());
    
    client.init(&owner, &initial_members);
    println!("Wallet initialized successfully!");

    // 5. [Read] Check roles of members
    let owner_member = client.get_member(&owner).unwrap();
    println!("\nOwner Role: {:?}", owner_member.role);
    
    let m1_member = client.get_member(&member1).unwrap();
    println!("Member 1 Role: {:?}", m1_member.role);

    // 6. [Write] Add a new family member with a specific role and spending limit
    println!("\nAdding new member: {:?}", member2);
    let spending_limit = 1000i128;
    client.add_member(&owner, &member2, &FamilyRole::Member, &spending_limit).unwrap();
    println!("Member added successfully!");

    // 7. [Read] Verify the new member
    let m2_member = client.get_member(&member2).unwrap();
    println!("Member 2 Details:");
    println!("  Role: {:?}", m2_member.role);
    println!("  Spending Limit: {}", m2_member.spending_limit);

    println!("\nExample completed successfully!");
}
