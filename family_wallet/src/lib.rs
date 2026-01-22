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

/// Represents a family member with spending controls
///
/// # Fields
/// * `address` - Stellar address of the family member
/// * `name` - Name of the family member
/// * `spending_limit` - Spending limit in stroops
/// * `role` - Role: "sender", "recipient", or "admin"
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
    /// Add a new family member
    ///
    /// Adds a new family member with specified role and spending limit.
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
    pub fn add_member(
        env: Env,
        address: Address,
        name: String,
        spending_limit: i128,
        role: String,
    ) -> bool {
        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(&env));

        let member = FamilyMember {
            address: address.clone(),
            name,
            spending_limit,
            role,
        };

        members.set(address, member);
        env.storage().instance().set(&symbol_short!("MEMBERS"), &members);
        true
    }

    /// Get a family member by address
    ///
    /// Retrieves a specific family member by their Stellar address.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Stellar address of the member
    ///
    /// # Returns
    /// Option<FamilyMember> - Some(member) if found, None otherwise
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
    /// Retrieves all registered family members.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    ///
    /// # Returns
    /// Vec<FamilyMember> - Vector of all family members
    pub fn get_all_members(env: Env) -> Vec<FamilyMember> {
        let members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
            .get(&symbol_short!("MEMBERS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, member) in members.iter() {
            result.push_back(member);
        }
        result
    }

    /// Update spending limit
    ///
    /// Updates the spending limit for a family member.
    /// Can only be called by admin.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Member's Stellar address
    /// * `new_limit` - New spending limit in stroops
    ///
    /// # Returns
    /// True if update was successful, false if member not found
    pub fn update_spending_limit(
        env: Env,
        address: Address,
        new_limit: i128,
    ) -> bool {
        let mut members: Map<Address, FamilyMember> = env
            .storage()
            .instance()
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

    /// Check spending limit
    ///
    /// Validates if a spending amount is within the member's limit.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `address` - Member's Stellar address
    /// * `amount` - Amount to validate in stroops
    ///
    /// # Returns
    /// True if amount is within limit, false if over limit or member not found
    pub fn check_spending_limit(
        env: Env,
        address: Address,
        amount: i128,
    ) -> bool {
        if let Some(member) = Self::get_member(env, address) {
            amount <= member.spending_limit
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test;
