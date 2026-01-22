#![no_std]
//! # Bill Payments Contract
//!
//! This contract manages bill payments including creation, tracking, and payment of bills.
//! It supports both one-time and recurring bills with automatic renewal functionality.
//!
//! ## Features
//! - Create bills with customizable amounts and due dates
//! - Mark bills as paid
//! - Automatic recurring bill creation
//! - Query unpaid bills and total amounts

use soroban_sdk::{
    contract, contractimpl, symbol_short, vec, Env, Map, Symbol, Vec, String,
};

/// Represents a bill with its associated metadata
///
/// # Fields
/// * `id` - Unique identifier for the bill
/// * `name` - Bill description (e.g., "Electricity")
/// * `amount` - Payment amount in stroops
/// * `due_date` - Due date as Unix timestamp
/// * `recurring` - Whether bill repeats
/// * `frequency_days` - Repeat interval in days (30 for monthly)
/// * `paid` - Payment status
pub struct Bill {
    pub id: u32,
    pub name: String,
    pub amount: i128,
    pub due_date: u64, // Unix timestamp
    pub recurring: bool,
    pub frequency_days: u32, // For recurring bills (e.g., 30 for monthly)
    pub paid: bool,
}

#[contract]
pub struct BillPayments;

#[contractimpl]
impl BillPayments {
    /// Create a new bill
    ///
    /// Creates a new bill entry in the contract storage and returns its ID.
    /// For recurring bills, a next bill will be automatically created when paid.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay (in stroops)
    /// * `due_date` - Due date as Unix timestamp (seconds since epoch)
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills (e.g., 30 for monthly)
    ///
    /// # Returns
    /// The ID of the created bill as u32
    ///
    /// # Example
    /// ```ignore
    /// let bill_id = BillPayments::create_bill(
    ///     env,
    ///     String::from_small_str("Electricity"),
    ///     100_000_000, // 10 USDC (assuming 8 decimals)
    ///     1704067200,  // 2024-01-01
    ///     true,
    ///     30
    /// );
    /// ```
    pub fn create_bill(
        env: Env,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
    ) -> u32 {
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        let bill = Bill {
            id: next_id,
            name: name.clone(),
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
        };

        bills.set(next_id, bill);
        env.storage().instance().set(&symbol_short!("BILLS"), &bills);
        env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);

        next_id
    }

    /// Mark a bill as paid
    ///
    /// Marks an existing bill as paid. If the bill is recurring, automatically
    /// creates a new bill instance for the next payment cycle.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `bill_id` - ID of the bill to mark as paid
    ///
    /// # Returns
    /// True if payment was successful, false if:
    /// - Bill not found
    /// - Bill already paid
    ///
    /// # Error Codes
    /// - Implicit error: Bill not found (returns false)
    /// - Implicit error: Bill already paid (returns false)
    pub fn pay_bill(env: Env, bill_id: u32) -> bool {
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        if let Some(mut bill) = bills.get(bill_id) {
            if bill.paid {
                return false; // Already paid
            }

            bill.paid = true;

            // If recurring, create next bill
            if bill.recurring {
                let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
                let next_bill = Bill {
                    id: env
                        .storage()
                        .instance()
                        .get(&symbol_short!("NEXT_ID"))
                        .unwrap_or(0u32)
                        + 1,
                    name: bill.name.clone(),
                    amount: bill.amount,
                    due_date: next_due_date,
                    recurring: true,
                    frequency_days: bill.frequency_days,
                    paid: false,
                };

                let next_id = next_bill.id;
                bills.set(next_id, next_bill);
                env.storage().instance().set(&symbol_short!("NEXT_ID"), &next_id);
            }

            bills.set(bill_id, bill);
            env.storage().instance().set(&symbol_short!("BILLS"), &bills);
            true
        } else {
            false
        }
    }

    /// Get a bill by ID
    ///
    /// Retrieves a specific bill from storage by its ID.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    /// * `bill_id` - ID of the bill to retrieve
    ///
    /// # Returns
    /// Option<Bill> - Some(Bill) if found, None otherwise
    pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        bills.get(bill_id)
    }

    /// Get all unpaid bills
    ///
    /// Retrieves all bills that have not been marked as paid.
    /// This is useful for dashboard displays and payment reminders.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    ///
    /// # Returns
    /// Vec<Bill> - Vector of all unpaid bills
    pub fn get_unpaid_bills(env: Env) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        let max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for i in 1..=max_id {
            if let Some(bill) = bills.get(i) {
                if !bill.paid {
                    result.push_back(bill);
                }
            }
        }
        result
    }

    /// Get total unpaid amount
    ///
    /// Sums up the amounts of all unpaid bills.
    /// Useful for calculating remaining payment obligations.
    ///
    /// # Arguments
    /// * `env` - Soroban environment context
    ///
    /// # Returns
    /// i128 - Total amount of all unpaid bills in stroops
    pub fn get_total_unpaid(env: Env) -> i128 {
        let unpaid = Self::get_unpaid_bills(env);
        let mut total = 0i128;
        for bill in unpaid.iter() {
            total += bill.amount;
        }
        total
    }
}

#[cfg(test)]
mod test;
