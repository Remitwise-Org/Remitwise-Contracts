#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, Symbol,
    Vec,
};

#[allow(dead_code)]
mod interface {
    use soroban_sdk::{contractclient, Address, Env, Vec};

    /// Shared prefix for the cross-contract epoch arguments every privileged
    /// downstream call now carries.  `(orchestrator, epoch)` is appended to the
    /// front of each interface method so the downstream contract can validate
    /// both the caller's identity and a fresh epoch before mutating state.
    #[contractclient(name = "FamilyWalletClient")]
    pub trait FamilyWalletInterface {
        fn check_spending_limit(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            user: Address,
            amount: i128,
        ) -> bool;
        fn bump_cross_contract_epoch(env: Env, orchestrator: Address) -> u64;
        fn get_cross_contract_epoch(env: Env) -> u64;
    }

    #[contractclient(name = "RemittanceSplitClient")]
    pub trait RemittanceSplitInterface {
        fn calculate_split(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            total_amount: i128,
        ) -> Vec<i128>;
        fn get_split(env: Env, orchestrator: Address, epoch: u64) -> Vec<u32>;
        fn bump_cross_contract_epoch(env: Env, orchestrator: Address) -> u64;
        fn get_cross_contract_epoch(env: Env) -> u64;
    }

    #[contractclient(name = "SavingsGoalsClient")]
    pub trait SavingsGoalsInterface {
        fn add_to_goal(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            caller: Address,
            goal_id: u32,
            amount: i128,
        ) -> i128;
        fn bump_cross_contract_epoch(env: Env, orchestrator: Address) -> u64;
        fn get_cross_contract_epoch(env: Env) -> u64;
    }

    #[contractclient(name = "BillPaymentsClient")]
    pub trait BillPaymentsInterface {
        fn pay_bill(env: Env, orchestrator: Address, epoch: u64, caller: Address, bill_id: u32);
        fn bump_cross_contract_epoch(env: Env, orchestrator: Address) -> u64;
        fn get_cross_contract_epoch(env: Env) -> u64;
    }

    #[contractclient(name = "InsuranceClient")]
    pub trait InsuranceInterface {
        fn pay_premium(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            caller: Address,
            policy_id: u32,
        );
        fn bump_cross_contract_epoch(env: Env, orchestrator: Address);
        fn get_cross_contract_epoch(env: Env) -> u64;
    }

    /// Compensation / reverse interfaces for rollback support.
    /// These are expected to be implemented by the respective downstream contracts.
    /// If a contract does not implement compensation, the orchestrator records
    /// the partial state and surfaces `RemittanceFlowRolledBack` without attempting
    /// the reverse call.
    #[contractclient(name = "SavingsGoalsCompClient")]
    pub trait SavingsGoalsCompInterface {
        fn remove_from_goal(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            user: Address,
            goal_id: u32,
            amount: i128,
        ) -> bool;
    }

    #[contractclient(name = "BillPaymentsCompClient")]
    pub trait BillPaymentsCompInterface {
        fn reverse_payment(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            user: Address,
            bill_id: u32,
            amount: i128,
        ) -> bool;
    }

    #[contractclient(name = "InsuranceCompClient")]
    pub trait InsuranceCompInterface {
        fn reverse_premium(
            env: Env,
            orchestrator: Address,
            epoch: u64,
            user: Address,
            policy_id: u32,
            amount: i128,
        ) -> bool;
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

    /// Interface for interacting with the emergency killswitch contract.
    #[contractclient(name = "KillswitchClient")]
    pub trait KillswitchInterface {
        fn is_paused(env: Env) -> bool;
        fn is_module_paused(env: Env, module_id: Symbol) -> bool;
        fn is_function_paused(env: Env, module_id: Symbol, func: Symbol) -> bool;
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
    EventCategory, EventPriority, RemitwiseEvents, CONTRACT_VERSION, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD, SNAPSHOT_KEY, SNAPSHOT_VERSION,
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
/// Storage key prefix for pending dependency-update proposals.
/// Value type: `PendingDependencyUpdate`, keyed by `(DEP_PEND, kind as u32)`.
const DEP_PEND: Symbol = symbol_short!("DEP_PEND");
/// Timelock (in seconds) that must elapse between proposing and executing a
/// dependency update. 24 hours.
const DEP_TIMELOCK_SECS: u64 = 86_400;
/// Maximum number of `bump_cross_contract_epoch` calls allowed when
/// provisioning a freshly deployed downstream contract during a rotation.
const MAX_EPOCH_CATCHUP: u64 = 32;
/// Storage key for the emergency killswitch contract address.
/// Value type: `Address`.
const KS_ADDR: Symbol = symbol_short!("KS_ADDR");

/// Pre-upgrade snapshot for upgrade rollback protection.
///
/// Captures critical instance storage before a contract upgrade so state
/// can be restored if the upgrade fails or produces inconsistent results.
///
/// Per-executor [`FlowRoute`] entries live in *persistent* storage and are
/// intentionally excluded here (only the default route, via
/// `goal_id`/`bill_id`/`policy_id`, is captured) — see [`FlowRoute`].
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

/// Identifies which downstream dependency a rotation proposal targets.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum DependencyKind {
    FamilyWallet = 0,
    RemittanceSplit = 1,
    SavingsGoals = 2,
    BillPayments = 3,
    Insurance = 4,
}

impl DependencyKind {
    /// Returns the instance-storage key holding this dependency's address.
    fn storage_key(self) -> Symbol {
        match self {
            DependencyKind::FamilyWallet => symbol_short!("FW_ADDR"),
            DependencyKind::RemittanceSplit => symbol_short!("RS_ADDR"),
            DependencyKind::SavingsGoals => symbol_short!("SG_ADDR"),
            DependencyKind::BillPayments => symbol_short!("BP_ADDR"),
            DependencyKind::Insurance => symbol_short!("INS_ADDR"),
        }
    }
}

/// A pending, timelocked dependency-update proposal.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDependencyUpdate {
    pub kind: DependencyKind,
    pub new_address: Address,
    pub eta: u64,
}

/// Per-executor routing overrides for the signed remittance flow.
///
/// Stored in *persistent* storage keyed by `(symbol_short!("ROUTE"), executor)`
/// rather than instance storage, because the number of executors is unbounded
/// and must not inflate the instance entry. An executor with no entry falls
/// back to the owner-controlled default (`GOAL_ID`/`BILL_ID`/`POL_ID` in
/// instance storage, seeded to `1/1/1` at init).
///
/// Deliberately **not** part of [`PreUpgradeSnapshot`]: routes are per-executor
/// persistent data, not critical instance state, and carrying an unbounded map
/// in the snapshot would blow the instance entry budget. The default route is
/// still captured via the `GOAL_ID`/`BILL_ID`/`POL_ID` fields.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FlowRoute {
    /// Savings goal id the executor's flow pays into.
    pub goal_id: u32,
    /// Bill id the executor's flow pays.
    pub bill_id: u32,
    /// Insurance policy id the executor's flow pays.
    pub policy_id: u32,
    /// Ledger timestamp of the most recent write.
    pub updated_at: u64,
}

/// Storage key for an executor's [`FlowRoute`] entry.
fn flow_route_key(executor: &Address) -> (Symbol, Address) {
    (symbol_short!("ROUTE"), executor.clone())
}

/// Read an executor's [`FlowRoute`], bumping the persistent TTL on hit.
///
/// Shared by the public `get_flow_route` view and the signed-flow/fan-out
/// resolution path so a route that is actively read is never archived.
fn read_flow_route(env: &Env, executor: &Address) -> Option<FlowRoute> {
    let key = flow_route_key(executor);
    let route: Option<FlowRoute> = env.storage().persistent().get(&key);
    if route.is_some() {
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }
    route
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
    fn from_params(env: &Env, params: &RemittanceFlowParams) -> Result<Self, OrchestratorError> {
        let stored = Self::from_storage(env)?;

        if params.family_wallet != stored.family_wallet
            || params.remittance_split != stored.remittance_split
            || params.savings != stored.savings
            || params.bills != stored.bills
            || params.insurance != stored.insurance
        {
            return Err(OrchestratorError::InvalidDependency);
        }

        Ok(Self {
            family_wallet: stored.family_wallet,
            remittance_split: stored.remittance_split,
            savings: stored.savings,
            bills: stored.bills,
            insurance: stored.insurance,
            goal_id: params.goal_id,
            bill_id: params.bill_id,
            policy_id: params.policy_id,
        })
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

    /// Resolve routing for a specific executor.
    ///
    /// Dependency addresses and the owner-controlled default ids come from
    /// instance storage (`from_storage`); when the executor has a per-executor
    /// [`FlowRoute`], its ids override the defaults. Addresses are never
    /// overridable per executor.
    fn for_executor(env: &Env, executor: &Address) -> Result<Self, OrchestratorError> {
        let mut routing = Self::from_storage(env)?;
        if let Some(route) = read_flow_route(env, executor) {
            routing.goal_id = route.goal_id;
            routing.bill_id = route.bill_id;
            routing.policy_id = route.policy_id;
        }
        Ok(routing)
    }
}

/// Classification of a downstream cross-contract call result.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StepOutcome {
    /// The call succeeded and returned a valid decoded value.
    Success,
    /// The callee contract rejected the invocation (contract error or false return).
    ContractRejection,
    /// An invocation failure (missing contract, panic) or ABI decoding error.
    InvocationFailure,
}

fn classify_call<T, E1, E2, E3>(r: Result<Result<T, E1>, Result<E2, E3>>) -> StepOutcome {
    match r {
        Ok(Ok(_)) => StepOutcome::Success,
        Err(Ok(_)) => StepOutcome::ContractRejection,
        Err(Err(_)) | Ok(Err(_)) => StepOutcome::InvocationFailure,
    }
}

fn classify_premium<E1, E2, E3>(r: Result<Result<bool, E1>, Result<E2, E3>>) -> StepOutcome {
    match r {
        Ok(Ok(true)) => StepOutcome::Success,
        Ok(Ok(false)) => StepOutcome::ContractRejection,
        Err(Ok(_)) => StepOutcome::ContractRejection,
        Err(Err(_)) | Ok(Err(_)) => StepOutcome::InvocationFailure,
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
    /// The pre-upgrade snapshot is older than the freshness window.
    SnapshotTooOld = 14,
    /// The actor epoch does not match the contract's current epoch.
    EpochMismatch = 15,
    /// A route id was zero. Object ids are 1-based, so 0 is never a valid
    /// goal/bill/policy id.
    InvalidRoute = 16,
    /// A dependency-update proposal was executed before its timelock elapsed.
    TimelockNotElapsed = 17,
    /// No pending dependency-update proposal exists for the requested kind.
    NoPendingUpdate = 18,
    DownstreamEpochBumpFailed = 19,
    EmergencyPaused = 20,
}

#[contract]
pub struct Orchestrator;

#[contractimpl]
impl Orchestrator {
    /// Executes the full remittance flow across multiple contracts.
    ///
    /// Validates caller-supplied dependency addresses against the trusted addresses stored
    /// at contract initialization. If any dependency address differs or storage is uninitialized,
    /// returns [`OrchestratorError::InvalidDependency`] before making any cross-contract calls.
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
        Self::require_not_emergency_paused(&env, symbol_short!("flow"))?;

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
        let routing = FlowRouting::from_params(env, params)?;
        Self::run_remittance_fan_out(env, &params.caller, params.total_amount, &routing, false)
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
        Self::init_internal(
            env,
            caller,
            family_wallet,
            remittance_split,
            savings_goals,
            bill_payments,
            insurance,
            None,
        )
    }

    /// Same as [`init`], but also configures the signed-flow deadline window
    /// (see `require_nonce_hardened`) instead of relying on the
    /// `MAX_DEADLINE_WINDOW_SECS` default. Kept as a separate entrypoint so
    /// `init`'s signature and existing callers are unaffected.
    ///
    /// # Errors
    /// - `InvalidAmount` if `deadline_window_secs == 0` -- a zero window
    ///   would make every signed flow deadline immediately expired.
    pub fn init_with_deadline_window(
        env: Env,
        caller: Address,
        family_wallet: Address,
        remittance_split: Address,
        savings_goals: Address,
        bill_payments: Address,
        insurance: Address,
        deadline_window_secs: u64,
    ) -> Result<bool, OrchestratorError> {
        if deadline_window_secs == 0 {
            return Err(OrchestratorError::InvalidAmount);
        }
        Self::init_internal(
            env,
            caller,
            family_wallet,
            remittance_split,
            savings_goals,
            bill_payments,
            insurance,
            Some(deadline_window_secs),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn init_internal(
        env: Env,
        caller: Address,
        family_wallet: Address,
        remittance_split: Address,
        savings_goals: Address,
        bill_payments: Address,
        insurance: Address,
        deadline_window_secs: Option<u64>,
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
        if let Some(window) = deadline_window_secs {
            env.storage()
                .instance()
                .set(&symbol_short!("DL_WIN"), &window);
        }

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
        env.storage().instance().set(&ACTOR_EPOCH, &0u64);

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

    /// Validate that `new_address` may be configured as the dependency of
    /// `kind`: it must not be the owner, must not be the orchestrator itself,
    /// and must not collide with any *other* currently configured dependency.
    ///
    /// Shared by `init_internal` and `propose_dependency_update`.
    fn validate_dependency_address(
        env: &Env,
        owner: &Address,
        kind: DependencyKind,
        new_address: &Address,
    ) -> Result<(), OrchestratorError> {
        let orch = env.current_contract_address();
        if new_address == owner || new_address == &orch {
            return Err(OrchestratorError::DuplicateDependency);
        }
        for other in [
            DependencyKind::FamilyWallet,
            DependencyKind::RemittanceSplit,
            DependencyKind::SavingsGoals,
            DependencyKind::BillPayments,
            DependencyKind::Insurance,
        ] {
            if other == kind {
                continue;
            }
            if let Some(existing) = env
                .storage()
                .instance()
                .get::<Symbol, Address>(&other.storage_key())
            {
                if &existing == new_address {
                    return Err(OrchestratorError::DuplicateDependency);
                }
            }
        }
        Ok(())
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

        // Check emergency killswitch before nonces, locks, or flow execution
        Self::require_not_emergency_paused(&env, symbol_short!("flow_sig"))?;

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
        let routing = FlowRouting::for_executor(&env, &executor)?;
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
    ///
    /// # Security
    /// - Authorization required for `executor`.
    /// - Rejects `amount <= 0` and records validation failure.
    /// - Protected against reentrancy: checks and acquires `EXEC_LOCK` during execution.
    /// - Enforces spending limit via `check_spending_limit` on the stored `family_wallet`
    ///   before any downstream mutations; returns [`OrchestratorError::Unauthorized`] on denial.
    /// - Emits standard lifecycle events (`flow`, `flow_ok`, `flow_fail`).
    /// - Records outcome in execution statistics and audit ring-buffer via [`Self::record_flow_outcome`].
    ///
    /// # Memoised fee lookup — fix for #1339
    /// Dependency addresses and execution IDs are read from instance storage exactly
    /// once via `FlowRouting::from_storage`, and the split allocation is computed via
    /// a single `calculate_split` cross-contract call.  Previous versions called
    /// 6 individual `instance().get(...)` lookups and used a hardcoded `amount / 3`
    /// approximation rather than the configured split percentages.
    ///
    /// # Correct succeeded semantics — fix for #1345
    /// `succeeded` is set to `true` when the downstream `try_*` call returns `Ok`,
    /// and `false` when it returns `Err`.  The previous code used `.is_err()` for
    /// all three assignments, which inverted both per-step flags and `all_succeeded`.
    pub fn execute_flow_fanout(
        env: Env,
        executor: Address,
        amount: i128,
    ) -> Result<FanOutFlowResult, OrchestratorError> {
        Self::extend_instance_ttl(&env);
        executor.require_auth();
        Self::require_not_emergency_paused(&env, symbol_short!("fanout"))?;

        if amount <= 0 {
            Self::record_flow_validation_failure(&env, &executor);
            return Err(OrchestratorError::InvalidAmount);
        }

        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            Self::record_flow_validation_failure(&env, &executor);
            return Err(OrchestratorError::ExecutionLocked);
        }

        Self::emit_flow_started(&env, &executor, amount);

        let result = {
            let _guard = Self::acquire_execution_lock(&env)?;
            Self::perform_flow_fanout(&env, &executor, amount)
        };

        match result {
            Ok(fanout_result) => {
                let outcome = if fanout_result.all_succeeded {
                    Ok(())
                } else {
                    Err(OrchestratorError::CrossContractCallFailed)
                };
                let _ = Self::record_flow_outcome(&env, &executor, amount, outcome);
                Ok(fanout_result)
            }
            Err(e) => {
                let _ = Self::record_flow_outcome(&env, &executor, amount, Err(e));
                Err(e)
            }
        }
    }

    fn perform_flow_fanout(
        env: &Env,
        executor: &Address,
        amount: i128,
    ) -> Result<FanOutFlowResult, OrchestratorError> {
        // Every privileged cross-contract call carries the orchestrator's own
        // contract identity and the current actor epoch so each downstream
        // contract can validate both before mutating.
        let orch = env.current_contract_address();
        let epoch = Self::get_actor_epoch(env);

        // Surface the epoch used for this flow so off-chain reconciliation can
        // correlate every downstream event emitted during the fan-out.
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("flow_ep")), epoch);

        // --- #1339: read all dependency addresses and IDs in one pass ----------
        let routing = FlowRouting::for_executor(&env, &executor)?;

        // --- #1339: single cross-contract call to fetch the configured split ---
        // `calculate_split` returns [spending, savings, bills, insurance] amounts.
        // We call it once and reuse the result for all three fan-out steps,
        // so the fee schedule is fetched exactly once per batch invocation.
        let rs_client = interface::RemittanceSplitClient::new(env, &routing.remittance_split);
        let allocations = rs_client.calculate_split(&orch, &epoch, &amount);

        if allocations.len() < 4 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let savings_amt = allocations.get(1).ok_or(OrchestratorError::InvalidAmount)?;
        let bills_amt = allocations.get(2).ok_or(OrchestratorError::InvalidAmount)?;
        let insurance_amt = allocations.get(3).ok_or(OrchestratorError::InvalidAmount)?;

        if savings_amt < 0 || bills_amt < 0 || insurance_amt < 0 {
            return Err(OrchestratorError::InvalidAmount);
        }

        let s_res = interface::SavingsGoalsClient::new(&env, &routing.savings).try_add_to_goal(
            &orch,
            &epoch,
            &executor,
            &routing.goal_id,
            &savings_amt,
        );
        let s_ok = matches!(classify_call(s_res), StepOutcome::Success);

        let b_res = interface::BillPaymentsClient::new(&env, &routing.bills).try_pay_bill(
            &orch,
            &epoch,
            &executor,
            &routing.bill_id,
        );
        let b_ok = matches!(classify_call(b_res), StepOutcome::Success);

        let i_res = interface::InsuranceClient::new(&env, &routing.insurance).try_pay_premium(
            &orch,
            &epoch,
            &executor,
            &routing.policy_id,
        );
        let i_ok = matches!(classify_premium(i_res), StepOutcome::Success);

        let savings = FanOutStepResult {
            step: FlowStep::SavingsGoal,
            succeeded: s_ok,
            amount: savings_amt,
        };
        let bills = FanOutStepResult {
            step: FlowStep::BillPayment,
            succeeded: b_ok,
            amount: bills_amt,
        };
        let insurance = FanOutStepResult {
            step: FlowStep::InsurancePremium,
            succeeded: i_ok,
            amount: insurance_amt,
        };
        let all_succeeded = s_ok && b_ok && i_ok;

        Ok(FanOutFlowResult {
            savings,
            bills,
            insurance,
            all_succeeded,
        })
    }

    /// Set the calling executor's routing for the signed remittance flow.
    ///
    /// The route is stored in *persistent* storage under
    /// `(symbol_short!("ROUTE"), executor)` and overrides the owner-controlled
    /// default for that executor only. Rejected while a flow is in flight
    /// (`EXEC_LOCK` held) so a route can never change underneath a flow that has
    /// already started.
    ///
    /// # Authorization
    /// The `executor` address must authorize the call.
    ///
    /// # Errors
    /// - `Unauthorized` if `executor` did not authorize
    /// - `InvalidRoute` if any id is 0 (object ids are 1-based)
    /// - `ExecutionLocked` if a flow is currently executing
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("route_set"))` with
    /// `(executor, goal_id, bill_id, policy_id)`.
    pub fn set_flow_route(
        env: Env,
        executor: Address,
        goal_id: u32,
        bill_id: u32,
        policy_id: u32,
    ) -> Result<(), OrchestratorError> {
        executor.require_auth();

        if goal_id == 0 || bill_id == 0 || policy_id == 0 {
            return Err(OrchestratorError::InvalidRoute);
        }

        Self::require_no_pending_ops(&env)?;

        let route = FlowRoute {
            goal_id,
            bill_id,
            policy_id,
            updated_at: env.ledger().timestamp(),
        };
        let key = flow_route_key(&executor);
        env.storage().persistent().set(&key, &route);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("route_set")),
            (executor, goal_id, bill_id, policy_id),
        );
        Ok(())
    }

    /// Remove the calling executor's routing override, reverting it to the
    /// owner-controlled default.
    ///
    /// # Authorization
    /// The `executor` address must authorize the call.
    ///
    /// # Errors
    /// - `Unauthorized` if `executor` did not authorize
    /// - `ExecutionLocked` if a flow is currently executing
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("route_clr"))` with `executor`.
    pub fn clear_flow_route(env: Env, executor: Address) -> Result<(), OrchestratorError> {
        executor.require_auth();

        Self::require_no_pending_ops(&env)?;

        env.storage()
            .persistent()
            .remove(&flow_route_key(&executor));
        env.events().publish(
            (symbol_short!("orch"), symbol_short!("route_clr")),
            executor,
        );
        Ok(())
    }

    /// Return an executor's route override, or `None` when it uses the default.
    ///
    /// Read-only; no authorization required. The persistent TTL is bumped when
    /// an entry exists.
    pub fn get_flow_route(env: Env, executor: Address) -> Option<FlowRoute> {
        read_flow_route(&env, &executor)
    }

    /// Owner-only: replace the default route used by executors with no override.
    ///
    /// Writes the legacy instance keys `GOAL_ID`/`BILL_ID`/`POL_ID` so
    /// [`FlowRouting::from_storage`] and [`PreUpgradeSnapshot`] continue to see
    /// the current default. Zero ids are rejected.
    ///
    /// # Authorization
    /// Only the stored `OWNER` may call this.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    /// - `InvalidRoute` if any id is 0
    /// - `ExecutionLocked` if a flow is currently executing
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("def_route"))` with
    /// `(caller, goal_id, bill_id, policy_id)`.
    pub fn set_default_route(
        env: Env,
        caller: Address,
        goal_id: u32,
        bill_id: u32,
        policy_id: u32,
    ) -> Result<(), OrchestratorError> {
        caller.require_auth();

        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }

        if goal_id == 0 || bill_id == 0 || policy_id == 0 {
            return Err(OrchestratorError::InvalidRoute);
        }

        Self::require_no_pending_ops(&env)?;

        env.storage()
            .instance()
            .set(&symbol_short!("GOAL_ID"), &goal_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILL_ID"), &bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("POL_ID"), &policy_id);
        Self::extend_instance_ttl(&env);

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("def_route")),
            (caller, goal_id, bill_id, policy_id),
        );
        Ok(())
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

    /// Get the current fee schedule (split percentages) from the remittance split contract.
    ///
    /// Returns a tuple of (spending_percent, savings_percent, bills_percent, insurance_percent)
    /// representing the percentage allocation for each category. The percentages are
    /// in basis points (1% = 100 basis points).
    ///
    /// This is a read-only view function that does not require authorization.
    /// Returns the configured signed-flow deadline window in seconds -- the
    /// maximum distance into the future a caller-supplied deadline may sit
    /// (see `require_nonce_hardened`). Falls back to `MAX_DEADLINE_WINDOW_SECS`
    /// if `init_with_deadline_window` was never called (i.e. plain `init` was
    /// used). No authentication required — observable on-chain.
    pub fn get_deadline_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&symbol_short!("DL_WIN"))
            .unwrap_or(MAX_DEADLINE_WINDOW_SECS)
    }

    pub fn get_fee_schedule(env: Env) -> Option<(u32, u32, u32, u32)> {
        let rs_addr: Address = env.storage().instance().get(&symbol_short!("RS_ADDR"))?;

        let rs_client = interface::RemittanceSplitClient::new(&env, &rs_addr);
        let split = rs_client.get_split(
            &env.current_contract_address(),
            &Self::get_actor_epoch(&env),
        );

        if split.len() != 4 {
            return None;
        }

        Some((split.get(0)?, split.get(1)?, split.get(2)?, split.get(3)?))
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
        Self::require_not_emergency_paused(&env, symbol_short!("clm_rwd"))?;

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
    /// See [`docs/PAGINATION_HANDBOOK.md`](../../docs/PAGINATION_HANDBOOK.md) for the invariants
    /// all paginated reads must satisfy, cursor semantics, and the reviewer checklist.
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
    /// - `DownstreamEpochBumpFailed` if a configured downstream rejects the
    ///   bump or reports an epoch other than the new orchestrator epoch
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("epch_bump"))` with (old_epoch, new_epoch).
    /// On downstream failure, publishes `("orch", "epch_fail")` with the
    /// failing downstream step symbol before the transaction reverts.
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
        let new_epoch = old_epoch
            .checked_add(1)
            .ok_or(OrchestratorError::Overflow)?;

        env.storage().instance().set(&ACTOR_EPOCH, &new_epoch);

        // Strict lockstep is required: a downstream bump failure must revert
        // this write and every earlier downstream bump in the transaction.
        Self::coordinated_bump_downstream_epochs(&env, new_epoch)?;

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("epch_bump")),
            (old_epoch, new_epoch),
        );

        Ok(new_epoch)
    }

    /// Propose a timelocked update of one downstream dependency address.
    ///
    /// The proposal has no effect until [`Self::execute_dependency_update`] is
    /// called by the owner after `DEP_TIMELOCK_SECS` have elapsed, giving users
    /// time to observe the pending change on-chain.
    ///
    /// # Authorization
    /// Only the contract owner may propose.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    /// - `DuplicateDependency` if `new_address` is the owner, the orchestrator
    ///   itself, or equal to another currently configured dependency
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("dep_prop"))` with
    /// `(kind, new_address, eta)`.
    pub fn propose_dependency_update(
        env: Env,
        caller: Address,
        kind: DependencyKind,
        new_address: Address,
    ) -> Result<u64, OrchestratorError> {
        caller.require_auth();
        let owner: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .ok_or(OrchestratorError::Unauthorized)?;
        if caller != owner {
            return Err(OrchestratorError::Unauthorized);
        }

        Self::validate_dependency_address(&env, &owner, kind, &new_address)?;

        Self::extend_instance_ttl(&env);
        let eta = env
            .ledger()
            .timestamp()
            .checked_add(DEP_TIMELOCK_SECS)
            .ok_or(OrchestratorError::Overflow)?;
        let pending = PendingDependencyUpdate {
            kind,
            new_address: new_address.clone(),
            eta,
        };
        env.storage()
            .instance()
            .set(&(DEP_PEND, kind as u32), &pending);

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("dep_prop")),
            (kind, new_address, eta),
        );

        Ok(eta)
    }

    /// Execute a previously proposed dependency update once its timelock has
    /// elapsed.
    ///
    /// Before writing the new address, the target contract is provisioned so
    /// its cross-contract epoch matches `ACTOR_EPOCH`:
    /// - For `SavingsGoals`, `set_trusted_orchestrator(orch, orch)` is called
    ///   first (the savings contract requires the orchestrator to self-register).
    /// - For all kinds, the target's epoch is read; if it is ahead of
    ///   `ACTOR_EPOCH` the update fails with `EpochMismatch`. Otherwise
    ///   `bump_cross_contract_epoch` is called `ACTOR_EPOCH - e` times, capped
    ///   at `MAX_EPOCH_CATCHUP`, and the epoch is re-read and required to equal
    ///   `ACTOR_EPOCH`. Any failure reverts the whole update.
    ///
    /// # Authorization
    /// Only the contract owner may execute.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    /// - `NoPendingUpdate` if no proposal exists for `kind`
    /// - `TimelockNotElapsed` if `now < eta`
    /// - `ExecutionLocked` if a flow is in progress
    /// - `EpochMismatch` if the new contract's epoch cannot be reconciled
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("dep_upd"))` with
    /// `(kind, old_address, new_address)`.
    pub fn execute_dependency_update(
        env: Env,
        caller: Address,
        kind: DependencyKind,
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

        let pending: PendingDependencyUpdate = env
            .storage()
            .instance()
            .get(&(DEP_PEND, kind as u32))
            .ok_or(OrchestratorError::NoPendingUpdate)?;
        if env.ledger().timestamp() < pending.eta {
            return Err(OrchestratorError::TimelockNotElapsed);
        }

        Self::require_no_pending_ops(&env)?;
        Self::extend_instance_ttl(&env);

        let orch = env.current_contract_address();
        let target_epoch = Self::get_actor_epoch(&env);

        // Provision the new contract so its cross-contract epoch matches ours.
        // A freshly deployed contract starts at epoch 0 and only the trusted
        // orchestrator may advance it.
        if kind == DependencyKind::SavingsGoals {
            // `savings_goals::set_trusted_orchestrator` requires the caller to
            // be the orchestrator itself, so we self-register here.
            let sg = interface::SavingsGoalsClient::new(&env, &pending.new_address);
            sg.set_trusted_orchestrator(&orch, &orch);
        }

        let current: u64 = env.invoke_contract(
            &pending.new_address,
            &Symbol::new(&env, "get_cross_contract_epoch"),
            soroban_sdk::vec![&env],
        );
        if current > target_epoch {
            return Err(OrchestratorError::EpochMismatch);
        }
        let needed = target_epoch - current;
        if needed > MAX_EPOCH_CATCHUP {
            return Err(OrchestratorError::EpochMismatch);
        }
        for _ in 0..needed {
            let _: u64 = env.invoke_contract(
                &pending.new_address,
                &Symbol::new(&env, "bump_cross_contract_epoch"),
                soroban_sdk::vec![&env, orch.clone().into()],
            );
        }
        let after: u64 = env.invoke_contract(
            &pending.new_address,
            &Symbol::new(&env, "get_cross_contract_epoch"),
            soroban_sdk::vec![&env],
        );
        if after != target_epoch {
            return Err(OrchestratorError::EpochMismatch);
        }

        let key = kind.storage_key();
        let old: Option<Address> = env.storage().instance().get(&key);
        env.storage().instance().set(&key, &pending.new_address);
        env.storage().instance().remove(&(DEP_PEND, kind as u32));

        env.events().publish(
            (symbol_short!("orch"), symbol_short!("dep_upd")),
            (kind, old, pending.new_address),
        );

        Ok(true)
    }

    /// Cancel a pending dependency-update proposal.
    ///
    /// # Authorization
    /// Only the contract owner may cancel.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the owner
    /// - `NoPendingUpdate` if no proposal exists for `kind`
    pub fn cancel_dependency_update(
        env: Env,
        caller: Address,
        kind: DependencyKind,
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
        let key = (DEP_PEND, kind as u32);
        if !env.storage().instance().has(&key) {
            return Err(OrchestratorError::NoPendingUpdate);
        }
        env.storage().instance().remove(&key);
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("dep_cncl")), kind);
        Ok(true)
    }

    /// Return the pending dependency-update proposal for `kind`, if any.
    pub fn get_pending_dependency_update(
        env: Env,
        kind: DependencyKind,
    ) -> Option<PendingDependencyUpdate> {
        env.storage().instance().get(&(DEP_PEND, kind as u32))
    }

    /// Advance and verify every configured downstream epoch in strict lockstep.
    ///
    /// Any failed invocation or incorrect read-back returns an error. Because
    /// this runs in the `bump_actor_epoch` transaction, the error reverts the
    /// orchestrator epoch and all earlier downstream writes as well.
    fn coordinated_bump_downstream_epochs(
        env: &Env,
        expected_epoch: u64,
    ) -> Result<(), OrchestratorError> {
        let orch = env.current_contract_address();

        if let Some(addr) = env.storage().instance().get(&symbol_short!("FW_ADDR")) {
            let client = interface::FamilyWalletClient::new(env, &addr);
            if client.try_bump_cross_contract_epoch(&orch).is_err()
                || !matches!(
                    client.try_get_cross_contract_epoch(),
                    Ok(Ok(epoch)) if epoch == expected_epoch
                )
            {
                return Self::fail_downstream_epoch_bump(env, symbol_short!("family"));
            }
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("RS_ADDR")) {
            let client = interface::RemittanceSplitClient::new(env, &addr);
            if client.try_bump_cross_contract_epoch(&orch).is_err()
                || !matches!(
                    client.try_get_cross_contract_epoch(),
                    Ok(Ok(epoch)) if epoch == expected_epoch
                )
            {
                return Self::fail_downstream_epoch_bump(env, symbol_short!("split"));
            }
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("SG_ADDR")) {
            let _ =
                interface::SavingsGoalsClient::new(env, &addr).try_bump_cross_contract_epoch(&orch);
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("BP_ADDR")) {
            let _ =
                interface::BillPaymentsClient::new(env, &addr).try_bump_cross_contract_epoch(&orch);
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("INS_ADDR")) {
            let client = interface::InsuranceClient::new(env, &addr);
            if client.try_bump_cross_contract_epoch(&orch).is_err()
                || !matches!(
                    client.try_get_cross_contract_epoch(),
                    Ok(Ok(epoch)) if epoch == expected_epoch
                )
            {
                return Self::fail_downstream_epoch_bump(env, symbol_short!("insur"));
            }
        }

        Ok(())
    }

    fn fail_downstream_epoch_bump(env: &Env, step: Symbol) -> Result<(), OrchestratorError> {
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("epch_fail")), step);
        Err(OrchestratorError::DownstreamEpochBumpFailed)
    }

    /// Return the orchestrator epoch and each configured downstream epoch.
    ///
    /// This read-only status view lets operators identify epoch drift without
    /// attempting a state-changing flow.
    pub fn get_epoch_status(env: Env) -> Vec<(Symbol, u64)> {
        let mut status = Vec::new(&env);
        status.push_back((symbol_short!("orch"), Self::get_actor_epoch(&env)));

        if let Some(addr) = env.storage().instance().get(&symbol_short!("FW_ADDR")) {
            status.push_back((
                symbol_short!("family"),
                interface::FamilyWalletClient::new(&env, &addr).get_cross_contract_epoch(),
            ));
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("RS_ADDR")) {
            status.push_back((
                symbol_short!("split"),
                interface::RemittanceSplitClient::new(&env, &addr).get_cross_contract_epoch(),
            ));
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("SG_ADDR")) {
            status.push_back((
                symbol_short!("savings"),
                interface::SavingsGoalsClient::new(&env, &addr).get_cross_contract_epoch(),
            ));
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("BP_ADDR")) {
            status.push_back((
                symbol_short!("bills"),
                interface::BillPaymentsClient::new(&env, &addr).get_cross_contract_epoch(),
            ));
        }
        if let Some(addr) = env.storage().instance().get(&symbol_short!("INS_ADDR")) {
            status.push_back((
                symbol_short!("insur"),
                interface::InsuranceClient::new(&env, &addr).get_cross_contract_epoch(),
            ));
        }

        status
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
        env.storage()
            .persistent()
            .set(&symbol_short!("SNAP_TS"), &env.ledger().timestamp());
        // The only other extend_ttl call in this contract targets the
        // *instance* bucket (see `extend_instance_ttl`); it does not, and
        // cannot, keep this *persistent* entry alive. Without bumping the
        // persistent bucket's own TTL here, the snapshot can be archived off
        // the ledger and `restore_from_snapshot` starts failing with
        // `InvalidDependency` well before `PERSISTENT_LIFETIME_THRESHOLD`
        // would suggest, even while the instance itself is still healthy.
        env.storage().persistent().extend_ttl(
            &SNAPSHOT_KEY,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().extend_ttl(
            &symbol_short!("SNAP_TS"),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
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
        let snapshot_taken_at: u64 = env
            .storage()
            .persistent()
            .get(&symbol_short!("SNAP_TS"))
            .unwrap_or(0);
        if remitwise_common::require_recent_snapshot(&env, snapshot_taken_at).is_err() {
            return Err(OrchestratorError::SnapshotTooOld);
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

    /// Sets or removes the emergency killswitch contract address.
    ///
    /// # Authorization
    /// Only the contract owner may configure the killswitch address.
    ///
    /// # Validation
    /// If `Some(addr)` is provided, it must not be the orchestrator's own address
    /// or any of the configured downstream dependencies (`FW_ADDR`, `RS_ADDR`, `SG_ADDR`,
    /// `BP_ADDR`, `INS_ADDR`).
    ///
    /// # Events
    /// Emits `(symbol_short!("orch"), symbol_short!("ks_set"))` with the configured address or `None`.
    pub fn set_killswitch(
        env: Env,
        caller: Address,
        killswitch: Option<Address>,
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

        if let Some(ref ks) = killswitch {
            if *ks == env.current_contract_address() {
                return Err(OrchestratorError::DuplicateDependency);
            }
            let dep_keys = [
                symbol_short!("FW_ADDR"),
                symbol_short!("RS_ADDR"),
                symbol_short!("SG_ADDR"),
                symbol_short!("BP_ADDR"),
                symbol_short!("INS_ADDR"),
            ];
            for key in dep_keys.iter() {
                if let Some(dep_addr) = env.storage().instance().get::<Symbol, Address>(key) {
                    if *ks == dep_addr {
                        return Err(OrchestratorError::DuplicateDependency);
                    }
                }
            }
            env.storage().instance().set(&KS_ADDR, ks);
        } else {
            env.storage().instance().remove(&KS_ADDR);
        }

        env.events()
            .publish((symbol_short!("orch"), symbol_short!("ks_set")), killswitch);

        Ok(true)
    }

    /// Checks the emergency killswitch contract if configured.
    ///
    /// Fails closed: if the killswitch contract returns `true` for global pause,
    /// module pause (`"orch"`), or function pause (`"orch"`, `func`), or if any
    /// cross-contract invocation fails, returns `Err(OrchestratorError::EmergencyPaused)`.
    fn require_not_emergency_paused(env: &Env, func: Symbol) -> Result<(), OrchestratorError> {
        let ks_addr: Option<Address> = env.storage().instance().get(&KS_ADDR);
        if let Some(addr) = ks_addr {
            let client = interface::KillswitchClient::new(env, &addr);
            // 1. Global pause check (fails closed on call failure or true)
            match client.try_is_paused() {
                Ok(Ok(false)) => {}
                _ => return Err(OrchestratorError::EmergencyPaused),
            }
            // 2. Module pause check ("orch")
            let orch_module = symbol_short!("orch");
            match client.try_is_module_paused(&orch_module) {
                Ok(Ok(false)) => {}
                _ => return Err(OrchestratorError::EmergencyPaused),
            }
            // 3. Function pause check ("orch", func)
            match client.try_is_function_paused(&orch_module, &func) {
                Ok(Ok(false)) => {}
                _ => return Err(OrchestratorError::EmergencyPaused),
            }
        }
        Ok(())
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
        let routing = FlowRouting::for_executor(env, executor)?;
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
        // Every privileged cross-contract call carries the orchestrator's own
        // contract identity and the current actor epoch so each downstream
        // contract can validate both before mutating. The actor epoch is the
        // single source of truth; downstream `XC_EPOCH` values are kept in
        // lockstep via the coordinated bump in `bump_actor_epoch`.
        let orch = env.current_contract_address();
        let epoch = Self::get_actor_epoch(env);

        // Surface the epoch used for this flow so off-chain reconciliation can
        // correlate every downstream event emitted during the fan-out.
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("flow_ep")), epoch);

        let fw_client = interface::FamilyWalletClient::new(env, &routing.family_wallet);
        if !fw_client.check_spending_limit(&orch, &epoch, caller, &amount) {
            return Err(OrchestratorError::Unauthorized);
        }

        let rs_client = interface::RemittanceSplitClient::new(env, &routing.remittance_split);
        let allocations = rs_client.calculate_split(&orch, &epoch, &amount);

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
            let s_res =
                s_client.try_add_to_goal(&orch, &epoch, caller, &routing.goal_id, &savings_amt);
            match classify_call(s_res) {
                StepOutcome::Success => savings_done = true,
                StepOutcome::ContractRejection => {
                    Self::emit_cross_contract_failure(env, symbol_short!("savings"), true);
                    return Err(OrchestratorError::CrossContractCallFailed);
                }
                StepOutcome::InvocationFailure => {
                    Self::emit_cross_contract_failure(env, symbol_short!("savings"), false);
                    return Err(OrchestratorError::CrossContractCallFailed);
                }
            }
        }

        if bills_amt > 0 {
            let b_client = interface::BillPaymentsClient::new(env, &routing.bills);
            let rejected_by_contract =
                match b_client.try_pay_bill(&orch, &epoch, caller, &routing.bill_id) {
                    Ok(Ok(_)) => {
                        bills_done = true;
                        None
                    }
                    Ok(Err(_)) => Some(true),
                    Err(_) => Some(false),
                };
            if let Some(from_contract) = rejected_by_contract {
                Self::emit_cross_contract_failure(env, symbol_short!("bills"), from_contract);
                if compensate_on_failure {
                    Self::compensate_savings(
                        env,
                        &orch,
                        epoch,
                        caller,
                        routing.goal_id,
                        savings_amt,
                        savings_done,
                    );
                    return Err(OrchestratorError::RemittanceFlowRolledBack);
                }
                return Err(OrchestratorError::CrossContractCallFailed);
            }
        }

        if insurance_amt > 0 {
            let i_client = interface::InsuranceClient::new(env, &routing.insurance);
            let rejected_by_contract =
                match i_client.try_pay_premium(&orch, &epoch, caller, &routing.policy_id) {
                    Ok(Ok(_)) => None,
                    Ok(Err(_)) => Some(true),
                    Err(_) => Some(false),
                };
            if let Some(from_contract) = rejected_by_contract {
                Self::emit_cross_contract_failure(env, symbol_short!("insur"), from_contract);
                if compensate_on_failure {
                    Self::compensate_savings(
                        env,
                        &orch,
                        epoch,
                        caller,
                        routing.goal_id,
                        savings_amt,
                        savings_done,
                    );
                    Self::compensate_bill(
                        env,
                        &orch,
                        epoch,
                        caller,
                        routing.bill_id,
                        bills_amt,
                        bills_done,
                    );
                    return Err(OrchestratorError::RemittanceFlowRolledBack);
                }
                return Err(OrchestratorError::CrossContractCallFailed);
            }
        }

        Ok(())
    }

    /// Surfaces the inner failure that `run_remittance_fan_out` would otherwise
    /// swallow behind a single generic [`OrchestratorError::CrossContractCallFailed`].
    ///
    /// `step` identifies which downstream call failed; `rejected_by_contract`
    /// distinguishes a deliberate rejection from the callee's own logic (`true`,
    /// e.g. its own validation/auth failed) from a lower-level invocation failure
    /// (`false`, e.g. the address is not a deployed contract with a matching
    /// interface). Both were previously indistinguishable from the caller's
    /// perspective.
    fn emit_cross_contract_failure(env: &Env, step: Symbol, rejected_by_contract: bool) {
        env.events()
            .publish((symbol_short!("cctx_err"), step), rejected_by_contract);
    }

    /// Emits an event indicating that a compensation (reversal) operation failed.
    fn emit_compensation_failure(env: &Env, step: Symbol) {
        env.events()
            .publish((symbol_short!("orch"), symbol_short!("comp_fail")), step);
    }

    /// Compensate a savings-goal contribution if it was applied.
    fn compensate_savings(
        env: &Env,
        orch: &Address,
        epoch: u64,
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
            None => {
                Self::emit_compensation_failure(env, symbol_short!("savings"));
                return;
            }
        };
        let client = interface::SavingsGoalsCompClient::new(env, &sg_addr);
        let res = client.try_remove_from_goal(orch, &epoch, executor, &goal_id, &amount);
        if !matches!(res, Ok(Ok(_))) {
            Self::emit_compensation_failure(env, symbol_short!("savings"));
        }
    }

    /// Compensate a bill payment if it was applied.
    fn compensate_bill(
        env: &Env,
        orch: &Address,
        epoch: u64,
        executor: &Address,
        bill_id: u32,
        amount: i128,
        applied: bool,
    ) {
        if !applied || amount <= 0 {
            return;
        }
        let bp_addr = match env.storage().instance().get(&symbol_short!("BP_ADDR")) {
            Some(a) => a,
            None => {
                Self::emit_compensation_failure(env, symbol_short!("bills"));
                return;
            }
        };
        let client = interface::BillPaymentsCompClient::new(env, &bp_addr);
        let res = client.try_reverse_payment(orch, &epoch, executor, &bill_id, &amount);
        if !matches!(res, Ok(Ok(_))) {
            Self::emit_compensation_failure(env, symbol_short!("bills"));
        }
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
        if deadline > now + Self::get_deadline_window(env.clone()) {
            return Err(OrchestratorError::DeadlineExpired);
        }

        if Self::is_nonce_used(env, address, nonce) {
            return Err(OrchestratorError::NonceAlreadyUsed);
        }

        Self::require_nonce(env, address, nonce)?;

        if !Self::constant_time_eq(request_hash, expected_hash) {
            return Err(OrchestratorError::InvalidNonce);
        }

        Ok(())
    }

    /// Constant-time equality check for the request-hash binding above.
    ///
    /// Plain `!=` on primitives is not guaranteed to be constant-time --
    /// depending on target and optimization level, integer comparison can be
    /// lowered in ways whose timing varies with where the values first
    /// differ. XOR-then-compare-to-zero never branches on the operands
    /// themselves: every input pair takes the same path regardless of
    /// whether (or where) `a` and `b` differ, which is what we want when
    /// comparing a caller-supplied value against a secret-derived binding
    /// hash.
    fn constant_time_eq(a: u64, b: u64) -> bool {
        (a ^ b) == 0
    }

    /// Guard that rejects calls while any operation is in progress.
    ///
    /// Returns `ExecutionLocked` when the execution lock (`EXEC_LOCK`) is held,
    /// indicating a pending operation has not yet completed.  Callers should use
    /// this as a pre-condition before entering mutating logic that must not
    /// overlap with an in-flight remittance or reward-claim flow.
    ///
    /// This is a read-only check that does **not** acquire the lock — it mirrors
    /// the pre-lock check already performed by the public entrypoints and lets
    /// internal helpers and future callers express the same constraint uniformly.
    fn require_no_pending_ops(env: &Env) -> Result<(), OrchestratorError> {
        let is_locked: bool = env.storage().instance().get(&EXEC_LOCK).unwrap_or(false);
        if is_locked {
            return Err(OrchestratorError::ExecutionLocked);
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
        env.storage().instance().get(&ACTOR_EPOCH).unwrap_or(0)
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
        pub fn check_spending_limit(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _amount: i128,
        ) -> bool {
            true
        }
        pub fn calculate_split(
            env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _total_amount: i128,
        ) -> Vec<i128> {
            soroban_sdk::vec![&env, 2500i128, 2500i128, 2500i128, 2500i128]
        }
        pub fn add_to_goal(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _goal_id: u32,
            _amount: i128,
        ) {
        }
        pub fn pay_bill(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _bill_id: u32,
        ) {
        }
        pub fn pay_premium(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _policy_id: u32,
        ) {
        }
        pub fn remove_from_goal(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _goal_id: u32,
            _amount: i128,
        ) {
        }
        pub fn reverse_payment(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _bill_id: u32,
            _amount: i128,
        ) {
        }
        pub fn reverse_premium(
            _env: Env,
            _orchestrator: Address,
            _epoch: u64,
            _user: Address,
            _policy_id: u32,
            _amount: i128,
        ) {
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
        assert!(client
            .execute_remittance_flow_signed(executor, &amount, &nonce, &deadline, &hash, &0u64));
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
    #[ignore = "Aborts with HostError: Error(WasmVm, InvalidAction) during signed flow execution"]
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
            &0u64,
        );
        assert_eq!(replay, Err(Ok(OrchestratorError::NonceAlreadyUsed)));

        let skipped_hash = request_hash(FLOW_AMOUNT, 3, deadline);
        let skipped = client.try_execute_remittance_flow_signed(
            &executor,
            &FLOW_AMOUNT,
            &3,
            &deadline,
            &skipped_hash,
            &0u64,
        );
        assert_eq!(skipped, Err(Ok(OrchestratorError::InvalidNonce)));
        assert_eq!(client.get_nonce(&executor), 1);
    }

    #[test]
    #[ignore = "Aborts with HostError: Error(WasmVm, InvalidAction) during signed flow execution"]
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
            &0u64,
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
            &0u64,
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
    #[ignore = "Aborts with HostError: Error(WasmVm, InvalidAction) during signed flow execution"]
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
            &0u64,
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
            &0u64,
        );
        assert_eq!(beyond_window, Err(Ok(OrchestratorError::DeadlineExpired)));
        assert_eq!(client.get_nonce(&executor), 0);

        execute_signed_flow(&client, &executor, FLOW_AMOUNT, 0, valid_deadline());
        assert_eq!(client.get_nonce(&executor), 1);
    }

    #[test]
    #[ignore = "Aborts with HostError: Error(WasmVm, InvalidAction) during signed flow execution"]
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
            &0u64,
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
