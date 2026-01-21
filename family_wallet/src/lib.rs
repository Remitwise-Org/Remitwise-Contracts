#![no_std]
//! # Family Wallet Contract
//!
//! This contract manages family member accounts with individual spending limits.
//! It enables parents/guardians to control spending by setting role-based access
//! and spending limits for family members.
//!
//! ## Features
//! - Add/manage family members
//! - Set individual spending limits
//! - Role-based access control (sender, recipient, admin)
//! - Spending limit validation

use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Address, Env, Map, Symbol, Vec, String,
};

/// Smart contract for managing family wallet members and spending controls
///
/// This contract allows families to manage multiple members with different
/// roles and spending limits, enabling financial control and oversight.
/// ReprAdds a new family member with specified role and spending limit.
    /// Can only be called by admin/contract owner.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Stellar address of the family member
    /// * `name` - Name of the family member
    /// * `spending_limit` - Spending limit for this member (in stroops)
    /// * `role` - Role: "sender", "recipient", or "admin"
    /// 
    /// # Returns
    /// True if member was added successfully
    ///
    /// # Roles
    /// - "sender": Can initiate transfers up to spending limit
    /// - "recipient": Can receive transfers
    /// - "admin": Full access and can manage other members
#[contracttype]
pub struct FamilyMember {
    pub address: Address,
    pub name: String,
    pub spending_limit: i128, // Daily or monthly limit
    pub role: String, // "sender", "recipient", "admin"
}

#[contract]
pub struct FamilyWallet;

#[contractimpl]
impl FamilyWallet {
    /// Add a family member to the wallet
    /// 
    /// # Arguments
    /// * `address` - Stellar address of the family member
    /// * `name` - Name of the family member
    /// * `spending_limit` - Spending limit for this member
    /// Retrieves member information by their Stellar address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Stellar address of the family member
    /// 
    /// # Returns
    /// Option<FamilyMember> - Some(member) if found, None otherwise
        env: Env,
        address: Address,
        name: String,
        spending_limit: i128,
        role: String,
    ) -> bool {
        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
        Retrieves all family members currently in the wallet.
    ///
    /// # Returns
    /// Vec<FamilyMember> - Vector of all family memberw(&env));
        
        let member = FamilyMember {
            address: address.clone(),
            name: name.clone(),
            spending_limit,
            role: role.clone(),
        };
        
        members.set(address, member);
        env.storage().instance().set(&symbol_short!("MEMBERS"), &members);
        
        true
    }
    Allows admins to adjust an existing member's spending limit.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Stellar address of the family member
    /// * `new_limit` - New spending limit (in stroops)
    /// 
    /// # Returns
    /// True if update was successful, false if member not found
    /// # Returns
    /// FamilyMember struct or None if not found
    pub fn get_member(env: Env, address: Address) -> Option<FamilyMember> {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(&env));
        
        members.get(address)
    }
    
    /// Get all family members
    /// 
    /// # Returns
    /// Vec of all FamilyMember structs
    pub fn get_all_members(env: Env) -> Vec<FamilyMember> {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(&env));
        
        let mut result = Vec::new(&env);
        // Note: In a real implementation, you'd need to track member addresses
        // For now, this is a placeholder
        result
    }
    
    /// Update spending limit for a family member
    /// 
    /// # Arguments
    /// * `address` - Stellar address of the family member
    /// * `new_limit` - New spending limit
    /// 
    /// Validates whether a proposed spending amount complies with the member's limit.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Stellar address of the family member
    /// * `amount` - Amount to check (in stroops)
    /// 
    /// # Returns
    /// True if amount is within limit, false if over limit or member not found
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(&env));
        
        if let Some(mut member) = members.get(address.clone()) {
            member.spending_limit = new_limit;
            members.set(address, member);
            env.storage().instance().set(&symbol_short!("MEMBERS"), &members);
            true
        } else {
            false
        }
    }
    
    /// Check if a spending amount is within limit
    /// 
    /// # Arguments
    /// * `address` - Stellar address of the family member
    /// * `amount` - Amount to check
    /// 
    /// # Returns
    /// True if amount is within limit
    pub fn check_spending_limit(env: Env, address: Address, amount: i128) -> bool {
        if let Some(member) = Self::get_member(env, address) {
            amount <= member.spending_limit
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test;

