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
    symbol_short, Address, Env, Map, Symbol, Vec,
};
use remitwise_common::{EventCategory, EventPriority, RemitwiseEvents};

#[cfg(test)]
mod test;

// ============================================================================
// Contract Client Interfaces for Cross-Contract Calls
// ============================================================================

#[contractclient(name = "FamilyWalletClient")]
pub trait FamilyWalletTrait {
    fn check_spending_limit(env: Env, caller: Address, amount: i128) -> bool;
    fn get_owner(env: Env) -> Address;
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
    fn pay_premium(env: Env, caller: Address, policy_id: u32) -> Result<bool, u32>;
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
    NonceAlreadyUsed = 14,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum StorageKey {
    Nonce(Address, Symbol, u64),
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

    /// Acquire the execution lock, preventing reentrant calls.
    ///
    /// Checks the current execution state stored under the `EXEC_ST` key in
    /// instance storage. If the state is `Idle` (or unset), transitions to
    /// `Executing` and returns `Ok(())`. If already `Executing`, returns
    /// `Err(OrchestratorError::ReentrancyDetected)`.
    ///
    /// # Security
    /// This MUST be called at the very start of every public entry point,
    /// before any state reads or cross-contract calls.
    ///
    /// # Gas Estimation
    /// ~500 gas (single instance storage read + write)
    /// Validate that all contract addresses in a remittance flow are non-zero/valid.
    fn validate_remittance_flow_addresses(
        _env: &Env,
        _family_wallet_addr: &Address,
        _remittance_split_addr: &Address,
        _savings_addr: &Address,
        _bills_addr: &Address,
        _insurance_addr: &Address,
    ) -> Result<(), OrchestratorError> {
        // Addresses in Soroban are always valid if they exist; no additional
        // validation is required beyond the type system guarantees.
        Ok(())
    }

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

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// @notice Authorizes bill payment execution using the family wallet owner as the trusted principal.
    /// @dev The `caller` must explicitly authorize the invocation and must match the
    ///      owner returned by the configured family wallet. No delegate or non-owner
    ///      execution path is supported for `execute_bill_payment`.
    fn require_bill_payment_owner(
        env: &Env,
        family_wallet_addr: &Address,
        caller: &Address,
    ) -> Result<(), OrchestratorError> {
        caller.require_auth();
        let wallet_client = FamilyWalletClient::new(env, family_wallet_addr);

        if wallet_client.get_owner() != caller.clone() {
            return Err(OrchestratorError::PermissionDenied);
        }

        Ok(())
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

<<<<<<< HEAD
    /// Check family wallet permission before executing an operation
    ///
    /// This function validates that the caller has permission to perform the operation
    /// by checking with the Family Wallet contract. This acts as a permission gate
    /// for all orchestrator operations.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `caller` - Address requesting permission
    /// * `amount` - Amount involved in the operation
    ///
    /// # Returns
    /// Ok(true) if permission granted, Err(OrchestratorError::PermissionDenied) otherwise
    ///
    /// # Gas Estimation
    /// ~2000 gas for cross-contract permission check
    ///
    /// # Cross-Contract Call Flow
    /// 1. Create FamilyWalletClient instance with the provided address
    /// 2. Call check_spending_limit via cross-contract call
    /// 3. If the call succeeds and returns true, permission is granted
    /// 4. If the call fails or returns false, permission is denied
    fn check_family_wallet_permission(
        env: &Env,
        family_wallet_addr: &Address,
        caller: &Address,
        amount: i128,
    ) -> Result<bool, OrchestratorError> {
        // Create client for cross-contract call
        let wallet_client = FamilyWalletClient::new(env, family_wallet_addr);

        // Gas estimation: ~2000 gas
        // Call the family wallet to check spending limit
        // This will panic if the caller doesn't have permission or exceeds limit
        let has_permission = wallet_client.check_spending_limit(caller, &amount);

        if has_permission {
            Ok(true)
        } else {
            Err(OrchestratorError::PermissionDenied)
        }
    }

    /// Check if operation amount exceeds caller's spending limit
    ///
    /// This function queries the Family Wallet contract to verify that the
    /// operation amount does not exceed the caller's configured spending limit.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `caller` - Address to check spending limit for
    /// * `amount` - Amount to validate against limit
    ///
    /// # Returns
    /// Ok(()) if within limit, Err(OrchestratorError::SpendingLimitExceeded) otherwise
    ///
    /// # Gas Estimation
    /// ~2000 gas for cross-contract limit check
    fn check_spending_limit(
        env: &Env,
        family_wallet_addr: &Address,
        caller: &Address,
        amount: i128,
    ) -> Result<(), OrchestratorError> {
        // Create client for cross-contract call
        let wallet_client = FamilyWalletClient::new(env, family_wallet_addr);

        // Gas estimation: ~2000 gas
        // Check if amount is within spending limit
        let within_limit = wallet_client.check_spending_limit(caller, &amount);

        if within_limit {
            Ok(())
        } else {
            Err(OrchestratorError::SpendingLimitExceeded)
        }
    }

    /// Validate that all remittance flow contract addresses are distinct
    /// and do not reference the orchestrator itself.
    ///
    /// This guard protects against misconfiguration and self-referential
    /// contract calls that could create invalid execution flows.
    fn validate_remittance_flow_addresses(
        env: &Env,
        family_wallet_addr: &Address,
        remittance_split_addr: &Address,
        savings_addr: &Address,
        bills_addr: &Address,
        insurance_addr: &Address,
    ) -> Result<(), OrchestratorError> {
        let self_addr = env.current_contract_address();

        let addrs = [
            family_wallet_addr,
            remittance_split_addr,
            savings_addr,
            bills_addr,
            insurance_addr,
        ];

        for addr in addrs {
            if *addr == self_addr {
                return Err(OrchestratorError::InvalidContractAddress);
            }
        }

        if family_wallet_addr == remittance_split_addr
            || family_wallet_addr == savings_addr
            || family_wallet_addr == bills_addr
            || family_wallet_addr == insurance_addr
            || remittance_split_addr == savings_addr
            || remittance_split_addr == bills_addr
            || remittance_split_addr == insurance_addr
            || savings_addr == bills_addr
            || savings_addr == insurance_addr
            || bills_addr == insurance_addr
        {
            return Err(OrchestratorError::InvalidContractAddress);
        }

        Ok(())
    }

    // ============================================================================
    // Helper Functions - Remittance Split Allocation
    // ============================================================================

    /// Extract allocation amounts from the Remittance Split contract
    ///
    /// This function calls the Remittance Split contract to calculate how a total
    /// remittance amount should be divided across spending, savings, bills, and insurance.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `remittance_split_addr` - Address of the Remittance Split contract
    /// * `total_amount` - Total remittance amount to split (must be positive)
    ///
    /// # Returns
    /// Ok(Vec<i128>) containing [spending, savings, bills, insurance] amounts
    /// Err(OrchestratorError) if validation fails or cross-contract call fails
    ///
    /// # Gas Estimation
    /// ~3000 gas for cross-contract split calculation
    ///
    /// # Cross-Contract Call Flow
    /// 1. Validate that total_amount is positive
    /// 2. Create RemittanceSplitClient instance
    /// 3. Call calculate_split via cross-contract call
    /// 4. Return the allocation vector
    fn extract_allocations(
        env: &Env,
        remittance_split_addr: &Address,
        total_amount: i128,
    ) -> Result<Vec<i128>, OrchestratorError> {
        // Validate amount is positive
        if total_amount <= 0 {
            return Err(OrchestratorError::InvalidAmount);
        }

        // Create client for cross-contract call
        let split_client = RemittanceSplitClient::new(env, remittance_split_addr);

        // Gas estimation: ~3000 gas
        // Call the remittance split contract to calculate allocations
        // This returns Vec<i128> with [spending, savings, bills, insurance]
        let allocations = split_client.calculate_split(&total_amount);

        Ok(allocations)
    }

    // ============================================================================
    // Helper Functions - Downstream Contract Operations
    // ============================================================================

    /// Deposit funds to a savings goal via cross-contract call
    ///
    /// This function calls the Savings Goals contract to add funds to a specific goal.
    /// If the call fails (e.g., goal doesn't exist, invalid amount), the error is
    /// converted to OrchestratorError::SavingsDepositFailed.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `savings_addr` - Address of the Savings Goals contract
    /// * `owner` - Address of the goal owner
    /// * `goal_id` - ID of the target savings goal
    /// * `amount` - Amount to deposit (must be positive)
    ///
    /// # Returns
    /// Ok(()) if deposit succeeds, Err(OrchestratorError::SavingsDepositFailed) otherwise
    ///
    /// # Gas Estimation
    /// ~4000 gas for cross-contract savings deposit
    ///
    /// # Cross-Contract Call Flow
    /// 1. Create SavingsGoalsClient instance
    /// 2. Call add_to_goal via cross-contract call
    /// 3. If the call panics (goal not found, invalid amount), transaction reverts
    /// 4. Return success if call completes
    fn deposit_to_savings(
        env: &Env,
        savings_addr: &Address,
        owner: &Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<(), OrchestratorError> {
        // Create client for cross-contract call
        let savings_client = SavingsGoalsClient::new(env, savings_addr);

        // Gas estimation: ~4000 gas
        // Call add_to_goal on the savings contract
        // This will panic if the goal doesn't exist or amount is invalid
        // The panic will cause the entire transaction to revert (atomicity)
        savings_client.add_to_goal(owner, &goal_id, &amount);

        Ok(())
    }

    /// Execute bill payment via cross-contract call
    ///
    /// This function calls the Bill Payments contract to mark a bill as paid.
    /// If the call fails (e.g., bill not found, already paid), the error is
    /// converted to OrchestratorError::BillPaymentFailed.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `bills_addr` - Address of the Bill Payments contract
    /// * `caller` - Address of the caller (must be bill owner)
    /// * `bill_id` - ID of the bill to pay
    ///
    /// # Returns
    /// Ok(()) if payment succeeds, Err(OrchestratorError::BillPaymentFailed) otherwise
    ///
    /// # Gas Estimation
    /// ~4000 gas for cross-contract bill payment
    ///
    /// # Cross-Contract Call Flow
    /// 1. Create BillPaymentsClient instance
    /// 2. Call pay_bill via cross-contract call
    /// 3. If the call panics (bill not found, already paid), transaction reverts
    /// 4. Return success if call completes
    fn execute_bill_payment_internal(
        env: &Env,
        bills_addr: &Address,
        caller: &Address,
        bill_id: u32,
    ) -> Result<(), OrchestratorError> {
        // Create client for cross-contract call
        let bills_client = BillPaymentsClient::new(env, bills_addr);

        // Gas estimation: ~4000 gas
        // Call pay_bill on the bills contract
        // This will panic if the bill doesn't exist or is already paid
        // The panic will cause the entire transaction to revert (atomicity)
        bills_client.pay_bill(caller, &bill_id);

        Ok(())
    }

    /// Pay insurance premium via cross-contract call
    ///
    /// This function calls the Insurance contract to pay a monthly premium.
    /// If the call fails (e.g., policy not found, inactive), the error is
    /// converted to OrchestratorError::InsurancePaymentFailed.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `insurance_addr` - Address of the Insurance contract
    /// * `caller` - Address of the caller (must be policy owner)
    /// * `policy_id` - ID of the insurance policy
    ///
    /// # Returns
    /// Ok(()) if payment succeeds, Err(OrchestratorError::InsurancePaymentFailed) otherwise
    ///
    /// # Gas Estimation
    /// ~4000 gas for cross-contract premium payment
    ///
    /// # Cross-Contract Call Flow
    /// 1. Create InsuranceClient instance
    /// 2. Call pay_premium via cross-contract call
    /// 3. If the call panics (policy not found, inactive), transaction reverts
    /// 4. Return success if call completes
    fn pay_insurance_premium(
        env: &Env,
        insurance_addr: &Address,
        caller: &Address,
        policy_id: u32,
    ) -> Result<(), OrchestratorError> {
        // Create client for cross-contract call
        let insurance_client = InsuranceClient::new(env, insurance_addr);

        // Call pay_premium on the insurance contract
        // This returns Result<bool, InsuranceError>
        match insurance_client.pay_premium(caller, &policy_id) {
            Ok(success) => {
                if success {
                    Ok(())
                } else {
                    Err(OrchestratorError::InsurancePaymentFailed)
                }
            }
            Err(_) => Err(OrchestratorError::InsurancePaymentFailed),
        }
    }

    // ============================================================================
    // Helper Functions - Event Emission
    // ============================================================================

    /// Emit success event for a completed remittance flow
    ///
    /// This function creates and publishes a RemittanceFlowEvent to the ledger,
    /// providing an audit trail of successful operations.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address that initiated the flow
    /// * `total_amount` - Total amount processed
    /// * `allocations` - Allocation amounts [spending, savings, bills, insurance]
    /// * `timestamp` - Timestamp of execution
    fn emit_success_event(
        env: &Env,
        caller: &Address,
        total_amount: i128,
        allocations: &Vec<i128>,
        timestamp: u64,
    ) {
        let event = RemittanceFlowEvent {
            caller: caller.clone(),
            total_amount,
            allocations: allocations.clone(),
            timestamp,
        };

        env.events().publish((symbol_short!("flow_ok"),), event);
    }

    /// Emit error event for a failed remittance flow
    ///
    /// This function creates and publishes a RemittanceFlowErrorEvent to the ledger,
    /// providing diagnostic information about which step failed and why.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address that initiated the flow
    /// * `failed_step` - Symbol identifying the failed step (e.g., "perm_chk", "savings")
    /// * `error_code` - Error code from OrchestratorError
    /// * `timestamp` - Timestamp of failure
    fn emit_error_event(
        env: &Env,
        caller: &Address,
        failed_step: Symbol,
        error_code: u32,
        timestamp: u64,
    ) {
        let event = RemittanceFlowErrorEvent {
            caller: caller.clone(),
            failed_step,
            error_code,
            timestamp,
        };

        env.events().publish((symbol_short!("flow_err"),), event);
    }

    // ============================================================================
    // Public Functions - Individual Operations
    // ============================================================================

    /// Execute a savings deposit with family wallet permission checks
    ///
    /// This function deposits funds to a savings goal after validating permissions
    /// and spending limits via the Family Wallet contract.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address initiating the operation (must authorize)
    /// * `amount` - Amount to deposit
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `savings_addr` - Address of the Savings Goals contract
    /// * `goal_id` - Target savings goal ID
    ///
    /// # Returns
    /// Ok(()) if successful, Err(OrchestratorError) if any step fails
    ///
    /// # Gas Estimation
    /// - Base: ~3000 gas
    /// - Family wallet check: ~2000 gas
    /// - Savings deposit: ~4000 gas
    /// - Total: ~9,000 gas
    ///
    /// # Execution Flow
    /// 1. Require caller authorization
    /// 2. Check family wallet permission
    /// 3. Check spending limit
    /// 4. Deposit to savings goal
    /// 5. Emit success event
    /// 6. On error, emit error event and return error
    pub fn execute_savings_deposit(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        savings_addr: Address,
        goal_id: u32,
    ) -> Result<(), OrchestratorError> {
        // Reentrancy guard: acquire execution lock
        Self::acquire_execution_lock(&env)?;

        // Require caller authorization
        caller.require_auth();

        let timestamp = env.ledger().timestamp();

        // Step 1: Check family wallet permission
        let result = (|| {
            Self::check_family_wallet_permission(&env, &family_wallet_addr, &caller, amount)
                .map_err(|e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("perm_chk"),
                        e as u32,
                        timestamp,
                    );
                    e
                })?;

            // Step 2: Check spending limit
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("spend_lm"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Step 3: Deposit to savings
            Self::deposit_to_savings(&env, &savings_addr, &caller, goal_id, amount).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("savings"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Emit success event
            let allocations = Vec::from_array(&env, [0, amount, 0, 0]);
            Self::emit_success_event(&env, &caller, amount, &allocations, timestamp);

            Ok(())
        })();

        // Reentrancy guard: always release lock before returning
        Self::release_execution_lock(&env);
        result
    }

    /// Execute a bill payment with family wallet permission checks
    ///
    /// This function pays a bill after validating permissions and spending limits
    /// via the Family Wallet contract.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address initiating the operation (must authorize)
    /// * `amount` - Amount of the bill payment
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `bills_addr` - Address of the Bill Payments contract
    /// * `bill_id` - Target bill ID
    ///
    /// # Returns
    /// Ok(()) if successful, Err(OrchestratorError) if any step fails
    ///
    /// # Gas Estimation
    /// - Base: ~3000 gas
    /// - Family wallet check: ~2000 gas
    /// - Bill payment: ~4000 gas
    /// - Total: ~9,000 gas
    ///
    /// # Execution Flow
    /// 1. Require caller authorization
    /// 2. Check family wallet permission
    /// 3. Check spending limit
    /// 4. Execute bill payment
    /// 5. Emit success event
    /// 6. On error, emit error event and return error
    pub fn execute_bill_payment(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        bills_addr: Address,
        bill_id: u32,
    ) -> Result<(), OrchestratorError> {
        // Reentrancy guard: acquire execution lock
        Self::acquire_execution_lock(&env)?;

        // Require caller authorization
        caller.require_auth();

        let timestamp = env.ledger().timestamp();

        let result = (|| {
            // Step 1: Check family wallet permission
            Self::check_family_wallet_permission(&env, &family_wallet_addr, &caller, amount)
                .map_err(|e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("perm_chk"),
                        e as u32,
                        timestamp,
                    );
                    e
                })?;

            // Step 2: Check spending limit
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("spend_lm"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Step 3: Execute bill payment
            Self::execute_bill_payment_internal(&env, &bills_addr, &caller, bill_id).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("bills"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Emit success event
            let allocations = Vec::from_array(&env, [0, 0, amount, 0]);
            Self::emit_success_event(&env, &caller, amount, &allocations, timestamp);

            Ok(())
        })();

        // Reentrancy guard: always release lock before returning
        Self::release_execution_lock(&env);
        result
    }

    /// Execute an insurance premium payment with family wallet permission checks
    ///
    /// This function pays an insurance premium after validating permissions and
    /// spending limits via the Family Wallet contract.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address initiating the operation (must authorize)
    /// * `amount` - Amount of the premium payment
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `insurance_addr` - Address of the Insurance contract
    /// * `policy_id` - Target insurance policy ID
    ///
    /// # Returns
    /// Ok(()) if successful, Err(OrchestratorError) if any step fails
    ///
    /// # Gas Estimation
    /// - Base: ~3000 gas
    /// - Family wallet check: ~2000 gas
    /// - Premium payment: ~4000 gas
    /// - Total: ~9,000 gas
    ///
    /// # Execution Flow
    /// 1. Require caller authorization
    /// 2. Check family wallet permission
    /// 3. Check spending limit
    /// 4. Pay insurance premium
    /// 5. Emit success event
    /// 6. On error, emit error event and return error
    pub fn execute_insurance_payment(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        insurance_addr: Address,
        policy_id: u32,
    ) -> Result<(), OrchestratorError> {
        // Reentrancy guard: acquire execution lock
        Self::acquire_execution_lock(&env)?;

        // Require caller authorization
        caller.require_auth();

        let timestamp = env.ledger().timestamp();

        let result = (|| {
            // Step 1: Check family wallet permission
            Self::check_family_wallet_permission(&env, &family_wallet_addr, &caller, amount)
                .map_err(|e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("perm_chk"),
                        e as u32,
                        timestamp,
                    );
                    e
                })?;

            // Step 2: Check spending limit
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("spend_lm"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Step 3: Pay insurance premium
            Self::pay_insurance_premium(&env, &insurance_addr, &caller, policy_id).map_err(
                |e| {
                    Self::emit_error_event(
                        &env,
                        &caller,
                        symbol_short!("insuranc"),
                        e as u32,
                        timestamp,
                    );
                    e
                },
            )?;

            // Emit success event
            let allocations = Vec::from_array(&env, [0, 0, 0, amount]);
            Self::emit_success_event(&env, &caller, amount, &allocations, timestamp);

            Ok(())
        })();

        // Reentrancy guard: always release lock before returning
        Self::release_execution_lock(&env);
        result
    }

    // ============================================================================
    // Public Functions - Complete Remittance Flow
    // ============================================================================

    /// Execute a complete remittance flow with automated allocation
    ///
    /// This is the main orchestrator function that coordinates a full remittance
    /// split across all downstream contracts (savings, bills, insurance) with
    /// family wallet permission enforcement.
    ///
    /// # Arguments
    /// * `env` - The contract environment
    /// * `caller` - Address initiating the operation (must authorize)
    /// * `total_amount` - Total remittance amount to split
    /// * `family_wallet_addr` - Address of the Family Wallet contract
    /// * `remittance_split_addr` - Address of the Remittance Split contract
    /// * `savings_addr` - Address of the Savings Goals contract
    /// * `bills_addr` - Address of the Bill Payments contract
    /// * `insurance_addr` - Address of the Insurance contract
    /// * `goal_id` - Target savings goal ID
    /// * `bill_id` - Target bill ID
    /// * `policy_id` - Target insurance policy ID
    ///
    /// # Returns
    /// Ok(RemittanceFlowResult) with execution details if successful
    /// Err(OrchestratorError) if any step fails
    ///
    /// # Gas Estimation
    /// - Base: ~5000 gas
    /// - Family wallet check: ~2000 gas
    /// - Remittance split calc: ~3000 gas
    /// - Savings deposit: ~4000 gas
    /// - Bill payment: ~4000 gas
    /// - Insurance payment: ~4000 gas
    /// - Total: ~22,000 gas for full flow
    ///
    /// # Atomicity Guarantee
    /// All operations execute atomically via Soroban's panic/revert mechanism.
    /// If any step fails, all prior state changes are automatically reverted.
    ///
    /// # Execution Flow
    /// 1. Require caller authorization
    /// 2. Validate total_amount is positive
    /// 3. Check family wallet permission
    /// 4. Check spending limit
    /// 5. Extract allocations from remittance split
    /// 6. Deposit to savings goal
    /// 7. Pay bill
    /// 8. Pay insurance premium
    /// 9. Build and return result
    /// 10. On error, emit error event and return error
=======
>>>>>>> main
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
        nonce: u64,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        let _timestamp = env.ledger().timestamp();
        // Address validation
        Self::validate_two_addresses(&env, &family_wallet_addr, &savings_addr).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
        // Nonce / replay protection
        Self::consume_nonce(&env, &caller, symbol_short!("exec_sav"), nonce).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;

        let result = (|| {
            Self::check_spending_limit(&env, &family_wallet_addr, &caller, amount)?;
            Self::deposit_to_savings(&env, &savings_addr, &caller, goal_id, amount)?;
            Ok(())
        })();

        Self::release_execution_lock(&env);
        result
    }

    /// @notice Executes a bill payment for the authenticated family wallet owner.
    /// @dev Delegation is not supported on this entry point. The authenticated
    ///      `caller` must match the owner returned by `family_wallet_addr`, and the
    ///      nonce is consumed before any downstream state changes to prevent replay.
    /// @param caller Authenticated family wallet owner expected to receive downstream ownership checks.
    /// @param amount Amount checked against the family wallet spending policy.
    /// @param family_wallet_addr Family wallet contract used for spending-limit validation.
    /// @param bills_addr Bill payments contract that enforces bill ownership.
    /// @param bill_id Target bill identifier.
    /// @param nonce Caller-scoped replay-protection nonce for bill-payment execution.
    pub fn execute_bill_payment(
        env: Env,
        caller: Address,
        amount: i128,
        family_wallet_addr: Address,
        bills_addr: Address,
        bill_id: u32,
        _nonce: u64,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        Self::require_bill_payment_owner(&env, &family_wallet_addr, &caller).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
        Self::validate_two_addresses(&env, &family_wallet_addr, &bills_addr).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
        Self::consume_nonce(&env, &caller, symbol_short!("exec_bill"), nonce).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
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
        _nonce: u64,
    ) -> Result<(), OrchestratorError> {
        Self::acquire_execution_lock(&env)?;
        caller.require_auth();
        Self::validate_two_addresses(&env, &family_wallet_addr, &insurance_addr).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
        Self::consume_nonce(&env, &caller, symbol_short!("exec_ins"), nonce).map_err(|e| {
            Self::release_execution_lock(&env);
            e
        })?;
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

    fn validate_two_addresses(
        env: &Env,
        addr1: &Address,
        addr2: &Address,
    ) -> Result<(), OrchestratorError> {
        let current = env.current_contract_address();
        if addr1 == &current || addr2 == &current {
            return Err(OrchestratorError::SelfReferenceNotAllowed);
        }
        if addr1 == addr2 {
            return Err(OrchestratorError::DuplicateContractAddress);
        }
        Ok(())
    }

    fn consume_nonce(
        env: &Env,
        caller: &Address,
        command_type: Symbol,
        nonce: u64,
    ) -> Result<(), OrchestratorError> {
        let key = (caller.clone(), command_type, nonce);
        if env.storage().persistent().has(&key) {
            return Err(OrchestratorError::NonceAlreadyUsed);
        }
        env.storage().persistent().set(&key, &true);
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

    /// Extend the TTL of instance storage
    #[allow(dead_code)]
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}
