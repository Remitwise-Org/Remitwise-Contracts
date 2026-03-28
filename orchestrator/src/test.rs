use crate::{ExecutionState, Orchestrator, OrchestratorClient, OrchestratorError};
use soroban_sdk::{contract, contractimpl, Address, Env, Vec, symbol_short};
use soroban_sdk::testutils::Address as _; 

// ============================================================================
// Mock Contract Implementations
// ============================================================================

#[contract]
pub struct MockFamilyWallet;

#[contractimpl]
impl MockFamilyWallet {
    pub fn check_spending_limit(_env: Env, _caller: Address, amount: i128) -> bool {
        amount <= 10000
    }
}

#[contract]
pub struct MockRemittanceSplit;

#[contractimpl]
impl MockRemittanceSplit {
    pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128> {
        let spending = (total_amount * 40) / 100;
        let savings = (total_amount * 30) / 100;
        let bills = (total_amount * 20) / 100;
        let insurance = (total_amount * 10) / 100;
        Vec::from_array(&env, [spending, savings, bills, insurance])
    }
}

#[contract]
pub struct MockSavingsGoals;

#[derive(Clone)]
#[contracttype]
pub struct SavingsState {
    pub deposit_count: u32,
}

#[contractimpl]
impl MockSavingsGoals {
    pub fn add_to_goal(_env: Env, _caller: Address, goal_id: u32, amount: i128) -> i128 {
        if goal_id == 999 { panic!("Goal not found"); }
        if goal_id == 998 { panic!("Goal already completed"); }
        if amount <= 0 { panic!("Amount must be positive"); }
        amount
    }
}

#[contract]
pub struct MockBillPayments;

#[derive(Clone)]
#[contracttype]
pub struct BillsState {
    pub payment_count: u32,
}

#[contractimpl]
impl MockBillPayments {
    pub fn pay_bill(_env: Env, _caller: Address, bill_id: u32) {
        if bill_id == 999 { panic!("Bill not found"); }
        if bill_id == 998 { panic!("Bill already paid"); }
    }
}

#[contract]
pub struct MockInsurance;

#[contractimpl]
impl MockInsurance {
    pub fn pay_premium(_env: Env, _caller: Address, policy_id: u32) -> bool {
        if policy_id == 999 { panic!("Policy not found"); }
        policy_id != 998
    }
}

// ============================================================================
// Test Functions
// ============================================================================

fn setup_test_env() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let remittance_split_id = env.register_contract(None, MockRemittanceSplit);
    let savings_id = env.register_contract(None, MockSavingsGoals);
    let bills_id = env.register_contract(None, MockBillPayments);
    let insurance_id = env.register_contract(None, MockInsurance);

    let user = Address::generate(&env);

    (env, orchestrator_id, family_wallet_id, remittance_split_id, savings_id, bills_id, insurance_id, user)
}

fn setup() -> (Env, Address, Address, Address, Address, Address, Address, Address) {
    setup_test_env()
}

fn generate_test_address(env: &Env) -> Address {
    Address::generate(env)
}

fn seed_audit_log(_env: &Env, _user: &Address, _count: u32) {}

fn collect_all_pages(client: &OrchestratorClient, _page_size: u32) -> Vec<crate::OrchestratorAuditEntry> {
    client.get_audit_log(&0, &100)
}

#[test]
fn test_execute_remittance_flow_succeeds() {
    let (env, orchestrator_id, family_wallet_id, remittance_split_id,
         savings_id, bills_id, insurance_id, user) = setup_test_env();
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    let result = client.try_execute_remittance_flow(
        &user, &10000, &family_wallet_id, &remittance_split_id,
        &savings_id, &bills_id, &insurance_id, &1, &1, &1,
    );

    assert!(result.is_ok());
    let flow_result = result.unwrap().unwrap();
    assert_eq!(flow_result.total_amount, 10000);
}

#[test]
fn test_reentrancy_guard_blocks_concurrent_flow() {
    let (env, orchestrator_id, family_wallet_id, remittance_split_id,
         savings_id, bills_id, insurance_id, user) = setup_test_env();
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Simulate lock held
    env.as_contract(&orchestrator_id, || {
        env.storage().instance().set(&symbol_short!("EXEC_ST"), &ExecutionState::Executing);
    });

    let result = client.try_execute_remittance_flow(
        &user, &10000, &family_wallet_id, &remittance_split_id,
        &savings_id, &bills_id, &insurance_id, &1, &1, &1,
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap() as u32, 10);
}

#[test]
fn test_self_reference_rejected() {
    let (env, orchestrator_id, family_wallet_id, remittance_split_id,
         savings_id, bills_id, insurance_id, user) = setup_test_env();
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Use orchestrator id as one of the downstream addresses
    let result = client.try_execute_remittance_flow(
        &user, &10000, &orchestrator_id, &remittance_split_id,
        &savings_id, &bills_id, &insurance_id, &1, &1, &1,
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap() as u32, 13);
}

#[test]
fn test_duplicate_addresses_rejected() {
    let (env, orchestrator_id, family_wallet_id, remittance_split_id,
         savings_id, bills_id, insurance_id, user) = setup_test_env();
    let client = OrchestratorClient::new(&env, &orchestrator_id);

    // Use same address for savings and bills
    let result = client.try_execute_remittance_flow(
        &user, &10000, &family_wallet_id, &remittance_split_id,
        &savings_id, &savings_id, &insurance_id, &1, &1, &1,
    );

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap() as u32, 11);
}
