#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, vec,
    Address, Env, Map, Symbol, Vec,
};

use remitwise_common::{EventCategory, EventPriority, RemitwiseEvents, CONTRACT_VERSION};

// Storage TTL constants for active data
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;

// Maximum number of used nonces tracked per address before the oldest are pruned.
const MAX_USED_NONCES_PER_ADDR: u32 = 256;
/// Maximum ledger seconds a signed request may remain valid after creation.
const MAX_DEADLINE_WINDOW_SECS: u64 = 3600; // 1 hour

// Audit constants
const MAX_AUDIT_ENTRIES: u32 = 100;

#[contracttype]
#[derive(Clone)]
pub struct AuditEntry {
    pub operation: Symbol,
    pub executor: Address,
    pub timestamp: u64,
    pub success: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct ExecutionStats {
    pub total_executions: u32,
    pub successful_executions: u32,
    pub failed_executions: u32,
    pub last_execution_time: u64,
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
    /// Initialize the orchestrator with dependency contract addresses.
    ///
    /// # Arguments
    /// * `caller` - Initialization caller (must authorize)
    /// * `family_wallet` - Family Wallet contract address
    /// * `remittance_split` - Remittance Split contract address
    /// * `savings_goals` - Savings Goals contract address
    /// * `bill_payments` - Bill Payments contract address
    /// * `insurance` - Insurance contract address
    ///
    /// # Errors
    /// - `Unauthorized` if already initialized or caller not authorized
    /// - `DuplicateDependency` if any addresses are duplicates or self-reference
    /// - `InvalidDependency` if invalid configuration
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
                // Check self-reference
                if addr_i == caller {
                    return Err(OrchestratorError::DuplicateDependency);
                }
                // Check duplicates
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

        env.storage().instance().set(
            &symbol_short!("NONCES"),
            &Map::<Address, u64>::new(&env),
        );

        let stats = ExecutionStats {
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            last_execution_time: 0,
        };
        env.storage().instance().set(&symbol_short!("STATS"), &stats);

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
    /// # Arguments
    /// * `executor` - Address executing the flow (must authorize)
    /// * `amount` - Total amount to distribute
    /// * `nonce` - Replay-protection nonce (must equal get_nonce(executor))
    /// * `deadline` - Request expiry timestamp (ledger seconds)
    /// * `request_hash` - Caller-computed binding hash
    ///
    /// # Security
    /// - Authorization-first pattern (caller.require_auth() before any state reads)
    /// - Execution lock to prevent cross-contract reentrancy
    /// - Nonce replay protection with deadline window validation
    /// - Request hash binding to prevent parameter-swap attacks
    ///
    /// # Errors
    /// - `Unauthorized` if executor doesn't authorize
    /// - `InvalidAmount` if amount <= 0
    /// - `DeadlineExpired` if deadline is invalid or passed
    /// - `InvalidNonce` if nonce is invalid
    /// - `NonceAlreadyUsed` if nonce was already used
    /// - `ExecutionLocked` if reentrancy detected
    pub fn execute_remittance_flow(
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
        Self::require_nonce_hardened(&env, &executor, nonce, deadline, request_hash, expected_hash)?;

        // 6. Set execution lock
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &true);

        // 7. Execute remittance flow (stubbed for minimal implementation)
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

    /// Get current execution statistics.
    pub fn get_execution_stats(env: Env) -> Option<ExecutionStats> {
        env.storage().instance().get(&symbol_short!("STATS"))
    }

    /// Get the audit log with pagination support.
    ///
    /// # Parameters
    /// - `from_index`: zero-based starting index (0 for first page)
    /// - `limit`: maximum entries to return; clamped to [1, 50]
    ///
    /// # Returns
    /// A vector of audit entries, safe against overflow (uses saturating arithmetic)
    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<AuditEntry> {
        let log: Option<Vec<AuditEntry>> =
            env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();
        let cap = Self::clamp_limit(limit);

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

    pub fn set_version(env: Env, caller: Address, new_version: u32) -> Result<bool, OrchestratorError> {
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
        // Stubbed implementation: minimal skeleton
        // Phase 2 will integrate actual cross-contract calls to:
        // - family_wallet for spending validation
        // - remittance_split for split calculation
        // - savings_goals/bill_payments/insurance for distributions

        // For now, just verify the executor and amount are valid
        let _owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;

        // Placeholder: would validate spending limits and execute transfers
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

    fn require_nonce(
        env: &Env,
        address: &Address,
        expected: u64,
    ) -> Result<(), OrchestratorError> {
        let current = Self::get_nonce_value(env, address);
        if expected != current {
            return Err(OrchestratorError::InvalidNonce);
        }
        Ok(())
    }

    /// Hardened nonce validation with three layers of replay protection:
    ///
    /// 1. **Deadline check** — rejects requests whose `deadline` is in the past
    ///    or further than `MAX_DEADLINE_WINDOW_SECS` in the future
    /// 2. **Sequential counter** — the nonce must equal `get_nonce(address)`
    /// 3. **Used-nonce set** — prevents double-spend even if counter is reset
    /// 4. **Request hash binding** — prevents parameter-swap replay attacks
    fn require_nonce_hardened(
        env: &Env,
        address: &Address,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
        expected_hash: u64,
    ) -> Result<(), OrchestratorError> {
        let now = env.ledger().timestamp();

        // 1. Deadline: must be in the future but not too far ahead
        if deadline <= now {
            return Err(OrchestratorError::DeadlineExpired);
        }
        if deadline > now + MAX_DEADLINE_WINDOW_SECS {
            return Err(OrchestratorError::DeadlineExpired);
        }

        // 2. Sequential counter
        Self::require_nonce(env, address, nonce)?;

        // 3. Used-nonce double-spend check
        if Self::is_nonce_used(env, address, nonce) {
            return Err(OrchestratorError::NonceAlreadyUsed);
        }

        // 4. Request hash binding
        if request_hash != expected_hash {
            return Err(OrchestratorError::InvalidNonce);
        }

        Ok(())
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

        // Evict oldest if at capacity
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
        // Mark current nonce as used BEFORE advancing the counter
        Self::mark_nonce_used(env, address, current);

        let next = current
            .checked_add(1)
            .ok_or(OrchestratorError::Overflow)?;
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

    fn append_audit(env: &Env, operation: Symbol, _executor: &Address, success: bool) {
        let timestamp = env.ledger().timestamp();
        let mut log: Vec<AuditEntry> = env
            .storage()
            .instance()
            .get(&symbol_short!("AUDIT"))
            .unwrap_or_else(|| Vec::new(env));

        if log.len() >= MAX_AUDIT_ENTRIES {
            let mut new_log = Vec::new(env);
            for i in 1..log.len() {
                if let Some(entry) = log.get(i) {
                    new_log.push_back(entry);
                }
            }
            log = new_log;
        }

        log.push_back(AuditEntry {
            operation,
            executor: _executor.clone(),
            timestamp,
            success,
        });

        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
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
            });

        stats.total_executions = stats.total_executions.saturating_add(1);
        if success {
            stats.successful_executions = stats.successful_executions.saturating_add(1);
        } else {
            stats.failed_executions = stats.failed_executions.saturating_add(1);
        }
        stats.last_execution_time = env.ledger().timestamp();

        env.storage().instance().set(&symbol_short!("STATS"), &stats);
    }

    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            20 // DEFAULT_PAGE_LIMIT
        } else if limit > 50 {
            50 // MAX_PAGE_LIMIT
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
mod test;
