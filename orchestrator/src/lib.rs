#![no_std]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::manual_inspect)]
#![allow(dead_code)]
#![allow(unused_imports)]

//! # Cross-Contract Orchestrator
//!
//! The Cross-Contract Orchestrator coordinates automated remittance allocation across
//! multiple Soroban smart contracts in the Remitwise ecosystem. It implements atomic,
//! multi-contract operations with family wallet permission enforcement.

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error,
    symbol_short, Address, Env, Symbol, Vec,
};

#[cfg(test)]
mod test;

// ============================================================================
// Contract Client Interfaces for Cross-Contract Calls
// ============================================================================

#[contractclient(name = "FamilyWalletClient")]
pub trait FamilyWalletTrait {
    fn check_spending_limit(env: Env, caller: Address, amount: i128) -> bool;
}

#[contractclient(name = "RemittanceSplitClient")]
pub trait RemittanceSplitTrait {
    fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>;
}

#[contractclient(name = "SavingsGoalsClient")]
pub trait SavingsGoalsTrait {
    fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128;
}

#[contractclient(name = "BillPaymentsClient")]
pub trait BillPaymentsTrait {
    fn pay_bill(env: Env, caller: Address, bill_id: u32);
}

#[contractclient(name = "InsuranceClient")]
pub trait InsuranceTrait {
    fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool;
}

// ============================================================================
// Data Types
// ============================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OrchestratorError {
    PermissionDenied = 1,
    SpendingLimitExceeded = 2,
    SavingsDepositFailed = 3,
    BillPaymentFailed = 4,
    InsurancePaymentFailed = 5,
    RemittanceSplitFailed = 6,
    InvalidAmount = 7,
    InvalidContractAddress = 8,
    CrossContractCallFailed = 9,
    ReentrancyDetected = 10,
    DuplicateContractAddress = 11,
    ContractNotConfigured = 12,
    SelfReferenceNotAllowed = 13,
}

#[contracttype]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ExecutionState {
    Idle = 0,
    Executing = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemittanceFlowResult {
    pub total_amount: i128,
    pub spending_amount: i128,
    pub savings_amount: i128,
    pub bills_amount: i128,
    pub insurance_amount: i128,
    pub savings_success: bool,
    pub bills_success: bool,
    pub insurance_success: bool,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemittanceFlowEvent {
    pub caller: Address,
    pub total_amount: i128,
    pub allocations: Vec<i128>,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemittanceFlowErrorEvent {
    pub caller: Address,
    pub failed_step: Symbol,
    pub error_code: u32,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionStats {
    pub total_flows_executed: u64,
    pub total_flows_failed: u64,
    pub total_amount_processed: i128,
    pub last_execution: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorAuditEntry {
    pub caller: Address,
    pub operation: Symbol,
    pub amount: i128,
    pub success: bool,
    pub timestamp: u64,
    pub error_code: Option<u32>,
}

const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;
const MAX_AUDIT_ENTRIES: u32 = 100;

// ============================================================================
// Contract Implementation
// ============================================================================

#[contract]
pub struct Orchestrator;

#[contractimpl]
impl Orchestrator {
    // -----------------------------------------------------------------------
    // Reentrancy Guard
    // -----------------------------------------------------------------------

    fn acquire_execution_lock(env: &Env) -> Result<(), OrchestratorError> {
        let state: ExecutionState = env
            .storage()
            .instance()
            .get(&symbol_short!("EXEC_ST"))
            .unwrap_or(ExecutionState::Idle);

        if state == ExecutionState::Executing {
            return Err(OrchestratorError::ReentrancyDetected);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_ST"), &ExecutionState::Executing);

        Ok(())
    }

    fn release_execution_lock(env: &Env) {
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_ST"), &ExecutionState::Idle);
    }

    pub fn get_execution_state(env: Env) -> ExecutionState {
        env.storage()
            .instance()
            .get(&symbol_short!("EXEC_ST"))
            .unwrap_or(ExecutionState::Idle)
    }

    // -----------------------------------------------------------------------
    // Main Entry Points
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn execute_remittance_flow(
        env: Env,
        caller: Address,
        total_amount: i128,
        family_wallet_addr: Address,
        remittance_split_addr: Address,
        savings_addr: Address,
        bills_addr: Address,
        insurance_addr: Address,
        goal_id: u32,
        bill_id: u32,
        policy_id: u32,
    ) -> Result<RemittanceFlowResult, OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        let timestamp = env.ledger().timestamp();

        let res = (|| {
            Self::validate_remittance_flow_addresses(
                &env,
                &family_wallet_addr,
                &remittance_split_addr,
                &savings_addr,
                &bills_addr,
                &insurance_addr,
            )?;

            if total_amount <= 0 {
                return Err(OrchestratorError::InvalidAmount);
            }

            Self::check_spending_limit(&env, &family_wallet_addr, &caller, total_amount)?;

            let allocations = Self::extract_allocations(&env, &remittance_split_addr, total_amount)?;

            let spending_amount = allocations.get(0).unwrap_or(0);
            let savings_amount = allocations.get(1).unwrap_or(0);
            let bills_amount = allocations.get(2).unwrap_or(0);
            let insurance_amount = allocations.get(3).unwrap_or(0);

            let savings_success = Self::deposit_to_savings(&env, &savings_addr, &caller, goal_id, savings_amount).is_ok();
            let bills_success = Self::execute_bill_payment_internal(&env, &bills_addr, &caller, bill_id).is_ok();
            let insurance_success = Self::pay_insurance_premium(&env, &insurance_addr, &caller, policy_id).is_ok();

            let flow_result = RemittanceFlowResult {
                total_amount,
                spending_amount,
                savings_amount,
                bills_amount,
                insurance_amount,
                savings_success,
                bills_success,
                insurance_success,
                timestamp,
            };

            Self::emit_success_event(&env, &caller, total_amount, &allocations, timestamp);
            Ok(flow_result)
        })();

        if let Err(e) = &res {
             Self::emit_error_event(&env, &caller, symbol_short!("flow"), *e as u32, timestamp);
        }

        Self::release_execution_lock(&env);
        res
    }

    pub fn execute_savings_deposit(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        savings_addr: Address,
        goal_id: u32,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        let timestamp = env.ledger().timestamp();

        let result = (|| {
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount)?;
            Self::deposit_to_savings(&env, &savings_addr, &caller, goal_id, amount)?;
            Ok(())
        })();

        Self::release_execution_lock(&env);
        result
    }

    pub fn execute_bill_payment(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        bills_addr: Address,
        bill_id: u32,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        let result = (|| {
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount)?;
            Self::execute_bill_payment_internal(&env, &bills_addr, &caller, bill_id)?;
            Ok(())
        })();
        Self::release_execution_lock(&env);
        result
    }

    pub fn execute_insurance_payment(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        insurance_addr: Address,
        policy_id: u32,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        let result = (|| {
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount)?;
            Self::pay_insurance_premium(&env, &insurance_addr, &caller, policy_id)?;
            Ok(())
        })();
        Self::release_execution_lock(&env);
        result
    }

    // -----------------------------------------------------------------------
    // Internal Helpers
    // -----------------------------------------------------------------------

    fn check_spending_limit(env: &Env, family_wallet_addr: &Address, caller: &Address, amount: i128) -> Result<(), OrchestratorError> {
        let wallet_client = FamilyWalletClient::new(env, family_wallet_addr);
        if wallet_client.check_spending_limit(caller, &amount) {
            Ok(())
        } else {
            Err(OrchestratorError::SpendingLimitExceeded)
        }
    }

    fn extract_allocations(env: &Env, split_addr: &Address, total: i128) -> Result<Vec<i128>, OrchestratorError> {
        let client = RemittanceSplitClient::new(env, split_addr);
        Ok(client.calculate_split(&total))
    }

    fn deposit_to_savings(env: &Env, addr: &Address, caller: &Address, goal_id: u32, amount: i128) -> Result<(), OrchestratorError> {
        let client = SavingsGoalsClient::new(env, addr);
        client.add_to_goal(caller, &goal_id, &amount);
        Ok(())
    }

    fn execute_bill_payment_internal(env: &Env, addr: &Address, caller: &Address, bill_id: u32) -> Result<(), OrchestratorError> {
        let client = BillPaymentsClient::new(env, addr);
        client.pay_bill(caller, &bill_id);
        Ok(())
    }

    fn pay_insurance_premium(env: &Env, addr: &Address, caller: &Address, policy_id: u32) -> Result<(), OrchestratorError> {
        let client = InsuranceClient::new(env, addr);
        client.pay_premium(caller, &policy_id);
        Ok(())
    }

    fn validate_remittance_flow_addresses(
        env: &Env,
        family: &Address,
        split: &Address,
        savings: &Address,
        bills: &Address,
        insurance: &Address,
    ) -> Result<(), OrchestratorError> {
        let current = env.current_contract_address();
        if family == &current || split == &current || savings == &current || bills == &current || insurance == &current {
            return Err(OrchestratorError::SelfReferenceNotAllowed);
        }
        if family == split || family == savings || family == bills || family == insurance ||
           split == savings || split == bills || split == insurance ||
           savings == bills || savings == insurance ||
           bills == insurance {
            return Err(OrchestratorError::DuplicateContractAddress);
        }
        Ok(())
    }

    fn emit_success_event(env: &Env, caller: &Address, total: i128, allocations: &Vec<i128>, timestamp: u64) {
        env.events().publish((symbol_short!("flow_ok"),), RemittanceFlowEvent {
            caller: caller.clone(),
            total_amount: total,
            allocations: allocations.clone(),
            timestamp,
        });
    }

    fn emit_error_event(env: &Env, caller: &Address, step: Symbol, code: u32, timestamp: u64) {
        env.events().publish((symbol_short!("flow_err"),), RemittanceFlowErrorEvent {
            caller: caller.clone(),
            failed_step: step,
            error_code: code,
            timestamp,
        });
    }

    pub fn get_execution_stats(env: Env) -> ExecutionStats {
        env.storage().instance().get(&symbol_short!("STATS")).unwrap_or(ExecutionStats {
            total_flows_executed: 0,
            total_flows_failed: 0,
            total_amount_processed: 0,
            last_execution: 0,
        })
    }

    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<OrchestratorAuditEntry> {
        let log: Vec<OrchestratorAuditEntry> = env.storage().instance().get(&symbol_short!("AUDIT")).unwrap_or_else(|| Vec::new(&env));
        let mut out = Vec::new(&env);
        let len = log.len();
        let end = from_index.saturating_add(limit).min(len);
        for i in from_index..end {
            if let Some(e) = log.get(i) { out.push_back(e); }
        }
        out
    }
}
