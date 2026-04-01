use crate::{ExecutionState, Orchestrator, OrchestratorClient, OrchestratorError};
use soroban_sdk::testutils::{Address as _, MockAuth, MockAuthInvoke};
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, ConversionError, Env, IntoVal,
    InvokeError, Vec,
};

#[contract]
pub struct MockFamilyWallet;

#[contractimpl]
impl MockFamilyWallet {
    pub fn set_owner(env: Env, owner: Address) {
        env.storage().instance().set(&symbol_short!("OWNER"), &owner);
    }

    pub fn get_owner(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .unwrap()
    }

    pub fn check_spending_limit(_env: Env, _caller: Address, amount: i128) -> bool {
        amount <= 10_000
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
        let insurance = total_amount - spending - savings - bills;
        Vec::from_array(&env, [spending, savings, bills, insurance])
    }
}

#[contract]
pub struct MockSavingsGoals;

#[contractimpl]
impl MockSavingsGoals {
    pub fn add_to_goal(_env: Env, _caller: Address, _goal_id: u32, amount: i128) -> i128 {
        amount
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum MockBillDataKey {
    Owner(u32),
    PaymentCount,
    LastCaller,
}

#[contract]
pub struct MockBillPayments;

#[contractimpl]
impl MockBillPayments {
    pub fn set_bill_owner(env: Env, bill_id: u32, owner: Address) {
        env.storage()
            .instance()
            .set(&MockBillDataKey::Owner(bill_id), &owner);
    }

    pub fn payment_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&MockBillDataKey::PaymentCount)
            .unwrap_or(0)
    }

    pub fn last_caller(env: Env) -> Option<Address> {
        env.storage().instance().get(&MockBillDataKey::LastCaller)
    }

    pub fn pay_bill(env: Env, caller: Address, bill_id: u32) {
        let owner: Address = env
            .storage()
            .instance()
            .get(&MockBillDataKey::Owner(bill_id))
            .unwrap_or(caller.clone());

        if caller != owner {
            panic!("unauthorized bill payer");
        }

        let count: u32 = env
            .storage()
            .instance()
            .get(&MockBillDataKey::PaymentCount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&MockBillDataKey::PaymentCount, &(count + 1));
        env.storage()
            .instance()
            .set(&MockBillDataKey::LastCaller, &caller);
    }
}

#[contract]
pub struct MockInsurance;

#[contractimpl]
impl MockInsurance {
    pub fn pay_premium(_env: Env, _caller: Address, _policy_id: u32) -> bool {
        true
    }
}

#[contract]
pub struct ForwardingProxy;

#[contractimpl]
impl ForwardingProxy {
    pub fn forward_execute_bill_payment(
        env: Env,
        orchestrator_addr: Address,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        bills_addr: Address,
        bill_id: u32,
        nonce: u64,
    ) {
        let client = OrchestratorClient::new(&env, &orchestrator_addr);
        client.execute_bill_payment(
            &caller,
            &amount,
            &family_wallet_addr,
            &bills_addr,
            &bill_id,
            &nonce,
        );
    }
}

type TestSetup = (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
);

fn setup_test_env() -> TestSetup {
    let env = Env::default();

    let orchestrator_id = env.register_contract(None, Orchestrator);
    let family_wallet_id = env.register_contract(None, MockFamilyWallet);
    let remittance_split_id = env.register_contract(None, MockRemittanceSplit);
    let savings_id = env.register_contract(None, MockSavingsGoals);
    let bills_id = env.register_contract(None, MockBillPayments);
    let insurance_id = env.register_contract(None, MockInsurance);
    let proxy_id = env.register_contract(None, ForwardingProxy);

    let user = Address::generate(&env);
    let attacker = Address::generate(&env);

    (
        env,
        orchestrator_id,
        family_wallet_id,
        remittance_split_id,
        savings_id,
        bills_id,
        insurance_id,
        proxy_id,
        user,
        attacker,
    )
}

fn set_wallet_owner(env: &Env, family_wallet_id: &Address, owner: &Address) {
    let wallet_client = MockFamilyWalletClient::new(env, family_wallet_id);
    wallet_client.set_owner(owner);
}

fn set_bill_owner(env: &Env, bills_id: &Address, bill_id: u32, owner: &Address) {
    let bills_client = MockBillPaymentsClient::new(env, bills_id);
    bills_client.set_bill_owner(&bill_id, owner);
}

type TryCall<T> = Result<Result<T, ConversionError>, Result<OrchestratorError, InvokeError>>;

fn assert_contract_ok<T: core::fmt::Debug>(result: TryCall<T>) -> T {
    match result {
        Ok(Ok(value)) => value,
        other => panic!("expected contract success, got {:?}", other),
    }
}

fn assert_contract_error<T: core::fmt::Debug>(result: TryCall<T>, expected: OrchestratorError) {
    match result {
        Err(Ok(err)) => assert_eq!(err, expected),
        other => panic!("expected contract error {:?}, got {:?}", expected, other),
    }
}

#[test]
fn test_execute_remittance_flow_succeeds() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        remittance_split_id,
        savings_id,
        bills_id,
        insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    env.mock_all_auths();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 1, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &family_wallet_id,
        &remittance_split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &1,
        &1,
        &1,
    );

    assert!(result.is_ok());
    let flow_result = result.unwrap().unwrap();
    assert_eq!(flow_result.total_amount, 10_000);
}

#[test]
fn test_reentrancy_guard_blocks_concurrent_flow() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        remittance_split_id,
        savings_id,
        bills_id,
        insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    env.mock_all_auths();

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    env.as_contract(&orchestrator_id, || {
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_ST"), &ExecutionState::Executing);
    });

    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &family_wallet_id,
        &remittance_split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &1,
        &1,
        &1,
    );

    assert_contract_error(result, OrchestratorError::ReentrancyDetected);
}

#[test]
fn test_self_reference_rejected() {
    let (
        env,
        orchestrator_id,
        _family_wallet_id,
        remittance_split_id,
        savings_id,
        bills_id,
        insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    env.mock_all_auths();

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &orchestrator_id,
        &remittance_split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &1,
        &1,
        &1,
    );

    assert_contract_error(result, OrchestratorError::SelfReferenceNotAllowed);
}

#[test]
fn test_duplicate_addresses_rejected() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        remittance_split_id,
        savings_id,
        _bills_id,
        insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    env.mock_all_auths();

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client.try_execute_remittance_flow(
        &user,
        &10_000,
        &family_wallet_id,
        &remittance_split_id,
        &savings_id,
        &savings_id,
        &insurance_id,
        &1,
        &1,
        &1,
    );

    assert_contract_error(result, OrchestratorError::DuplicateContractAddress);
}

#[test]
fn test_execute_bill_payment_owner_direct_invoker_succeeds() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 7, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client
        .mock_auths(&[MockAuth {
            address: &user,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    7u32,
                    1u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &7, &1u64);
    assert_contract_ok(result);

    let bills_client = MockBillPaymentsClient::new(&env, &bills_id);
    assert_eq!(bills_client.payment_count(), 1);
    assert_eq!(bills_client.last_caller(), Some(user));
}

#[test]
fn test_execute_bill_payment_rejects_argument_spoofing_without_owner_auth() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        _proxy_id,
        user,
        attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 8, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    8u32,
                    2u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &8, &2u64);

    assert!(result.is_err());

    let bills_client = MockBillPaymentsClient::new(&env, &bills_id);
    assert_eq!(bills_client.payment_count(), 0);
}

#[test]
fn test_execute_bill_payment_blocks_forwarded_non_owner_delegation() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        proxy_id,
        user,
        attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 9, &user);

    let proxy_client = ForwardingProxyClient::new(&env, &proxy_id);
    let result = proxy_client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &proxy_id,
                fn_name: "forward_execute_bill_payment",
                args: (
                    orchestrator_id.clone(),
                    attacker.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    9u32,
                    3u64,
                )
                    .into_val(&env),
                sub_invokes: &[MockAuthInvoke {
                    contract: &orchestrator_id,
                    fn_name: "execute_bill_payment",
                    args: (
                        attacker.clone(),
                        3_000i128,
                        family_wallet_id.clone(),
                        bills_id.clone(),
                        9u32,
                        3u64,
                    )
                        .into_val(&env),
                    sub_invokes: &[],
                }],
            },
        }])
        .try_forward_execute_bill_payment(
            &orchestrator_id,
            &attacker,
            &3_000,
            &family_wallet_id,
            &bills_id,
            &9,
            &3u64,
        );

    assert!(result.is_err());

    let bills_client = MockBillPaymentsClient::new(&env, &bills_id);
    assert_eq!(bills_client.payment_count(), 0);
}

#[test]
fn test_execute_bill_payment_cross_user_execution_attempt_fails() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        _proxy_id,
        user,
        attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 10, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let result = client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    attacker.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    10u32,
                    4u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&attacker, &3_000, &family_wallet_id, &bills_id, &10, &4u64);

    assert_contract_error(result, OrchestratorError::PermissionDenied);

    let bills_client = MockBillPaymentsClient::new(&env, &bills_id);
    assert_eq!(bills_client.payment_count(), 0);
}

#[test]
fn test_execute_bill_payment_rejects_nonce_replay() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 11, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let first = client
        .mock_auths(&[MockAuth {
            address: &user,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    11u32,
                    55u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &11, &55u64);
    assert_contract_ok(first);

    let replayed = client
        .mock_auths(&[MockAuth {
            address: &user,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    11u32,
                    55u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &11, &55u64);

    assert_contract_error(replayed, OrchestratorError::NonceAlreadyUsed);
}

#[test]
fn test_execute_bill_payment_accepts_distinct_nonces_for_same_owner() {
    let (
        env,
        orchestrator_id,
        family_wallet_id,
        _remittance_split_id,
        _savings_id,
        bills_id,
        _insurance_id,
        _proxy_id,
        user,
        _attacker,
    ) = setup_test_env();
    set_wallet_owner(&env, &family_wallet_id, &user);
    set_bill_owner(&env, &bills_id, 12, &user);

    let client = OrchestratorClient::new(&env, &orchestrator_id);
    let first = client
        .mock_auths(&[MockAuth {
            address: &user,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    12u32,
                    100u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &12, &100u64);
    assert_contract_ok(first);

    let second = client
        .mock_auths(&[MockAuth {
            address: &user,
            invoke: &MockAuthInvoke {
                contract: &orchestrator_id,
                fn_name: "execute_bill_payment",
                args: (
                    user.clone(),
                    3_000i128,
                    family_wallet_id.clone(),
                    bills_id.clone(),
                    12u32,
                    101u64,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }])
        .try_execute_bill_payment(&user, &3_000, &family_wallet_id, &bills_id, &12, &101u64);

    assert_contract_ok(second);

    let bills_client = MockBillPaymentsClient::new(&env, &bills_id);
    assert_eq!(bills_client.payment_count(), 2);
}
