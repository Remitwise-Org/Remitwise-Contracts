#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Symbol, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    LimitExceeded = 4,
    InvalidSchedule = 5,
    InvalidAdmin = 6,
    EpochMismatch = 7,
    InvalidSignerThreshold = 8,
    DuplicateSigner = 9,
    SignerNotConfigured = 10,
    DuplicateApproval = 11,
    ActivationAlreadyActive = 12,
    RecoveryTooEarly = 13,
    NotActive = 14,
    ScopeRequired = 15,
}

/// The exact pause surface affected by a threshold-approved activation.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauseScope {
    Global,
    Module(Symbol),
    Function(Symbol, Symbol),
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    GlobalPaused,
    PausedSince,
    PauseReason,
    ModulePaused(Symbol),
    PausedFunctions(Symbol),
    UnpauseSchedule,
    KillSwitchEpoch,
    Signers,
    SignerThreshold,
    SignerEpoch,
    ActivationEpoch,
    ActiveScope,
    RecoveryReadyAt,
    ScopeWasPaused,
}

/// Delay between a threshold-approved activation and recovery.
pub const RECOVERY_DELAY: u64 = 3600;

pub const MAX_PAUSED_FUNCTIONS: u32 = 10;

/// Contract version, bumped on every on-chain-upgrade-relevant change so
/// callers/tooling can detect which build of the WASM they are talking to.
pub const CONTRACT_VERSION: u32 = 1;

/// Emitted when the killswitch admin is successfully transferred.
#[contracttype]
#[derive(Clone)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

#[contract]
pub struct EmergencyKillswitch;

#[contractimpl]
impl EmergencyKillswitch {
    /// Initializes the killswitch with an admin address.
    ///
    /// Requires `admin`'s signature — without it, anyone could front-run
    /// deployment and call `initialize` with themselves (or any address they
    /// control) as `admin` before the intended admin does, permanently
    /// seizing control of the kill switch.
    ///
    /// Rejects the contract's own address as admin to prevent unrecoverable bricking.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        if admin == env.current_contract_address() {
            return Err(Error::InvalidAdmin);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::KillSwitchEpoch, &0u64);
        Ok(())
    }

    fn configured_signers(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Signers)
            .unwrap_or(Vec::new(env))
    }

    fn validate_approvals(env: &Env, approvals: &Vec<Address>) -> Result<(), Error> {
        let signers = Self::configured_signers(env);
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SignerThreshold)
            .ok_or(Error::SignerNotConfigured)?;

        // Validate all approvals: must be configured signers, and no duplicates.
        //
        // Correctness note: the previous implementation guarded the inner
        // duplicate-detection loop with `if accepted > 0`, which caused the
        // very first element to never be checked against subsequent copies of
        // itself.  For example, `[A, A]` would pass when it should return
        // `DuplicateApproval`.  The fix: compare every approval at index `i`
        // against every prior approval at indices `0..i` unconditionally.
        let n = approvals.len();
        for i in 0..n {
            let approval = approvals.get(i).unwrap();
            if !signers.contains(&approval) {
                return Err(Error::SignerNotConfigured);
            }
            // Check for duplicates: compare this approval against all prior ones.
            for j in 0..i {
                if approvals.get(j).unwrap() == approval {
                    return Err(Error::DuplicateApproval);
                }
            }
        }

        if n < threshold {
            return Err(Error::InvalidSignerThreshold);
        }
        Ok(())
    }

    /// Configure the signer set used by the explicit activation protocol.
    /// Updating the set increments its epoch and invalidates all old approval
    /// bundles. The admin remains the only authority that can change policy.
    pub fn configure_signers(
        env: Env,
        caller: Address,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<u64, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin || signers.is_empty() || threshold == 0 || threshold > signers.len() {
            return Err(Error::InvalidSignerThreshold);
        }
        for (index, signer) in signers.iter().enumerate() {
            if signer == env.current_contract_address() {
                return Err(Error::InvalidAdmin);
            }
            for prior in signers.iter().take(index) {
                if prior == signer {
                    return Err(Error::DuplicateSigner);
                }
            }
        }
        let old_epoch: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SignerEpoch)
            .unwrap_or(0);
        let epoch = old_epoch.checked_add(1).ok_or(Error::EpochMismatch)?;
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage()
            .instance()
            .set(&DataKey::SignerThreshold, &threshold);
        env.storage().instance().set(&DataKey::SignerEpoch, &epoch);
        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "signers_set")),
            (epoch, threshold, signers.len()),
        );
        Ok(epoch)
    }

    pub fn get_signer_epoch(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::SignerEpoch)
            .unwrap_or(0)
    }

    pub fn get_signer_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SignerThreshold)
            .unwrap_or(0)
    }

    /// Activate exactly one explicit scope after validating a current-epoch
    /// signer quorum. A second activation is rejected until recovery completes.
    ///
    /// # Atomicity guarantee
    ///
    /// This function uses a strict **validate-everything-then-write-everything**
    /// pattern to prevent partial state:
    ///
    /// 1. All precondition checks (epoch, already-active, approvals, limit) are
    ///    evaluated before any storage write is issued.
    /// 2. If any check fails the function returns an error with zero storage
    ///    mutations, zero events emitted, and the contract state unchanged.
    /// 3. After all checks pass, writes are committed in a single logical batch.
    ///    Because Soroban transactions are atomic at the host level, an unexpected
    ///    panic after writes have begun would still roll back the entire
    ///    transaction.  The validate-first ordering provides the additional
    ///    guarantee that *application-level* errors also produce no partial state.
    ///
    /// ## Previous bug (fixed here)
    ///
    /// The old implementation wrote `ActivationEpoch`, `ActiveScope`,
    /// `RecoveryReadyAt`, and `ScopeWasPaused` *before* it reached the
    /// `LimitExceeded` check for `Function` scopes.  When the limit was hit,
    /// those four keys remained in storage as orphaned activation markers.
    /// Every subsequent call to `activate` then failed with
    /// `ActivationAlreadyActive`, permanently blocking any new activation even
    /// though the scope was never actually paused.
    pub fn activate(
        env: Env,
        epoch: u64,
        approvals: Vec<Address>,
        scope: PauseScope,
    ) -> Result<(), Error> {
        // ── Phase 1: validate everything — no writes yet ─────────────────────
        if approvals.is_empty() {
            return Err(Error::InvalidSignerThreshold);
        }
        if epoch != Self::get_signer_epoch(env.clone()) {
            return Err(Error::EpochMismatch);
        }
        if env.storage().instance().has(&DataKey::ActivationEpoch) {
            return Err(Error::ActivationAlreadyActive);
        }
        Self::validate_approvals(&env, &approvals)?;

        // Read the pre-activation state of the target scope so we know what to
        // restore during recovery.  This is a read-only probe — no writes yet.
        let scope_was_paused = match scope.clone() {
            PauseScope::Global => env
                .storage()
                .instance()
                .get(&DataKey::GlobalPaused)
                .unwrap_or(false),
            PauseScope::Module(ref module) => env
                .storage()
                .instance()
                .get(&DataKey::ModulePaused(module.clone()))
                .unwrap_or(false),
            PauseScope::Function(ref module, ref function) => env
                .storage()
                .instance()
                .get(&DataKey::PausedFunctions(module.clone()))
                .unwrap_or(Vec::new(&env))
                .contains(function.clone()),
        };

        // For Function scope: validate the list capacity limit BEFORE writing
        // anything.  This is the critical fix — the old code performed this check
        // after writing the activation marker keys, leaving orphaned state on
        // failure.
        let function_paused_list: Option<Vec<Symbol>> = match scope.clone() {
            PauseScope::Function(ref module, ref function) => {
                let paused: Vec<Symbol> = env
                    .storage()
                    .instance()
                    .get(&DataKey::PausedFunctions(module.clone()))
                    .unwrap_or(Vec::new(&env));
                if !paused.contains(function.clone()) {
                    if paused.len() >= MAX_PAUSED_FUNCTIONS {
                        return Err(Error::LimitExceeded);
                    }
                    // Build the updated list now so the write phase is trivial.
                    let mut updated = paused;
                    updated.push_back(function.clone());
                    Some(updated)
                } else {
                    // Already in the list — scope_was_paused is true; no list
                    // mutation needed.
                    None
                }
            }
            _ => None,
        };

        // ── Phase 2: write everything — all checks passed ────────────────────
        env.storage()
            .instance()
            .set(&DataKey::ActivationEpoch, &epoch);
        env.storage().instance().set(&DataKey::ActiveScope, &scope);
        env.storage().instance().set(
            &DataKey::RecoveryReadyAt,
            &(env.ledger().timestamp().saturating_add(RECOVERY_DELAY)),
        );
        env.storage()
            .instance()
            .set(&DataKey::ScopeWasPaused, &scope_was_paused);

        // Apply the scope pause.
        match scope.clone() {
            PauseScope::Global => {
                env.storage().instance().set(&DataKey::GlobalPaused, &true)
            }
            PauseScope::Module(module) => env
                .storage()
                .instance()
                .set(&DataKey::ModulePaused(module), &true),
            PauseScope::Function(module, _function) => {
                // `function_paused_list` is `Some` when the function was not
                // already in the list (the only case that requires a write).
                if let Some(updated) = function_paused_list {
                    env.storage()
                        .instance()
                        .set(&DataKey::PausedFunctions(module), &updated);
                }
            }
        }

        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("activated")),
            (epoch, scope),
        );
        Ok(())
    }

    /// Recover the active scope after the mandatory delay and a fresh quorum.
    /// The epoch is checked again so a signer-set rotation invalidates a stale
    /// recovery bundle. Clearing the activation marker makes recovery retryable
    /// only after a new activation, never by replaying the same request.
    pub fn recover(env: Env, epoch: u64, approvals: Vec<Address>) -> Result<(), Error> {
        let active_epoch: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ActivationEpoch)
            .ok_or(Error::NotActive)?;
        if epoch != active_epoch || epoch != Self::get_signer_epoch(env.clone()) {
            return Err(Error::EpochMismatch);
        }
        let ready_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RecoveryReadyAt)
            .ok_or(Error::RecoveryTooEarly)?;
        if env.ledger().timestamp() < ready_at {
            return Err(Error::RecoveryTooEarly);
        }
        Self::validate_approvals(&env, &approvals)?;
        let scope: PauseScope = env
            .storage()
            .instance()
            .get(&DataKey::ActiveScope)
            .ok_or(Error::NotActive)?;
        let scope_was_paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::ScopeWasPaused)
            .unwrap_or(false);
        match scope.clone() {
            PauseScope::Global if !scope_was_paused => {
                env.storage().instance().set(&DataKey::GlobalPaused, &false)
            }
            PauseScope::Global => {}
            PauseScope::Module(module) => env
                .storage()
                .instance()
                .set(&DataKey::ModulePaused(module), &scope_was_paused),
            PauseScope::Function(module, function) => {
                if !scope_was_paused {
                    let mut paused = env
                        .storage()
                        .instance()
                        .get(&DataKey::PausedFunctions(module.clone()))
                        .unwrap_or(Vec::new(&env));
                    if let Some(index) = paused.first_index_of(function) {
                        paused.remove(index);
                    }
                    env.storage()
                        .instance()
                        .set(&DataKey::PausedFunctions(module), &paused);
                }
            }
        }
        env.storage().instance().remove(&DataKey::ActivationEpoch);
        env.storage().instance().remove(&DataKey::ActiveScope);
        env.storage().instance().remove(&DataKey::RecoveryReadyAt);
        env.storage().instance().remove(&DataKey::ScopeWasPaused);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("recovered")),
            (epoch, scope),
        );
        Ok(())
    }

    /// Verify that the caller-supplied kill-switch epoch matches the current
    /// epoch stored in the contract.
    ///
    /// This is a defence-in-depth check against replay of stale authorizations.
    /// Without this guard, an actor who obtains a signed `transfer_admin`
    /// payload from a previous epoch can replay it after the epoch has been
    /// bumped by the contract admin, effectively holding on to stale authority.
    ///
    /// # Errors
    /// - [`Error::EpochMismatch`] if the provided epoch does not match.
    pub fn require_killswitch_epoch(env: Env, ep: u64) -> Result<(), Error> {
        let current: u64 = env
            .storage()
            .instance()
            .get(&DataKey::KillSwitchEpoch)
            .unwrap_or(0);
        if ep != current {
            return Err(Error::EpochMismatch);
        }
        Ok(())
    }

    /// Bump the kill-switch epoch to invalidate all prior authorizations.
    ///
    /// Only the current kill-switch admin may bump the epoch. After a bump,
    /// any call to [`transfer_admin`] with an epoch value captured before
    /// the bump will fail with [`Error::EpochMismatch`].
    ///
    /// # Threat mitigated
    /// An attacker who obtains a stale signed authorization payload can replay
    /// it indefinitely without an epoch check. Bumping the epoch atomically
    /// invalidates every authorization created at or before the old epoch.
    ///
    /// # Events
    /// Emits `(symbol_short!("emergency"), symbol_short!("epch_bump"))` with
    /// `(old_epoch, new_epoch)`.
    ///
    /// # Errors
    /// - [`Error::NotInitialized`] if the contract has no admin.
    /// - [`Error::Unauthorized`] if `caller` is not the admin.
    /// - [`Error::Overflow`] if the epoch counter wraps (practically unreachable).
    pub fn bump_kill_switch_epoch(env: Env, caller: Address) -> Result<u64, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        let old_epoch: u64 = env
            .storage()
            .instance()
            .get(&DataKey::KillSwitchEpoch)
            .unwrap_or(0);
        let new_epoch = old_epoch.checked_add(1).ok_or(Error::InvalidAdmin)?; // Overflow guard — saturate on wrap
        env.storage()
            .instance()
            .set(&DataKey::KillSwitchEpoch, &new_epoch);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("epch_bump")),
            (old_epoch, new_epoch),
        );
        Ok(new_epoch)
    }

    /// Returns [`CONTRACT_VERSION`], the version of this deployed WASM build.
    ///
    /// Intended for off-chain tooling/upgrade scripts to confirm which
    /// contract version they are interacting with before/after an upgrade.
    /// No authentication required — the version is observable on-chain.
    pub fn version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    /// Return the current kill-switch epoch.
    ///
    /// No authentication required — the epoch is observable on-chain.
    pub fn get_kill_switch_epoch(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::KillSwitchEpoch)
            .unwrap_or(0)
    }

    /// Transfers admin authority to a new address.
    ///
    /// # Rejects
    /// - `new_admin` == contract own address (unrecoverable brick)
    /// - `new_admin` == current admin (no-op, to prevent accidental re-auth)
    /// - `ep` does not match the current kill-switch epoch (stale authorization)
    ///
    /// Emits [AdminTransferred] on successful handover.
    pub fn transfer_admin(env: Env, new_admin: Address, ep: u64) -> Result<(), Error> {
        Self::require_killswitch_epoch(env.clone(), ep)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        if new_admin == env.current_contract_address() {
            return Err(Error::InvalidAdmin);
        }
        if remitwise_common::same_address(&new_admin, &admin) {
            return Err(Error::InvalidAdmin);
        }

        let old_admin = admin.clone();
        env.storage().instance().set(&DataKey::Admin, &new_admin);

        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("admn_xfer")),
            AdminTransferred {
                old_admin,
                new_admin: new_admin.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );
        Ok(())
    }

    pub fn pause(env: Env) -> Result<(), Error> {
        Self::pause_internal(env, None)
    }

    /// Same as [`pause`], but records `reason` for later retrieval via
    /// [`pause_reason`]. Additive alternative kept separate from `pause` so
    /// existing callers/signatures are unaffected.
    pub fn pause_with_reason(env: Env, reason: Symbol) -> Result<(), Error> {
        Self::pause_internal(env, Some(reason))
    }

    fn pause_internal(env: Env, reason: Option<Symbol>) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let now = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::GlobalPaused, &true);
        env.storage().instance().set(&DataKey::PausedSince, &now);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        match &reason {
            Some(r) => env.storage().instance().set(&DataKey::PauseReason, r),
            None => env.storage().instance().remove(&DataKey::PauseReason),
        }
        env.events().publish(
            (
                symbol_short!("emergency"),
                soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_PAUSED_V2),
            ),
            remitwise_common::events::PauseEvent {
                paused_at: now,
                paused_by: admin.clone(),
            },
        );
        Ok(())
    }

    /// Returns the reason recorded by [`pause_with_reason`], or `None` if the
    /// contract is not paused or was paused via plain [`pause`] with no reason.
    /// Cleared on `unpause` and `clear_emergency_state`.
    pub fn pause_reason(env: Env) -> Option<Symbol> {
        env.storage().instance().get(&DataKey::PauseReason)
    }

    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let schedule: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UnpauseSchedule)
            .ok_or(Error::InvalidSchedule)?;
        let now = env.ledger().timestamp();
        if now < schedule {
            return Err(Error::Unauthorized);
        }
        env.storage().instance().set(&DataKey::GlobalPaused, &false);
        env.storage().instance().remove(&DataKey::PausedSince);
        env.storage().instance().remove(&DataKey::PauseReason);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        env.events().publish(
            (
                symbol_short!("emergency"),
                soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_UNPAUSED_V2),
            ),
            remitwise_common::events::UnpauseEvent {
                unpaused_at: now,
                unpaused_by: admin.clone(),
            },
        );
        Ok(())
    }

    /// Admin-only recovery path that immediately clears the global emergency
    /// pause, bypassing the unpause timelock.
    ///
    /// [unpause] can only succeed once a future [schedule_unpause] has been set
    /// *and* the ledger has reached it. A re-[pause] removes any pending
    /// schedule (see [pause]), so a contract can be left globally paused with no
    /// valid schedule — at which point `unpause` fails with
    /// [Error::InvalidSchedule] and the only options were to wait out a stale
    /// schedule or redeploy. This entrypoint lets the admin recover from that
    /// stuck-paused state in a single call.
    ///
    /// Sets [DataKey::GlobalPaused] to `false` and removes any pending
    /// [DataKey::UnpauseSchedule]. It is idempotent: calling it when the
    /// contract is not paused is a successful no-op. Module- and function-level
    /// pauses are intentionally left untouched — lift those with
    /// [unpause_module] / [unpause_function].
    ///
    /// Emits an `emergency`/`cleared` event on success.
    pub fn clear_emergency_state(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::GlobalPaused, &false);
        env.storage().instance().remove(&DataKey::PausedSince);
        env.storage().instance().remove(&DataKey::PauseReason);
        env.storage().instance().remove(&DataKey::UnpauseSchedule);
        env.events().publish(
            (
                symbol_short!("emergency"),
                soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_UNPAUSED_V2),
            ),
            remitwise_common::events::UnpauseEvent {
                unpaused_at: env.ledger().timestamp(),
                unpaused_by: admin.clone(),
            },
        );
        Ok(())
    }

    pub fn schedule_unpause(env: Env, time: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if time < env.ledger().timestamp() {
            return Err(Error::InvalidSchedule);
        }
        env.storage()
            .instance()
            .set(&DataKey::UnpauseSchedule, &time);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::GlobalPaused)
            .unwrap_or(false)
    }

    pub fn get_paused_since(env: Env) -> Option<u64> {
        if Self::is_paused(env.clone()) {
            env.storage().instance().get(&DataKey::PausedSince)
        } else {
            None
        }
    }

    pub fn get_pause_state(env: Env) -> remitwise_common::PauseState {
        remitwise_common::PauseState {
            paused: Self::is_paused(env.clone()),
            paused_since: Self::get_paused_since(env),
        }
    }

    /// Returns the pending unpause timestamp set by `schedule_unpause`, or `None` if no unpause
    /// is scheduled. The schedule is cleared when `pause` or `unpause` is called.
    ///
    /// No authentication required — the schedule is observable on-chain.
    pub fn get_unpause_schedule(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::UnpauseSchedule)
    }

    /// Returns the list of paused function names for `module_id`, or an empty vec if none.
    ///
    /// Bounded by [`MAX_PAUSED_FUNCTIONS`] (10); no pagination required.
    ///
    /// Note: a function may appear unpaused here yet still be blocked if the module
    /// (`is_module_paused`) or global pause (`is_paused`) is active — the precedence order
    /// is global → module → function.
    ///
    /// No authentication required — state is observable on-chain.
    pub fn list_paused_functions(env: Env, module_id: Symbol) -> Vec<Symbol> {
        env.storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns whether `module_id` is directly paused via `pause_module`.
    ///
    /// Note: this reflects only the module-level flag. For the full precedence check
    /// (global → module → function) use `is_function_paused`.
    ///
    /// No authentication required — state is observable on-chain.
    pub fn is_module_paused(env: Env, module_id: Symbol) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::ModulePaused(module_id))
            .unwrap_or(false)
    }

    pub fn pause_function(env: Env, module_id: Symbol, func: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let mut paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id.clone()))
            .unwrap_or(Vec::new(&env));
        if !paused_funcs.contains(func.clone()) {
            if paused_funcs.len() >= MAX_PAUSED_FUNCTIONS {
                return Err(Error::LimitExceeded);
            }
            paused_funcs.push_back(func.clone());
            env.storage()
                .instance()
                .set(&DataKey::PausedFunctions(module_id.clone()), &paused_funcs);
            env.events().publish(
                (
                    symbol_short!("emergency"),
                    soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_F_PAUSED_V2),
                ),
                remitwise_common::events::FunctionPauseEvent {
                    module_id: module_id.clone(),
                    func: func.clone(),
                    paused_at: env.ledger().timestamp(),
                    paused_by: admin.clone(),
                },
            );
        }
        Ok(())
    }

    pub fn unpause_function(env: Env, module_id: Symbol, func: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        let mut paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id.clone()))
            .unwrap_or(Vec::new(&env));
        if let Some(index) = paused_funcs.first_index_of(func.clone()) {
            paused_funcs.remove(index);
            env.storage()
                .instance()
                .set(&DataKey::PausedFunctions(module_id.clone()), &paused_funcs);
            env.events().publish(
                (
                    symbol_short!("emergency"),
                    soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_F_UNPAUSED_V2),
                ),
                remitwise_common::events::FunctionUnpauseEvent {
                    module_id: module_id.clone(),
                    func: func.clone(),
                    unpaused_at: env.ledger().timestamp(),
                    unpaused_by: admin.clone(),
                },
            );
        }
        Ok(())
    }

    pub fn is_function_paused(env: Env, module_id: Symbol, func: Symbol) -> bool {
        if env
            .storage()
            .instance()
            .get(&DataKey::GlobalPaused)
            .unwrap_or(false)
        {
            return true;
        }
        if env
            .storage()
            .instance()
            .get(&DataKey::ModulePaused(module_id.clone()))
            .unwrap_or(false)
        {
            return true;
        }
        let paused_funcs: Vec<Symbol> = env
            .storage()
            .instance()
            .get(&DataKey::PausedFunctions(module_id))
            .unwrap_or(Vec::new(&env));
        paused_funcs.contains(func)
    }

    pub fn pause_module(env: Env, module_id: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ModulePaused(module_id.clone()), &true);
        env.events().publish(
            (
                symbol_short!("emergency"),
                soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_M_PAUSED_V2),
            ),
            remitwise_common::events::ModulePauseEvent {
                module_id: module_id.clone(),
                paused_at: env.ledger().timestamp(),
                paused_by: admin.clone(),
            },
        );
        Ok(())
    }

    pub fn unpause_module(env: Env, module_id: Symbol) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ModulePaused(module_id.clone()), &false);
        env.events().publish(
            (
                symbol_short!("emergency"),
                soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_M_UNPAUSED_V2),
            ),
            remitwise_common::events::ModuleUnpauseEvent {
                module_id: module_id.clone(),
                unpaused_at: env.ledger().timestamp(),
                unpaused_by: admin.clone(),
            },
        );
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests — transfer_admin authorization and post-transfer privilege revocation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};

    fn setup_env() -> (Env, EmergencyKillswitchClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        (env, client)
    }

    /// Assert that a `transfer_admin` call with the correct epoch succeeds.
    fn assert_transfer_admin_succeeds(
        client: &EmergencyKillswitchClient<'_>,
        new_admin: &Address,
        ep: u64,
    ) {
        let res = client.try_transfer_admin(new_admin, &ep);
        assert_eq!(res, Ok(Ok(())));
    }

    /// Assert that a `transfer_admin` call fails with the expected error.
    fn assert_transfer_admin_fails(
        client: &EmergencyKillswitchClient<'_>,
        new_admin: &Address,
        ep: u64,
        expected: Error,
    ) {
        let res = client.try_transfer_admin(new_admin, &ep);
        assert_eq!(res, Err(Ok(expected)));
    }

    /// transfer_admin before initialize returns NotInitialized.
    #[test]
    fn test_transfer_admin_before_init_returns_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);

        let res = client.try_transfer_admin(&new_admin, &0);
        assert_eq!(res, Err(Ok(Error::NotInitialized)));
    }

    /// Transferring to the current admin is rejected (prevents accidental re-auth).
    #[test]
    fn test_transfer_admin_to_self_rejected() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);

        client.initialize(&admin);

        assert_transfer_admin_fails(&client, &admin, 0, Error::InvalidAdmin);
    }

    /// After a successful transfer, the new admin can pause and unpause,
    /// proving DataKey::Admin was updated.
    #[test]
    fn test_transfer_admin_grants_powers_to_new_admin() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        assert_transfer_admin_succeeds(&client, &new_admin, 0);

        // New admin can pause
        client.pause();
        assert!(client.is_paused());

        // New admin can schedule unpause and unpause
        let now = env.ledger().timestamp();
        client.schedule_unpause(&(now + 100));
        env.ledger().with_mut(|li| li.timestamp = now + 200);
        client.unpause();
        assert!(!client.is_paused());
    }

    /// After transfer, new admin can use pause_module and unpause_module.
    #[test]
    fn test_new_admin_can_pause_module_after_transfer() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);
        assert_transfer_admin_succeeds(&client, &new_admin, 0);

        client.pause_module(&symbol_short!("insurance"));
        assert!(client.is_module_paused(&symbol_short!("insurance")));

        client.unpause_module(&symbol_short!("insurance"));
        assert!(!client.is_module_paused(&symbol_short!("insurance")));
    }

    /// Double transfer (A→B→C) — all intermediate transfers succeed
    /// and the final admin retains full control.
    #[test]
    fn test_double_transfer() {
        let (env, client) = setup_env();
        let admin_a = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.initialize(&admin_a);
        assert_transfer_admin_succeeds(&client, &admin_b, 0);
        assert_transfer_admin_succeeds(&client, &admin_c, 0);

        // Admin C can pause
        client.pause();
        assert!(client.is_paused());
    }

    /// Transferring to the contract's own address is rejected (prevents bricking).
    /// Uses the address returned by `register_contract` as the self-address.
    #[test]
    fn test_transfer_admin_to_contract_self_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // transfer_admin to the contract's own address
        let res = client.try_transfer_admin(&contract_id, &0);
        assert_eq!(res, Err(Ok(Error::InvalidAdmin)));
    }

    #[test]
    fn test_paused_since_and_pause_state() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        assert_eq!(client.get_paused_since(), None);
        let initial_state = client.get_pause_state();
        assert!(!initial_state.paused);
        assert_eq!(initial_state.paused_since, None);

        let now = 1_000_000u64;
        env.ledger().with_mut(|li| li.timestamp = now);
        client.pause();

        assert_eq!(client.get_paused_since(), Some(now));
        let paused_state = client.get_pause_state();
        assert!(paused_state.paused);
        assert_eq!(paused_state.paused_since, Some(now));

        client.schedule_unpause(&(now + 100));
        env.ledger().with_mut(|li| li.timestamp = now + 200);
        client.unpause();

        assert_eq!(client.get_paused_since(), None);
        let unpaused_state = client.get_pause_state();
        assert!(!unpaused_state.paused);
        assert_eq!(unpaused_state.paused_since, None);
    }

    /// Verify DataKey::Admin value is updated by checking a second transfer
    /// succeeds (new admin is stored).
    #[test]
    fn test_transfer_admin_updates_stored_admin() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let admin_b = Address::generate(&env);
        let admin_c = Address::generate(&env);

        client.initialize(&admin);
        assert_transfer_admin_succeeds(&client, &admin_b, 0);
        // A→B succeeded. Now B→C should also succeed, proving B is stored.
        assert_transfer_admin_succeeds(&client, &admin_c, 0);
        // C can pause, proving C is now admin
        client.pause();
        assert!(client.is_paused());
    }

    // ── Kill-switch epoch guard tests ─────────────────────────────────────

    /// A transfer with a wrong epoch returns EpochMismatch.
    #[test]
    fn test_transfer_admin_wrong_epoch_rejected() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);

        // Epoch 0 is the default, so providing epoch 1 should fail.
        assert_transfer_admin_fails(&client, &new_admin, 1, Error::EpochMismatch);
    }

    /// After bumping the epoch, a transfer with the old epoch is rejected.
    #[test]
    fn test_stale_epoch_rejected_after_bump() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        client.initialize(&admin);

        // Bump the epoch to 1
        let new_epoch = client.bump_kill_switch_epoch(&admin);
        assert_eq!(new_epoch, 1);

        // Transfer with old epoch 0 should now fail
        assert_transfer_admin_fails(&client, &new_admin, 0, Error::EpochMismatch);

        // Transfer with new epoch 1 should succeed
        assert_transfer_admin_succeeds(&client, &new_admin, 1);
    }

    /// get_kill_switch_epoch returns the current epoch.
    #[test]
    fn test_get_kill_switch_epoch_after_initialize() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // After init the epoch must be 0
        let ep = client.get_kill_switch_epoch();
        assert_eq!(ep, 0);
    }

    /// get_kill_switch_epoch returns 0 before initialize (default, no storage).
    #[test]
    fn test_get_kill_switch_epoch_before_initialize() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let ep = client.get_kill_switch_epoch();
        assert_eq!(ep, 0);
    }

    /// require_killswitch_epoch passes with correct epoch.
    #[test]
    fn test_require_killswitch_epoch_ok() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let res = client.try_require_killswitch_epoch(&0);
        assert_eq!(res, Ok(Ok(())));
    }

    /// require_killswitch_epoch fails with wrong epoch.
    #[test]
    fn test_require_killswitch_epoch_fails() {
        let (env, client) = setup_env();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let res = client.try_require_killswitch_epoch(&42);
        assert_eq!(res, Err(Ok(Error::EpochMismatch)));
    }

    /// bump_kill_switch_epoch requires initialization.
    #[test]
    fn test_bump_kill_switch_epoch_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let caller = Address::generate(&env);

        let res = client.try_bump_kill_switch_epoch(&caller);
        assert_eq!(res, Err(Ok(Error::NotInitialized)));
    }

    /// bump_kill_switch_epoch requires the admin caller argument to match the stored admin.
    #[test]
    fn test_bump_kill_switch_epoch_unauthorized_caller() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let stranger = Address::generate(&env);

        env.mock_all_auths();
        client.initialize(&admin);

        // Authorize the stranger to pass require_auth, but it should still fail
        // because caller != admin
        let res = client.try_bump_kill_switch_epoch(&stranger);
        assert_eq!(res, Err(Ok(Error::Unauthorized)));
    }
}

#[cfg(test)]
mod threshold_tests;

// ─────────────────────────────────────────────────────────────────────────────
// Expanded kill-switch-epoch guard tests (#1293)
//
// Covers: same epoch, off-by-one (too low, too high), ancient epoch,
// consecutive bumps, replay with stale epoch, epoch boundary semantics,
// and error discriminant stability.
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod kill_switch_epoch_guard_comprehensive_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup() -> (Env, EmergencyKillswitchClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── Happy path: exact epoch match ─────────────────────────────────────

    /// `require_killswitch_epoch` passes when the caller supplies the correct
    /// epoch (0 immediately after initialization).
    #[test]
    fn correct_epoch_zero_passes_after_init() {
        let (_env, client, _admin) = setup();
        let res = client.try_require_killswitch_epoch(&0u64);
        assert_eq!(res, Ok(Ok(())));
    }

    /// After one bump the epoch is 1; supplying 1 must pass.
    #[test]
    fn correct_epoch_one_passes_after_single_bump() {
        let (_env, client, admin) = setup();
        client.bump_kill_switch_epoch(&admin);
        let res = client.try_require_killswitch_epoch(&1u64);
        assert_eq!(res, Ok(Ok(())));
    }

    /// After multiple consecutive bumps, supplying the resulting epoch passes.
    #[test]
    fn correct_epoch_passes_after_five_consecutive_bumps() {
        let (_env, client, admin) = setup();
        let mut last = 0u64;
        for _ in 0..5 {
            last = client.bump_kill_switch_epoch(&admin);
        }
        assert_eq!(last, 5);
        let res = client.try_require_killswitch_epoch(&5u64);
        assert_eq!(res, Ok(Ok(())));
    }

    // ── Off-by-one: one below current epoch ───────────────────────────────

    /// Supplying epoch = current - 1 (one below) is rejected with EpochMismatch.
    /// This is the classic "stale authorization" off-by-one.
    #[test]
    fn one_below_current_epoch_rejected_off_by_one() {
        let (_env, client, admin) = setup();
        client.bump_kill_switch_epoch(&admin); // epoch is now 1

        // Supply 0 (one below current 1) — must fail.
        let res = client.try_require_killswitch_epoch(&0u64);
        assert_eq!(res, Err(Ok(Error::EpochMismatch)));
    }

    /// Supplying epoch = current + 1 (one above) is also rejected — the guard
    /// requires an exact match, not just >= or <=.
    #[test]
    fn one_above_current_epoch_rejected_off_by_one() {
        let (_env, client, _admin) = setup();
        // Epoch is 0 after init; supply 1 (one above).
        let res = client.try_require_killswitch_epoch(&1u64);
        assert_eq!(res, Err(Ok(Error::EpochMismatch)));
    }

    // ── Ancient epoch ─────────────────────────────────────────────────────

    /// An ancient epoch (many bumps ago) is rejected. Pins that the guard does
    /// not compare with >= (which would allow old epochs after bumps).
    #[test]
    fn ancient_epoch_rejected_after_many_bumps() {
        let (_env, client, admin) = setup();
        // Bump 10 times — current epoch is now 10.
        for _ in 0..10 {
            client.bump_kill_switch_epoch(&admin);
        }

        // Epoch 0 (the initial value, now 10 bumps stale) must be rejected.
        let res = client.try_require_killswitch_epoch(&0u64);
        assert_eq!(res, Err(Ok(Error::EpochMismatch)));

        // Epoch 5 (half-stale) must also be rejected.
        let res = client.try_require_killswitch_epoch(&5u64);
        assert_eq!(res, Err(Ok(Error::EpochMismatch)));
    }

    // ── Replay attack simulation ──────────────────────────────────────────

    /// Simulate a replay attack: obtain a valid authorization at epoch N,
    /// bump the epoch to N+1, then try to replay the old authorization.
    /// The replay must fail with EpochMismatch.
    #[test]
    fn stale_authorization_replay_rejected_after_epoch_bump() {
        let (env, client, admin) = setup();
        let new_admin = Address::generate(&env);

        // Capture current epoch (0) — represents a "signed" transfer_admin call.
        let captured_epoch = client.get_kill_switch_epoch();
        assert_eq!(captured_epoch, 0);

        // Transfer to new_admin using epoch 0 (valid at capture time).
        let res = client.try_transfer_admin(&new_admin, &captured_epoch);
        assert_eq!(res, Ok(Ok(())));

        // New admin bumps the epoch to invalidate any further epoch-0 auths.
        let new_epoch = client.bump_kill_switch_epoch(&new_admin);
        assert_eq!(new_epoch, 1);

        // A second address tries to replay the old epoch-0 authorization.
        let another_admin = Address::generate(&env);
        let replay = client.try_transfer_admin(&another_admin, &captured_epoch);
        assert_eq!(
            replay,
            Err(Ok(Error::EpochMismatch)),
            "replay with stale epoch must be rejected after bump"
        );
    }

    // ── Consecutive bump semantics ────────────────────────────────────────

    /// Every bump increments by exactly one and returns the new epoch.
    #[test]
    fn consecutive_bumps_increment_by_one_and_return_new_epoch() {
        let (_env, client, admin) = setup();
        for expected_new in 1u64..=10 {
            let returned = client.bump_kill_switch_epoch(&admin);
            assert_eq!(
                returned, expected_new,
                "bump must return the new epoch (expected {expected_new})"
            );
            let stored = client.get_kill_switch_epoch();
            assert_eq!(
                stored, expected_new,
                "stored epoch must equal returned epoch after bump {expected_new}"
            );
        }
    }

    /// After each bump, the previous epoch is immediately rejected.
    #[test]
    fn previous_epoch_rejected_immediately_after_each_bump() {
        let (_env, client, admin) = setup();
        for bump_count in 1u64..=5 {
            client.bump_kill_switch_epoch(&admin);
            // The epoch that was valid before this bump is now stale.
            let stale_epoch = bump_count - 1;
            let res = client.try_require_killswitch_epoch(&stale_epoch);
            assert_eq!(
                res,
                Err(Ok(Error::EpochMismatch)),
                "epoch {stale_epoch} must be rejected immediately after bump to {bump_count}"
            );
        }
    }

    // ── Epoch at boundary: u64::MAX - 1 ──────────────────────────────────

    /// The overflow guard in `bump_kill_switch_epoch` uses `checked_add`, which
    /// returns an error (mapped to `Error::InvalidAdmin`) when the epoch would
    /// wrap past u64::MAX.  This test bumps to u64::MAX - 1 via storage
    /// manipulation to verify the overflow guard fires on the next bump.
    ///
    /// We write the epoch directly to instance storage to avoid the prohibitive
    /// cost of calling `bump_kill_switch_epoch` u64::MAX - 1 times.
    #[test]
    fn overflow_guard_fires_at_u64_max() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Inject u64::MAX directly into instance storage — the contract's
        // bump function uses checked_add so the next bump must overflow.
        env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::KillSwitchEpoch, &u64::MAX);
        });

        assert_eq!(client.get_kill_switch_epoch(), u64::MAX);

        // The next bump would overflow u64 — must return an error.
        let res = client.try_bump_kill_switch_epoch(&admin);
        // bump_kill_switch_epoch maps checked_add None to Error::InvalidAdmin
        assert_eq!(res, Err(Ok(Error::InvalidAdmin)));
    }

    // ── Error discriminant stability ──────────────────────────────────────

    /// The EpochMismatch discriminant must be 7 (ABI contract — pinned for
    /// encoding stability across contract versions and downstream integrators).
    #[test]
    fn epoch_mismatch_error_discriminant_is_seven() {
        assert_eq!(
            Error::EpochMismatch as u32,
            7u32,
            "Error::EpochMismatch discriminant must be 7 (ABI contract)"
        );
    }

    // ── get_kill_switch_epoch — observable without auth ───────────────────

    /// `get_kill_switch_epoch` always reflects the current epoch and its
    /// return value matches what `bump_kill_switch_epoch` reports.
    #[test]
    fn get_kill_switch_epoch_reflects_current_epoch_after_bumps() {
        let (_env, client, admin) = setup();

        // Before any bump: epoch must be 0.
        assert_eq!(client.get_kill_switch_epoch(), 0);

        // After first bump: epoch must be 1.
        client.bump_kill_switch_epoch(&admin);
        assert_eq!(client.get_kill_switch_epoch(), 1);

        // After second bump: epoch must be 2.
        client.bump_kill_switch_epoch(&admin);
        assert_eq!(
            client.get_kill_switch_epoch(),
            2,
            "get_kill_switch_epoch must return 2 after two bumps"
        );
    }

    // ── transfer_admin epoch binding ──────────────────────────────────────

    /// `transfer_admin` requires the caller to supply the current epoch. A
    /// call with a future epoch (one not yet reached) must also fail, preventing
    /// pre-authorization for a future epoch.
    #[test]
    fn transfer_admin_with_future_epoch_rejected() {
        let (env, client, _admin) = setup();
        let new_admin = Address::generate(&env);
        // Current epoch is 0; supply 99 (future, never bumped to).
        let res = client.try_transfer_admin(&new_admin, &99u64);
        assert_eq!(
            res,
            Err(Ok(Error::EpochMismatch)),
            "transfer_admin with future epoch must be rejected"
        );
    }

    /// `transfer_admin` with epoch 0 at initialization succeeds, confirming
    /// the default epoch from `initialize` is 0 and is the only valid value.
    #[test]
    fn transfer_admin_with_correct_epoch_zero_succeeds() {
        let (env, client, _admin) = setup();
        let new_admin = Address::generate(&env);
        let res = client.try_transfer_admin(&new_admin, &0u64);
        assert_eq!(res, Ok(Ok(())));
    }
}
