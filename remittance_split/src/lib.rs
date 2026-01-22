#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, vec, Env, Symbol, Vec};

// Event topics
const SPLIT_INITIALIZED: Symbol = symbol_short!("init");
const SPLIT_CALCULATED: Symbol = symbol_short!("calc");

// Event data structures
#[derive(Clone)]
#[contracttype]
pub struct SplitInitializedEvent {
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub timestamp: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct SplitCalculatedEvent {
    pub total_amount: i128,
    pub spending_amount: i128,
    pub savings_amount: i128,
    pub bills_amount: i128,
    pub insurance_amount: i128,
    pub timestamp: u64,
}

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    /// Initialize a remittance split configuration
    pub fn initialize_split(
        env: Env,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> bool {
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;

        if total != 100 {
            return false;
        }

        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        // Emit SplitInitialized event
        let event = SplitInitializedEvent {
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((SPLIT_INITIALIZED,), event);

        true
    }

    /// Get the current split configuration
    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![env, 50, 30, 15, 5])
    }

    /// Calculate split amounts from a total remittance amount
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let split = Self::get_split(&env);

        let spending = (total_amount * split.get(0).unwrap() as i128) / 100;
        let savings = (total_amount * split.get(1).unwrap() as i128) / 100;
        let bills = (total_amount * split.get(2).unwrap() as i128) / 100;
        let insurance = total_amount - spending - savings - bills;

        // Emit SplitCalculated event
        let event = SplitCalculatedEvent {
            total_amount,
            spending_amount: spending,
            savings_amount: savings,
            bills_amount: bills,
            insurance_amount: insurance,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((SPLIT_CALCULATED,), event);

        vec![&env, spending, savings, bills, insurance]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Events;

    #[test]
    fn test_initialize_split_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        // Initialize split
        let result = client.initialize_split(&50, &30, &15, &5);
        assert!(result);

        // Verify event was emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_calculate_split_emits_event() {
        let env = Env::default();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        // Initialize split first
        client.initialize_split(&40, &30, &20, &10);

        // Get events before calculating
        let events_before = env.events().all().len();

        // Calculate split
        let result = client.calculate_split(&1000);
        assert_eq!(result.len(), 4);
        assert_eq!(result.get(0).unwrap(), 400); // 40% of 1000
        assert_eq!(result.get(1).unwrap(), 300); // 30% of 1000
        assert_eq!(result.get(2).unwrap(), 200); // 20% of 1000
        assert_eq!(result.get(3).unwrap(), 100); // 10% of 1000

        // Verify 1 new event was emitted
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 1);
    }

    #[test]
    fn test_multiple_operations_emit_multiple_events() {
        let env = Env::default();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        // Initialize split
        client.initialize_split(&50, &25, &15, &10);

        // Calculate split twice
        client.calculate_split(&2000);
        client.calculate_split(&3000);

        // Should have 3 events total (1 init + 2 calc)
        let events = env.events().all();
        assert_eq!(events.len(), 3);
    }
}
