#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events},
    Address, Env, IntoVal, Symbol, TryFromVal, Vec,
};

use testutils::set_ledger_time;

// Removed local set_time in favor of testutils::set_ledger_time

#[test]
fn test_initialize_split_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

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
    assert_eq!(result, Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100)));
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
fn test_create_remittance_schedule_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 1000);

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
    set_ledger_time(&env, 1, 5000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_create_remittance_schedule(&owner, &10000, &3000, &86400);
    assert!(result.is_err());
}

#[test]
fn test_remittance_schedule_zero_amount() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = <soroban_sdk::Address as AddressTrait>::generate(&env);

    env.mock_all_auths();
    set_ledger_time(&env, 1, 1000);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let result = client.try_create_remittance_schedule(&owner, &0, &3000, &86400);
    assert!(result.is_err());
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

#[test]
#[should_panic(expected = "HostError: Error(Auth, InvalidAction)")]
fn test_update_split_non_owner_auth_failure() {
    let env = Env::default();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client
        .mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize_split",
                args: (&owner, 0u64, 50u32, 30u32, 15u32, 5u32).into_val(&env),
                sub_invokes: &[],
            },
        }])
        .initialize_split(&owner, &0, &50, &30, &15, &5);

    // Call as other without mocking auth, expecting panic
    client.update_split(&other, &0, &40, &40, &10, &10);
}

// ──────────────────────────────────────────────────────────────────────────
// Boundary tests for split percentages (#103)
// ──────────────────────────────────────────────────────────────────────────
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

#[test]
fn test_update_split_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    let result = client.try_update_split(&caller, &0, &25, &25, &25, &25);
    assert_eq!(result, Err(Ok(RemittanceSplitError::NotInitialized)));

    let config = client.get_config();
    assert!(config.is_none());

    let split = client.get_split();
    assert_eq!(split.get(0).unwrap(), 50);
    assert_eq!(split.get(1).unwrap(), 30);
    assert_eq!(split.get(2).unwrap(), 15);
    assert_eq!(split.get(3).unwrap(), 5);
}

// ============================================================================
// Issue #252 – validate_snapshot_import hardening tests
// ============================================================================

/// Helper: build a minimal valid MultiSplitSnapshot with a single entry at 100%.
fn make_single_entry_snapshot(env: &Env, owner: &Address) -> MultiSplitSnapshot {
    let mut entries = Vec::new(env);
    entries.push_back(SplitEntry {
        owner: owner.clone(),
        percentage: 100,
    });
    MultiSplitSnapshot {
        version: 1,
        declared_len: 1,
        entries,
    }
}

/// Helper: build a valid two-entry snapshot (60 / 40).
fn make_two_entry_snapshot(env: &Env, owner_a: &Address, owner_b: &Address) -> MultiSplitSnapshot {
    let mut entries = Vec::new(env);
    entries.push_back(SplitEntry {
        owner: owner_a.clone(),
        percentage: 60,
    });
    entries.push_back(SplitEntry {
        owner: owner_b.clone(),
        percentage: 40,
    });
    MultiSplitSnapshot {
        version: 1,
        declared_len: 2,
        entries,
    }
}

// ── Happy path ───────────────────────────────────────────────────────────────

/// Invariant: a single entry at exactly 100% is accepted.
#[test]
fn test_validate_snapshot_import_single_entry_100_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let snapshot = make_single_entry_snapshot(&env, &owner);
    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Ok(Ok(true)));
}

/// Invariant: two entries summing to exactly 100% are accepted.
#[test]
fn test_validate_snapshot_import_multi_entry_sums_to_100_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let snapshot = make_two_entry_snapshot(&env, &owner, &other);
    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Ok(Ok(true)));
}

// ── Percentage errors ────────────────────────────────────────────────────────

/// Invariant: sum < 100 must be rejected with PercentageSumInvalid.
#[test]
fn test_validate_snapshot_import_sum_less_than_100_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 40 });
    entries.push_back(SplitEntry { owner: other.clone(), percentage: 40 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::PercentageSumInvalid)));
}

/// Invariant: sum > 100 must be rejected with PercentageSumInvalid.
#[test]
fn test_validate_snapshot_import_sum_greater_than_100_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 60 });
    entries.push_back(SplitEntry { owner: other.clone(), percentage: 60 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::PercentageSumInvalid)));
}

/// Invariant: an individual entry with percentage == 0 must be rejected with ZeroPercentage.
#[test]
fn test_validate_snapshot_import_zero_percentage_entry_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 0 });
    entries.push_back(SplitEntry { owner: other.clone(), percentage: 100 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::ZeroPercentage)));
}

/// Invariant: an individual entry with percentage > 100 must be rejected with PercentageOutOfRange.
#[test]
fn test_validate_snapshot_import_percentage_over_100_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 101 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 1, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::PercentageOutOfRange)));
}

// ── Duplicate owner ──────────────────────────────────────────────────────────

/// Invariant: two entries with the same owner address must be rejected with DuplicateOwner.
#[test]
fn test_validate_snapshot_import_duplicate_owner_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 50 });
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 50 }); // duplicate
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::DuplicateOwner)));
}

// ── Empty input guard ────────────────────────────────────────────────────────

/// Invariant: an empty entries list must be rejected with EmptySnapshot.
#[test]
fn test_validate_snapshot_import_empty_entries_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let entries: Vec<SplitEntry> = Vec::new(&env);
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 0, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::EmptySnapshot)));
}

// ── Structural integrity ─────────────────────────────────────────────────────

/// Invariant: declared_len != actual entries.len() must be rejected with LengthMismatch.
#[test]
fn test_validate_snapshot_import_length_mismatch_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 100 });
    // declared_len says 2 but only 1 entry present
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::LengthMismatch)));
}

// ── Version check ────────────────────────────────────────────────────────────

/// Invariant: an unsupported version must be rejected with UnsupportedVersion.
#[test]
fn test_validate_snapshot_import_wrong_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 100 });
    let snapshot = MultiSplitSnapshot { version: 99, declared_len: 1, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::UnsupportedVersion)));
}

// ── Owner / authorisation errors ─────────────────────────────────────────────

/// Invariant: a caller who is not the stored contract owner must be rejected with Unauthorized.
#[test]
fn test_validate_snapshot_import_non_owner_caller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    // attacker uses nonce 0 (their own nonce, not owner's)
    let snapshot = make_single_entry_snapshot(&env, &attacker);
    let result = client.try_validate_snapshot_import(&attacker, &0, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::Unauthorized)));
}

/// Invariant: calling validate_snapshot_import before initialize_split must be rejected.
#[test]
fn test_validate_snapshot_import_not_initialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    // No initialize_split called
    let snapshot = make_single_entry_snapshot(&env, &owner);
    let result = client.try_validate_snapshot_import(&owner, &0, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::Unauthorized)));
}

// ── Replay protection ────────────────────────────────────────────────────────

/// Invariant: replaying the same valid snapshot (same nonce) must be rejected.
#[test]
fn test_validate_snapshot_import_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let snapshot = make_single_entry_snapshot(&env, &owner);

    // First import succeeds (nonce == 1 after initialize_split)
    let first = client.try_validate_snapshot_import(&owner, &1, &snapshot.clone());
    assert_eq!(first, Ok(Ok(true)));

    // Replaying with the same nonce (1) must fail — nonce is now 2
    let replay = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(replay, Err(Ok(SplitImportError::Unauthorized)));
}

// ── Boundary conditions ──────────────────────────────────────────────────────

/// Invariant: minimum valid percentage (1) is accepted when sum == 100.
#[test]
fn test_validate_snapshot_import_min_percentage_1_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 99 });
    entries.push_back(SplitEntry { owner: other.clone(), percentage: 1 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 2, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Ok(Ok(true)));
}

/// Invariant: maximum valid single-entry percentage (100) is accepted.
#[test]
fn test_validate_snapshot_import_max_percentage_100_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let snapshot = make_single_entry_snapshot(&env, &owner);
    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Ok(Ok(true)));
}

/// Invariant: percentage of exactly 101 (one over max) must be rejected.
#[test]
fn test_validate_snapshot_import_percentage_101_boundary_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: 101 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 1, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::PercentageOutOfRange)));
}

/// Invariant: maximum u32 percentage value must be rejected with PercentageOutOfRange.
#[test]
fn test_validate_snapshot_import_u32_max_percentage_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: owner.clone(), percentage: u32::MAX });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 1, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Err(Ok(SplitImportError::PercentageOutOfRange)));
}

/// Invariant: four entries each at 25% (equal split) are accepted.
#[test]
fn test_validate_snapshot_import_four_equal_entries_accepted() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.initialize_split(&owner, &0, &50, &30, &15, &5);

    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let d = Address::generate(&env);

    let mut entries = Vec::new(&env);
    entries.push_back(SplitEntry { owner: a.clone(), percentage: 25 });
    entries.push_back(SplitEntry { owner: b.clone(), percentage: 25 });
    entries.push_back(SplitEntry { owner: c.clone(), percentage: 25 });
    entries.push_back(SplitEntry { owner: d.clone(), percentage: 25 });
    let snapshot = MultiSplitSnapshot { version: 1, declared_len: 4, entries };

    let result = client.try_validate_snapshot_import(&owner, &1, &snapshot);
    assert_eq!(result, Ok(Ok(true)));
}
