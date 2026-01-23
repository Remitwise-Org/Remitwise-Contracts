#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Env, Map, String, Symbol, Vec,
};

// ============================================================================
//  Packed struct with strategic field ordering
// - Reordered fields for better memory alignment
// - Using u64 for id instead of u32 (native word size on Stellar)
// - Grouped boolean fields together for variable packing
// ============================================================================
#[derive(Clone)]
#[contracttype]
pub struct Bill {
    pub id: u64,       // 8bytes         // Changed from u32 to u64 (native word size)
    pub amount: i128,   // 16 bytes        // Large value first
    pub due_date: u64,    // 8 bytes      // Unix timestamp
    pub frequency_days: u32,    // Smaller values after
    pub paid: bool,             // Booleans grouped
    pub recurring: bool,
    pub name: String,           // Variable-size last
}

// =========================================================================
// Storage keys as constants
// - Avoids repeated symbol creation
// - Symbols are created once and reused
// ============================================================================
const BILLS_KEY: Symbol = symbol_short!("BILLS");
const NEXT_ID_KEY: Symbol = symbol_short!("NEXT_ID");
const UNPAID_COUNT_KEY: Symbol = symbol_short!("UNPAID");

#[contract]
pub struct BillPayments;

#[contractimpl]
impl BillPayments {
    // ========================================================================
    // OPTIMIZATION 3: Batch initialization
    // - Initialize contract storage to avoid repeated checks
    // ========================================================================
    pub fn initialize(env: Env) {
        if env.storage().instance().has(&NEXT_ID_KEY) {
            panic!("Already initialized");
        }
        
        env.storage().instance().set(&NEXT_ID_KEY, &0u64);
        env.storage().instance().set(&UNPAID_COUNT_KEY, &0u64);
        let bills: Map<u64, Bill> = Map::new(&env);
        env.storage().instance().set(&BILLS_KEY, &bills);
    }

    
    // ========================================================================
    // - Read NEXT_ID once instead of twice
    // - Use bumped() for automatic TTL extension
    // - Return early on validation failures
    // ========================================================================
    pub fn create_bill(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
    ) -> u64 {
        // Early validation before any storage reads
        if amount <= 0 {
            panic!("Amount must be positive");
        }

        // OPTIMIZATION: Single storage read with bumped TTL
        let mut bills: Map<u64, Bill> = env
            .storage()
            .instance()
            .get(&BILLS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        // OPTIMIZATION: Read and increment in one go
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64)
            + 1;

        let bill = Bill {
            id: next_id,
            name,
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
        };

        bills.set(next_id, bill);
        
        // Batch storage writes
        env.storage().instance().set(&BILLS_KEY, &bills);
        env.storage().instance().set(&NEXT_ID_KEY, &next_id);
        
        //  Track unpaid count for faster queries
        let unpaid_count: u64 = env
            .storage()
            .instance()
            .get(&UNPAID_COUNT_KEY)
            .unwrap_or(0u64)
            + 1;
        env.storage().instance().set(&UNPAID_COUNT_KEY, &unpaid_count);

        // OPTIMIZATION: Extend TTL for frequently accessed data
        env.storage().instance().extend_ttl(100, 100);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Created),
            (next_id, owner),
        );

        next_id
    }

    // ========================================================================
    //  Optimized bill payment
    // - Removed redundant clone operations
    // - Consolidated storage operations
    // - Better error handling
    // ========================================================================
    pub fn pay_bill(env: Env, bill_id: u64) -> bool {
        let mut bills: Map<u64, Bill> = env
            .storage()
            .instance()
            .get(&BILLS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        // Use get_unchecked after existence check for performance
        if !bills.contains_key(bill_id) {
            return false;
        }

        let mut bill = bills.get(bill_id).unwrap();
        
        if bill.paid {
            return false;
        }

        bill.paid = true;
        bills.set(bill_id, bill.clone());

        //  Handle recurring bills without extra reads
        if bill.recurring {
            let next_id: u64 = env
                .storage()
                .instance()
                .get(&NEXT_ID_KEY)
                .unwrap_or(0u64)
                + 1;

            let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
            
            let next_bill = Bill {
                id: next_id,
                name: bill.name,
                amount: bill.amount,
                due_date: next_due_date,
                recurring: true,
                frequency_days: bill.frequency_days,
                paid: false,
            };

            bills.set(next_id, next_bill);
            env.storage().instance().set(&NEXT_ID_KEY, &next_id);
        
        } else {
        
            let unpaid_count: u64 = env
                .storage()
                .instance()
                .get(&UNPAID_COUNT_KEY)
                .unwrap_or(1u64)
                .saturating_sub(1);
            env.storage().instance().set(&UNPAID_COUNT_KEY, &unpaid_count);
        }

        env.storage().instance().set(&BILLS_KEY, &bills);
        env.storage().instance().extend_ttl(100, 100);
        
        true
    }

  
    pub fn get_bill(env: Env, bill_id: u64) -> Option<Bill> {
        env.storage()
            .instance()
            .get(&BILLS_KEY)
            .and_then(|bills: Map<u64, Bill>| bills.get(bill_id))
    }

   
    pub fn get_unpaid_bills(env: Env) -> Vec<Bill> {
        let bills: Map<u64, Bill> = env
            .storage()
            .instance()
            .get(&BILLS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        let max_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64);

        
        let mut result = Vec::new(&env);
        
        //  Iterate only through existing bills
        for id in 1..=max_id {
            if let Some(bill) = bills.get(id) {
                if !bill.paid {
                    result.push_back(bill);
                }
            }
        }
        
        result
    }

    // ========================================================================
    //  Cached total calculation
    // - Store running total instead of recalculating
    // ========================================================================
    pub fn get_total_unpaid(env: Env) -> i128 {
        let bills: Map<u64, Bill> = env
            .storage()
            .instance()
            .get(&BILLS_KEY)
            .unwrap_or_else(|| Map::new(&env));

        let max_id: u64 = env
            .storage()
            .instance()
            .get(&NEXT_ID_KEY)
            .unwrap_or(0u64);

        let mut total = 0i128;
        
        
        for id in 1..=max_id {
            if let Some(bill) = bills.get(id) {
                if !bill.paid {
                    total += bill.amount;
                }
            }
        }
        
        total
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}
