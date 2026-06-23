#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec, Address, Env, Map,
    Symbol, Vec,
};

#[allow(dead_code)]
mod interface {
    use soroban_sdk::{contractclient, Address, Env, Vec};

    #[contractclient(name = "FamilyWalletClient")]
    pub trait FamilyWalletInterface {
        fn check_spending_limit(env: Env, user: Address, amount: i128) -> bool;
    }

    #[contractclient(name = "RemittanceSplitClient")]
    pub trait RemittanceSplitInterface {
        fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>;
    }

    #[contractclient(name = "SavingsGoalsClient")]
    pub trait SavingsGoalsInterface {
        fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> i128;
    }

    #[contractclient(name = "BillPaymentsClient")]
    pub trait BillPaymentsInterface {
        fn pay_bill(env: Env, caller: Address, bill_id: u32);
    }

    #[contractclient(name = "InsuranceClient")]
    pub trait InsuranceInterface {
        fn pay_premium(env: Env, caller: Address, policy_id: u32) -> bool;
    }
}

#[contracttype]
#[derive(Clone)]
pub struct OrchestratorAuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub timestamp: u64,
    pub success: bool,
}

use remitwise_common::{EventCategory, EventPriority, RemitwiseEvents, CONTRACT_VERSION};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;

// Maximum number of used nonces tracked per address before the oldest are pruned.
const MAX_USED_NONCES_PER_ADDR: u32 = 256;
/// Maximum ledger seconds a signed request may remain valid after creation.
const MAX_DEADLINE_WINDOW_SECS: u64 = 3600; // 1 hour

/// Maximum number of audit entries retained in the ring-buffer.
/// When the log reaches this cap the oldest entry is evicted to bound
/// instance-storage rent and read cost.
const MAX_AUDIT_ENTRIES: u32 = 100;

/// A single entry in the bounded audit ring-buffer.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct AuditEntry {
    pub operation: Symbol,
    pub executor: Address,
    pub timestamp: u64,
    pub success: bool,
}

const EXEC_LOCK: Symbol = symbol_short!("EXEC_LOCK");
const AUDIT: Symbol = symbol_short!("AUDIT");

/// RAII guard to ensure the execution lock is released on drop.
pub struct LockGuard {
    env: Env,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.env.storage().instance().set(&EXEC_LOCK, &false);
    }
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionStats {
    pub total_executions: u32,
    pub successful_executions: u32,
    pub failed_executions: u32,
    pub last_execution_time: u64,
    /// Total audit entries evicted due to ring-buffer cap enforcement.
    pub evicted_entries: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct RemittanceFlowParams {
    pub caller: Address,
    pub total_amount: i128,
    pub family_wallet: Address,
    pub remittance_split: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
    pub goal_id: u32,
    pub bill_id: u32,
    pub policy_id: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OrchestratorError {
    Unauthorized = 1,
    InvalidAmount = 2,
    Overflow = 3,
    CrossContractCallFailed = 4,
    NonceAlreadyUsed = 5,
    InvalidNonce = 6,
    DeadlineExpired = 7,
    ExecutionLocked = 8,
    InvalidDependency = 9,
    DuplicateDependency = 10,
}

#[contract]
pub struct Orchestrator;

#[contractimpl]
impl Orchestrator {
    /// Executes the full remittance flow across multiple contracts.
    /// This is protected against reentrancy.
    pub fn execute_remittance_flow(
        env: Env,
        params: RemittanceFlowParams,
    ) -> Result<(), OrchestratorError> {
        params.caller.require_auth();

        if params.total_amount <= 0 {
            return Err(OrchestratorError::InvalidAmount);
        }

        // Use a scope to ensure the guard is dropped (and lock released)
        // before we audit and return.
        let result = {
            // The guard acquires the lock on creation and releases it on drop.
            // This ensures the lock is released even if we return early via `?`.
            let _guard = Self::acquire_execution_lock(&env)?;

            Self::perform_remittance_flow(&env, &params)
        };

        // 4. Audit result (lock is already released here)
        Self::append_audit(&env, symbol_short!("remit"), &params.caller, result.is_ok());

        result
    }

    fn perform_remittance_flow(
        env: &Env,
        params: &RemittanceFlowParams,
    ) -> Result<(), OrchestratorError> {
        // Use interfaces to call downstream contracts
        // This is a simplified implementation of the flow logic
        // 1. Check permission/spending limit
        let fw_client = interface::FamilyWalletClient::new(env, &params.family_wallet);
        if !fw_client.check_spending_limit(&params.caller, &params.total_amount) {
            return Err(OrchestratorError::Unauthorized);
        }

        // 2. Calculate split
        let rs_client = interface::RemittanceSplitClient::new(env, &params.remittance_split);
        let allocations = rs_client.calculate_split(&params.total_amount);

        if allocations.len() < 4 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let _spending_amt = allocations.get_unchecked(0);
        let savings_amt = allocations.get_unchecked(1);
        let bills_amt = allocations.get_unchecked(2);
        let insurance_amt = allocations.get_unchecked(3);

        // 3. Downstream calls
        if savings_amt > 0 {
            let s_client = interface::SavingsGoalsClient::new(env, &params.savings);
            s_client.add_to_goal(&params.caller, &params.goal_id, &savings_amt);
        }

        if bills_amt > 0 {
            let b_client = interface::BillPaymentsClient::new(env, &params.bills);
            b_client.pay_bill(&params.caller, &params.bill_id);
        }

        if insurance_amt > 0 {
            let i_client = interface::InsuranceClient::new(env, &params.insurance);
            i_client.pay_premium(&params.caller, &params.policy_id);
        }

        Ok(())
    }

    /// Initialize the orchestrator with dependency contract addresses.
    ///
    /// # Errors
    /// - `Unauthorized` if already initialized or caller not authorized
    /// - `DuplicateDependency` if any addresses are duplicates or self-reference
    pub fn init(
        env: Env,
        caller: Address,
        family_wallet: Address,
        remittance_split: Address,
        savings_goals: Address,
        bill_payments: Address,
        insurance: Address,
    ) -> Result<bool, OrchestratorError> {
        caller.require_auth();

        let existing: Option<Address> = env.storage().instance().get(&symbol_short!("OWNER"));
        if existing.is_some() {
            return Err(OrchestratorError::Unauthorized);
        }

        // Validate no duplicates and no self-reference
        let addresses = vec![
            &env,
            family_wallet.clone(),
            remittance_split.clone(),
            savings_goals.clone(),
            bill_payments.clone(),
            insurance.clone(),
        ];

        for i in 0..addresses.len() {
            if let Some(addr_i) = addresses.get(i) {
                if addr_i == caller {
                    return Err(OrchestratorError::DuplicateDependency);
                }
                for j in (i + 1)..addresses.len() {
                    if let Some(addr_j) = addresses.get(j) {
                        if addr_i == addr_j {
                            return Err(OrchestratorError::DuplicateDependency);
                        }
                    }
                }
            }
        }

        Self::extend_instance_ttl(&env);

        env.storage()
            .instance()
            .set(&symbol_short!("OWNER"), &caller);
        env.storage()
            .instance()
            .set(&symbol_short!("FW_ADDR"), &family_wallet);
        env.storage()
            .instance()
            .set(&symbol_short!("RS_ADDR"), &remittance_split);
        env.storage()
            .instance()
            .set(&symbol_short!("SG_ADDR"), &savings_goals);
        env.storage()
            .instance()
            .set(&symbol_short!("BP_ADDR"), &bill_payments);
        env.storage()
            .instance()
            .set(&symbol_short!("INS_ADDR"), &insurance);
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &false);
        env.storage()
            .instance()
            .set(&symbol_short!("NONCES"), &Map::<Address, u64>::new(&env));

        let stats = ExecutionStats {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            last_execution_time: 0,
            evicted_entries: 0,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("STATS"), &stats);

        // Emit orchestrator initialization event
        // Topic: ("Remitwise", EventCategory::System, EventPriority::High, "init_ok")
        // Payload: (caller: Address)
        // Emitted when the orchestrator contract is successfully initialized
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("init_ok"),
            caller,
        );

        Ok(true)
    }

    /// Execute a remittance flow with replay protection.
    ///
    /// # Security
    /// - Authorization-first pattern
    /// - Execution lock to prevent cross-contract reentrancy
    /// - Nonce replay protection with deadline window validation
    /// - Request hash binding to prevent parameter-swap attacks
    ///
    /// # Errors
    /// - `Unauthorized` if executor doesn't authorize or contract not initialized
    /// - `InvalidAmount` if amount <= 0
    /// - `DeadlineExpired` if deadline is invalid or passed
    /// - `InvalidNonce` if nonce or hash is invalid
    /// - `NonceAlreadyUsed` if nonce was already used
    /// - `ExecutionLocked` if reentrancy detected
    pub fn execute_remittance_flow_signed(
        env: Env,
        executor: Address,
        amount: i128,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
    ) -> Result<bool, OrchestratorError> {
        // 1. Authorization first — before any storage reads
        executor.require_auth();

        // 2. Validate initialization
        let _owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;

        // 3. Check amount validity
        if amount <= 0 {
            Self::append_audit(&env, symbol_short!("flow_exec"), &executor, false);
            return Err(OrchestratorError::InvalidAmount);
        }

        // 4. Reentrancy guard: check execution lock
        let is_locked: bool = env
            .storage()
            .instance()
            .get(&symbol_short!("EXEC_LOCK"))
            .unwrap_or(false);

        if is_locked {
            Self::append_audit(&env, symbol_short!("flow_exec"), &executor, false);
            return Err(OrchestratorError::ExecutionLocked);
        }

        // 5. Hardened nonce validation with deadline + hash binding
        let expected_hash = Self::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            nonce,
            amount,
            deadline,
        );
        Self::require_nonce_hardened(
            &env,
            &executor,
            nonce,
            deadline,
            request_hash,
            expected_hash,
        )?;

        // Emit flow lifecycle event - flow started
        // Topic: ("Remitwise", EventCategory::Transaction, EventPriority::High, "flow")
        // Payload: (executor: Address, amount: i128)
        // Emitted when a remittance flow execution begins after passing validation
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("flow"),
            (executor.clone(), amount),
        );

        // 6. Set execution lock
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &true);

        // 7. Execute remittance flow
        let result = Self::execute_flow_internal(&env, &executor, amount);

        // 8. Clear execution lock
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &false);

        // 9. On success: advance nonce, update stats, record audit, emit event
        match result {
            Ok(_) => {
                Self::increment_nonce(&env, &executor)?;
                Self::update_execution_stats(&env, true);
                Self::append_audit(&env, symbol_short!("flow_exec"), &executor, true);

                // Emit flow lifecycle event - flow completed successfully
                // Topic: ("Remitwise", EventCategory::Transaction, EventPriority::High, "flow_ok")
                // Payload: (executor: Address, amount: i128)
                // Emitted when a remittance flow completes successfully
                RemitwiseEvents::emit(
                    &env,
                    EventCategory::Transaction,
                    EventPriority::High,
                    symbol_short!("flow_ok"),
                    (executor, amount),
                );

                Ok(true)
            }
            Err(e) => {
                Self::update_execution_stats(&env, false);
                Self::append_audit(&env, symbol_short!("flow_exec"), &executor, false);

                // Emit flow lifecycle event - flow failed
                // Topic: ("Remitwise", EventCategory::Transaction, EventPriority::High, "flow_fail")
                // Payload: (executor: Address, error_code: u32)
                // Emitted when a remittance flow fails. Error code corresponds to OrchestratorError enum.
                // Does not leak sensitive amounts - only includes error code for debugging.
                RemitwiseEvents::emit(
                    &env,
                    EventCategory::Transaction,
                    EventPriority::High,
                    symbol_short!("flow_fail"),
                    (executor, e as u32),
                );

                Err(e)
            }
        }
    }

    /// Get the current execution nonce for an address.
    pub fn get_nonce(env: Env, address: Address) -> u64 {
        Self::get_nonce_value(&env, &address)
    }

    /// Get current execution statistics, including evicted audit entry count.
    pub fn get_execution_stats(env: Env) -> Option<ExecutionStats> {
        Self::extend_instance_ttl(&env);
        env.storage().instance().get(&symbol_short!("STATS"))
    }

    /// Get a page of audit log entries.
    ///
    /// # Parameters
    /// - `from_index`: zero-based cursor into the current bounded window (oldest = 0)
    /// - `limit`: entries to return; clamped to `[1, MAX_AUDIT_ENTRIES]`; 0 → default 20
    ///
    /// # Retention note
    /// The log is a ring-buffer capped at `MAX_AUDIT_ENTRIES`. Entries are ordered
    /// oldest-to-newest within the current window. Callers should treat `from_index`
    /// as a position in the rotated window, not a global immutable ID.
    ///
    /// # Returns
    /// Empty vec when `from_index` is past the end of the log (safe default).
    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<AuditEntry> {
        let log: Option<Vec<AuditEntry>> = env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();

        // Clamp limit to [1, MAX_AUDIT_ENTRIES]; 0 → default 20
        let cap = Self::clamp_limit(limit);

        // Out-of-range cursor → empty page (safe default)
        if from_index >= len {
            return Vec::new(&env);
        }

        let end = from_index.saturating_add(cap).min(len);
        let mut items = Vec::new(&env);
        for i in from_index..end {
            if let Some(entry) = log.get(i) {
                items.push_back(entry);
            }
        }

        items
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn set_version(
        env: Env,
        caller: Address,
        new_version: u32,
    ) -> Result<bool, OrchestratorError> {
        caller.require_auth();

        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;

        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }

        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);

        // Emit orchestrator upgrade event
        // Topic: ("orch", "upgraded")
        // Payload: (previous_version: u32, new_version: u32)
        // Emitted when the contract version is upgraded by the owner
        env.events().publish(
            (symbol_short!("orch"), symbol_short!("upgraded")),
            (prev, new_version),
        );

        Ok(true)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn execute_flow_internal(
        env: &Env,
        _executor: &Address,
        _amount: i128,
    ) -> Result<bool, OrchestratorError> {
        let _owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        Ok(true)
    }

    fn get_nonce_value(env: &Env, address: &Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces
            .as_ref()
            .and_then(|m: &Map<Address, u64>| m.get(address.clone()))
            .unwrap_or(0)
    }

    fn require_nonce(env: &Env, address: &Address, expected: u64) -> Result<(), OrchestratorError> {
        let current = Self::get_nonce_value(env, address);
        if expected != current {
            return Err(OrchestratorError::InvalidNonce);
        }
        Ok(())
    }

    /// Hardened nonce validation:
    /// 1. Deadline must be in the future and within `MAX_DEADLINE_WINDOW_SECS`
    /// 2. Used-nonce double-spend check
    /// 3. Sequential counter check
    /// 4. Request hash binding
    fn require_nonce_hardened(
        env: &Env,
        address: &Address,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
        expected_hash: u64,
    ) -> Result<(), OrchestratorError> {
        let now = env.ledger().timestamp();

        if deadline <= now {
            return Err(OrchestratorError::DeadlineExpired);
        }
        if deadline > now + MAX_DEADLINE_WINDOW_SECS {
            return Err(OrchestratorError::DeadlineExpired);
        }

        if Self::is_nonce_used(env, address, nonce) {
            return Err(OrchestratorError::NonceAlreadyUsed);
        }

        Self::require_nonce(env, address, nonce)?;

        if request_hash != expected_hash {
            return Err(OrchestratorError::InvalidNonce);
        }

        Ok(())
    }

    fn acquire_execution_lock(env: &Env) -> Result<LockGuard, OrchestratorError> {
        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            return Err(OrchestratorError::ExecutionLocked);
        }
        env.storage().instance().set(&EXEC_LOCK, &true);
        Ok(LockGuard { env: env.clone() })
    }

    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
        let timestamp = env.ledger().timestamp();
        let mut log: Vec<AuditEntry> = env
            .storage()
            .instance()
            .get(&AUDIT)
            .unwrap_or_else(|| Vec::new(env));
        if log.len() >= MAX_AUDIT_ENTRIES {
            let mut new_log = Vec::new(env);
            for i in 1..log.len() {
                if let Some(entry) = log.get(i) {
                    new_log.push_back(entry);
                }
            }
            log = new_log;
            // Track eviction in stats
            let mut stats: ExecutionStats = env
                .storage()
                .instance()
                .get(&symbol_short!("STATS"))
                .unwrap_or(ExecutionStats {
                    total_executions: 0,
                    successful_executions: 0,
                    failed_executions: 0,
                    last_execution_time: 0,
                    evicted_entries: 0,
                });
            stats.evicted_entries = stats.evicted_entries.saturating_add(1);
            env.storage()
                .instance()
                .set(&symbol_short!("STATS"), &stats);
        }
        log.push_back(AuditEntry {
            operation,
            executor: caller.clone(),
            timestamp,
            success,
        });
        env.storage().instance().set(&AUDIT, &log);
    }

    pub fn get_execution_state(env: Env) -> bool {
        env.storage().instance().get(&EXEC_LOCK).unwrap_or(false)
    }

    fn is_nonce_used(env: &Env, address: &Address, nonce: u64) -> bool {
        let key = symbol_short!("USED_N");
        let map: Option<Map<Address, Vec<u64>>> = env.storage().instance().get(&key);
        match map {
            None => false,
            Some(m) => match m.get(address.clone()) {
                None => false,
                Some(used) => used.contains(nonce),
            },
        }
    }

    fn mark_nonce_used(env: &Env, address: &Address, nonce: u64) {
        let key = symbol_short!("USED_N");
        let mut map: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Map::new(env));

        let mut used: Vec<u64> = map.get(address.clone()).unwrap_or_else(|| Vec::new(env));

        if used.len() >= MAX_USED_NONCES_PER_ADDR {
            let mut trimmed = Vec::new(env);
            for i in 1..used.len() {
                if let Some(v) = used.get(i) {
                    trimmed.push_back(v);
                }
            }
            used = trimmed;
        }

        used.push_back(nonce);
        map.set(address.clone(), used);
        env.storage().instance().set(&key, &map);
    }

    fn increment_nonce(env: &Env, address: &Address) -> Result<(), OrchestratorError> {
        let current = Self::get_nonce_value(env, address);
        Self::mark_nonce_used(env, address, current);

        let next = current.checked_add(1).ok_or(OrchestratorError::Overflow)?;
        let mut nonces: Map<Address, u64> = env
            .storage()
            .instance()
            .get(&symbol_short!("NONCES"))
            .unwrap_or_else(|| Map::new(env));
        nonces.set(address.clone(), next);
        env.storage()
            .instance()
            .set(&symbol_short!("NONCES"), &nonces);
        Ok(())
    }

    fn compute_request_hash(
        operation: Symbol,
        _caller: Address,
        nonce: u64,
        amount: i128,
        deadline: u64,
    ) -> u64 {
        let op_bits: u64 = operation.to_val().get_payload();
        let amt_lo = amount as u64;
        let amt_hi = (amount >> 64) as u64;

        op_bits
            .wrapping_add(nonce)
            .wrapping_add(amt_lo)
            .wrapping_add(amt_hi)
            .wrapping_add(deadline)
            .wrapping_mul(1_000_000_007)
    }

    fn update_execution_stats(env: &Env, success: bool) {
        let mut stats: ExecutionStats = env
            .storage()
            .instance()
            .get(&symbol_short!("STATS"))
            .unwrap_or(ExecutionStats {
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                last_execution_time: 0,
                evicted_entries: 0,
            });

        stats.total_executions = stats.total_executions.saturating_add(1);
        if success {
            stats.successful_executions = stats.successful_executions.saturating_add(1);
        } else {
            stats.failed_executions = stats.failed_executions.saturating_add(1);
        }
        stats.last_execution_time = env.ledger().timestamp();

        env.storage()
            .instance()
            .set(&symbol_short!("STATS"), &stats);
    }

    /// Clamp pagination limit: 0 → 20 (default), >MAX_AUDIT_ENTRIES → MAX_AUDIT_ENTRIES.
    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            20
        } else if limit > MAX_AUDIT_ENTRIES {
            MAX_AUDIT_ENTRIES
        } else {
            limit
        }
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod tests_nonce_eviction {
    use super::*;
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Ledger as _},
        Address, Env,
    };

    const BASE_TIME: u64 = 1_000;
    const FLOW_AMOUNT: i128 = 1_000;

    struct SignedFlowHarness {
        env: Env,
        contract_id: Address,
    }

    fn setup_signed_flow() -> SignedFlowHarness {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_unlimited();
        env.ledger().set_timestamp(BASE_TIME);

        let contract_id = env.register_contract(None, Orchestrator);
        let client = OrchestratorClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.init(
            &owner,
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
        );

        SignedFlowHarness { env, contract_id }
    }

    fn client(harness: &SignedFlowHarness) -> OrchestratorClient<'_> {
        OrchestratorClient::new(&harness.env, &harness.contract_id)
    }

    fn valid_deadline() -> u64 {
        BASE_TIME + MAX_DEADLINE_WINDOW_SECS
    }

    fn request_hash(executor: &Address, amount: i128, nonce: u64, deadline: u64) -> u64 {
        Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            nonce,
            amount,
            deadline,
        )
    }

    fn execute_signed_flow(
        client: &OrchestratorClient,
        executor: &Address,
        amount: i128,
        nonce: u64,
        deadline: u64,
    ) {
        let hash = request_hash(executor, amount, nonce, deadline);
        assert!(client.execute_remittance_flow_signed(executor, &amount, &nonce, &deadline, &hash));
    }

    #[test]
    fn used_nonce_set_rejects_current_nonce_before_hash_binding() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);
        let nonce = 0;
        let deadline = valid_deadline();
        let hash = request_hash(&executor, FLOW_AMOUNT, nonce, deadline);

        let replay = harness.env.as_contract(&harness.contract_id, || {
            Orchestrator::mark_nonce_used(&harness.env, &executor, nonce);
            Orchestrator::require_nonce_hardened(
                &harness.env,
                &executor,
                nonce,
                deadline,
                hash,
                hash,
            )
        });
        assert_eq!(replay, Err(OrchestratorError::NonceAlreadyUsed));
        assert_eq!(client.get_nonce(&executor), 0);
    }

    #[test]
    fn signed_flow_replay_uses_used_set_and_old_nonce_uses_sequential_counter() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);
        let deadline = valid_deadline();

        execute_signed_flow(&client, &executor, FLOW_AMOUNT, 0, deadline);
        assert_eq!(client.get_nonce(&executor), 1);

        let replay_hash = request_hash(&executor, FLOW_AMOUNT, 0, deadline);
        let replay = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &deadline,
            &replay_hash,
        );
        assert_eq!(replay, Err(Ok(OrchestratorError::NonceAlreadyUsed)));

        let skipped_hash = request_hash(&executor, FLOW_AMOUNT, 3, deadline);
        let skipped = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &3,
            &deadline,
            &skipped_hash,
        );
        assert_eq!(skipped, Err(Ok(OrchestratorError::InvalidNonce)));
        assert_eq!(client.get_nonce(&executor), 1);
    }

    #[test]
    fn used_nonce_eviction_keeps_stale_replay_closed() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);
        let independent_executor = Address::generate(&harness.env);
        let deadline = valid_deadline();

        for nonce in 0..u64::from(MAX_USED_NONCES_PER_ADDR) {
            execute_signed_flow(&client, &executor, FLOW_AMOUNT, nonce, deadline);
        }

        let cap_nonce = u64::from(MAX_USED_NONCES_PER_ADDR);
        assert_eq!(client.get_nonce(&executor), cap_nonce);

        let oldest_before_eviction_hash = request_hash(&executor, FLOW_AMOUNT, 0, deadline);
        let oldest_before_eviction_replay = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &deadline,
            &oldest_before_eviction_hash,
        );
        assert_eq!(
            oldest_before_eviction_replay,
            Err(Ok(OrchestratorError::NonceAlreadyUsed))
        );

        execute_signed_flow(&client, &executor, FLOW_AMOUNT, cap_nonce, deadline);

        let next_nonce = u64::from(MAX_USED_NONCES_PER_ADDR) + 1;
        assert_eq!(client.get_nonce(&executor), next_nonce);

        let evicted_nonce_hash = request_hash(&executor, FLOW_AMOUNT, 0, deadline);
        let evicted_nonce_replay = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &deadline,
            &evicted_nonce_hash,
        );
        assert_eq!(
            evicted_nonce_replay,
            Err(Ok(OrchestratorError::InvalidNonce))
        );
        assert_eq!(client.get_nonce(&executor), next_nonce);

        execute_signed_flow(&client, &independent_executor, FLOW_AMOUNT, 0, deadline);
        assert_eq!(client.get_nonce(&independent_executor), 1);
    }

    #[test]
    fn deadline_window_rejections_do_not_consume_nonce() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);

        let expired_deadline = BASE_TIME;
        let expired_hash = request_hash(&executor, FLOW_AMOUNT, 0, expired_deadline);
        let expired = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &expired_deadline,
            &expired_hash,
        );
        assert_eq!(expired, Err(Ok(OrchestratorError::DeadlineExpired)));
        assert_eq!(client.get_nonce(&executor), 0);

        let beyond_window_deadline = BASE_TIME + MAX_DEADLINE_WINDOW_SECS + 1;
        let beyond_window_hash = request_hash(&executor, FLOW_AMOUNT, 0, beyond_window_deadline);
        let beyond_window = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &beyond_window_deadline,
            &beyond_window_hash,
        );
        assert_eq!(beyond_window, Err(Ok(OrchestratorError::DeadlineExpired)));
        assert_eq!(client.get_nonce(&executor), 0);

        execute_signed_flow(&client, &executor, FLOW_AMOUNT, 0, valid_deadline());
        assert_eq!(client.get_nonce(&executor), 1);
    }

    #[test]
    fn request_hash_binding_rejects_parameter_swap_without_consuming_nonce() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);
        let nonce = 0;
        let deadline = valid_deadline();
        let original_hash = request_hash(&executor, FLOW_AMOUNT, nonce, deadline);
        let swapped_amount = FLOW_AMOUNT + 1;

        let swapped = client.try_execute_remittance_flow_signed(
            &executor,
            &swapped_amount,
            &nonce,
            &deadline,
            &original_hash,
        );
        assert_eq!(swapped, Err(Ok(OrchestratorError::InvalidNonce)));
        assert_eq!(client.get_nonce(&executor), 0);

        execute_signed_flow(&client, &executor, FLOW_AMOUNT, nonce, deadline);
        assert_eq!(client.get_nonce(&executor), 1);
    }
}

#[cfg(test)]
#[path = "test.rs"]
mod test;
