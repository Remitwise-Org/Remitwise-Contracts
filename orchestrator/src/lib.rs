#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol,
    Vec,
};

#[allow(dead_code)]
mod interface {
    use soroban_sdk::{contractclient, Address, ConversionError, Env, Vec};

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
        fn add_to_goal(env: Env, caller: Address, goal_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    #[contractclient(name = "BillPaymentsClient")]
    pub trait BillPaymentsInterface {
        fn pay_bill(env: Env, caller: Address, bill_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    #[contractclient(name = "InsuranceClient")]
    pub trait InsuranceInterface {
        fn pay_premium(env: Env, caller: Address, policy_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    /// Compensation / reverse interfaces for rollback support.
    /// These are expected to be implemented by the respective downstream contracts.
    /// If a contract does not implement compensation, the orchestrator records
    /// the partial state and surfaces `RemittanceFlowRolledBack` without attempting
    /// the reverse call.
    #[contractclient(name = "SavingsGoalsCompClient")]
    pub trait SavingsGoalsCompInterface {
        fn remove_from_goal(env: Env, user: Address, goal_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    #[contractclient(name = "BillPaymentsCompClient")]
    pub trait BillPaymentsCompInterface {
        fn reverse_payment(env: Env, user: Address, bill_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    #[contractclient(name = "InsuranceCompClient")]
    pub trait InsuranceCompInterface {
        fn reverse_premium(env: Env, user: Address, policy_id: u32, amount: i128) -> Result<(), ConversionError>;
    }

    /// External token contract interface used by `claim_rewards_summary_external`.
    ///
    /// Follows the standard Stellar Asset Contract / SEP-41 surface: only the
    /// `transfer` entry-point is needed here.  Keeping the trait minimal avoids
    /// pulling in unneeded ABI surface under `#![no_std]`.
    #[contractclient(name = "RewardTokenClient")]
    pub trait RewardTokenInterface {
        /// Transfer `amount` tokens from `from` to `to`.
        fn transfer(env: Env, from: Address, to: Address, amount: i128);
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

/// Identifies a step in the multi-contract remittance flow.
/// Used to track which step failed and to drive compensation logic.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum FlowStep {
    SpendingCheck = 1,
    SplitCalculation = 2,
    SavingsGoal = 3,
    BillPayment = 4,
    InsurancePremium = 5,
}

use remitwise_common::{
    reversible_op::{BillPaymentsReversibleClient, SavingsGoalsReversibleClient},
    EventCategory, EventPriority, RemitwiseEvents, CONTRACT_VERSION, SNAPSHOT_KEY, SNAPSHOT_VERSION,
};

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
/// Audit operation symbol for remittance flow executions (signed and unsigned).
const FLOW_EXEC_AUDIT: Symbol = symbol_short!("flow_exec");
/// Storage key for per-address pending reward balances.
/// Value type: `Map<Address, i128>`.
const PENDING_REWARDS: Symbol = symbol_short!("PNDG_RWD");
/// Storage key for the current actor epoch.
/// Value type: `u64`.
const ACTOR_EPOCH: Symbol = symbol_short!("ACT_EPOCH");

/// Pre-upgrade snapshot for upgrade rollback protection.
///
/// Captures critical instance storage before a contract upgrade so state
/// can be restored if the upgrade fails or produces inconsistent results.
#[contracttype]
#[derive(Clone)]
pub struct PreUpgradeSnapshot {
    /// Snapshot schema version (`SNAPSHOT_VERSION`).
    pub schema_version: u32,
    /// Contract owner address.
    pub owner: Address,
    /// Contract version at snapshot time.
    pub version: u32,
    /// Family wallet dependency address.
    pub family_wallet: Address,
    /// Remittance split dependency address.
    pub remittance_split: Address,
    /// Savings goals dependency address.
    pub savings_goals: Address,
    /// Bill payments dependency address.
    pub bill_payments: Address,
    /// Insurance dependency address.
    pub insurance: Address,
    /// Execution lock state.
    pub execution_locked: bool,
    /// Execution statistics.
    pub stats: ExecutionStats,
    /// Goal execution parameter ID.
    pub goal_id: u32,
    /// Bill execution parameter ID.
    pub bill_id: u32,
    /// Policy execution parameter ID.
    pub policy_id: u32,
    /// Current actor epoch.
    pub actor_epoch: u64,
}

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

/// Per-step outcome of a fan-out execute call.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FanOutStepResult {
    pub step: FlowStep,
    pub succeeded: bool,
    pub amount: i128,
}

/// Aggregate result of execute_flow_fanout — all steps attempted, successes and
/// failures reported independently. The caller can decide how to handle partial success.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FanOutFlowResult {
    pub savings: FanOutStepResult,
    pub bills: FanOutStepResult,
    pub insurance: FanOutStepResult,
    pub all_succeeded: bool,
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

/// Resolved downstream targets for the remittance fan-out.
#[derive(Clone)]
struct FlowRouting {
    family_wallet: Address,
    remittance_split: Address,
    savings: Address,
    bills: Address,
    insurance: Address,
    goal_id: u32,
    bill_id: u32,
    policy_id: u32,
}

impl FlowRouting {
    fn from_params(params: &RemittanceFlowParams) -> Self {
        Self {
            family_wallet: params.family_wallet.clone(),
            remittance_split: params.remittance_split.clone(),
            savings: params.savings.clone(),
            bills: params.bills.clone(),
            insurance: params.insurance.clone(),
            goal_id: params.goal_id,
            bill_id: params.bill_id,
            policy_id: params.policy_id,
        }
    }

    fn from_storage(env: &Env) -> Result<Self, OrchestratorError> {
        Ok(Self {
            family_wallet: env
                .storage()
                .instance()
                .get(&symbol_short!("FW_ADDR"))
                .ok_or(OrchestratorError::InvalidDependency)?,
            remittance_split: env
                .storage()
                .instance()
                .get(&symbol_short!("RS_ADDR"))
                .ok_or(OrchestratorError::InvalidDependency)?,
            savings: env
                .storage()
                .instance()
                .get(&symbol_short!("SG_ADDR"))
                .ok_or(OrchestratorError::InvalidDependency)?,
            bills: env
                .storage()
                .instance()
                .get(&symbol_short!("BP_ADDR"))
                .ok_or(OrchestratorError::InvalidDependency)?,
            insurance: env
                .storage()
                .instance()
                .get(&symbol_short!("INS_ADDR"))
                .ok_or(OrchestratorError::InvalidDependency)?,
            goal_id: env
                .storage()
                .instance()
                .get(&symbol_short!("GOAL_ID"))
                .unwrap_or(1),
            bill_id: env
                .storage()
                .instance()
                .get(&symbol_short!("BILL_ID"))
                .unwrap_or(1),
            policy_id: env
                .storage()
                .instance()
                .get(&symbol_short!("POL_ID"))
                .unwrap_or(1),
        })
    }
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
    /// One or more downstream steps failed and previously-applied steps
    /// have been compensated (best-effort). The audit log records which
    /// step triggered the failure. The caller should inspect the audit
    /// log to determine the partial-execution state.
    RemittanceFlowRolledBack = 11,
    /// A re-entrant call to `claim_rewards_summary_external` was detected
    /// while the execution lock (`EXEC_LOCK`) was already held.
    ///
    /// # Threat mitigated (T-RE-02)
    /// Without the lock a malicious reward-token contract can call back into
    /// this function before the pending-reward balance is zeroed. Each
    /// re-entrant invocation would observe the un-cleared balance and trigger
    /// an additional transfer, draining funds beyond entitlement.
    ///
    /// Surface this typed error to callers instead of panicking so the
    /// condition is observable off-chain (indexers, dashboards).
    ReentrancyDetected = 12,
    /// The caller has no pending rewards to claim.
    NoPendingRewards = 13,
    /// The provided actor epoch does not match the current epoch.
    /// This prevents replay of stale actor tokens after epoch bumps.
    EpochMismatch = 14,
}

#[contract]
pub struct Orchestrator;

#[contractimpl]
impl Orchestrator {
    /// Executes the full remittance flow across multiple contracts.
    ///
    /// Emits the same lifecycle events (`flow`, `flow_ok`, `flow_fail`) and writes
    /// `flow_exec` audit entries as [`Self::execute_remittance_flow_signed`], so
    /// indexers observe all remittance executions regardless of entrypoint.
    ///
    /// This is protected against reentrancy.
    pub fn execute_remittance_flow(
        env: Env,
        params: RemittanceFlowParams,
    ) -> Result<(), OrchestratorError> {
        params.caller.require_auth();

        if params.total_amount <= 0 {
            Self::record_flow_validation_failure(&env, &params.caller);
            return Err(OrchestratorError::InvalidAmount);
        }

        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            Self::record_flow_validation_failure(&env, &params.caller);
            return Err(OrchestratorError::ExecutionLocked);
        }

        Self::emit_flow_started(&env, &params.caller, params.total_amount);

        // Use a scope to ensure the guard is dropped (and lock released)
        // before we record stats, audit, and lifecycle completion events.
        let result = {
            Self::extend_instance_ttl(&env);
            // The guard acquires the lock on creation and releases it on drop.
            // This ensures the lock is released even if we return early via `?`.
            let _guard = Self::acquire_execution_lock(&env)?;

            Self::perform_remittance_flow(&env, &params)
        };

        Self::record_flow_outcome(&env, &params.caller, params.total_amount, result)
    }

    fn perform_remittance_flow(
        env: &Env,
        params: &RemittanceFlowParams,
    ) -> Result<(), OrchestratorError> {
        Self::run_remittance_fan_out(
            env,
            &params.caller,
            params.total_amount,
            &FlowRouting::from_params(params),
            false,
        )
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
        let addresses = soroban_sdk::vec![
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

        // Store default execution parameters for the signed flow.
        // These can be updated by the owner via a future admin method.
        env.storage()
            .instance()
            .set(&symbol_short!("GOAL_ID"), &1u32);
        env.storage()
            .instance()
            .set(&symbol_short!("BILL_ID"), &1u32);
        env.storage()
            .instance()
            .set(&symbol_short!("POL_ID"), &1u32);

        // Initialize actor epoch to 0. This can be bumped by the owner
        // to invalidate stale actor tokens (defence-in-depth).
        env.storage()
            .instance()
            .set(&ACTOR_EPOCH, &0u64);

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
    /// - Epoch validation to prevent stale actor token replay
    ///
    /// # Errors
    /// - `Unauthorized` if executor doesn't authorize or contract not initialized
    /// - `InvalidAmount` if amount <= 0
    /// - `DeadlineExpired` if deadline is invalid or passed
    /// - `InvalidNonce` if nonce or hash is invalid
    /// - `NonceAlreadyUsed` if nonce was already used
    /// - `ExecutionLocked` if reentrancy detected
    /// - `EpochMismatch` if actor_epoch does not match current epoch
    pub fn execute_remittance_flow_signed(
        env: Env,
        executor: Address,
        amount: i128,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
        actor_epoch: u64,
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
            Self::record_flow_validation_failure(&env, &executor);
            return Err(OrchestratorError::InvalidAmount);
        }

        // 4. Reentrancy guard: check execution lock before flow starts
        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            Self::record_flow_validation_failure(&env, &executor);
            return Err(OrchestratorError::ExecutionLocked);
        }

        // 5. Validate actor epoch to prevent stale token replay
        Self::verify_matching_epoch(&env, actor_epoch)?;

        // 6. Hardened nonce validation with deadline + hash binding.
        // Execution parameter IDs are read from instance storage (defaults set
        // at init) and folded into the hash so relayers cannot redirect funds
        // to a different goal/bill/policy after signing.
        let routing = FlowRouting::from_storage(&env)?;
        let expected_hash = Self::compute_request_hash(
            symbol_short!("flow"),
            nonce,
            amount,
            deadline,
            routing.goal_id,
            routing.bill_id,
            routing.policy_id,
        );
        Self::require_nonce_hardened(
            &env,
            &executor,
            nonce,
            deadline,
            request_hash,
            expected_hash,
        )?;

        Self::emit_flow_started(&env, &executor, amount);

        // 7. Execute under reentrancy guard (LockGuard RAII ensures release on all paths)
        let result = {
            let _guard = Self::acquire_execution_lock(&env)?;
            Self::execute_flow_internal(&env, &executor, amount)
        };

        // 8. On success: advance nonce, then record shared flow outcome
        match result {
            Ok(_) => {
                Self::increment_nonce(&env, &executor)?;
                Self::record_flow_outcome(&env, &executor, amount, Ok(()))?;
                Ok(true)
            }
            Err(e) => Self::record_flow_outcome(&env, &executor, amount, Err(e)).map(|_| true),
        }
    }

    /// Fan-out execute: attempt all three downstream cross-contract steps independently,
    /// reporting per-step success/failure. Unlike execute_remittance_flow, no compensation
    /// is applied — callers receive the full result and decide how to handle partial success.
    ///
    /// Useful for idempotent retries or best-effort distribution where partial progress
    /// is preferable to full rollback.
    pub fn execute_flow_fanout(
        env: Env,
        executor: Address,
        amount: i128,
    ) -> Result<FanOutFlowResult, OrchestratorError> {
        Self::extend_instance_ttl(&env);
        executor.require_auth();

        if amount <= 0 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let sg_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("SG_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let bp_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("BP_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let ins_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("INS_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let goal_id: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("GOAL_ID"))
            .unwrap_or(1);
        let bill_id: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("BILL_ID"))
            .unwrap_or(1);
        let policy_id: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("POL_ID"))
            .unwrap_or(1);

        let split = amount / 3;
        let remainder = amount - split * 3;

        let s_ok = interface::SavingsGoalsClient::new(&env, &sg_addr)
            .add_to_goal(&executor, &goal_id, &(split + remainder)).is_ok();
        let b_ok = interface::BillPaymentsClient::new(&env, &bp_addr)
            .pay_bill(&executor, &bill_id, &split).is_ok();
        let i_ok = interface::InsuranceClient::new(&env, &ins_addr)
            .pay_premium(&executor, &policy_id, &split).is_ok();

        let savings = FanOutStepResult {
            step: FlowStep::SavingsGoal,
            succeeded: s_ok,
            amount: split + remainder,
        };
        let bills = FanOutStepResult {
            step: FlowStep::BillPayment,
            succeeded: b_ok,
            amount: split,
        };
        let insurance = FanOutStepResult {
            step: FlowStep::InsurancePremium,
            succeeded: i_ok,
            amount: split,
        };
        let all_succeeded = s_ok && b_ok && i_ok;

        Ok(FanOutFlowResult {
            savings,
            bills,
            insurance,
            all_succeeded,
        })
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

    /// Claim accrued rewards and transfer them from the reward-token contract
    /// to the caller.
    ///
    /// # Threat mitigated — T-RE-02: re-entrant reward drain
    ///
    /// **Attack surface without this guard:**
    /// 1. `caller` invokes `claim_rewards_summary_external`.
    /// 2. Contract reads `pending` balance (e.g. 1 000 tokens) from storage.
    /// 3. Contract calls `token.transfer(this, caller, 1_000)`.
    /// 4. A malicious token contract re-enters `claim_rewards_summary_external`
    ///    before the balance is written to zero.
    /// 5. Step 4 reads `pending` again — still 1 000 — and triggers a second
    ///    transfer, draining twice the entitlement.
    ///
    /// **Fix applied:**
    /// * `EXEC_LOCK` is acquired (via RAII `LockGuard`) **before** any read of
    ///   the pending balance.
    /// * The pending balance is zeroed in storage (**effect written**) *before*
    ///   the external `token.transfer` call (**interaction**), following the
    ///   Checks-Effects-Interactions pattern.
    /// * Any re-entrant call while the lock is held receives
    ///   `OrchestratorError::ReentrancyDetected` immediately, without performing
    ///   any state mutation or token transfer.
    ///
    /// # Authorization
    /// `caller` must authorize the transaction.
    ///
    /// # Parameters
    /// - `caller`       — address whose pending reward balance is claimed.
    /// - `reward_token` — address of the SEP-41-compatible reward token contract.
    ///
    /// # Returns
    /// The amount transferred (always > 0 on success).
    ///
    /// # Errors
    /// - `NoPendingRewards`   — caller has no accrued rewards.
    /// - `ReentrancyDetected` — lock already held; re-entrant call rejected.
    pub fn claim_rewards_summary_external(
        env: Env,
        caller: Address,
        reward_token: Address,
    ) -> Result<i128, OrchestratorError> {
        // 1. Authorize caller before touching any state.
        caller.require_auth();

        // 2. Reentrancy guard — check and acquire lock atomically.
        //    Surfaces a typed error instead of panicking so the condition is
        //    observable by indexers and off-chain dashboards.
        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            Self::append_audit(&env, symbol_short!("clm_rwd"), &caller, false);
            return Err(OrchestratorError::ReentrancyDetected);
        }
        // The LockGuard sets EXEC_LOCK = true on creation and resets it to
        // false on drop, covering all return paths including early `?` returns.
        let _guard = Self::acquire_execution_lock(&env)?;

        // 3. CHECK — read pending balance.
        let mut rewards: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&PENDING_REWARDS)
            .unwrap_or_else(|| Map::new(&env));
        let pending: i128 = rewards.get(caller.clone()).unwrap_or(0);
        if pending <= 0 {
            Self::append_audit(&env, symbol_short!("clm_rwd"), &caller, false);
            return Err(OrchestratorError::NoPendingRewards);
        }

        // 4. EFFECT — zero out the balance *before* the external call.
        //    This is the critical ordering that defeats the re-entrant drain:
        //    any re-entrant call will now see pending == 0 (and hit the
        //    NoPendingRewards guard above), even if the lock check somehow
        //    passed.
        rewards.set(caller.clone(), 0i128);
        env.storage().instance().set(&PENDING_REWARDS, &rewards);

        // 5. INTERACTION — call the external token contract.
        //    The contract address of this orchestrator acts as the token
        //    holder/escrow; it transfers `pending` tokens to `caller`.
        let this = env.current_contract_address();
        let token = interface::RewardTokenClient::new(&env, &reward_token);
        token.transfer(&this, &caller, &pending);

        // 6. Audit and event.
        Self::append_audit(&env, symbol_short!("clm_rwd"), &caller, true);
        env.events().publish(
            (symbol_short!("orch"), symbol_short!("clm_rwd")),
            (caller.clone(), pending),
        );

        Ok(pending)
    }

    /// Credit pending rewards for an address (called by internal flow logic or
    /// admin seeding).  Exposed as a private helper only; no public entry-point
    /// credits rewards directly to avoid bypassing accrual logic.
    #[allow(dead_code)]
    fn credit_pending_rewards(env: &Env, recipient: &Address, amount: i128) {
        if amount <= 0 {
            return;
        }
        let mut rewards: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&PENDING_REWARDS)
            .unwrap_or_else(|| Map::new(env));
        let current: i128 = rewards.get(recipient.clone()).unwrap_or(0);
        let new_balance = current.saturating_add(amount);
        rewards.set(recipient.clone(), new_balance);
        env.storage().instance().set(&PENDING_REWARDS, &rewards);
    }

    /// Return the pending reward balance for an address without claiming it.
    pub fn get_pending_rewards(env: Env, address: Address) -> i128 {
        let rewards: Option<Map<Address, i128>> = env.storage().instance().get(&PENDING_REWARDS);
        match rewards {
            Some(m) => m.get(address).unwrap_or(0),
            None => 0,
        }
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

    /// Bump the actor epoch to invalidate stale actor tokens.
    ///
    /// This is a defence-in-depth mechanism. When called, all actor tokens
    /// created before the bump will fail the `verify_matching_epoch` check.
    ///
    /// # Threat mitigated
    /// Without this check, an attacker who obtains a stale actor token (e.g.,
    /// through a compromised signing service) could replay it indefinitely.
    /// Bumping the epoch forces all actors to obtain fresh tokens.
    ///
    /// # Authorization
    /// Only the contract owner may bump the epoch.
    ///
    /// # Errors
    /// - `Unauthorized` if caller is not the owner
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("epoch_bump"))` with (old_epoch, new_epoch).
    pub fn bump_actor_epoch(env: Env, caller: Address) -> Result<u64, OrchestratorError> {
        caller.require_auth();

        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;

        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let old_epoch = Self::get_actor_epoch(&env);
        let new_epoch = old_epoch.checked_add(1).ok_or(OrchestratorError::Overflow)?;

        env.storage().instance().set(&ACTOR_EPOCH, &new_epoch);

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("epch_bump")),
            (old_epoch, new_epoch),
        );

        Ok(new_epoch)
    }

    /// Get the current actor epoch.
    ///
    /// This allows actors to query the current epoch before creating tokens.
    pub fn get_actor_epoch_public(env: Env) -> u64 {
        Self::get_actor_epoch(&env)
    }

    /// Capture a pre-upgrade snapshot of critical instance storage.
    ///
    /// Call this before performing a contract upgrade. The snapshot captures
    /// the owner, dependency addresses, execution state, statistics, and
    /// parameter IDs so the contract can be restored if the upgrade fails.
    ///
    /// # Authorization
    /// Only the contract owner may take a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the contract owner
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("snap_pre"))`.
    pub fn pre_upgrade(env: Env, caller: Address) -> Result<bool, OrchestratorError> {
        caller.require_auth();
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }
        let fw_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("FW_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let rs_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("RS_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let sg_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("SG_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let bp_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("BP_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let ins_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("INS_ADDR"))
            .ok_or(OrchestratorError::InvalidDependency)?;
        let stats: ExecutionStats = env
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
        let snapshot = PreUpgradeSnapshot {
            schema_version: SNAPSHOT_VERSION,
            owner: owner.clone(),
            version: Self::get_version(env.clone()),
            family_wallet: fw_addr,
            remittance_split: rs_addr,
            savings_goals: sg_addr,
            bill_payments: bp_addr,
            insurance: ins_addr,
            execution_locked: env.storage().instance().get(&EXEC_LOCK).unwrap_or(false),
            stats,
            goal_id: env
                .storage()
                .instance()
                .get(&symbol_short!("GOAL_ID"))
                .unwrap_or(1),
            bill_id: env
                .storage()
                .instance()
                .get(&symbol_short!("BILL_ID"))
                .unwrap_or(1),
            policy_id: env
                .storage()
                .instance()
                .get(&symbol_short!("POL_ID"))
                .unwrap_or(1),
            actor_epoch: Self::get_actor_epoch(&env),
        };
        env.storage().persistent().set(&SNAPSHOT_KEY, &snapshot);
        env.events().publish(
            (symbol_short!("orch"), symbol_short!("snap_pre")),
            SNAPSHOT_VERSION,
        );
        Ok(true)
    }

    /// Restore critical instance storage from a pre-upgrade snapshot.
    ///
    /// Reads the snapshot stored by `pre_upgrade` and writes the captured
    /// owner, dependencies, execution state, stats, and parameter IDs back
    /// to instance storage. The snapshot is consumed after a successful
    /// restore.
    ///
    /// # Authorization
    /// Only the contract owner may restore from a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    /// - `InvalidDependency` if no snapshot exists
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("snap_rst"))`.
    pub fn restore_from_snapshot(env: Env, caller: Address) -> Result<bool, OrchestratorError> {
        caller.require_auth();
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }
        let snapshot: PreUpgradeSnapshot = env
            .storage()
            .persistent()
            .get(&SNAPSHOT_KEY)
            .ok_or(OrchestratorError::InvalidDependency)?;
        if snapshot.schema_version != SNAPSHOT_VERSION {
            return Err(OrchestratorError::InvalidDependency);
        }
        if snapshot.owner != owner {
            return Err(OrchestratorError::Unauthorized);
        }
        Self::extend_instance_ttl(&env);

        // Restore dependency addresses
        env.storage()
            .instance()
            .set(&symbol_short!("FW_ADDR"), &snapshot.family_wallet);
        env.storage()
            .instance()
            .set(&symbol_short!("RS_ADDR"), &snapshot.remittance_split);
        env.storage()
            .instance()
            .set(&symbol_short!("SG_ADDR"), &snapshot.savings_goals);
        env.storage()
            .instance()
            .set(&symbol_short!("BP_ADDR"), &snapshot.bill_payments);
        env.storage()
            .instance()
            .set(&symbol_short!("INS_ADDR"), &snapshot.insurance);

        // Restore version
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &snapshot.version);

        // Restore execution lock
        env.storage()
            .instance()
            .set(&EXEC_LOCK, &snapshot.execution_locked);

        // Restore stats
        env.storage()
            .instance()
            .set(&symbol_short!("STATS"), &snapshot.stats);

        // Restore parameter IDs
        env.storage()
            .instance()
            .set(&symbol_short!("GOAL_ID"), &snapshot.goal_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILL_ID"), &snapshot.bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("POL_ID"), &snapshot.policy_id);

        // Restore actor epoch
        env.storage()
            .instance()
            .set(&ACTOR_EPOCH, &snapshot.actor_epoch);

        // Consume the snapshot
        env.storage().persistent().remove(&SNAPSHOT_KEY);

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("snap_rst")),
            snapshot.version,
        );
        Ok(true)
    }

    /// Discard a pre-upgrade snapshot without restoring it.
    ///
    /// Use after a successful upgrade to free persistent storage.
    ///
    /// # Authorization
    /// Only the contract owner may discard a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    pub fn discard_snapshot(env: Env, caller: Address) -> Result<bool, OrchestratorError> {
        caller.require_auth();
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }
        env.storage().persistent().remove(&SNAPSHOT_KEY);
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("snap_dsc")), ());
        Ok(true)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Emits the `flow` lifecycle event after validation passes and execution begins.
    ///
    /// Topic: `("Remitwise", Transaction, High, "flow")`
    /// Payload: `(executor: Address, amount: i128)`
    fn emit_flow_started(env: &Env, executor: &Address, amount: i128) {
        RemitwiseEvents::emit(
            env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("flow"),
            (executor.clone(), amount),
        );
    }

    /// Records a pre-execution validation failure in the audit log only.
    ///
    /// No lifecycle events or `ExecutionStats` updates are emitted — the flow
    /// never started. Matches the signed path for `InvalidAmount` / `ExecutionLocked`.
    fn record_flow_validation_failure(env: &Env, executor: &Address) {
        Self::append_audit(env, FLOW_EXEC_AUDIT, executor, false);
    }

    /// Updates stats, audit log, and lifecycle events after flow execution completes.
    ///
    /// Must be called only after downstream state mutations finish so failures
    /// emit `flow_fail`, not `flow_ok`.
    fn record_flow_outcome(
        env: &Env,
        executor: &Address,
        amount: i128,
        result: Result<(), OrchestratorError>,
    ) -> Result<(), OrchestratorError> {
        match result {
            Ok(()) => {
                Self::update_execution_stats(env, true);
                Self::append_audit(env, FLOW_EXEC_AUDIT, executor, true);
                RemitwiseEvents::emit(
                    env,
                    EventCategory::Transaction,
                    EventPriority::High,
                    symbol_short!("flow_ok"),
                    (executor.clone(), amount),
                );
                Ok(())
            }
            Err(e) => {
                Self::update_execution_stats(env, false);
                Self::append_audit(env, FLOW_EXEC_AUDIT, executor, false);
                RemitwiseEvents::emit(
                    env,
                    EventCategory::Transaction,
                    EventPriority::High,
                    symbol_short!("flow_fail"),
                    (executor.clone(), e as u32),
                );
                Err(e)
            }
        }
    }

    /// Execute the signed remittance fan-out under `EXEC_LOCK`.
    ///
    /// Resolves downstream contract addresses and execution parameter IDs from
    /// instance storage (written at `init`) and delegates to
    /// [`Self::run_remittance_fan_out`] with compensation enabled.
    ///
    /// Call ordering and failure semantics are documented on
    /// [`Self::run_remittance_fan_out`].
    fn execute_flow_internal(
        env: &Env,
        executor: &Address,
        amount: i128,
    ) -> Result<bool, OrchestratorError> {
        let routing = FlowRouting::from_storage(env)?;
        Self::run_remittance_fan_out(env, executor, amount, &routing, true)?;
        Ok(true)
    }

    /// Shared remittance fan-out for unsigned and signed entrypoints.
    ///
    /// Downstream call order (all under `EXEC_LOCK` when invoked from public
    /// entrypoints):
    /// 1. `check_spending_limit` on family wallet (read-only)
    /// 2. `calculate_split` on remittance split (read-only)
    /// 3. `add_to_goal` when savings allocation > 0
    /// 4. `pay_bill` when bills allocation > 0
    /// 5. `pay_premium` when insurance allocation > 0
    ///
    /// Failure semantics:
    /// - Spending limit denial → [`OrchestratorError::Unauthorized`]
    /// - Split vector shorter than 4 or negative allocation →
    ///   [`OrchestratorError::InvalidAmount`]
    /// - First write failure → [`OrchestratorError::CrossContractCallFailed`]
    /// - Later write failure with `compensate_on_failure == true` → best-effort
    ///   reverse calls then [`OrchestratorError::RemittanceFlowRolledBack`]
    /// - Later write failure with `compensate_on_failure == false` →
    ///   [`OrchestratorError::CrossContractCallFailed`]
    fn run_remittance_fan_out(
        env: &Env,
        caller: &Address,
        amount: i128,
        routing: &FlowRouting,
        compensate_on_failure: bool,
    ) -> Result<(), OrchestratorError> {
        let fw_client = interface::FamilyWalletClient::new(env, &routing.family_wallet);
        if !fw_client.check_spending_limit(caller, &amount) {
            return Err(OrchestratorError::Unauthorized);
        }

        let rs_client = interface::RemittanceSplitClient::new(env, &routing.remittance_split);
        let allocations = rs_client.calculate_split(&amount);

        if allocations.len() < 4 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let savings_amt = allocations.get(1).ok_or(OrchestratorError::InvalidAmount)?;
        let bills_amt = allocations.get(2).ok_or(OrchestratorError::InvalidAmount)?;
        let insurance_amt = allocations.get(3).ok_or(OrchestratorError::InvalidAmount)?;

        if savings_amt < 0 || bills_amt < 0 || insurance_amt < 0 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let mut savings_done = false;
        let mut bills_done = false;

        if savings_amt > 0 {
            let s_client = interface::SavingsGoalsClient::new(env, &routing.savings);
        if s_client.add_to_goal(caller, &routing.goal_id, &savings_amt).is_err() {
                return Err(OrchestratorError::CrossContractCallFailed);
            }
            savings_done = true;
        }

        if bills_amt > 0 {
            let b_client = interface::BillPaymentsClient::new(env, &routing.bills);
        if b_client.pay_bill(caller, &routing.bill_id, &bills_amt).is_err() {
                if compensate_on_failure {
                    Self::compensate_savings(
                        env,
                        caller,
                        routing.goal_id,
                        savings_amt,
                        savings_done,
                    );
                    return Err(OrchestratorError::RemittanceFlowRolledBack);
                }
                return Err(OrchestratorError::CrossContractCallFailed);
            }
            bills_done = true;
        }

        if insurance_amt > 0 {
            let i_client = interface::InsuranceClient::new(env, &routing.insurance);
        if i_client.pay_premium(caller, &routing.policy_id, &insurance_amt).is_err() {
                if compensate_on_failure {
                    Self::compensate_savings(
                        env,
                        caller,
                        routing.goal_id,
                        savings_amt,
                        savings_done,
                    );
                    Self::compensate_bill(env, caller, routing.bill_id, bills_amt, bills_done);
                    return Err(OrchestratorError::RemittanceFlowRolledBack);
                }
                return Err(OrchestratorError::CrossContractCallFailed);
            }
        }

        Ok(())
    }

    /// Compensate a savings-goal contribution if it was applied.
    fn compensate_savings(
        env: &Env,
        executor: &Address,
        goal_id: u32,
        amount: i128,
        applied: bool,
    ) {
        if !applied || amount <= 0 {
            return;
        }
        let sg_addr = match env.storage().instance().get(&symbol_short!("SG_ADDR")) {
            Some(a) => a,
            None => return,
        };
        let client = interface::SavingsGoalsCompClient::new(env, &sg_addr);
        let _ = client.remove_from_goal(executor, &goal_id, &amount);
    }

    /// Compensate a bill payment if it was applied.
    fn compensate_bill(env: &Env, executor: &Address, bill_id: u32, amount: i128, applied: bool) {
        if !applied || amount <= 0 {
            return;
        }
        let bp_addr = match env.storage().instance().get(&symbol_short!("BP_ADDR")) {
            Some(a) => a,
            None => return,
        };
        let client = interface::BillPaymentsCompClient::new(env, &bp_addr);
        let _ = client.reverse_payment(executor, &bill_id, &amount);
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

    /// Deterministic request hash for signed remittance authorizations.
    ///
    /// Binds `operation`, `nonce`, `amount`, `deadline`, and the execution
    /// parameter IDs (`goal_id`, `bill_id`, `policy_id`) read from instance
    /// storage at validation time.
    fn compute_request_hash(
        operation: Symbol,
        nonce: u64,
        amount: i128,
        deadline: u64,
        goal_id: u32,
        bill_id: u32,
        policy_id: u32,
    ) -> u64 {
        let op_bits: u64 = operation.to_val().get_payload();
        let amt_lo = amount as u64;
        let amt_hi = (amount >> 64) as u64;

        op_bits
            .wrapping_add(nonce)
            .wrapping_add(amt_lo)
            .wrapping_add(amt_hi)
            .wrapping_add(deadline)
            .wrapping_add(u64::from(goal_id))
            .wrapping_add(u64::from(bill_id))
            .wrapping_add(u64::from(policy_id))
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

    /// Get the current actor epoch from instance storage.
    fn get_actor_epoch(env: &Env) -> u64 {
        env.storage()
            .instance()
            .get(&ACTOR_EPOCH)
            .unwrap_or(0)
    }

    /// Verify that the provided actor epoch matches the current epoch.
    ///
    /// This is a defence-in-depth check to prevent replay of stale actor tokens
    /// after epoch bumps. An attacker who obtains a stale actor token cannot
    /// replay it after the epoch has been bumped by the contract owner.
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `actor_epoch` - The epoch value provided by the actor
    ///
    /// # Returns
    /// * `Ok(())` if the epochs match
    /// * `Err(OrchestratorError::EpochMismatch)` if they differ
    fn verify_matching_epoch(env: &Env, actor_epoch: u64) -> Result<(), OrchestratorError> {
        let current_epoch = Self::get_actor_epoch(env);
        if actor_epoch != current_epoch {
            return Err(OrchestratorError::EpochMismatch);
        }
        Ok(())
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
        contract, contractimpl, symbol_short,
        testutils::{Address as _, Ledger as _},
        Address, Env,
    };

    /// A mock downstream contract whose methods always succeed.
    #[contract]
    struct MockSimpleContract;

    #[contractimpl]
    impl MockSimpleContract {
        pub fn check_spending_limit(_env: Env, _user: Address, _amount: i128) -> bool {
            true
        }
        pub fn calculate_split(env: Env, _total_amount: i128) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
        pub fn pay_bill(_env: Env, _user: Address, _bill_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
        pub fn pay_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
        pub fn remove_from_goal(_env: Env, _user: Address, _goal_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
        pub fn reverse_payment(_env: Env, _user: Address, _bill_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
        pub fn reverse_premium(_env: Env, _user: Address, _policy_id: u32, _amount: i128) -> Result<(), ConversionError> {
            true
        }
    }

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

        // Register a mock downstream contract for each dependency so
        // execute_flow_internal's cross-contract calls succeed.
        let fw = env.register_contract(None, MockSimpleContract);
        let rs = env.register_contract(None, MockSimpleContract);
        let sg = env.register_contract(None, MockSimpleContract);
        let bp = env.register_contract(None, MockSimpleContract);
        let ins = env.register_contract(None, MockSimpleContract);

        client.init(&owner, &fw, &rs, &sg, &bp, &ins);

        SignedFlowHarness { env, contract_id }
    }

    fn client(harness: &SignedFlowHarness) -> OrchestratorClient<'_> {
        OrchestratorClient::new(&harness.env, &harness.contract_id)
    }

    fn valid_deadline() -> u64 {
        BASE_TIME + MAX_DEADLINE_WINDOW_SECS
    }

    fn request_hash(amount: i128, nonce: u64, deadline: u64) -> u64 {
        Orchestrator::compute_request_hash(symbol_short!("flow"), nonce, amount, deadline, 1, 1, 1)
    }

    fn execute_signed_flow(
        client: &OrchestratorClient,
        executor: &Address,
        amount: i128,
        nonce: u64,
        deadline: u64,
    ) {
        let hash = request_hash(amount, nonce, deadline);
        assert!(client.execute_remittance_flow_signed(executor, &amount, &nonce, &deadline, &hash));
    }

    #[test]
    fn used_nonce_set_rejects_current_nonce_before_hash_binding() {
        let harness = setup_signed_flow();
        let client = client(&harness);
        let executor = Address::generate(&harness.env);
        let nonce = 0;
        let deadline = valid_deadline();
        let hash = request_hash(FLOW_AMOUNT, nonce, deadline);

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

        let replay_hash = request_hash(FLOW_AMOUNT, 0, deadline);
        let replay = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &0,
            &deadline,
            &replay_hash,
        );
        assert_eq!(replay, Err(Ok(OrchestratorError::NonceAlreadyUsed)));

        let skipped_hash = request_hash(FLOW_AMOUNT, 3, deadline);
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

        let oldest_before_eviction_hash = request_hash(FLOW_AMOUNT, 0, deadline);
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

        let evicted_nonce_hash = request_hash(FLOW_AMOUNT, 0, deadline);
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
        let expired_hash = request_hash(FLOW_AMOUNT, 0, expired_deadline);
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
        let beyond_window_hash = request_hash(FLOW_AMOUNT, 0, beyond_window_deadline);
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
        let original_hash = request_hash(FLOW_AMOUNT, nonce, deadline);
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
#[macro_use]
extern crate std;

#[cfg(test)]
#[path = "test.rs"]
mod test;

#[cfg(test)]
mod events_schema_test;
