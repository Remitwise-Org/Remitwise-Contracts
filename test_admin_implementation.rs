#!/usr/bin/env rust-script
//! Standalone test for admin role transfer implementation
//! 
//! This tests the core logic we implemented without workspace dependencies

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
struct Address(String);

impl Address {
    fn new(name: &str) -> Self {
        Address(name.to_string())
    }
}

/// Mock contract to test our admin role transfer logic
struct MockContract {
    storage: HashMap<String, Address>,
}

impl MockContract {
    fn new() -> Self {
        Self {
            storage: HashMap::new(),
        }
    }

    fn get_upgrade_admin(&self) -> Option<Address> {
        self.storage.get("UPG_ADM").cloned()
    }

    /// This implements the exact logic we added to the contracts
    fn set_upgrade_admin(&mut self, caller: Address, new_admin: Address) -> Result<(), &'static str> {
        let current_upgrade_admin = self.get_upgrade_admin();
        
        // Authorization logic:
        // 1. If no upgrade admin exists, caller must equal new_admin (bootstrap)
        // 2. If upgrade admin exists, only current upgrade admin can transfer
        match current_upgrade_admin {
            None => {
                // Bootstrap pattern - caller must be setting themselves as admin
                if caller != new_admin {
                    return Err("Unauthorized: bootstrap requires caller == new_admin");
                }
            }
            Some(current_admin) => {
                // Admin transfer - only current admin can transfer
                if current_admin != caller {
                    return Err("Unauthorized: only current upgrade admin can transfer");
                }
            }
        }
        
        self.storage.insert("UPG_ADM".to_string(), new_admin);
        Ok(())
    }
}

fn main() {
    println!("🧪 Testing Admin Role Transfer Implementation");
    println!("{}", "=".repeat(50));

    // Test 1: Bootstrap Admin Setup
    println!("\n1️⃣  Testing Bootstrap Admin Setup");
    let mut contract = MockContract::new();
    let admin = Address::new("admin1");
    
    let result = contract.set_upgrade_admin(admin.clone(), admin.clone());
    assert!(result.is_ok(), "Bootstrap should succeed when caller == new_admin");
    
    let current_admin = contract.get_upgrade_admin();
    assert_eq!(current_admin, Some(admin.clone()));
    println!("   ✅ Bootstrap succeeded: {:?}", current_admin);

    // Test 2: Unauthorized Bootstrap
    println!("\n2️⃣  Testing Unauthorized Bootstrap");
    let mut contract2 = MockContract::new();
    let caller = Address::new("unauthorized");
    let admin = Address::new("admin1");
    
    let result = contract2.set_upgrade_admin(caller, admin);
    assert!(result.is_err(), "Bootstrap should fail when caller != new_admin");
    
    let current_admin = contract2.get_upgrade_admin();
    assert_eq!(current_admin, None);
    println!("   ✅ Unauthorized bootstrap blocked: {:?}", result.unwrap_err());

    // Test 3: Authorized Admin Transfer
    println!("\n3️⃣  Testing Authorized Admin Transfer");
    let mut contract3 = MockContract::new();
    let admin1 = Address::new("admin1");
    let admin2 = Address::new("admin2");
    
    // Setup initial admin
    contract3.set_upgrade_admin(admin1.clone(), admin1.clone()).unwrap();
    
    // Transfer to new admin
    let result = contract3.set_upgrade_admin(admin1.clone(), admin2.clone());
    assert!(result.is_ok(), "Transfer should succeed when current admin transfers");
    
    let current_admin = contract3.get_upgrade_admin();
    assert_eq!(current_admin, Some(admin2.clone()));
    println!("   ✅ Admin transfer succeeded: {:?}", current_admin);

    // Test 4: Unauthorized Admin Transfer
    println!("\n4️⃣  Testing Unauthorized Admin Transfer");
    let mut contract4 = MockContract::new();
    let admin1 = Address::new("admin1");
    let admin2 = Address::new("admin2");
    let unauthorized = Address::new("unauthorized");
    
    // Setup initial admin
    contract4.set_upgrade_admin(admin1.clone(), admin1.clone()).unwrap();
    
    // Attempt unauthorized transfer
    let result = contract4.set_upgrade_admin(unauthorized, admin2);
    assert!(result.is_err(), "Transfer should fail when unauthorized user attempts");
    
    let current_admin = contract4.get_upgrade_admin();
    assert_eq!(current_admin, Some(admin1.clone()));
    println!("   ✅ Unauthorized transfer blocked: {:?}", result.unwrap_err());

    // Test 5: Self-Transfer
    println!("\n5️⃣  Testing Self-Transfer");
    let mut contract5 = MockContract::new();
    let admin = Address::new("admin1");
    
    // Setup initial admin
    contract5.set_upgrade_admin(admin.clone(), admin.clone()).unwrap();
    
    // Self-transfer should succeed
    let result = contract5.set_upgrade_admin(admin.clone(), admin.clone());
    assert!(result.is_ok(), "Self-transfer should succeed");
    
    let current_admin = contract5.get_upgrade_admin();
    assert_eq!(current_admin, Some(admin.clone()));
    println!("   ✅ Self-transfer succeeded: {:?}", current_admin);

    // Test 6: Rapid Successive Transfers
    println!("\n6️⃣  Testing Rapid Successive Transfers");
    let mut contract6 = MockContract::new();
    let admin1 = Address::new("admin1");
    let admin2 = Address::new("admin2");
    let admin3 = Address::new("admin3");
    
    // Setup initial admin
    contract6.set_upgrade_admin(admin1.clone(), admin1.clone()).unwrap();
    
    // Transfer to admin2
    let result = contract6.set_upgrade_admin(admin1, admin2.clone());
    assert!(result.is_ok(), "First transfer should succeed");
    
    // Immediately transfer to admin3
    let result = contract6.set_upgrade_admin(admin2, admin3.clone());
    assert!(result.is_ok(), "Second transfer should succeed");
    
    let current_admin = contract6.get_upgrade_admin();
    assert_eq!(current_admin, Some(admin3.clone()));
    println!("   ✅ Rapid transfers succeeded: {:?}", current_admin);

    println!("\n🎉 All Admin Role Transfer Tests Passed!");
    println!("{}", "=".repeat(50));
    
    println!("\n📋 Test Summary:");
    println!("   ✅ Bootstrap security (caller == new_admin)");
    println!("   ✅ Unauthorized bootstrap prevention");
    println!("   ✅ Authorized admin transfers");
    println!("   ✅ Unauthorized transfer prevention");
    println!("   ✅ Self-transfer capability");
    println!("   ✅ Rapid successive transfers");
    
    println!("\n🔒 Security Properties Validated:");
    println!("   • No unauthorized bootstrap");
    println!("   • Transfer isolation (only current admin can transfer)");
    println!("   • State consistency (failed transfers don't change admin)");
    println!("   • Edge case handling (self-transfer, rapid succession)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_admin_setup() {
        let mut contract = MockContract::new();
        let admin = Address::new("admin1");
        
        let result = contract.set_upgrade_admin(admin.clone(), admin.clone());
        assert!(result.is_ok());
        
        let current_admin = contract.get_upgrade_admin();
        assert_eq!(current_admin, Some(admin));
    }

    #[test]
    fn test_unauthorized_bootstrap() {
        let mut contract = MockContract::new();
        let caller = Address::new("unauthorized");
        let admin = Address::new("admin1");
        
        let result = contract.set_upgrade_admin(caller, admin);
        assert!(result.is_err());
        
        let current_admin = contract.get_upgrade_admin();
        assert_eq!(current_admin, None);
    }

    #[test]
    fn test_authorized_transfer() {
        let mut contract = MockContract::new();
        let admin1 = Address::new("admin1");
        let admin2 = Address::new("admin2");
        
        contract.set_upgrade_admin(admin1.clone(), admin1.clone()).unwrap();
        
        let result = contract.set_upgrade_admin(admin1, admin2.clone());
        assert!(result.is_ok());
        
        let current_admin = contract.get_upgrade_admin();
        assert_eq!(current_admin, Some(admin2));
    }

    #[test]
    fn test_unauthorized_transfer() {
        let mut contract = MockContract::new();
        let admin1 = Address::new("admin1");
        let admin2 = Address::new("admin2");
        let unauthorized = Address::new("unauthorized");
        
        contract.set_upgrade_admin(admin1.clone(), admin1.clone()).unwrap();
        
        let result = contract.set_upgrade_admin(unauthorized, admin2);
        assert!(result.is_err());
        
        let current_admin = contract.get_upgrade_admin();
        assert_eq!(current_admin, Some(admin1));
    }
}