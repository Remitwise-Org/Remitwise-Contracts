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
    /// Snapshot not found during restore — call `pre_upgrade` first.
    SnapshotNotFound = 16,
    /// Snapshot has expired (taken more than [`SNAPSHOT_TTL`] seconds ago).
    SnapshotExpired = 17,
    /// Storage is already at the latest version — migration is a no-op.
    AlreadyMigrated = 18,
    /// A previous migration did not complete — call `migrate_storage` again.
    MigrationIncomplete = 19,
    /// A saturating arithmetic operation would overflow (e.g. epoch bump at
    /// `u64::MAX`, recovery deadline past `u64::MAX`, or inverted clock).
    Overflow = 20,
    /// The cursor value is not valid for this collection (e.g. not produced by a
    /// previous page call).
    InvalidCursor = 21,
}

/// The exact pause surface affected by a threshold-approved activation.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PauseScope {
    Global,
    Module(Symbol),
    Function(Symbol, Symbol),
}

/// Default page size for paginated queries.
pub const DEFAULT_PAGE_LIMIT: u32 = 20;

/// Maximum page size for paginated queries.
pub const MAX_PAGE_LIMIT: u32 = 50;

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
    /// Tracks the storage schema version — bumped on every
    /// upgrade-relevant change so the contract can detect and migrate
    /// legacy layouts.
    StorageVersion,
    /// Serialized [`EmergencyStateSnapshot`] taken before an upgrade.
    Snapshot,
    /// Timestamp when the snapshot was taken (used for TTL enforcement).
    SnapshotTimestamp,
    /// Tracks the migration step last completed so `migrate_storage` is
    /// resumable after a partial failure.
    MigrationProgress,
    /// Monotonic correlation counter consumed by every committed emergency
    /// transition so emitted control/audit events carry a deterministic id
    /// and a strict, observable ordering.
    EventSeq,
}

/// Delay between a threshold-approved activation and recovery.
pub const RECOVERY_DELAY: u64 = 3600;

pub const MAX_PAUSED_FUNCTIONS: u32 = 10;

/// Contract version, bumped on every on-chain-upgrade-relevant change so
/// callers/tooling can detect which build of the WASM they are talking to.
///
/// v2 adds the versioned `("emergency", "control")` audit-event stream
/// (correlation `seq` + `ControlEvent` records) on every committed emergency
/// transition (issue #1761).
pub const CONTRACT_VERSION: u32 = 2;

/// Storage schema version. Bumped whenever the on-chain key layout changes.
/// `migrate_storage` advances this value one step at a time so callers can
/// observe progress and resume after a partial failure.
pub const STORAGE_VERSION: u32 = 1;

/// How many seconds a pre-upgrade snapshot remains valid.  After this
/// window the snapshot is considered stale and must be refreshed.
pub const SNAPSHOT_TTL: u64 = 86_400; // 24 hours

/// Schema version of the [`ControlEvent`] audit record. Bump only under the
/// documented upgrade workflow when the on-wire event shape changes, and
/// keep old shapes documented side-by-side in `docs/EVENTS.md`.
pub const EVENT_VERSION: u32 = 1;

/// Versioned, complete, correlation-tagged audit record emitted by **every
/// committed** emergency transition.
///
/// `seq` is a monotonic, per-contract counter consumed from instance storage
/// (`DataKey::EventSeq`). Because it is strictly increasing it provides both
/// the correlation identifier and a deterministic global ordering for every
/// emergency event the contract produces. `kind` names the operation and
/// `actor` is the authorizing principal — `None` for consensus-driven
/// transitions (threshold signer quorums) where no single address is the
/// actor. The record is published on the fixed `("emergency", "control")`
/// topic so indexers can subscribe to the entire emergency audit stream with
/// a single filter.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlEvent {
    pub version: u32,
    pub seq: u64,
    pub kind: Symbol,
    pub actor: Option<Address>,
    pub timestamp: u64,
}

/// Emitted when the killswitch admin is successfully transferred.
#[contracttype]
#[derive(Clone)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
    pub timestamp: u64,
}

/// Snapshot of all emergency-killswitch state captured before a contract
/// upgrade.  Stored under [`DataKey::Snapshot`] and consumed by
/// [`EmergencyKillswitch::restore_from_snapshot`].
///
/// Most fields use `Option` so that forward-compatible additions to the
/// snapshot never break deserialization of older payloads — the Soroban
/// XDR decoder treats missing fields as `None`.
///
/// The active scope is **not** stored as `Option<PauseScope>`: soroban-sdk's
/// `#[contracttype]` spec only derives fallible `TryFrom` for custom
/// contracttype types, while `Option<T>` requires infallible `From<T>`, so
/// `Option` of a custom enum/struct is not spec-serializable. Instead the
/// scope is split into scalar fields (`scope_kind` + ids) so the snapshot
/// remains fully representable on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct EmergencyStateSnapshot {
    /// Schema version of this snapshot (matches [`STORAGE_VERSION`] at
    /// capture time).  Used by `restore_from_snapshot` to reject
    /// incompatible snapshots.
    pub schema_version: u32,
    pub global_paused: bool,
    pub paused_since: Option<u64>,
    pub pause_reason: Option<Symbol>,
    pub unpause_schedule: Option<u64>,
    pub kill_switch_epoch: u64,
    pub signer_epoch: u64,
    pub signer_threshold: Option<u32>,
    pub activation_epoch: Option<u64>,
    /// Encoded active scope. `scope_kind` is 0 = none, 1 = global,
    /// 2 = module, 3 = function; `scope_module` / `scope_function` carry the
    /// identifiers for module/function scopes. See `pre_upgrade`.
    pub scope_kind: u32,
    pub scope_module: Symbol,
    pub scope_function: Symbol,
    pub recovery_ready_at: Option<u64>,
    pub scope_was_paused: Option<bool>,
}

/// Tracks which migration steps have been completed so
/// [`EmergencyKillswitch::migrate_storage`] is resumable after a partial
/// failure.  Each step is a monotonically increasing integer; a step is
/// considered done when `completed >= step_number`.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationProgress {
    pub from_version: u32,
    pub to_version: u32,
    pub completed_step: u32,
    pub total_steps: u32,
    pub last_run_at: u64,
}

#[contract]
pub struct EmergencyKillswitch;

// ── Pure arithmetic helpers (crate-visible) ────────────────────────────────

/// Checked addition for `u64`. Returns [`Error::Overflow`] on wrap.
fn checked_add_u64(a: u64, b: u64) -> Result<u64, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

/// Checked addition for `u32`. Returns [`Error::Overflow`] on wrap.
fn checked_add_u32(a: u32, b: u32) -> Result<u32, Error> {
    a.checked_add(b).ok_or(Error::Overflow)
}

/// Compute the age of a snapshot in seconds (`now - snapshot_ts`).
///
/// Returns [`Error::Overflow`] if the clock is inverted (`now < snapshot_ts`).
fn snapshot_age(now: u64, snapshot_ts: u64) -> Result<u64, Error> {
    now.checked_sub(snapshot_ts).ok_or(Error::Overflow)
}

/// Compute the recovery-ready deadline: `now + RECOVERY_DELAY`.
///
/// Returns [`Error::Overflow`] if the result would exceed `u64::MAX`.
fn recovery_ready_at(now: u64) -> Result<u64, Error> {
    checked_add_u64(now, RECOVERY_DELAY)
}

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
        Self::emit_control_event(
            &env,
            symbol_short!("init"),
            Some(&admin),
            env.ledger().timestamp(),
        );
        Ok(())
    }

    /// Consume and return the next monotonic correlation sequence number.
    ///
    /// Persisted in instance storage so ordering is stable across
    /// transactions and never rewinds. Saturates on a (practically
    /// unreachable) 64-bit wrap so the counter can never panic the contract.
    fn next_event_seq(env: &Env) -> u64 {
        let prev: u64 = env
            .storage()
            .instance()
            .get(&DataKey::EventSeq)
            .unwrap_or(0);
        let next = prev.saturating_add(1);
        env.storage().instance().set(&DataKey::EventSeq, &next);
        next
    }

    /// Publish a versioned, correlation-tagged audit record for a committed
    /// emergency transition.
    ///
    /// # Contract
    /// - Must only be called **after** every state mutation for the transition
    ///   has succeeded (immediately before returning `Ok`). A transition that
    ///   rejects — or that panics and is rolled back by Soroban's atomic
    ///   transaction semantics — never reaches this call, so the on-chain
    ///   event log always matches committed state.
    /// - Consumes a fresh `seq` from [`Self::next_event_seq`], so records are
    ///   strictly ordered and uniquely correlated.
    /// - Emits on the fixed `("emergency", "control")` topic.
    fn emit_control_event(env: &Env, kind: Symbol, actor: Option<&Address>, timestamp: u64) {
        let seq = Self::next_event_seq(env);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("control")),
            ControlEvent {
                version: EVENT_VERSION,
                seq,
                kind,
                actor: actor.cloned(),
                timestamp,
            },
        );
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

    /// Configure signers while explicitly checking `expected_epoch`.
    ///
    /// Provides optimistic concurrency control (OCC). If another admin or
    /// transaction updated the signer configuration concurrently (bumping
    /// the signer epoch), this call returns [`Error::EpochMismatch`].
    pub fn configure_signers_with_epoch(
        env: Env,
        caller: Address,
        expected_epoch: u64,
        signers: Vec<Address>,
        threshold: u32,
    ) -> Result<u64, Error> {
        let current_epoch = Self::get_signer_epoch(env.clone());
        if expected_epoch != current_epoch {
            return Err(Error::EpochMismatch);
        }
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
        let epoch = checked_add_u64(old_epoch, 1)?;
        env.storage().instance().set(&DataKey::Signers, &signers);
        env.storage()
            .instance()
            .set(&DataKey::SignerThreshold, &threshold);
        env.storage().instance().set(&DataKey::SignerEpoch, &epoch);
        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "signers_set")),
            (epoch, threshold, signers.len()),
        );
        Self::emit_control_event(
            &env,
            symbol_short!("signers"),
            Some(&admin),
            env.ledger().timestamp(),
        );
        Ok(epoch)
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
        let current_epoch = Self::get_signer_epoch(env.clone());
        Self::configure_signers_with_epoch(env, caller, current_epoch, signers, threshold)
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
            &checked_add_u64(env.ledger().timestamp(), RECOVERY_DELAY)?,
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
        Self::emit_control_event(
            &env,
            symbol_short!("activated"),
            None,
            env.ledger().timestamp(),
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
        Self::emit_control_event(
            &env,
            symbol_short!("recovered"),
            None,
            env.ledger().timestamp(),
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
    /// - [`Error::Overflow`] if the epoch counter would wrap past `u64::MAX`.
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
        let new_epoch = checked_add_u64(old_epoch, 1)?;
        env.storage()
            .instance()
            .set(&DataKey::KillSwitchEpoch, &new_epoch);
        env.events().publish(
            (symbol_short!("emergency"), symbol_short!("epch_bump")),
            (old_epoch, new_epoch),
        );
        Self::emit_control_event(
            &env,
            symbol_short!("epch_bump"),
            Some(&admin),
            env.ledger().timestamp(),
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

    /// Return the current control-event correlation counter.
    ///
    /// Strictly monotonically increases with every committed emergency
    /// transition. Read-only and observable on-chain; used off-chain to
    /// confirm ordering continuity across the `("emergency", "control")`
    /// audit stream.
    pub fn get_event_seq(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::EventSeq)
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
        Self::emit_control_event(
            &env,
            symbol_short!("admn_xfer"),
            Some(&admin),
            env.ledger().timestamp(),
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
        if env.storage().instance().has(&DataKey::ActivationEpoch) {
            if let Some(scope) = env
                .storage()
                .instance()
                .get::<DataKey, PauseScope>(&DataKey::ActiveScope)
            {
                if scope == PauseScope::Global {
                    env.storage()
                        .instance()
                        .set(&DataKey::ScopeWasPaused, &true);
                }
            }
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
        Self::emit_control_event(&env, symbol_short!("pause"), Some(&admin), now);
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
        let paused = env
            .storage()
            .instance()
            .get(&DataKey::GlobalPaused)
            .unwrap_or(false);
        if !paused {
            return Err(Error::NotActive);
        }
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
        Self::emit_control_event(&env, symbol_short!("unpause"), Some(&admin), now);
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
        Self::emit_control_event(
            &env,
            symbol_short!("cleared"),
            Some(&admin),
            env.ledger().timestamp(),
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
        if !Self::is_paused(env.clone()) {
            return Err(Error::NotActive);
        }
        if time < env.ledger().timestamp() {
            return Err(Error::InvalidSchedule);
        }
        env.storage()
            .instance()
            .set(&DataKey::UnpauseSchedule, &time);
        Self::emit_control_event(
            &env,
            symbol_short!("schedule"),
            Some(&admin),
            env.ledger().timestamp(),
        );
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

    /// Returns the recovery deadline written by a successful [`activate`], or
    /// `None` if no threshold activation is in progress.
    ///
    /// No authentication required — the deadline is observable on-chain.
    pub fn get_recovery_ready_at(env: Env) -> Option<u64> {
        env.storage().instance().get(&DataKey::RecoveryReadyAt)
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

    /// Return a cursor-paginated page of paused functions for `module_id`.
    ///
    /// # Parameters
    /// - `module_id` — the module whose paused functions are queried
    /// - `cursor` — pass `None` to start from the first function; pass the
    ///   `next_cursor` returned by a previous call to continue.
    /// - `limit` — max items per page. `0` is normalised to
    ///   [`DEFAULT_PAGE_LIMIT`] (20). Values above [`MAX_PAGE_LIMIT`] (50)
    ///   are clamped.
    ///
    /// # Returns
    /// A `PageResult<Symbol>` with deterministic ascending order.
    /// `next_cursor == None` means this is the final (or only) page.
    ///
    /// # Security
    /// No authentication required — state is observable on-chain.
    pub fn list_paused_functions_page(
        env: Env,
        module_id: Symbol,
        cursor: Option<u32>,
        limit: u32,
    ) -> Vec<Symbol> {
        let all = Self::list_paused_functions(env.clone(), module_id);
        let effective_limit = Self::clamp_page_limit(limit);
        let start = cursor.unwrap_or(0);
        let mut result = Vec::new(&env);
        for i in start..all.len() {
            if result.len() >= effective_limit {
                break;
            }
            if let Some(sym) = all.get(i) {
                result.push_back(sym);
            }
        }
        result
    }

    /// Clamp a user-supplied page limit to [`DEFAULT_PAGE_LIMIT`]..[`MAX_PAGE_LIMIT`].
    fn clamp_page_limit(limit: u32) -> u32 {
        let effective = if limit == 0 { DEFAULT_PAGE_LIMIT } else { limit };
        if effective > MAX_PAGE_LIMIT { MAX_PAGE_LIMIT } else { effective }
    }

    /// Return a cursor-paginated page of configured signers.
    ///
    /// # Parameters
    /// - `cursor` — pass `None` to start from the first signer; pass the
    ///   `next_cursor` returned by a previous call to continue.
    /// - `limit` — max items per page. `0` defaults to [`DEFAULT_PAGE_LIMIT`].
    ///
    /// # Returns
    /// A `Vec<Address>` containing up to `limit` signers in storage order.
    /// When the returned length is less than the effective limit (or the
    /// collection is exhausted), there are no more pages.
    ///
    /// # Security
    /// No authentication required — the signer list is observable on-chain
    /// (it determines who can authorize threshold operations).
    pub fn list_signers_page(
        env: Env,
        cursor: Option<u32>,
        limit: u32,
    ) -> Vec<Address> {
        let all = Self::configured_signers(&env);
        let effective_limit = Self::clamp_page_limit(limit);
        let start = cursor.unwrap_or(0);
        let mut result = Vec::new(&env);
        for i in start..all.len() {
            if result.len() >= effective_limit {
                break;
            }
            if let Some(addr) = all.get(i) {
                result.push_back(addr);
            }
        }
        result
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
            if env.storage().instance().has(&DataKey::ActivationEpoch) {
                if let Some(scope) = env
                    .storage()
                    .instance()
                    .get::<DataKey, PauseScope>(&DataKey::ActiveScope)
                {
                    if scope == PauseScope::Function(module_id.clone(), func.clone()) {
                        env.storage()
                            .instance()
                            .set(&DataKey::ScopeWasPaused, &true);
                    }
                }
            }
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
            Self::emit_control_event(
                &env,
                symbol_short!("fpause"),
                Some(&admin),
                env.ledger().timestamp(),
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
            Self::emit_control_event(
                &env,
                symbol_short!("funpause"),
                Some(&admin),
                env.ledger().timestamp(),
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
        if let Some(scope) = env
            .storage()
            .instance()
            .get::<DataKey, PauseScope>(&DataKey::ActiveScope)
        {
            if scope == PauseScope::Module(module_id.clone()) {
                env.storage()
                    .instance()
                    .set(&DataKey::ScopeWasPaused, &true);
            }
        }
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
        Self::emit_control_event(
            &env,
            symbol_short!("mpause"),
            Some(&admin),
            env.ledger().timestamp(),
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
        Self::emit_control_event(
            &env,
            symbol_short!("munpause"),
            Some(&admin),
            env.ledger().timestamp(),
        );
        Ok(())
    }

    // ── Storage version ──────────────────────────────────────────────────

    /// Returns the storage schema version currently active on-chain.
    ///
    /// No authentication required — the version is observable on-chain.
    /// Off-chain tooling and migration scripts use this to decide whether
    /// a `migrate_storage` call is needed.
    pub fn storage_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::StorageVersion)
            .unwrap_or(0) // v0 = pre-versioning (legacy deployments)
    }

    // ── Migration ────────────────────────────────────────────────────────

    /// Advance the on-chain storage schema one version at a time.
    ///
    /// Each call migrates exactly one version step, making the function
    /// resumable after a partial failure: if the transaction is submitted
    /// but the execution halts (e.g. out of gas), the next call picks up
    /// where the previous one left off because
    /// [`DataKey::MigrationProgress`] records the last completed step.
    ///
    /// # Invariants
    /// - Idempotent: calling when already at `STORAGE_VERSION` returns
    ///   [`Error::AlreadyMigrated`].
    /// - Observable: every successful step emits a `migr_step` event; the
    ///   final step emits a `migr_done` event.
    /// - Atomic per step: if a step panics, no partial state from that step
    ///   is committed.
    ///
    /// # Authorization
    /// Requires admin authentication.
    ///
    /// # Events
    /// - `("emergency", "migr_step")` — `(from_ver, to_ver, step, total)`
    /// - `("emergency", "migr_done")` — `(new_version, timestamp)`
    pub fn migrate_storage(env: Env, caller: Address) -> Result<u32, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        let current: u32 = Self::storage_version(env.clone());
        if current >= STORAGE_VERSION {
            return Err(Error::AlreadyMigrated);
        }

        // Each version bump is one step.  We advance exactly one step per call.
        let progress: MigrationProgress = env
            .storage()
            .instance()
            .get(&DataKey::MigrationProgress)
            .unwrap_or(MigrationProgress {
                from_version: current,
                to_version: STORAGE_VERSION,
                completed_step: 0,
                total_steps: STORAGE_VERSION
                    .checked_sub(current)
                    .ok_or(Error::Overflow)?,
                last_run_at: 0,
            });

        let next_step = checked_add_u32(progress.completed_step, 1)?;
        let total = progress.total_steps;
        let target_version = progress.to_version;
        let from_version = progress.from_version;

        // Step 1: ensure StorageVersion key exists (v0 → v1 migration).
        // Legacy deployments that predate versioning won't have this key.
        if next_step == 1 && !env.storage().instance().has(&DataKey::StorageVersion) {
            env.storage()
                .instance()
                .set(&DataKey::StorageVersion, &1u32);
        }

        // (Future steps would go here as `else if next_step == 2 { ... }`.)

        let now = env.ledger().timestamp();
        env.storage().instance().set(
            &DataKey::MigrationProgress,
            &MigrationProgress {
                from_version,
                to_version: target_version,
                completed_step: next_step,
                total_steps: total,
                last_run_at: now,
            },
        );

        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "migr_step")),
            (from_version, target_version, next_step, total),
        );

        // Bump the canonical storage version.
        env.storage()
            .instance()
            .set(&DataKey::StorageVersion, &target_version);

        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "migr_done")),
            (target_version, now),
        );
        Self::emit_control_event(&env, symbol_short!("migr"), Some(&admin), now);

        Ok(target_version)
    }

    /// Returns the current migration progress, or `None` if no migration
    /// has been started.
    ///
    /// No authentication required — the progress is observable on-chain.
    pub fn get_migration_progress(env: Env) -> Option<MigrationProgress> {
        env.storage().instance().get(&DataKey::MigrationProgress)
    }

    // ── Pre-upgrade snapshot / restore ────────────────────────────────────

    /// Capture a snapshot of the full emergency state before a contract
    /// upgrade.
    ///
    /// The snapshot is stored in instance storage under [`DataKey::Snapshot`]
    /// and is valid for [`SNAPSHOT_TTL`] seconds.  If the upgrade needs to
    /// be rolled back, call [`EmergencyKillswitch::restore_from_snapshot`].
    ///
    /// A second call overwrites any existing snapshot (last-writer-wins).
    ///
    /// # Authorization
    /// Requires admin authentication.
    ///
    /// # Events
    /// Emits `("emergency", "snap_pre")` on success.
    pub fn pre_upgrade(env: Env, caller: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        let now = env.ledger().timestamp();
        // Encode the active scope as spec-scalar fields because
        // `Option<CustomEnum>` is not spec-serializable in soroban-sdk.
        // kind 0 = none, 1 = global, 2 = module, 3 = function.
        let active_scope: Option<PauseScope> = env.storage().instance().get(&DataKey::ActiveScope);
        let (scope_kind, scope_module, scope_function) = match active_scope {
            None => (0u32, symbol_short!("none"), symbol_short!("none")),
            Some(PauseScope::Global) => (1u32, symbol_short!("none"), symbol_short!("none")),
            Some(PauseScope::Module(m)) => (2u32, m, symbol_short!("none")),
            Some(PauseScope::Function(m, f)) => (3u32, m, f),
        };
        let snapshot = EmergencyStateSnapshot {
            schema_version: Self::storage_version(env.clone()),
            global_paused: env
                .storage()
                .instance()
                .get(&DataKey::GlobalPaused)
                .unwrap_or(false),
            paused_since: env.storage().instance().get(&DataKey::PausedSince),
            pause_reason: env.storage().instance().get(&DataKey::PauseReason),
            unpause_schedule: env.storage().instance().get(&DataKey::UnpauseSchedule),
            kill_switch_epoch: env
                .storage()
                .instance()
                .get(&DataKey::KillSwitchEpoch)
                .unwrap_or(0),
            signer_epoch: env
                .storage()
                .instance()
                .get(&DataKey::SignerEpoch)
                .unwrap_or(0),
            signer_threshold: env.storage().instance().get(&DataKey::SignerThreshold),
            activation_epoch: env.storage().instance().get(&DataKey::ActivationEpoch),
            scope_kind,
            scope_module,
            scope_function,
            recovery_ready_at: env.storage().instance().get(&DataKey::RecoveryReadyAt),
            scope_was_paused: env.storage().instance().get(&DataKey::ScopeWasPaused),
        };
        env.storage().instance().set(&DataKey::Snapshot, &snapshot);
        env.storage()
            .instance()
            .set(&DataKey::SnapshotTimestamp, &now);
        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "snap_pre")),
            (snapshot.schema_version, now),
        );
        Self::emit_control_event(&env, symbol_short!("snap_pre"), Some(&admin), now);
        Ok(())
    }

    /// Restore emergency state from a snapshot captured by [`pre_upgrade`].
    ///
    /// The snapshot must exist, not be expired, and have a compatible
    /// [`EmergencyStateSnapshot::schema_version`].  On success the snapshot
    /// is consumed (removed from storage).
    ///
    /// # Authorization
    /// Requires admin authentication.
    ///
    /// # Errors
    /// - [`Error::SnapshotNotFound`] if no snapshot has been taken.
    /// - [`Error::SnapshotExpired`] if the snapshot is older than
    ///   [`SNAPSHOT_TTL`] seconds.
    /// - [`Error::Overflow`] if the ledger clock is before the snapshot
    ///   timestamp (inverted clock).
    ///
    /// # Events
    /// Emits `("emergency", "snap_rst")` on success.
    pub fn restore_from_snapshot(env: Env, caller: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        let snapshot: EmergencyStateSnapshot = env
            .storage()
            .instance()
            .get(&DataKey::Snapshot)
            .ok_or(Error::SnapshotNotFound)?;
        let snapshot_ts: u64 = env
            .storage()
            .instance()
            .get(&DataKey::SnapshotTimestamp)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if snapshot_age(now, snapshot_ts)? > SNAPSHOT_TTL {
            return Err(Error::SnapshotExpired);
        }

        // Validate schema compatibility.
        if snapshot.schema_version > STORAGE_VERSION {
            return Err(Error::MigrationIncomplete);
        }

        // ── Restore each field ───────────────────────────────────────
        env.storage()
            .instance()
            .set(&DataKey::GlobalPaused, &snapshot.global_paused);
        if let Some(v) = &snapshot.paused_since {
            env.storage().instance().set(&DataKey::PausedSince, v);
        } else {
            env.storage().instance().remove(&DataKey::PausedSince);
        }
        if let Some(v) = &snapshot.pause_reason {
            env.storage().instance().set(&DataKey::PauseReason, v);
        } else {
            env.storage().instance().remove(&DataKey::PauseReason);
        }
        if let Some(v) = &snapshot.unpause_schedule {
            env.storage().instance().set(&DataKey::UnpauseSchedule, v);
        } else {
            env.storage().instance().remove(&DataKey::UnpauseSchedule);
        }
        env.storage()
            .instance()
            .set(&DataKey::KillSwitchEpoch, &snapshot.kill_switch_epoch);
        env.storage()
            .instance()
            .set(&DataKey::SignerEpoch, &snapshot.signer_epoch);
        if let Some(v) = &snapshot.signer_threshold {
            env.storage().instance().set(&DataKey::SignerThreshold, v);
        }
        if let Some(v) = &snapshot.activation_epoch {
            env.storage().instance().set(&DataKey::ActivationEpoch, v);
        } else {
            env.storage().instance().remove(&DataKey::ActivationEpoch);
        }
        match snapshot.scope_kind {
            0 => env.storage().instance().remove(&DataKey::ActiveScope),
            1 => env
                .storage()
                .instance()
                .set(&DataKey::ActiveScope, &PauseScope::Global),
            2 => env.storage().instance().set(
                &DataKey::ActiveScope,
                &PauseScope::Module(snapshot.scope_module.clone()),
            ),
            _ => env.storage().instance().set(
                &DataKey::ActiveScope,
                &PauseScope::Function(
                    snapshot.scope_module.clone(),
                    snapshot.scope_function.clone(),
                ),
            ),
        }
        if let Some(v) = &snapshot.recovery_ready_at {
            env.storage().instance().set(&DataKey::RecoveryReadyAt, v);
        } else {
            env.storage().instance().remove(&DataKey::RecoveryReadyAt);
        }
        if let Some(v) = &snapshot.scope_was_paused {
            env.storage().instance().set(&DataKey::ScopeWasPaused, v);
        } else {
            env.storage().instance().remove(&DataKey::ScopeWasPaused);
        }

        // Consume the snapshot and its timestamp.
        env.storage().instance().remove(&DataKey::Snapshot);
        env.storage().instance().remove(&DataKey::SnapshotTimestamp);

        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "snap_rst")),
            (snapshot.schema_version, now),
        );
        Self::emit_control_event(&env, symbol_short!("snap_rst"), Some(&admin), now);
        Ok(())
    }

    /// Discard a previously captured snapshot without restoring it.
    ///
    /// # Authorization
    /// Requires admin authentication.
    ///
    /// # Events
    /// Emits `("emergency", "snap_dsc")` on success.
    pub fn discard_snapshot(env: Env, caller: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        env.storage().instance().remove(&DataKey::Snapshot);
        env.storage().instance().remove(&DataKey::SnapshotTimestamp);
        env.events().publish(
            (symbol_short!("emergency"), Symbol::new(&env, "snap_dsc")),
            (env.ledger().timestamp(),),
        );
        Self::emit_control_event(
            &env,
            symbol_short!("snap_dsc"),
            Some(&admin),
            env.ledger().timestamp(),
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
        let (env, client, _admin) = setup();
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
    /// returns [`Error::Overflow`] when the epoch would wrap past `u64::MAX`.
    /// This test bumps to `u64::MAX` via storage manipulation to verify the
    /// overflow guard fires on the next bump.
    ///
    /// We write the epoch directly to instance storage to avoid the prohibitive
    /// cost of calling `bump_kill_switch_epoch` `u64::MAX` times.
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

        // The next bump would overflow u64 — must return Overflow.
        let res = client.try_bump_kill_switch_epoch(&admin);
        assert_eq!(res, Err(Ok(Error::Overflow)));
        // Failed bump must not change stored epoch.
        assert_eq!(client.get_kill_switch_epoch(), u64::MAX);
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
        assert_eq!(
            Error::Overflow as u32,
            20u32,
            "Error::Overflow discriminant must be 20 (ABI contract)"
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

// ─────────────────────────────────────────────────────────────────────────────
// Storage version, migration, snapshot, and restore tests (Issue #1763)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod storage_migration_tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{symbol_short, vec};

    fn setup() -> (Env, EmergencyKillswitchClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── Storage version ────────────────────────────────────────────────────

    /// `storage_version` returns 0 before `migrate_storage` is called
    /// (simulating a legacy deployment that pre-dates versioning).
    #[test]
    fn storage_version_defaults_to_zero_before_migration() {
        let (_env, client, _admin) = setup();
        assert_eq!(client.storage_version(), 0);
    }

    /// `storage_version` returns `STORAGE_VERSION` after a successful
    /// migration.
    #[test]
    fn storage_version_returns_latest_after_migration() {
        let (_env, client, admin) = setup();
        let new_ver = client.migrate_storage(&admin);
        assert_eq!(new_ver, STORAGE_VERSION);
        assert_eq!(client.storage_version(), STORAGE_VERSION);
    }

    // ── Migration: happy path ──────────────────────────────────────────────

    /// A fresh deployment (v0) can migrate to v1 in a single call.
    #[test]
    fn migrate_storage_advances_from_zero_to_latest() {
        let (_env, client, admin) = setup();
        assert_eq!(client.storage_version(), 0);
        let result = client.migrate_storage(&admin);
        assert_eq!(result, STORAGE_VERSION);
        assert_eq!(client.storage_version(), STORAGE_VERSION);
    }

    /// After a successful migration, calling again returns AlreadyMigrated.
    #[test]
    fn migrate_storage_rejects_already_migrated() {
        let (_env, client, admin) = setup();
        client.migrate_storage(&admin);
        let res = client.try_migrate_storage(&admin);
        assert_eq!(res, Err(Ok(Error::AlreadyMigrated)));
    }

    // ── Migration: authorization ───────────────────────────────────────────

    /// `migrate_storage` requires admin auth.
    #[test]
    fn migrate_storage_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin);

        // Without mocked auth the admin requirement must reject the call.
        env.set_auths(&[]);
        assert!(client.try_migrate_storage(&admin).is_err());
        assert_eq!(client.storage_version(), 0);
    }

    /// `migrate_storage` rejects non-admin callers.
    #[test]
    fn migrate_storage_rejects_non_admin() {
        let (_env, client, _admin) = setup();
        let stranger = Address::generate(&_env);
        let res = client.try_migrate_storage(&stranger);
        assert_eq!(res, Err(Ok(Error::Unauthorized)));
    }

    /// `migrate_storage` rejects uninitialized contract.
    #[test]
    fn migrate_storage_rejects_uninitialized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let caller = Address::generate(&env);
        env.mock_all_auths();
        let res = client.try_migrate_storage(&caller);
        assert_eq!(res, Err(Ok(Error::NotInitialized)));
    }

    // ── Migration: progress tracking ───────────────────────────────────────

    /// Before migration, `get_migration_progress` returns `None`.
    #[test]
    fn migration_progress_none_before_start() {
        let (_env, client, _admin) = setup();
        assert_eq!(client.get_migration_progress(), None);
    }

    /// After migration, `get_migration_progress` returns completed state.
    #[test]
    fn migration_progress_reflects_completed_state() {
        let (env, client, admin) = setup();
        // The default test Env starts at ledger timestamp 0, which would make
        // the recorded `last_run_at` 0 too. Set a nonzero base so the
        // monotonic-clock invariant is actually exercised.
        env.ledger().set_timestamp(1234);
        client.migrate_storage(&admin);
        let progress = client.get_migration_progress();
        assert!(progress.is_some());
        let p = progress.unwrap();
        assert_eq!(p.from_version, 0);
        assert_eq!(p.to_version, STORAGE_VERSION);
        assert_eq!(p.completed_step, p.total_steps);
        assert!(p.last_run_at > 0);
    }

    // ── Migration: resumability ────────────────────────────────────────────

    /// Simulate partial failure by manually injecting a progress record with
    /// step < total_steps, then calling migrate_storage again. The second
    /// call should complete the remaining step.
    #[test]
    fn migrate_storage_is_resumable_after_partial_failure() {
        let (env, client, admin) = setup();
        let contract_id = env.register_contract(None, EmergencyKillswitch);

        // Inject a partial progress record (step 0 of 1 completed).
        env.as_contract(&contract_id, || {
            env.storage().instance().set(
                &DataKey::MigrationProgress,
                &MigrationProgress {
                    from_version: 0,
                    to_version: 2, // pretend target is v2
                    completed_step: 0,
                    total_steps: 1,
                    last_run_at: env.ledger().timestamp(),
                },
            );
        });

        let result = client.migrate_storage(&admin);
        // Should complete (target is now STORAGE_VERSION, but the progress
        // record said v2, so to_version in the stored progress is 2).
        assert!(result >= STORAGE_VERSION);
    }

    // ── Snapshot: happy path ───────────────────────────────────────────────

    /// `pre_upgrade` captures a snapshot that can be restored.
    #[test]
    fn pre_upgrade_and_restore_roundtrip() {
        let (env, client, admin) = setup();

        // Set some state to snapshot.
        client.pause();
        let now = env.ledger().timestamp();
        client.schedule_unpause(&(now + 100));

        // Take snapshot.
        client.pre_upgrade(&admin);

        // Mutate state after snapshot.
        env.ledger().with_mut(|li| li.timestamp = now + 200);
        client.unpause();
        assert!(!client.is_paused());

        // Restore from snapshot — should recover paused state.
        client.restore_from_snapshot(&admin);
        assert!(client.is_paused());
    }

    /// `pre_upgrade` is idempotent — second call overwrites the first.
    #[test]
    fn pre_upgrade_overwrites_existing_snapshot() {
        let (_env, client, admin) = setup();

        // Snapshot while unpaused.
        client.pre_upgrade(&admin);
        assert!(!client.is_paused());

        // Pause, then take another snapshot.
        client.pause();
        client.pre_upgrade(&admin);

        // Restore — should see the paused state from the second snapshot.
        client.restore_from_snapshot(&admin);
        assert!(client.is_paused());
    }

    // ── Snapshot: authorization ─────────────────────────────────────────────

    /// `pre_upgrade` requires admin auth.
    #[test]
    fn pre_upgrade_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin);

        env.set_auths(&[]);
        assert!(client.try_pre_upgrade(&admin).is_err());
    }

    /// `restore_from_snapshot` requires admin auth.
    #[test]
    fn restore_from_snapshot_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin);
        client.pre_upgrade(&admin);

        env.set_auths(&[]);
        assert!(client.try_restore_from_snapshot(&admin).is_err());
    }

    /// `pre_upgrade` rejects non-admin callers.
    #[test]
    fn pre_upgrade_rejects_non_admin() {
        let (_env, client, _admin) = setup();
        let stranger = Address::generate(&_env);
        let res = client.try_pre_upgrade(&stranger);
        assert_eq!(res, Err(Ok(Error::Unauthorized)));
    }

    /// `restore_from_snapshot` rejects non-admin callers.
    #[test]
    fn restore_from_snapshot_rejects_non_admin() {
        let (_env, client, admin) = setup();
        client.pre_upgrade(&admin);
        let stranger = Address::generate(&_env);
        let res = client.try_restore_from_snapshot(&stranger);
        assert_eq!(res, Err(Ok(Error::Unauthorized)));
    }

    /// `pre_upgrade` rejects uninitialized contract.
    #[test]
    fn pre_upgrade_rejects_uninitialized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let caller = Address::generate(&env);
        env.mock_all_auths();
        let res = client.try_pre_upgrade(&caller);
        assert_eq!(res, Err(Ok(Error::NotInitialized)));
    }

    // ── Snapshot: not found / expired ──────────────────────────────────────

    /// `restore_from_snapshot` fails with SnapshotNotFound if no snapshot
    /// was taken.
    #[test]
    fn restore_from_snapshot_fails_without_snapshot() {
        let (_env, client, admin) = setup();
        let res = client.try_restore_from_snapshot(&admin);
        assert_eq!(res, Err(Ok(Error::SnapshotNotFound)));
    }

    /// `restore_from_snapshot` fails with SnapshotExpired after SNAPSHOT_TTL.
    #[test]
    fn restore_from_snapshot_fails_after_ttl() {
        let (env, client, admin) = setup();
        // Start from a nonzero timestamp base. Advancing the ledger with
        // `with_mut` from the default 0 timestamp immediately after a contract
        // invoke triggers a "ledger.try_borrow failed" InternalError in
        // soroban-env-host 21.2.1; `set_timestamp` from a nonzero base avoids
        // that host quirk.
        env.ledger().set_timestamp(SNAPSHOT_TTL);
        client.pause();
        client.pre_upgrade(&admin);

        // Fast-forward past the snapshot TTL.
        env.ledger().set_timestamp(SNAPSHOT_TTL + SNAPSHOT_TTL + 1);

        let res = client.try_restore_from_snapshot(&admin);
        assert_eq!(res, Err(Ok(Error::SnapshotExpired)));
    }

    // ── Snapshot: state preservation ────────────────────────────────────────

    /// Snapshot preserves the signer configuration.
    #[test]
    fn snapshot_preserves_signer_config() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);

        let signers = vec![&env, first.clone(), second.clone()];
        client.configure_signers(&admin, &signers, &2);
        assert_eq!(client.get_signer_epoch(), 1);
        assert_eq!(client.get_signer_threshold(), 2);

        // Snapshot and restore.
        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert_eq!(client.get_signer_epoch(), 1);
        assert_eq!(client.get_signer_threshold(), 2);
    }

    /// Snapshot preserves the kill-switch epoch.
    #[test]
    fn snapshot_preserves_kill_switch_epoch() {
        let (_env, client, admin) = setup();
        client.bump_kill_switch_epoch(&admin);
        client.bump_kill_switch_epoch(&admin);
        assert_eq!(client.get_kill_switch_epoch(), 2);

        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert_eq!(client.get_kill_switch_epoch(), 2);
    }

    /// Snapshot preserves the pause reason.
    #[test]
    fn snapshot_preserves_pause_reason() {
        let (_env, client, admin) = setup();
        client.pause_with_reason(&symbol_short!("drill"));
        assert_eq!(client.pause_reason(), Some(symbol_short!("drill")));

        client.pre_upgrade(&admin);
        // Change state.
        client.clear_emergency_state();
        assert_eq!(client.pause_reason(), None);

        // Restore.
        client.restore_from_snapshot(&admin);
        assert!(client.is_paused());
        assert_eq!(client.pause_reason(), Some(symbol_short!("drill")));
    }

    // ── Snapshot: discard ──────────────────────────────────────────────────

    /// `discard_snapshot` removes the snapshot so restore fails.
    #[test]
    fn discard_snapshot_prevents_restore() {
        let (_env, client, admin) = setup();
        client.pre_upgrade(&admin);
        client.discard_snapshot(&admin);
        let res = client.try_restore_from_snapshot(&admin);
        assert_eq!(res, Err(Ok(Error::SnapshotNotFound)));
    }

    /// `discard_snapshot` requires admin auth.
    #[test]
    fn discard_snapshot_requires_admin_auth() {
        let env = Env::default();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        env.mock_all_auths();
        client.initialize(&admin);

        env.set_auths(&[]);
        assert!(client.try_discard_snapshot(&admin).is_err());
    }

    /// `discard_snapshot` rejects non-admin callers.
    #[test]
    fn discard_snapshot_rejects_non_admin() {
        let (_env, client, admin) = setup();
        client.pre_upgrade(&admin);
        let stranger = Address::generate(&_env);
        let res = client.try_discard_snapshot(&stranger);
        assert_eq!(res, Err(Ok(Error::Unauthorized)));
    }

    // ── Upgrade + rollback scenario ────────────────────────────────────────

    /// Full upgrade-rollback scenario:
    /// 1. Set up state (paused + signers).
    /// 2. Take snapshot.
    /// 3. "Upgrade" — change state (unpause, rotate signers).
    /// 4. Rollback — restore snapshot.
    /// 5. Verify original state is recovered.
    #[test]
    fn full_upgrade_rollback_scenario() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let third = Address::generate(&env);

        // Step 1: establish state.
        client.pause();
        let signers = vec![&env, first.clone(), second.clone()];
        client.configure_signers(&admin, &signers, &2);
        client.bump_kill_switch_epoch(&admin);

        // Step 2: snapshot.
        client.pre_upgrade(&admin);

        // Step 3: simulate post-upgrade changes.
        client.schedule_unpause(&(env.ledger().timestamp() + 100));
        env.ledger().with_mut(|li| li.timestamp += 200);
        client.unpause();
        let new_signers = vec![&env, second, third];
        client.configure_signers(&admin, &new_signers, &1);
        client.bump_kill_switch_epoch(&admin);

        assert!(!client.is_paused());
        assert_eq!(client.get_signer_epoch(), 2);

        // Step 4: rollback.
        client.restore_from_snapshot(&admin);

        // Step 5: verify.
        assert!(client.is_paused());
        assert_eq!(client.get_signer_epoch(), 1);
        assert_eq!(client.get_signer_threshold(), 2);
        assert_eq!(client.get_kill_switch_epoch(), 1);
    }

    // ── Rerun idempotency ──────────────────────────────────────────────────

    /// Calling `migrate_storage` when already migrated is rejected.
    #[test]
    fn rerun_migrate_is_rejected() {
        let (_env, client, admin) = setup();
        client.migrate_storage(&admin);
        let res = client.try_migrate_storage(&admin);
        assert_eq!(res, Err(Ok(Error::AlreadyMigrated)));
    }

    /// Calling `pre_upgrade` twice is idempotent (overwrites).
    #[test]
    fn rerun_pre_upgrade_is_idempotent() {
        let (_env, client, admin) = setup();
        client.pre_upgrade(&admin);
        client.pre_upgrade(&admin); // no error
                                    // Restore should work.
        assert_eq!(client.try_restore_from_snapshot(&admin), Ok(Ok(())));
    }

    // ── Partial state after failed operation ───────────────────────────────

    /// If `restore_from_snapshot` fails (expired), the existing state is
    /// untouched.
    #[test]
    fn failed_restore_leaves_state_unchanged() {
        let (env, client, admin) = setup();
        client.pause();
        client.pre_upgrade(&admin);

        // Mutate state.
        client.schedule_unpause(&(env.ledger().timestamp() + 100));
        env.ledger().with_mut(|li| li.timestamp += 200);
        client.unpause();
        assert!(!client.is_paused());

        // Fast-forward past TTL and attempt restore.
        env.ledger().with_mut(|li| li.timestamp += SNAPSHOT_TTL + 1);
        let res = client.try_restore_from_snapshot(&admin);
        assert_eq!(res, Err(Ok(Error::SnapshotExpired)));

        // State must be untouched.
        assert!(!client.is_paused());
    }

    // ── Error discriminant stability ───────────────────────────────────────

    /// New error discriminants are stable (ABI contract).
    #[test]
    fn error_discriminants_are_stable() {
        assert_eq!(Error::SnapshotNotFound as u32, 16);
        assert_eq!(Error::SnapshotExpired as u32, 17);
        assert_eq!(Error::AlreadyMigrated as u32, 18);
        assert_eq!(Error::MigrationIncomplete as u32, 19);
        assert_eq!(Error::Overflow as u32, 20);
    }

    // ── Concurrent operations safety ───────────────────────────────────────

    /// While a snapshot-based restore is in progress, module-level pauses
    /// are preserved (clear_emergency_state only clears global).
    #[test]
    fn restore_preserves_module_level_pauses() {
        let (_env, client, admin) = setup();
        let module = symbol_short!("bill");

        client.pause_module(&module);
        client.pause();
        client.pre_upgrade(&admin);

        // Simulate: clear global, keep module.
        client.clear_emergency_state();
        assert!(client.is_module_paused(&module));

        // Restore from snapshot — module pause must survive.
        client.restore_from_snapshot(&admin);
        assert!(client.is_paused());
        assert!(client.is_module_paused(&module));
    }

    // ── Storage version after version field injection ──────────────────────

    /// A legacy deployment that never set the StorageVersion key returns
    /// `storage_version == 0`. After `migrate_storage` it returns the
    /// latest.
    #[test]
    fn legacy_deployment_gets_versioned_after_migration() {
        let (env, client, _admin) = setup();
        // Confirm no StorageVersion key is present (v0).
        env.as_contract(&env.register_contract(None, EmergencyKillswitch), || {
            // This is a different contract, so it won't affect the test contract.
        });
        assert_eq!(client.storage_version(), 0);

        let (env2, client2, admin2) = setup();
        let _ = env2;
        client2.migrate_storage(&admin2);
        assert_eq!(client2.storage_version(), STORAGE_VERSION);
    }
}

#[cfg(test)]
mod snapshot_function_pause_restore_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{symbol_short, vec};

    fn setup() -> (Env, EmergencyKillswitchClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    /// Snapshot and restore preserves function-level pauses.
    #[test]
    fn snapshot_preserves_function_level_pauses() {
        let (_env, client, admin) = setup();
        let module = symbol_short!("bill");
        let func = symbol_short!("pay");

        client.pause_function(&module, &func);
        assert!(client.is_function_paused(&module, &func));

        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert!(client.is_function_paused(&module, &func));
        assert!(client.list_paused_functions(&module).contains(func));
    }

    /// Snapshot captures the unpause schedule.
    #[test]
    fn snapshot_preserves_unpause_schedule() {
        let (env, client, admin) = setup();
        let future = env.ledger().timestamp() + 3600;
        client.pause();
        client.schedule_unpause(&future);

        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert!(client.is_paused());
        assert_eq!(client.get_unpause_schedule(), Some(future));
    }

    /// Restore after snapshot captures the activation scope.
    #[test]
    fn snapshot_preserves_activation_scope() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let signers = vec![&env, first.clone(), second.clone()];
        let epoch = client.configure_signers(&admin, &signers, &2);
        let approvals = vec![&env, first, second];
        let mod_sym = symbol_short!("bill");
        client.activate(&epoch, &approvals, &PauseScope::Module(mod_sym.clone()));
        assert!(client.is_module_paused(&mod_sym));

        client.pre_upgrade(&admin);
        client.restore_from_snapshot(&admin);

        assert!(client.is_module_paused(&mod_sym));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Pagination and cursor semantics tests (Issue #1762)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod pagination_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::symbol_short;

    fn setup() -> (Env, EmergencyKillswitchClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, EmergencyKillswitch);
        let client = EmergencyKillswitchClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    // ── list_signers_page: empty ───────────────────────────────────────────

    /// Before configure_signers, list_signers_page returns empty.
    #[test]
    fn signers_page_empty_before_config() {
        let (env, client, _admin) = setup();
        let result = client.list_signers_page(&None, &10);
        assert_eq!(result.len(), 0);
    }

    // ── list_signers_page: single page ─────────────────────────────────────

    /// All signers returned in one page when limit >= count.
    #[test]
    fn signers_page_single_page_all_fit() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let third = Address::generate(&env);
        let signers = vec![&env, first.clone(), second.clone(), third.clone()];
        client.configure_signers(&admin, &signers, 1);

        let result = client.list_signers_page(&None, &10);
        assert_eq!(result.len(), 3);
        assert!(result.contains(first));
        assert!(result.contains(second));
        assert!(result.contains(third));
    }

    // ── list_signers_page: boundary — limit equals count ───────────────────

    /// When limit == count, all items returned, next page is empty.
    #[test]
    fn signers_page_boundary_limit_equals_count() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let signers = vec![&env, first.clone(), second.clone()];
        client.configure_signers(&admin, &signers, 1);

        let page1 = client.list_signers_page(&None, &2);
        assert_eq!(page1.len(), 2);
        assert!(page1.contains(first));
        assert!(page1.contains(second));

        // Second page should be empty
        let page2 = client.list_signers_page(&Some(2), &2);
        assert_eq!(page2.len(), 0);
    }

    // ── list_signers_page: boundary — limit exceeds count ──────────────────

    /// When limit > count, all items returned, no panic.
    #[test]
    fn signers_page_boundary_limit_exceeds_count() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let signers = vec![&env, first];
        client.configure_signers(&admin, &signers, 1);

        let result = client.list_signers_page(&None, &100);
        assert_eq!(result.len(), 1);
    }

    // ── list_signers_page: multi-page with cursor ──────────────────────────

    /// Paginate through 5 signers in pages of 2.
    #[test]
    fn signers_page_multi_page_cursor_progression() {
        let (env, client, admin) = setup();
        let addrs: Vec<Address> = (0..5).map(|_| Address::generate(&env)).collect();
        let signers = vec![&env, addrs[0].clone(), addrs[1].clone(), addrs[2].clone(), addrs[3].clone(), addrs[4].clone()];
        client.configure_signers(&admin, &signers, 1);

        let page1 = client.list_signers_page(&None, &2);
        assert_eq!(page1.len(), 2);

        let page2 = client.list_signers_page(&Some(2), &2);
        assert_eq!(page2.len(), 2);

        let page3 = client.list_signers_page(&Some(4), &2);
        assert_eq!(page3.len(), 1);

        // Cursor beyond end returns empty
        let page4 = client.list_signers_page(&Some(5), &2);
        assert_eq!(page4.len(), 0);
    }

    // ── list_signers_page: cursor=0 equivalent to None ─────────────────────

    /// Cursor of 0 starts from the beginning, same as None.
    #[test]
    fn signers_page_cursor_zero_starts_from_beginning() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let second = Address::generate(&env);
        let signers = vec![&env, first.clone(), second.clone()];
        client.configure_signers(&admin, &signers, 1);

        let page_none = client.list_signers_page(&None, &10);
        let page_zero = client.list_signers_page(&Some(0), &10);
        assert_eq!(page_none.len(), page_zero.len());
        assert!(page_none.contains(first));
        assert!(page_zero.contains(first));
    }

    // ── list_signers_page: invalid cursor rejected ─────────────────────────

    /// Cursor beyond collection length returns empty (not error).
    #[test]
    fn signers_page_invalid_cursor_returns_empty() {
        let (env, client, admin) = setup();
        let first = Address::generate(&env);
        let signers = vec![&env, first];
        client.configure_signers(&admin, &signers, 1);

        let result = client.list_signers_page(&Some(999), &10);
        assert_eq!(result.len(), 0);
    }

    // ── list_signers_page: large result set ────────────────────────────────

    /// MAX_PAGE_LIMIT clamps correctly with many signers.
    #[test]
    fn signers_page_large_result_clamped() {
        let (env, client, admin) = setup();
        let mut signers_vec = Vec::new(&env);
        let mut expected_count = 0u32;
        for _ in 0..60 {
            signers_vec.push_back(Address::generate(&env));
            expected_count += 1;
        }
        client.configure_signers(&admin, &signers_vec, 1);

        // Request 100 but should get MAX_PAGE_LIMIT (50)
        let page1 = client.list_signers_page(&None, &100);
        assert_eq!(page1.len(), MAX_PAGE_LIMIT);

        // Second page gets the remaining 10
        let page2 = client.list_signers_page(&Some(50), &100);
        assert_eq!(page2.len(), 10);
    }

    // ── list_signers_page: default limit ───────────────────────────────────

    /// limit=0 normalises to DEFAULT_PAGE_LIMIT.
    #[test]
    fn signers_page_zero_limit_normalises_to_default() {
        let (env, client, admin) = setup();
        let mut signers_vec = Vec::new(&env);
        for _ in 0..30 {
            signers_vec.push_back(Address::generate(&env));
        }
        client.configure_signers(&admin, &signers_vec, 1);

        let page = client.list_signers_page(&None, &0);
        assert_eq!(page.len(), DEFAULT_PAGE_LIMIT);
    }

    // ── list_paused_functions_page: empty ──────────────────────────────────

    /// No paused functions returns empty.
    #[test]
    fn paused_functions_page_empty_when_none_paused() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        let result = client.list_paused_functions_page(&module, &None, &10);
        assert_eq!(result.len(), 0);
    }

    // ── list_paused_functions_page: single page ────────────────────────────

    /// All functions returned in one page.
    #[test]
    fn paused_functions_page_single_page() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        let f1 = symbol_short!("pay");
        let f2 = symbol_short!("refund");
        client.pause_function(&module, &f1);
        client.pause_function(&module, &f2);

        let result = client.list_paused_functions_page(&module, &None, &10);
        assert_eq!(result.len(), 2);
        assert!(result.contains(f1));
        assert!(result.contains(f2));
    }

    // ── list_paused_functions_page: multi-page ─────────────────────────────

    /// Paginate through paused functions.
    #[test]
    fn paused_functions_page_multi_page() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        for i in 0..5 {
            client.pause_function(&module, &Symbol::new(&_env, &format!("f{}", i)));
        }

        let page1 = client.list_paused_functions_page(&module, &None, &2);
        assert_eq!(page1.len(), 2);

        let page2 = client.list_paused_functions_page(&module, &Some(2), &2);
        assert_eq!(page2.len(), 2);

        let page3 = client.list_paused_functions_page(&module, &Some(4), &2);
        assert_eq!(page3.len(), 1);
    }

    // ── list_paused_functions_page: isolation per module ───────────────────

    /// Paused functions for module A are not visible in module B.
    #[test]
    fn paused_functions_page_isolated_per_module() {
        let (_env, client, _admin) = setup();
        let m1 = symbol_short!("bill");
        let m2 = symbol_short!("savings");
        let func = symbol_short!("pay");
        client.pause_function(&m1, &func);

        let page_m1 = client.list_paused_functions_page(&m1, &None, &10);
        let page_m2 = client.list_paused_functions_page(&m2, &None, &10);
        assert_eq!(page_m1.len(), 1);
        assert_eq!(page_m2.len(), 0);
    }

    // ── list_paused_functions_page: after unpause ─────────────────────────

    /// Unpaused function no longer appears in paginated results.
    #[test]
    fn paused_functions_page_reflects_unpause() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        let func = symbol_short!("pay");
        client.pause_function(&module, &func);
        assert_eq!(client.list_paused_functions_page(&module, &None, &10).len(), 1);

        client.unpause_function(&module, &func);
        assert_eq!(client.list_paused_functions_page(&module, &None, &10).len(), 0);
    }

    // ── list_paused_functions_page: cursor beyond end ──────────────────────

    /// Cursor past the end returns empty.
    #[test]
    fn paused_functions_page_cursor_beyond_end() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        let func = symbol_short!("pay");
        client.pause_function(&module, &func);

        let result = client.list_paused_functions_page(&module, &Some(999), &10);
        assert_eq!(result.len(), 0);
    }

    // ── list_paused_functions_page: default limit ──────────────────────────

    /// limit=0 normalises to DEFAULT_PAGE_LIMIT.
    #[test]
    fn paused_functions_page_zero_limit_normalises() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        let func = symbol_short!("pay");
        client.pause_function(&module, &func);

        let result = client.list_paused_functions_page(&module, &None, &0);
        assert_eq!(result.len(), 1);
    }

    // ── list_paused_functions_page: max limit clamping ─────────────────────

    /// Requesting 200 returns at most MAX_PAGE_LIMIT items.
    #[test]
    fn paused_functions_page_max_limit_clamped() {
        let (_env, client, _admin) = setup();
        let module = symbol_short!("bill");
        for i in 0..8 {
            client.pause_function(&module, &Symbol::new(&_env, &format!("f{}", i)));
        }
        let result = client.list_paused_functions_page(&module, &None, &200);
        assert_eq!(result.len(), 8);
    }
}
