#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger, LedgerInfo},
    Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec,
};

fn set_time(env: &Env, timestamp: u64) {
    let proto = env.ledger().protocol_version();

    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100000,
    });
}

#[test]
fn test_initialize_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let success = client.initialize_split(
        &owner, &0,  // nonce
        &50, // spending
        &30, // savings
        &15, // bills
        &5,  // insurance
    );

    assert_eq!(success, true);

    let config = client.get_config().unwrap();
    assert_eq!(config.owner, owner);
    assert_eq!(config.spending_percent, 50);
    assert_eq!(config.savings_percent, 30);
    assert_eq!(config.bills_percent, 15);
    assert_eq!(config.insurance_percent, 5);
}

#[test]
fn test_initialize_split_invalid_sum() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let result = client.try_initialize_split(
        &owner, &0, // nonce
        &50, &50, &10, // Sums to 110
        &0,
    );
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100))
    );
}

#[test]
fn test_initialize_split_already_initialized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    // Second init should fail
    let result = client.try_initialize_split(&owner, &1, &50, &30, &15, &5);
    assert_eq!(result, Err(Ok(RemittanceSplitError::AlreadyInitialized)));
}

#[test]
fn test_update_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let success = client.update_split(&owner, &1, &40, &40, &10, &10);
    assert_eq!(success, true);

    let config = client.get_config().unwrap();
    assert_eq!(config.spending_percent, 40);
    assert_eq!(config.savings_percent, 40);
    assert_eq!(config.bills_percent, 10);
    assert_eq!(config.insurance_percent, 10);
}

#[test]
fn test_update_split_unauthorized() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_update_split(&other, &0, &40, &40, &10, &10);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
}

#[test]
fn test_calculate_split() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Test with 1000 units
    let amounts = client.calculate_split(&1000);

    // spending: 50% of 1000 = 500
    // savings: 30% of 1000 = 300
    // bills: 15% of 1000 = 150
    // insurance: remainder = 1000 - 500 - 300 - 150 = 50

    assert_eq!(amounts.get(0).unwrap(), 500);
    assert_eq!(amounts.get(1).unwrap(), 300);
    assert_eq!(amounts.get(2).unwrap(), 150);
    assert_eq!(amounts.get(3).unwrap(), 50);
}

#[test]
fn test_calculate_split_rounding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // 33, 33, 33, 1 setup
    client.initialize_split(&owner, &0, &33, &33, &33, &1);

    // Total 100
    // 33% = 33
    // Remainder should go to last one (insurance) logic in contract:
    // insurance = total - spending - savings - bills
    // 100 - 33 - 33 - 33 = 1. Correct.

    let amounts = client.calculate_split(&100);
    assert_eq!(amounts.get(0).unwrap(), 33);
    assert_eq!(amounts.get(1).unwrap(), 33);
    assert_eq!(amounts.get(2).unwrap(), 33);
    assert_eq!(amounts.get(3).unwrap(), 1);

    // Verify invariant: sum == total_amount
    let sum: i128 = amounts.into_iter().sum();
    assert_eq!(sum, 100);
}

#[test]
fn test_calculate_split_rounding_rigorous() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Case 1: 33/33/33/1 split, total 100
    // Each 33% of 100 is 33. Insurance gets remainder: 100 - 33 - 33 - 33 = 1.
    client.initialize_split(&owner, &0, &33, &33, &33, &1);
    let amounts = client.calculate_split(&100);
    let sum: i128 = amounts.clone().into_iter().sum();
    assert_eq!(
        sum, 100,
        "Sum must exactly equal total_amount for 33/33/33/1 split"
    );
    assert_eq!(
        amounts.get(3).unwrap(),
        1,
        "Insurance should be the remainder (1)"
    );

    // Case 2: 25/25/25/25 split, total 99
    // Each 25% of 99 is (99 * 25) / 100 = 24.
    // Spending: 24, Savings: 24, Bills: 24
    // Insurance (remainder) = 99 - 24 - 24 - 24 = 27.
    let nonce = client.get_nonce(&owner);
    let result = client.try_update_split(&owner, &nonce, &25, &25, &25, &25);
    assert!(result.is_ok(), "update_split Case 2 failed: {:?}", result);
    let amounts = client.calculate_split(&99);
    let sum: i128 = amounts.clone().into_iter().sum();
    assert_eq!(
        sum, 99,
        "Sum must exactly equal total_amount (99) for 25/25/25/25 split"
    );
    assert_eq!(
        amounts.get(3).unwrap(),
        27,
        "Insurance should absorb the rounding remainder (27)"
    );

    // Case 3: 100/0/0/0 split, total 1000
    // Spending: 1000, others 0. Remainder: 1000 - 1000 - 0 - 0 = 0.
    let nonce = client.get_nonce(&owner);
    let result = client.try_update_split(&owner, &nonce, &100, &0, &0, &0);
    assert!(result.is_ok(), "update_split Case 3 failed: {:?}", result);
    let amounts = client.calculate_split(&1000);
    let sum: i128 = amounts.clone().into_iter().sum();
    assert_eq!(sum, 1000);
    assert_eq!(amounts.get(0).unwrap(), 1000);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);

    // Case 4: Uneven split with large non-divisible amount
    // 30/30/30/10 split, total 1,000,001
    // Spending: (1,000,001 * 30) / 100 = 300,000
    // Savings: 300,000
    // Bills: 300,000
    // Insurance = 1,000,001 - 900,000 = 100,001
    let nonce = client.get_nonce(&owner);
    let result = client.try_update_split(&owner, &nonce, &30, &30, &30, &10);
    assert!(result.is_ok(), "update_split Case 4 failed: {:?}", result);
    let amounts = client.calculate_split(&1000001);
    let sum: i128 = amounts.into_iter().sum();
    assert_eq!(
        sum, 1000001,
        "Sum must exactly match even with large prime-like amounts"
    );

    // Documenting that the contract assigns the remainder to insurance to avoid rounding drift.
}

#[test]
fn test_calculate_split_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_calculate_split(&0);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
}

#[test]
fn test_calculate_complex_rounding() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();
    // 17, 19, 23, 41 (Primes summing to 100)
    client.initialize_split(&owner, &0, &17, &19, &23, &41);

    // Amount 1000
    // 17% = 170
    // 19% = 190
    // 23% = 230
    // 41% = 410
    // Sum = 1000. Perfect.
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 170);
    assert_eq!(amounts.get(1).unwrap(), 190);
    assert_eq!(amounts.get(2).unwrap(), 230);
    assert_eq!(amounts.get(3).unwrap(), 410);

    // Amount 3
    // 17% of 3 = 0
    // 19% of 3 = 0
    // 23% of 3 = 0
    // Remainder = 3 - 0 - 0 - 0 = 3. All goes to insurance.
    let tiny_amounts = client.calculate_split(&3);
    assert_eq!(tiny_amounts.get(0).unwrap(), 0);
    assert_eq!(tiny_amounts.get(3).unwrap(), 3);
}

#[test]
fn test_create_remittance_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let schedule_id = client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_remittance_schedule(&schedule_id);
    assert!(schedule.is_some());
    let schedule = schedule.unwrap();
    assert_eq!(schedule.amount, 10000);
    assert_eq!(schedule.next_due, 3000);
    assert_eq!(schedule.interval, 86400);
    assert!(schedule.active);
}

#[test]
fn test_modify_remittance_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let schedule_id = client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    client.modify_remittance_schedule(&owner, &schedule_id, &15000, &4000, &172800);

    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.amount, 15000);
    assert_eq!(schedule.next_due, 4000);
    assert_eq!(schedule.interval, 172800);
}

#[test]
fn test_cancel_remittance_schedule() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let schedule_id = client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    client.cancel_remittance_schedule(&owner, &schedule_id);

    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

#[test]
fn test_get_remittance_schedules() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    client.create_remittance_schedule(&owner, &5000, &4000, &172800);

    let schedules = client.get_remittance_schedules(&owner);
    assert_eq!(schedules.len(), 2);
}

#[test]
fn test_remittance_schedule_validation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 5000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_create_remittance_schedule(&owner, &10000, &3000, &86400);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidDueDate)));
}

#[test]
fn test_remittance_schedule_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_time(&env, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_create_remittance_schedule(&owner, &0, &3000, &86400);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
}
#[test]
fn test_initialize_split_events() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let events = env.events().all();
    let last_event = events.last().unwrap();

    // The event emitted is: env.events().publish((symbol_short!("split"), SplitEvent::Initialized), owner);
    assert_eq!(last_event.0, contract_id);

    let topics = &last_event.1;
    let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(topic0, symbol_short!("split"));
    assert_eq!(topic1, SplitEvent::Initialized);

    let data: Address = Address::try_from_val(&env, &last_event.2).unwrap();
    assert_eq!(data, owner);
}

#[test]
fn test_update_split_events() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);
    client.update_split(&owner, &1, &40, &40, &10, &10);

    let events = env.events().all();
    // update_split publishes two events:
    // 1. (SPLIT_INITIALIZED,), event
    // 2. (symbol_short!("split"), SplitEvent::Updated), caller
    let last_event = events.last().unwrap();

    assert_eq!(last_event.0, contract_id);

    let topics = &last_event.1;
    let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(topic0, symbol_short!("split"));
    assert_eq!(topic1, SplitEvent::Updated);

    let data: Address = Address::try_from_val(&env, &last_event.2).unwrap();
    assert_eq!(data, owner);
}

#[test]
fn test_calculate_split_events() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let total_amount = 1000i128;
    client.calculate_split(&total_amount);

    let events = env.events().all();
    // calculate_split publishes two events:
    // 1. (SPLIT_CALCULATED,), event
    // 2. (symbol_short!("split"), SplitEvent::Calculated), total_amount
    let last_event = events.last().unwrap();

    assert_eq!(last_event.0, contract_id);

    let topics = &last_event.1;
    let topic0: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
    assert_eq!(topic0, symbol_short!("split"));
    assert_eq!(topic1, SplitEvent::Calculated);

    let data: i128 = i128::try_from_val(&env, &last_event.2).unwrap();
    assert_eq!(data, total_amount);
}

// ──────────────────────────────────────────────────────────────────────────
// Boundary tests for split percentages (#103)
// ──────────────────────────────────────────────────────────────────────────

/// 100 % spending, all other categories zero.
#[test]
fn test_split_boundary_100_0_0_0() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let ok = client.initialize_split(&owner, &0, &100, &0, &0, &0);
    assert!(ok);

    // get_split must return the exact percentages
    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 100);
    assert_eq!(split.get(1).unwrap(), 0);
    assert_eq!(split.get(2).unwrap(), 0);
    assert_eq!(split.get(3).unwrap(), 0);

    // calculate_split must allocate the entire amount to spending
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 1000);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

/// 100 % savings, all other categories zero.
#[test]
fn test_split_boundary_0_100_0_0() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let ok = client.initialize_split(&owner, &0, &0, &100, &0, &0);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 0);
    assert_eq!(split.get(1).unwrap(), 100);
    assert_eq!(split.get(2).unwrap(), 0);
    assert_eq!(split.get(3).unwrap(), 0);

    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 0);
    assert_eq!(amounts.get(1).unwrap(), 1000);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

/// 100 % bills, all other categories zero.
#[test]
fn test_split_boundary_0_0_100_0() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let ok = client.initialize_split(&owner, &0, &0, &0, &100, &0);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 0);
    assert_eq!(split.get(1).unwrap(), 0);
    assert_eq!(split.get(2).unwrap(), 100);
    assert_eq!(split.get(3).unwrap(), 0);

    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 0);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 1000);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

/// 100 % insurance, all other categories zero.
#[test]
fn test_split_boundary_0_0_0_100() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let ok = client.initialize_split(&owner, &0, &0, &0, &0, &100);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 0);
    assert_eq!(split.get(1).unwrap(), 0);
    assert_eq!(split.get(2).unwrap(), 0);
    assert_eq!(split.get(3).unwrap(), 100);

    // Insurance gets the remainder: 1000 - 0 - 0 - 0 = 1000
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 0);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 1000);
}

/// Equal split: 25 / 25 / 25 / 25.
#[test]
fn test_split_boundary_25_25_25_25() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    let ok = client.initialize_split(&owner, &0, &25, &25, &25, &25);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 25);
    assert_eq!(split.get(1).unwrap(), 25);
    assert_eq!(split.get(2).unwrap(), 25);
    assert_eq!(split.get(3).unwrap(), 25);

    // 25 % of 1000 = 250 for each category
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 250);
    assert_eq!(amounts.get(1).unwrap(), 250);
    assert_eq!(amounts.get(2).unwrap(), 250);
    assert_eq!(amounts.get(3).unwrap(), 250);
}

/// update_split with boundary percentages: change from a normal split
/// to 100/0/0/0, then to 25/25/25/25.
#[test]
fn test_update_split_boundary_percentages() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    env.mock_all_auths();

    // Start with a typical split
    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // Update to 100/0/0/0
    let ok = client.update_split(&owner, &1, &100, &0, &0, &0);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 100);
    assert_eq!(split.get(1).unwrap(), 0);
    assert_eq!(split.get(2).unwrap(), 0);
    assert_eq!(split.get(3).unwrap(), 0);

    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 1000);
    assert_eq!(amounts.get(1).unwrap(), 0);
    assert_eq!(amounts.get(2).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 0);

    // Update again to 25/25/25/25
    let ok = client.update_split(&owner, &1, &25, &25, &25, &25);
    assert!(ok);

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 25);
    assert_eq!(split.get(1).unwrap(), 25);
    assert_eq!(split.get(2).unwrap(), 25);
    assert_eq!(split.get(3).unwrap(), 25);

    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 250);
    assert_eq!(amounts.get(1).unwrap(), 250);
    assert_eq!(amounts.get(2).unwrap(), 250);
    assert_eq!(amounts.get(3).unwrap(), 250);
}
