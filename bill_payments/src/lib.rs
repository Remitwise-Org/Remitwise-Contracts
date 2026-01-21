#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Env, Map, Vec, String,
};

#[derive(Clone)]
#[contracttype]
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
    /// # Arguments
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay
    /// * `due_date` - Due date as Unix timestamp
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills
    /// 
    /// # Returns
    /// The ID of the created bill
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
    /// # Arguments
    /// * `bill_id` - ID of the bill
    /// 
    /// # Returns
    /// True if payment was successful, false if bill not found or already paid
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
    /// # Arguments
    /// * `bill_id` - ID of the bill
    /// 
    /// # Returns
    /// Bill struct or None if not found
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
    /// # Returns
    /// Vec of unpaid Bill structs
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
    
    /// Get total amount of unpaid bills
    /// 
    /// # Returns
    /// Total amount of all unpaid bills
    pub fn get_total_unpaid(env: Env) -> i128 {
        let unpaid = Self::get_unpaid_bills(env);
        let mut total = 0i128;
        for bill in unpaid.iter() {
            total += bill.amount;
        }
        total
    }
}


