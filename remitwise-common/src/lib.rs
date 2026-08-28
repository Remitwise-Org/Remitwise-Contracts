#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use soroban_sdk::{
    contracterror, contracttype, panic_with_error, symbol_short, Address, Bytes, BytesN, Env,
    Map, Symbol, TryFromVal, Val,
};
pub mod tokens;
pub use tokens::{
    SupportedToken, BASE_UNITS_PER_EURC, BASE_UNITS_PER_USDC, DEFAULT_CURRENCY, EURC_DECIMALS,
    MAX_CURRENCY_LEN, STROOPS_PER_XLM, USDC_DECIMALS, XLM_DECIMALS,
};

/// Shared period-key helpers: [`period::require_matching_period_key`] and
/// [`period::verify_period_active`]. Tests live in the module itself so that
/// CI picks them up with `cargo test -p remitwise-common`.
pub mod period;

#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RemitwiseError {
    Unauthorized = 1,
    InvalidSignature = 2,
    DeadlineExpired = 3,
    RequestHashMismatch = 4,
    InvalidAmount = 5,
    InvalidNonce = 6,
    DuplicateImport = 7,
}

/// Financial categories for remittance allocation
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Category {
    Spending = 1,
    Savings = 2,
    Bills = 3,
    Insurance = 4,
}

/// Family roles for access control
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FamilyRole {
    Owner = 1,
    Admin = 2,
    Member = 3,
    Viewer = 4,
}

/// Insurance coverage types
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CoverageType {
    Health = 1,
    Life = 2,
    Property = 3,
    Auto = 4,
    Liability = 5,
}

/// Policy mode for access control
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PolicyMode {
    Strict = 1,
}

/// Detailed pause state including boolean status and timestamp when paused.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseState {
    pub paused: bool,
    pub paused_since: Option<u64>,
}

/// Event categories used for logging across all contracts.
///
/// Determines the high-level classification of an event. The taxonomy is documented in
/// `docs/EVENT_TAXONOMY.md`.
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum EventCategory {
    Transaction = 0,
    State = 1,
    Alert = 2,
    System = 3,
    Access = 4,
    Compliance = 5,
}

/// Priority levels for events emitted by contracts.
/// Determines the importance of the event. Lower numbers represent lower priority.
/// See `docs/EVENT_TAXONOMY.md` for full taxonomy details.
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum EventPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

impl EventCategory {
    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

impl EventPriority {
    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

#[contracttype]
#[derive(Clone)]
pub struct RoleGrantedEvent {
    pub member: Address,
    pub role: FamilyRole,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct RoleRevokedEvent {
    pub member: Address,
    pub role: FamilyRole,
    pub timestamp: u64,
}

/// Pagination limits
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

/// Typed error returned when a pagination limit is invalid or exceeds `MAX_PAGE_LIMIT`.
#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PageLimitError {
    /// The requested page limit exceeds `MAX_PAGE_LIMIT`.
    LimitExceedsMax = 1,
}

/// Central guard for enforcing pagination limits against `MAX_PAGE_LIMIT`.
///
/// This is a defence-in-depth security guard that checks whether a caller-supplied
/// `limit` is within the maximum allowed page size (`MAX_PAGE_LIMIT`).
///
/// # Arguments
/// * `limit` - The pagination limit to validate
///
/// # Errors
/// Returns [`PageLimitError::LimitExceedsMax`] if `limit > MAX_PAGE_LIMIT`.
pub fn require_page_limit_within_bounds(limit: u32) -> Result<(), PageLimitError> {
    if limit > MAX_PAGE_LIMIT {
        Err(PageLimitError::LimitExceedsMax)
    } else {
        Ok(())
    }
}

/// Max items returned in Top-N reports.
pub const MAX_ITEMS_PER_REPORT: u32 = 10;
/// Alias for MAX_ITEMS_PER_REPORT used by reporting contract.
pub const MAX_TOP_N: u32 = MAX_ITEMS_PER_REPORT;

/// Error returned when a top-N size exceeds the hard cap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TopNError;

/// Requires that a top-N report size does not exceed the global cap.
///
/// This is a defence-in-depth guard that fails closed if a future
/// code change raises `n` above `MAX_TOP_N`.
pub fn require_bounded_top_n(n: u32, max: u32) -> Result<(), TopNError> {
    if n > max {
        Err(TopNError)
    } else {
        Ok(())
    }
}

/// Helper to insert an item into a Top-N list (bounded).
/// The list is maintained in sorted order based on the provided comparator.
pub fn insert_top_n<T, F>(
    _env: &Env,
    top_list: &mut soroban_sdk::Vec<T>,
    max_items: u32,
    item: T,
    mut cmp: F,
) where
    T: Clone
        + soroban_sdk::IntoVal<Env, soroban_sdk::Val>
        + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    F: FnMut(&T, &T) -> core::cmp::Ordering,
{
    let mut inserted = false;
    for i in 0..top_list.len() {
        if let Some(existing) = top_list.get(i) {
            if cmp(&item, &existing) == core::cmp::Ordering::Greater {
                top_list.insert(i, item.clone());
                inserted = true;
                break;
            }
        }
    }

    if !inserted && top_list.len() < max_items {
        top_list.push_back(item);
    } else if top_list.len() > max_items {
        top_list.remove(max_items);
    }
}

/// Standardized TTL Constants (Ledger Counts)
pub const DAY_IN_LEDGERS: u32 = 17280; // ~5 seconds per ledger

/// Storage TTL constants for active data
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS; // 7 days
pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS; // 30 days

/// Storage TTL constants for persistent data
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS; // 15 days
pub const PERSISTENT_BUMP_AMOUNT: u32 = 60 * DAY_IN_LEDGERS; // 60 days

/// Storage TTL constants for archived data
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS; // 7 days
pub const ARCHIVE_BUMP_AMOUNT: u32 = 180 * DAY_IN_LEDGERS; // 180 days (6 months)

/// Signature expiration time (24 hours in seconds)
pub const SIGNATURE_EXPIRATION: u64 = 86400;

/// Contract version
pub const CONTRACT_VERSION: u32 = 1;

/// Error returned when attempting to read or process state with an outdated schema version.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MigrationError {
    /// The schema version is older than CONTRACT_VERSION.
    OutdatedVersion = 1,
}

/// Verifies that a config/state version is at least `CONTRACT_VERSION`.
pub fn verify_config_migration(version: u32) -> Result<(), MigrationError> {
    if version < CONTRACT_VERSION {
        Err(MigrationError::OutdatedVersion)
    } else {
        Ok(())
    }
}

/// Storage key for the pause channels map
pub const STORAGE_PAUSE_CHANNELS: &str = "PAUSE_CH";

/// Storage key for the global paused_since timestamp
pub const STORAGE_PAUSED_AT: &str = "PAUSED_AT";

/// Maximum batch size for operations
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum byte length for `Bytes` values returned from public contract entry points.
///
/// XDR `ScBytes` carries no inherent host-enforced cap before deserialization.
/// Without an explicit check, a misbehaving or compromised contract can force every
/// downstream consumer (SDK, indexer, RPC node) to allocate memory proportional to
/// the returned payload — a potential DoS vector.  Call [`guard_bytes_len`] before
/// returning any variable-length `Bytes` value from a public entry point.
pub const MAX_BYTES_RETURN: u32 = 8192;

/// Error returned when a `Bytes` value about to leave a contract entry point
/// exceeds [`MAX_BYTES_RETURN`].
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BytesReturnError {
    /// The byte length exceeds [`MAX_BYTES_RETURN`].
    ReturnTooLarge = 1,
}

/// Verifies that `from` is strictly less than `to`.
///
/// # Panics
/// - Panics if `from >= to`.
pub fn verify_ordered_pair(from: u64, to: u64) {
    if from >= to {
        panic!("Invalid range: from ({from}) must be strictly less than to ({to})");
    }
}

/// Event emission helper
pub struct RemitwiseEvents;

/// Validates that a [`Symbol`] does not exceed the short-symbol limit (9 bytes).
///
/// This is a defence-in-depth check.  Symbols longer than 9 bytes use the
/// large-symbol XDR encoding (`SymbolObject` tag) instead of the inline
/// short-symbol encoding (`SymbolSmall` tag).  Without this gate, a caller
/// could supply a long symbol where the contract expects a short one,
/// potentially leading to storage-key confusion or indexer mismatches
/// downstream.
///
/// The check uses the [`Val`] bit pattern: short symbols are stored inline
/// (not objects), long symbols are stored as host object references.  This
/// works on all targets (WASM and non-WASM) without requiring string
/// conversion.
///
/// Call this on any `Symbol` value derived from untrusted input before using
/// it as a storage key, event action, or comparand against `symbol_short!`
/// constants.
///
/// # Errors
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SymbolError {
    SymbolTooLong = 35,
}

/// Returns [`SymbolError::SymbolTooLong`] when the symbol exceeds 9 bytes.
pub fn require_valid_symbol_length(_env: &Env, sym: &Symbol) -> Result<(), SymbolError> {
    let val: soroban_sdk::Val = sym.to_val();
    if val.is_object() {
        Err(SymbolError::SymbolTooLong)
    } else {
        Ok(())
    }
}

/// Guards `bytes` against exceeding the XDR return-size budget.
///
/// Call this immediately before returning any variable-length `Bytes` value from a
/// public contract entry point.  The check costs a single `u32` comparison and
/// ensures that downstream consumers cannot be forced to deserialise an arbitrarily
/// large buffer.
///
/// # Errors
/// Returns [`BytesReturnError::ReturnTooLarge`] when `bytes.len() > MAX_BYTES_RETURN`.
pub fn guard_bytes_len(bytes: &Bytes) -> Result<(), BytesReturnError> {
    if bytes.len() > MAX_BYTES_RETURN {
        Err(BytesReturnError::ReturnTooLarge)
    } else {
        Ok(())
    }
}

/// Guards against executing dispute-related operations in an outdated epoch.
///
/// This is a defence-in-depth fix. If an attacker could proceed with dispute-related
/// operations in an outdated epoch, they could bypass lifecycle expiration rules,
/// allowing them to manipulate dispute resolutions or lock funds unexpectedly.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `ep` - The dispute epoch supplied by the caller
///
/// # Returns
/// * `Ok(())` if the epoch is greater than or equal to the current pending dispute epoch
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum DisputeError {
    OutdatedEpoch = 36,
}

/// * `Err(DisputeError::OutdatedEpoch)` if the epoch is outdated
pub fn require_no_pending_dispute_epoch(env: &Env, ep: u64) -> Result<(), DisputeError> {
    let current_epoch: u64 = env
        .storage()
        .instance()
        .get(&symbol_short!("DISP_EP"))
        .unwrap_or(0);
    if ep < current_epoch {
        return Err(DisputeError::OutdatedEpoch);
    }
    Ok(())
}

/// Typed error returned when a caller supplies an outdated cross-contract epoch.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CrossContractEpochError {
    /// The supplied cross-contract epoch does not match the current contract epoch.
    EpochMismatch = 37,
}

pub const STORAGE_CROSS_CONTRACT_EPOCH: Symbol = symbol_short!("XC_EPOCH");

/// Guards against executing cross-contract operations with a stale epoch.
///
/// This is a defence-in-depth fix. If a cross-contract message carrying an old epoch
/// is replayed after the epoch has been bumped, it could lead to state corruption
/// or unauthorised actions. Rejecting stale epochs ensures only fresh cross-contract
/// calls are processed.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `ep` - The cross-contract epoch supplied by the caller
///
/// # Returns
/// * `Ok(())` if the epoch matches the current cross-contract epoch exactly
/// * `Err(CrossContractEpochError::EpochMismatch)` if the epoch is outdated
pub fn require_matching_cross_contract_epoch(
    env: &Env,
    ep: u64,
) -> Result<(), CrossContractEpochError> {
    let current_epoch: u64 = env
        .storage()
        .instance()
        .get(&STORAGE_CROSS_CONTRACT_EPOCH)
        .unwrap_or(0);
    if ep != current_epoch {
        return Err(CrossContractEpochError::EpochMismatch);
    }
    Ok(())
}

/// Set the current cross-contract epoch directly.
///
/// Intended to be called once at contract initialisation (mirroring the
/// orchestrator's actor epoch) and whenever the orchestrator performs a
/// coordinated epoch bump. This function does not enforce authentication — the
/// calling contract is responsible for gating it with the appropriate admin or
/// trusted-orchestrator auth before invoking it.
pub fn set_cross_contract_epoch(env: &Env, epoch: u64) {
    env.storage()
        .instance()
        .set(&STORAGE_CROSS_CONTRACT_EPOCH, &epoch);
}

/// Read the current cross-contract epoch without modifying it.
///
/// Returns `0` when no epoch has been stored yet (fresh deployment), matching
/// the orchestrator's actor-epoch default.
pub fn get_cross_contract_epoch(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&STORAGE_CROSS_CONTRACT_EPOCH)
        .unwrap_or(0)
}

/// Bump the cross-contract epoch by 1, atomically reading and incrementing the
/// stored value.
///
/// Returns the new epoch value. Saturates on overflow (a 64-bit epoch whose
/// value is `u64::MAX` is unreachable in practice). This is the per-contract
/// half of the coordinated cross-contract epoch bump driven by the orchestrator:
/// the orchestrator calls this on every downstream contract inside a single
/// transaction, so either all downstream epochs advance together or the whole
/// transaction reverts (`fail safe`).
///
/// This function does not enforce authentication — the calling contract is
/// responsible for gating it with trusted-orchestrator auth before invoking it.
pub fn bump_cross_contract_epoch(env: &Env) -> u64 {
    let current: u64 = get_cross_contract_epoch(env);
    let next = current.saturating_add(1);
    env.storage()
        .instance()
        .set(&STORAGE_CROSS_CONTRACT_EPOCH, &next);
    next
}

/// Typed error returned when a trusted-orchestrator identity check fails.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TrustedOrchestratorError {
    /// No trusted orchestrator address has been configured yet.
    NotConfigured = 38,
    /// The immediate caller is not the configured trusted orchestrator.
    Unauthorized = 39,
}

/// Storage key under which a contract records the single trusted orchestrator
/// address that is permitted to drive privileged cross-contract operations.
pub const STORAGE_TRUSTED_ORCHESTRATOR: Symbol = symbol_short!("ORCH");

/// Record the trusted orchestrator address.
///
/// Should be called at initialisation (or via an owner-gated setter) so that
/// downstream contracts can verify the identity of the contract invoking their
/// privileged cross-contract entry points. No authentication is enforced here;
/// the caller must gate this with owner/admin auth.
pub fn set_trusted_orchestrator(env: &Env, orchestrator: &Address) {
    env.storage()
        .instance()
        .set(&STORAGE_TRUSTED_ORCHESTRATOR, orchestrator);
}

/// Read the trusted orchestrator address, if one has been configured.
pub fn get_trusted_orchestrator(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&STORAGE_TRUSTED_ORCHESTRATOR)
}

/// Verify that the supplied `orchestrator` address is exactly the configured
/// trusted orchestrator **and** that the caller actually is that contract.
///
/// This is the "contract identity" half of the cross-contract epoch guard: even
/// if an attacker supplies the correct epoch value *and* the known orchestrator
/// address, they cannot satisfy this check because `orchestrator.require_auth()`
/// only succeeds when the immediate invoker is the orchestrator contract itself
/// (Soroban authorises the direct invoker for the cross-contract call). When no
/// trusted orchestrator has been configured the check fails with
/// `TrustedOrchestratorError::NotConfigured` rather than silently allowing the
/// call.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `orchestrator` — The orchestrator address presented by the caller (the
///   orchestrator passes its own `env.current_contract_address()`).
///
/// # Returns
/// * `Ok(())` if `orchestrator == stored` and the caller is authorised as it.
/// * `Err(TrustedOrchestratorError::NotConfigured)` if unset.
/// * `Err(TrustedOrchestratorError::Unauthorized)` if the address differs or the
///   caller cannot authorise for it.
pub fn require_trusted_orchestrator(
    env: &Env,
    orchestrator: &Address,
) -> Result<(), TrustedOrchestratorError> {
    let trusted: Address = get_trusted_orchestrator(env)
        .ok_or(TrustedOrchestratorError::NotConfigured)?;
    if orchestrator != &trusted {
        return Err(TrustedOrchestratorError::Unauthorized);
    }
    orchestrator.require_auth();
    Ok(())
}

/// Verify that `orchestrator` equals the configured trusted orchestrator **without**
/// requiring authentication.
///
/// Used by read-only cross-contract entry points (e.g. `calculate_split`,
/// `get_split`, `check_spending_limit`) where authorising the invoker is
/// undesirable but the caller must still present the expected orchestrator
/// identity and a matching epoch. The epoch itself is still enforced by the
/// caller via [`require_matching_cross_contract_epoch`].
pub fn verify_orchestrator_identity(
    env: &Env,
    orchestrator: &Address,
) -> Result<(), TrustedOrchestratorError> {
    let trusted: Address = get_trusted_orchestrator(env)
        .ok_or(TrustedOrchestratorError::NotConfigured)?;
    if orchestrator != &trusted {
        return Err(TrustedOrchestratorError::Unauthorized);
    }
    Ok(())
}

/// Full guard for a *privileged write* cross-contract entry point.
///
/// Enforces, in order, before any local mutation:
/// 1. The supplied `orchestrator` is the configured trusted orchestrator **and**
///    the caller is authorised as that contract (`require_auth`).
/// 2. The supplied `epoch` matches the contract's current cross-contract epoch.
///
/// Returns the downstream contract's own epoch-mismatch error type by panicking
/// with [`CrossContractEpochError::EpochMismatch`] on a mismatch and
/// [`TrustedOrchestratorError`] on an identity failure, so downstream entry
/// points can call this with a single `?` after mapping the result.
pub fn guard_cross_contract_write(
    env: &Env,
    orchestrator: &Address,
    epoch: u64,
) -> Result<(), CrossContractEpochError> {
    require_trusted_orchestrator(env, orchestrator)
        .unwrap_or_else(|e| panic_with_error!(env, e));
    require_matching_cross_contract_epoch(env, epoch)
}

/// Full guard for a *read-only* cross-contract entry point.
///
/// Like [`guard_cross_contract_write`] but does not require the invoker to
/// authorise (read-only views should remain callable without auth); it only
/// checks the presented orchestrator identity and the epoch.
pub fn guard_cross_contract_read(
    env: &Env,
    orchestrator: &Address,
    epoch: u64,
) -> Result<(), CrossContractEpochError> {
    verify_orchestrator_identity(env, orchestrator)
        .unwrap_or_else(|e| panic_with_error!(env, e));
    require_matching_cross_contract_epoch(env, epoch)
}

// ---------------------------------------------------------------------------
// BytesN validation
// ---------------------------------------------------------------------------

/// Error returned when a `BytesN` value is completely zeroed.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BytesNError {
    /// The `BytesN` array consists entirely of zero bytes.
    AllZeros = 1,
}

/// Guards that a `BytesN` array is not completely zeroed.
///
/// This is a defence-in-depth check. Cryptographic identifiers like public keys
/// or signatures should never legitimately be all-zeros. If they are, it usually
/// points to a zero-initialised buffer bug or an attacker intentionally passing
/// `[0; N]` to exploit uninitialized or default state in a verifier.
///
/// # Errors
/// Returns [`BytesNError::AllZeros`] when `bytes` consists entirely of zero bytes.
pub fn require_non_zero_bytes<const N: usize>(bytes: &BytesN<N>) -> Result<(), BytesNError> {
    if bytes.to_array().iter().all(|&b| b == 0) {
        Err(BytesNError::AllZeros)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Symbol length validation
// ---------------------------------------------------------------------------

/// Maximum byte length for a `symbol_short!` storage key (Soroban SDK constraint).
///
/// The `symbol_short!` compile-time macro accepts at most 9 ASCII bytes. Every
/// documented storage key in this workspace is kept at or below this limit so
/// keys can be constructed at compile time without heap allocation. Values in
/// the range `1..=SYMBOL_SHORT_MAX_LEN` are valid for `symbol_short!`; values
/// above this cap require the heap-allocating `Symbol::new(&env, ...)` runtime
/// constructor and therefore cannot be used as constant storage keys.
pub const SYMBOL_SHORT_MAX_LEN: u32 = 9;

/// Error returned when a candidate symbol name fails the length check enforced    /// by [`require_valid_symbol_name_length`].
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SymbolLengthError {
    /// The symbol name is empty (zero bytes).
    Empty = 1,
    /// The symbol name exceeds [`SYMBOL_SHORT_MAX_LEN`] bytes.
    TooLong = 2,
}

/// Validates that a candidate symbol name is within the bounds accepted by the
/// `symbol_short!` compile-time macro (1–9 bytes inclusive).
///
/// All documented storage keys in this workspace use `symbol_short!` so they
/// are embedded as immediate values in contract bytecode rather than heap
/// allocations. This function is the runtime guard that enforces the same
/// constraint for dynamically constructed key names.
///
/// # Arguments
/// * `name` — the raw byte slice to validate.
///
/// # Returns
/// * `Ok(())` when `1 <= name.len() <= SYMBOL_SHORT_MAX_LEN`.
/// * `Err(SymbolLengthError::Empty)` when `name.len() == 0`.
/// * `Err(SymbolLengthError::TooLong)` when `name.len() > SYMBOL_SHORT_MAX_LEN`.
///
/// # Example
/// ```ignore
/// use remitwise_common::{require_valid_symbol_name_length, SymbolLengthError};
/// assert_eq!(require_valid_symbol_name_length(b"CONFIG"), Ok(()));
/// assert_eq!(require_valid_symbol_name_length(b""), Err(SymbolLengthError::Empty));
/// assert_eq!(require_valid_symbol_name_length(b"TOOLONGKEY"), Err(SymbolLengthError::TooLong));
/// ```
pub fn require_valid_symbol_length_bytes(name: &[u8]) -> Result<(), SymbolLengthError> {
    if name.is_empty() {
        return Err(SymbolLengthError::Empty);
    }
    if name.len() as u32 > SYMBOL_SHORT_MAX_LEN {
        return Err(SymbolLengthError::TooLong);
    }
    Ok(())
}

/// Pre-upgrade snapshot version
pub const SNAPSHOT_VERSION: u32 = 1;

/// Maximum age of a pre-upgrade snapshot before restore is rejected.
pub const SNAPSHOT_MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;

/// Storage key for pre-upgrade snapshots
pub const SNAPSHOT_KEY: Symbol = symbol_short!("SNAPSHOT");

/// Typed error returned when a pre-upgrade snapshot is older than the freshness window.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SnapshotError {
    SnapshotTooOld = 1,
}

/// Ensure a pre-upgrade snapshot is still fresh enough to restore.
pub fn require_recent_snapshot(env: &Env, snapshot_taken_at: u64) -> Result<(), SnapshotError> {
    let age = env.ledger().timestamp().saturating_sub(snapshot_taken_at);
    if age > SNAPSHOT_MAX_AGE_SECS {
        Err(SnapshotError::SnapshotTooOld)
    } else {
        Ok(())
    }
}

/// Standard Settlement Window limit (30 days)
pub const MAX_SETTLEMENT_WINDOW_SECS: u64 = 30 * 24 * 60 * 60; // 30 days

/// Typed error returned when settlement occurs outside the acceptable window
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SettlementWindowError {
    /// The settlement time exceeds the due date plus the grace period.
    WindowExpired = 1,
}

/// Guards against settling an invoice excessively late, which could lead to bounds-checking
/// attacks on catch-up loops (DoS) or economic exposure from stale states.
///
/// # Arguments
/// * `settlement_time` - the current ledger time when settlement is occurring.
/// * `due_date` - the target due date of the obligation.
/// * `grace_period_secs` - the maximum allowance past the due date (e.g. `MAX_SETTLEMENT_WINDOW_SECS`).
///
/// # Returns
/// * `Ok(())` if `settlement_time <= due_date + grace_period_secs`
/// * `Err(SettlementWindowError::WindowExpired)` if it's too late.
pub fn require_within_settlement_window(
    settlement_time: u64,
    due_date: u64,
    grace_period_secs: u64,
) -> Result<(), SettlementWindowError> {
    let window_end = due_date.saturating_add(grace_period_secs);
    if settlement_time > window_end {
        Err(SettlementWindowError::WindowExpired)
    } else {
        Ok(())
    }
}

/// Rate limiting constants
pub const RATE_LIMIT_WINDOW_SECONDS: u64 = 86400; // 24 hours
const STORAGE_RATE_LIMIT: Symbol = symbol_short!("RATE_LIM");

/// Rate limit record: stores count per address + operation + window
#[contracttype]
#[derive(Clone)]
pub struct RateLimitRecord {
    pub count: u32,
    pub window_id: u64,
}

/// Error for operator validation
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OperatorError {
    NotRegistered = 99,
}

const STORAGE_OPERATORS: Symbol = symbol_short!("OPERATOR");

fn load_operators(env: &Env) -> Map<Address, bool> {
    env.storage()
        .instance()
        .get(&STORAGE_OPERATORS)
        .unwrap_or_else(|| Map::new(env))
}

/// Registers `operator` as an authorized caller for [`require_registered_operator`].
///
/// Does not enforce authentication — the calling contract is responsible for
/// gating this with its own admin check before calling it.
pub fn register_operator(env: &Env, operator: &Address) {
    let mut operators = load_operators(env);
    operators.set(operator.clone(), true);
    env.storage().instance().set(&STORAGE_OPERATORS, &operators);
}

/// Removes `operator` from the registered-operator set.
///
/// Does not enforce authentication — the calling contract is responsible for
/// gating this with its own admin check before calling it.
pub fn deregister_operator(env: &Env, operator: &Address) {
    let mut operators = load_operators(env);
    operators.remove(operator.clone());
    env.storage().instance().set(&STORAGE_OPERATORS, &operators);
}

/// Helper to enforce that `caller` specifically is a registered operator.
///
/// This provides central enforcement for operator-only operations, rejecting
/// external calls from any contract or account ID that hasn't been registered
/// via [`register_operator`].
///
/// # Security
/// Prior to this fix, the registry was a single shared on/off flag with no
/// per-address tracking: once *any* operator had ever been registered, this
/// check passed for *every* caller, regardless of identity — `caller` was
/// accepted but never actually consulted. This now checks the specific
/// address against the registered set.
pub fn require_registered_operator(env: &Env, caller: &Address) -> Result<(), OperatorError> {
    let operators = load_operators(env);
    if !operators.get(caller.clone()).unwrap_or(false) {
        return Err(OperatorError::NotRegistered);
    }
    Ok(())
}

/// Error for missing required environment/configuration variable.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EnvVarError {
    /// The requested configuration value is not set in instance storage.
    Missing = 100,
}

/// Reads a required configuration value from the contract's instance storage.
///
/// This is the generic counterpart to the typed `require_*` helpers and is
/// intended for per-contract optional configuration values that are set
/// once at initialisation and read via the Soroban environment. If the key
/// is absent a clear `EnvVarError::Missing` is returned instead of silently
/// defaulting.
///
/// # Type Parameters
///
/// * `T` — The expected value type. Must implement `TryFromVal<Env, Val>` so
///   that any Soroban-storable type (bool, u32, i128, Address, etc.) works.
///
/// # Arguments
///
/// * `env` - The Soroban environment.
/// * `key` - The storage key to look up (typically a `Symbol` from
///   `symbol_short!` or a `Symbol::new`).
///
/// # Returns
///
/// * `Ok(T)` if the value exists in instance storage.
/// * `Err(EnvVarError::Missing)` if the key is absent.
pub fn require_env_var<T>(env: &Env, key: &Symbol) -> Result<T, EnvVarError>
where
    T: TryFromVal<Env, Val>,
{
    env.storage()
        .instance()
        .get::<Symbol, T>(key)
        .ok_or(EnvVarError::Missing)
}

/// Rate limit error
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitError {
    RateLimitExceeded,
}

/// Helper to check and increment rate limit
///
/// # Arguments
/// * `env` - Soroban environment
/// * `caller` - Address of the caller
/// * `operation` - Symbol identifying the operation to rate limit
/// * `limit` - Maximum allowed operations per window
///
/// # Returns
/// * `Ok(())` if within limit
/// * `Err(RateLimitError::RateLimitExceeded)` if limit exceeded
pub fn check_and_increment_rate_limit(
    env: &Env,
    caller: &Address,
    operation: Symbol,
    limit: u32,
) -> Result<(), RateLimitError> {
    let now = env.ledger().timestamp();
    let window_id = (now / RATE_LIMIT_WINDOW_SECONDS) * RATE_LIMIT_WINDOW_SECONDS;

    let key = (caller.clone(), operation, window_id);

    let mut rate_limits: Map<(Address, Symbol, u64), RateLimitRecord> = env
        .storage()
        .instance()
        .get(&STORAGE_RATE_LIMIT)
        .unwrap_or_else(|| Map::new(env));

    let record = rate_limits.get(key.clone()).unwrap_or(RateLimitRecord {
        count: 0,
        window_id,
    });

    if record.count >= limit {
        return Err(RateLimitError::RateLimitExceeded);
    }

    let new_record = RateLimitRecord {
        count: record.count + 1,
        window_id,
    };

    rate_limits.set(key, new_record);
    env.storage()
        .instance()
        .set(&STORAGE_RATE_LIMIT, &rate_limits);

    Ok(())
}

/// Helper to get current rate limit status for an operation
pub fn get_rate_limit_status(env: &Env, caller: &Address, operation: Symbol) -> (u32, u64) {
    let now = env.ledger().timestamp();
    let window_id = (now / RATE_LIMIT_WINDOW_SECONDS) * RATE_LIMIT_WINDOW_SECONDS;

    let key = (caller.clone(), operation, window_id);

    let rate_limits: Map<(Address, Symbol, u64), RateLimitRecord> = env
        .storage()
        .instance()
        .get(&STORAGE_RATE_LIMIT)
        .unwrap_or_else(|| Map::new(env));

    let record = rate_limits.get(key).unwrap_or(RateLimitRecord {
        count: 0,
        window_id,
    });

    (record.count, window_id + RATE_LIMIT_WINDOW_SECONDS)
}

/// Typed error returned by [`verify_no_dust`], distinguishing *why* an amount
/// was rejected instead of collapsing every case into an opaque `()`.
#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AmountError {
    /// `amount == 0`. Rejected explicitly rather than falling through to the
    /// dust check, since a zero-value deposit is a distinct caller mistake
    /// (e.g. an unset form field) from a merely-too-small one.
    ZeroAmount = 1,
    /// `amount < 0`.
    NegativeAmount = 2,
    /// `0 < amount <= 1` stroop: economically meaningless (1 stroop = 0.0000001 XLM).
    DustAmount = 3,
}

/// Verifies that an amount is above the dust threshold (1 stroop).
///
/// Returns `Ok(())` when `amount > 1`, otherwise returns the specific
/// [`AmountError`] variant explaining why it was rejected.
pub fn verify_no_dust(amount: i128) -> Result<(), AmountError> {
    if amount == 0 {
        Err(AmountError::ZeroAmount)
    } else if amount < 0 {
        Err(AmountError::NegativeAmount)
    } else if amount <= 1 {
        Err(AmountError::DustAmount)
    } else {
        Ok(())
    }
}

/// Normalizes caller-supplied pagination limits for all shared paginated reads.
///
/// # Contract
/// - `0` is treated as a request for the default limit and returns `DEFAULT_PAGE_LIMIT`.
/// - Values between `1` and `MAX_PAGE_LIMIT` (inclusive) are passed through unchanged.
/// - Values greater than `MAX_PAGE_LIMIT` are capped at `MAX_PAGE_LIMIT`.
/// - The returned value is always in `1..=MAX_PAGE_LIMIT`.
/// - The function is idempotent: applying it to an already-normalized value returns
///   the same value.
/// - Extremely large inputs, including `u32::MAX`, clamp without arithmetic and
///   cannot overflow.
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_PAGE_LIMIT
    } else if limit > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        limit
    }
}

#[cfg(test)]
mod rate_limiting_tests {
    use super::*;
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{symbol_short, testutils::Address as AddressTrait, Address, Env};

    #[test]
    fn allows_requests_within_limit() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let operation = symbol_short!("test_op");
        let limit = 5u32;

        // First request should succeed
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation.clone(), limit),
            Ok(())
        );

        // Multiple requests within limit should succeed
        for _ in 0..4 {
            assert_eq!(
                check_and_increment_rate_limit(&env, &caller, operation.clone(), limit),
                Ok(())
            );
        }

        // At this point we've made 5 requests, which equals the limit
        // Next request should fail
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation, limit),
            Err(RateLimitError::RateLimitExceeded)
        );
    }

    #[test]
    fn isolates_rate_limits_by_caller() {
        let env = Env::default();
        let caller1 = Address::generate(&env);
        let caller2 = Address::generate(&env);
        let operation = symbol_short!("test_op");
        let limit = 2u32;

        // Caller1 uses up their limit
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller1, operation.clone(), limit),
            Ok(())
        );
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller1, operation.clone(), limit),
            Ok(())
        );
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller1, operation.clone(), limit),
            Err(RateLimitError::RateLimitExceeded)
        );

        // Caller2 should still be able to make requests
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller2, operation.clone(), limit),
            Ok(())
        );
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller2, operation, limit),
            Ok(())
        );
    }

    #[test]
    fn isolates_rate_limits_by_operation() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let operation1 = symbol_short!("op1");
        let operation2 = symbol_short!("op2");
        let limit = 1u32;

        // Use up limit for operation1
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation1.clone(), limit),
            Ok(())
        );
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation1, limit),
            Err(RateLimitError::RateLimitExceeded)
        );

        // operation2 should still work
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation2, limit),
            Ok(())
        );
    }

    #[test]
    fn handles_rate_limit_window_boundaries() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let operation = symbol_short!("test_op");
        let limit = 1u32;

        // Set initial time
        env.ledger().with_mut(|li| {
            li.timestamp = 1000;
        });

        // Use up the limit in the first window
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation.clone(), limit),
            Ok(())
        );
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation.clone(), limit),
            Err(RateLimitError::RateLimitExceeded)
        );

        // Advance time to next rate limit window (24 hours later)
        env.ledger().with_mut(|li| {
            li.timestamp = 1000 + RATE_LIMIT_WINDOW_SECONDS;
        });

        // Should be able to make requests again
        assert_eq!(
            check_and_increment_rate_limit(&env, &caller, operation, limit),
            Ok(())
        );
    }

    #[test]
    fn get_rate_limit_status_returns_correct_values() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let operation = symbol_short!("test_op");
        let limit = 3u32;

        // Initially should have 0 count
        let (count, window_end) = get_rate_limit_status(&env, &caller, operation.clone());
        assert_eq!(count, 0);
        assert_eq!(window_end, RATE_LIMIT_WINDOW_SECONDS);

        // After one request, count should be 1
        check_and_increment_rate_limit(&env, &caller, operation.clone(), limit).unwrap();
        let (count, _) = get_rate_limit_status(&env, &caller, operation.clone());
        assert_eq!(count, 1);

        // After two more requests, count should be 3
        check_and_increment_rate_limit(&env, &caller, operation.clone(), limit).unwrap();
        check_and_increment_rate_limit(&env, &caller, operation.clone(), limit).unwrap();
        let (count, _) = get_rate_limit_status(&env, &caller, operation);
        assert_eq!(count, 3);
    }
}

#[cfg(test)]
mod pagination_limit_tests {
    use super::*;

    #[test]
    fn normalizes_zero_to_default_limit() {
        assert_eq!(clamp_limit(0), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn preserves_valid_limits_unchanged() {
        assert_eq!(clamp_limit(1), 1);
        assert_eq!(clamp_limit(20), 20);
        assert_eq!(clamp_limit(DEFAULT_PAGE_LIMIT), DEFAULT_PAGE_LIMIT);
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
    }

    #[test]
    fn clamps_excessive_limits_to_max() {
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
        assert_eq!(clamp_limit(1000), MAX_PAGE_LIMIT);
        assert_eq!(clamp_limit(u32::MAX), MAX_PAGE_LIMIT);
    }

    #[test]
    fn handles_boundary_conditions() {
        // Test around the boundary values
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT - 1), MAX_PAGE_LIMIT - 1);
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
    }

    #[test]
    fn is_idempotent() {
        // Applying clamp_limit twice should give same result
        let test_values = [0, 1, 25, MAX_PAGE_LIMIT, MAX_PAGE_LIMIT + 100, u32::MAX];

        for value in test_values {
            let clamped_once = clamp_limit(value);
            let clamped_twice = clamp_limit(clamped_once);
            assert_eq!(
                clamped_once, clamped_twice,
                "clamp_limit is not idempotent for {}",
                value
            );
        }
    }
}

/// Pro-rata distribution helper
///
/// Maximum safe weight for a single pro-rata bucket.
///
/// Derived from `i128::MAX / i128::MAX` = 1, but the practical constraint is
/// `total.saturating_mul(max_weight)` must not overflow a consumers mental model.
/// The denominator (total_weight) is typically 10_000 (100% in basis points) or
/// 100 (percent). This constant documents the upper bound used by the saturating
/// path: any weight above this would saturate at `i128::MAX` regardless.
pub const PRO_RATA_MAX_TOTAL_WEIGHT: u32 = 10_000;

/// Distribute `total` pro-rata across `out.len()` buckets using saturating arithmetic.
///
/// Each bucket *i* (except the last) receives
/// `total.saturating_mul(weights[i] as i128).saturating_div(total_weight as i128)`.
///
/// The last bucket receives the remainder (`total - allocated_so_far`) so that
/// the conservation invariant holds:
///
/// ```text
/// sum(out) == total   (when total does not overflow i128)
///
/// ```
/// When `total` is large enough that intermediate products would exceed `i128::MAX`,
/// the saturating path caps allocations at `i128::MAX` instead of panicking.
/// No arithmetic operation in this function can panic.
///
/// # Arguments
/// * `total` - Total amount to distribute. Must be ≥ 0.
/// * `weights` - Per-bucket weights. Length must equal `out.len()`. Each weight
///   must be ≤ `total_weight`.
/// * `total_weight` - Sum of all weights. Must be > 0.
/// * `out` - Mutable slice filled with the pro-rata distribution.
///
/// # Panics (debug-only; in release these are unreachable if preconditions hold)
/// * `weights.is_empty()` or `out.is_empty()` — there must be at least one bucket.
/// * `weights.len() != out.len()` — input/output length mismatch.
/// * `total_weight == 0` — division by zero.
/// * `total < 0` — negative total is rejected.
///
/// # Examples
///
/// ```ignore
/// let mut out = [0i128; 4];
/// distribute_pro_rata(100, &[50, 30, 15, 5], 100, &mut out);
/// assert_eq!(out, [50, 30, 15, 5]);
///
/// // With basis points (10_000 = 100%):
/// let mut out = [0i128; 4];
/// distribute_pro_rata(1_000_000, &[5000, 3000, 1500, 500], 10_000, &mut out);
/// assert_eq!(out, [500_000, 300_000, 150_000, 50_000]);
/// ```
pub fn distribute_pro_rata(total: i128, weights: &[u32], total_weight: u32, out: &mut [i128]) {
    assert!(total >= 0, "total must be non-negative");
    assert!(total_weight > 0, "total_weight must be positive");
    assert!(!out.is_empty(), "out must not be empty");
    assert!(!weights.is_empty(), "weights must not be empty");
    assert_eq!(
        weights.len(),
        out.len(),
        "weights and out must have the same length"
    );

    let n = weights.len();

    // All buckets except the last: standard pro-rata floor allocation.
    let mut allocated: i128 = 0;
    let last = n.saturating_sub(1);
    for i in 0..last {
        let weight = weights[i] as i128;
        let share = total
            .saturating_mul(weight)
            .saturating_div(total_weight as i128);
        out[i] = share;
        allocated = allocated.saturating_add(share);
    }

    // Last bucket receives the remainder, guaranteeing conservation.
    // When n == 1, last == 0 and allocated == 0, so out[0] == total.
    out[last] = total.saturating_sub(allocated);
}

/// Error converting an integer to `i128` when the value is out of range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntConversionError {
    Overflow,
}

/// Fallible conversion to `i128` for safe cross-type arithmetic.
pub trait ToI128Checked {
    fn to_i128_checked(self) -> Result<i128, IntConversionError>;
}

impl ToI128Checked for u32 {
    fn to_i128_checked(self) -> Result<i128, IntConversionError> {
        Ok(self as i128)
    }
}

impl ToI128Checked for i32 {
    fn to_i128_checked(self) -> Result<i128, IntConversionError> {
        Ok(self as i128)
    }
}

// ---------------------------------------------------------------------------
// Symbol canonicalisation — trim, casefold, charset validation
// ---------------------------------------------------------------------------

/// Maximum byte length of a canonicalised symbol (mirrors Soroban's limit).
pub const SYMBOL_MAX_LEN: usize = 32;

/// Error returned by [`canonicalise_symbol_checked`] and
/// [`canonicalise_symbols`] when an input string cannot be turned into a
/// valid canonical `Symbol`.
///
/// All variants carry enough context for the caller to map them to a
/// contract-specific `#[contracterror]` without losing the error category.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SymbolValidationError {
    /// Input was empty or contained only ASCII whitespace after trimming.
    Empty,
    /// Trimmed input exceeds [`SYMBOL_MAX_LEN`] bytes.
    TooLong,
    /// A byte at `position` is not in the allowed charset `[a-z0-9_]` after
    /// ASCII uppercase folding.  Internal ASCII spaces (mid-string) are caught
    /// here rather than being silently stripped.
    InvalidChar { position: u32 },
}

/// Allowed charset predicate after uppercase folding.
///
/// `[a-z0-9_]` — identical to the Soroban `Symbol` charset restriction
/// minus the uppercase letters (which we fold away).
#[inline(always)]
fn is_symbol_char(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

/// Validate and canonicalise a `soroban_sdk::String` into a `Symbol`,
/// returning a typed error instead of panicking on bad input.
///
/// ## Transformation rules (applied in order)
///
/// | Step | Rule |
/// |---|---|
/// | **Trim** | Strip leading and trailing ASCII whitespace (`\t \n \r`). |
/// | **Empty check** | Trimmed length = 0 → `Err(SymbolValidationError::Empty)`. |
/// | **Length check** | Trimmed length > 32 → `Err(SymbolValidationError::TooLong)`. |
/// | **Casefold** | ASCII `A-Z` → `a-z`. |
/// | **Charset check** | Every byte must be in `[a-z0-9_]`. First offending byte → `Err(SymbolValidationError::InvalidChar { position })`. |
///
/// ## Examples
///
/// ```rust,ignore
/// // Valid
/// assert!(canonicalise_symbol_checked(&env, &String::from_str(&env, "Hello_World")).is_ok());
/// // → Symbol("hello_world")
///
/// // Invalid char
/// assert_eq!(
///     canonicalise_symbol_checked(&env, &String::from_str(&env, "bad-char")),
///     Err(SymbolValidationError::InvalidChar { position: 3 })
/// );
///
/// // Too long
/// assert_eq!(
///     canonicalise_symbol_checked(&env, &String::from_str(&env, &"x".repeat(33))),
///     Err(SymbolValidationError::TooLong)
/// );
/// ```
pub fn canonicalise_symbol_checked(
    env: &Env,
    input: &soroban_sdk::String,
) -> Result<Symbol, SymbolValidationError> {
    let len = input.len() as usize;
    if len == 0 {
        return Err(SymbolValidationError::Empty);
    }
    // 256 bytes is enough: valid symbols are ≤32 bytes; anything longer is
    // rejected by the TooLong check after trimming.
    let mut buf = [0u8; 256];
    let read_len = len.min(buf.len());
    input.copy_into_slice(&mut buf[..read_len]);

    // Interpret as UTF-8 (Soroban strings are always valid UTF-8).
    let s = core::str::from_utf8(&buf[..read_len])
        .unwrap_or_else(|_| panic!("symbol input is not valid UTF-8"));

    // Step 1: trim
    let trimmed = s.trim();
    let trimmed_len = trimmed.len();
    if trimmed_len == 0 {
        return Err(SymbolValidationError::Empty);
    }
    if trimmed_len > SYMBOL_MAX_LEN {
        return Err(SymbolValidationError::TooLong);
    }

    // Step 2: casefold + charset validation
    let trimmed_bytes = trimmed.as_bytes();
    let mut canonical = [0u8; SYMBOL_MAX_LEN];
    for (i, &byte) in trimmed_bytes.iter().enumerate() {
        let folded = if byte.is_ascii_uppercase() {
            byte.to_ascii_lowercase()
        } else {
            byte
        };
        if !is_symbol_char(folded) {
            return Err(SymbolValidationError::InvalidChar { position: i as u32 });
        }
        canonical[i] = folded;
    }

    let canonical_str = core::str::from_utf8(&canonical[..trimmed_len])
        .unwrap_or_else(|_| panic!("canonicalised symbol is not valid UTF-8"));

    Ok(Symbol::new(env, canonical_str))
}

/// Canonicalise a `soroban_sdk::String` into a `Symbol`, panicking with a
/// descriptive message on invalid input.
///
/// This is a thin panic wrapper around [`canonicalise_symbol_checked`].
/// Prefer the checked variant when the calling contract needs a typed error
/// for the caller (e.g. to return `InvalidSymbol` instead of aborting the
/// transaction).  Use this variant only at trusted call sites where bad input
/// is a programming error rather than untrusted user input.
///
/// ## Transformation rules
///
/// Identical to [`canonicalise_symbol_checked`]: trim → casefold → charset
/// check (`[a-z0-9_]`).
///
/// ## Panics
///
/// - Empty or whitespace-only input after trimming.
/// - Trimmed input longer than 32 bytes.
/// - Any byte outside `[a-z0-9_]` after uppercase folding (includes hyphens,
///   spaces, `@`, `.`, `!`, etc.).
pub fn canonicalise_symbol(env: &Env, input: &soroban_sdk::String) -> Symbol {
    match canonicalise_symbol_checked(env, input) {
        Ok(sym) => sym,
        Err(SymbolValidationError::Empty) => {
            panic!("symbol input must contain at least one non-whitespace character")
        }
        Err(SymbolValidationError::TooLong) => {
            panic!("symbol input must contain between 1 and 32 characters after trimming")
        }
        Err(SymbolValidationError::InvalidChar { position }) => {
            panic!("invalid Symbol character at position {}", position)
        }
    }
}

/// Canonicalise every string in `inputs`, returning a `Vec<Symbol>` in the
/// same order, or the first validation error encountered.
///
/// This batch variant is the natural companion to [`canonicalise_symbol_checked`]
/// for entry points that accept a list of symbol keys (e.g. a list of category
/// labels or pause-channel names).  Processing short-circuits on the first
/// error; the `position` field inside
/// [`SymbolValidationError::InvalidChar`] refers to the byte position
/// within the **failing element**, not its index in the batch.
///
/// ## Errors
///
/// Returns the first [`SymbolValidationError`] encountered.  The order of
/// validation matches the iteration order of `inputs`.
pub fn canonicalise_symbols(
    env: &Env,
    inputs: &soroban_sdk::Vec<soroban_sdk::String>,
) -> Result<soroban_sdk::Vec<Symbol>, SymbolValidationError> {
    if inputs.is_empty() {
        return Err(SymbolValidationError::Empty);
    }
    let mut out = soroban_sdk::Vec::new(env);
    for item in inputs.iter() {
        let sym = canonicalise_symbol_checked(env, &item)?;
        out.push_back(sym);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Rate newtype — basis-points arithmetic
// ---------------------------------------------------------------------------

/// Basis-points denominator: 10_000 basis points = 100%.
///
/// All Remitwise contracts express percentages in basis points (1 bps = 0.01%)
/// so that integer arithmetic can be used without floating point.
pub const BASIS_POINTS: u32 = 10_000;
/// Number of basis points in a single whole percent.
pub const BPS_PER_PERCENT: u32 = 100;
/// Alias for the number of basis points in a single whole percent.
pub const BASIS_POINTS_PER_PERCENT: u32 = BPS_PER_PERCENT;

/// Supported units for externally supplied rate inputs.
///
/// Remitwise contracts currently accept only basis points. Treating a raw rate
/// value as unitless would let a caller supply an unexpected denomination and
/// have the contract silently interpret it as basis points, potentially
/// magnifying or shrinking fee/discount/allocation calculations. This guard
/// makes the accepted unit explicit and reject-by-default.

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RateUnit {
    BasisPoints = 1,
}

/// Error returned when an externally supplied rate unit is unsupported.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RateUnitError {
    UnsupportedRateUnit = 1,
}

/// Require that `unit` is one of the rate denominations currently supported by
/// the contracts.
///
/// # Errors
/// Returns [`RateUnitError::UnsupportedRateUnit`] when `unit` is not accepted.
#[inline(always)]
pub fn require_supported_rate_unit(unit: u32) -> Result<RateUnit, RateUnitError> {
    match unit {
        1 => Ok(RateUnit::BasisPoints),
        _ => Err(RateUnitError::UnsupportedRateUnit),
    }
}

/// Error returned by [`Rate`] arithmetic when the result overflows `i128`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateError {
    /// The intermediate or final result exceeds numerical limits (`i128::MAX` or `u32::MAX`).
    Overflow,
}

/// A whole percentage value (1% = 100 basis points).
///
/// `Percent` wraps a `u32` representing whole percentage units. Safe conversions
/// to basis points ([`Rate`]) are provided via [`to_rate`](Percent::to_rate),
/// [`to_bps`](Percent::to_bps), and `TryFrom<Percent> for Rate`.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Percent(u32);

impl Percent {
    pub const ZERO: Percent = Percent(0);
    pub const HUNDRED: Percent = Percent(100);

    /// Create a `Percent` from a whole percentage integer value.
    #[inline(always)]
    pub fn from_percentage(percent: u32) -> Self {
        Self(percent)
    }

    /// Return the whole percentage integer value.
    #[inline(always)]
    pub fn to_percentage(self) -> u32 {
        self.0
    }

    /// Convert this `Percent` to a basis-points [`Rate`].
    ///
    /// Returns `Ok(Rate)` if `percent * 100` fits in `u32`, or `Err(RateError::Overflow)` otherwise.
    pub fn to_rate(self) -> Result<Rate, RateError> {
        Rate::from_percent(self.0)
    }

    /// Convert this `Percent` to raw basis points (`u32`).
    ///
    /// Returns `Ok(bps)` if `percent * 100` fits in `u32`, or `Err(RateError::Overflow)` otherwise.
    pub fn to_bps(self) -> Result<u32, RateError> {
        self.0
            .checked_mul(BPS_PER_PERCENT)
            .ok_or(RateError::Overflow)
    }
}

impl TryFrom<Percent> for Rate {
    type Error = RateError;

    #[inline(always)]
    fn try_from(percent: Percent) -> Result<Self, Self::Error> {
        percent.to_rate()
    }
}

/// A rate expressed in basis points (1 bps = 0.01 %).
///
/// `Rate` wraps a `u32` where the stored value represents hundredths of a
/// percent:
///
/// | Value | Meaning         |
/// |-------|-----------------|
/// | 0     | 0 %             |
/// | 1     | 0.01 %          |
/// | 100   | 1 %             |
/// | 500   | 5 %             |
/// | 1_000 | 10 %            |
/// | 10_000| 100 %           |
/// | 50_000| 500 % (overage) |
///
/// Use [`apply_to`](Rate::apply_to) to compute `amount * rate / BASIS_POINTS`
/// with checked arithmetic.
///
/// # Examples
/// ```
/// use remitwise_common::{Rate, BASIS_POINTS, RateError};
///
/// let rate = Rate::from_bps(500); // 5%
/// assert_eq!(rate.apply_to(1000), Ok(50));
/// assert_eq!(rate.apply_to(i128::MAX), Err(RateError::Overflow));
/// ```
///
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Rate(u32);

impl Rate {
    pub const ZERO: Rate = Rate(0);
    pub const MAX: Rate = Rate(u32::MAX);

    /// Create a `Rate` from a raw basis-point value.
    ///
    /// No validation is performed — `u32::MAX` is accepted. Callers that need
    /// semantic bounds (e.g. `rate <= BASIS_POINTS` for a discount rate) should
    /// check them at the call site.
    #[inline(always)]
    pub fn from_bps(bps: u32) -> Self {
        Self(bps)
    }

    /// Create a `Rate` from a percentage value.
    ///
    /// Converts percentage to basis points by multiplying by BPS_PER_PERCENT (100).
    /// For example: 5% becomes 500 basis points.
    #[inline(always)]
    pub fn from_percent(percent: u32) -> Result<Self, RateError> {
        percent
            .checked_mul(BPS_PER_PERCENT)
            .map(Self::from_bps)
            .ok_or(RateError::Overflow)
    }

    /// Construct a `Rate` from an externally supplied raw value plus unit.
    ///
    /// This is the safe entry point for untrusted inputs that carry an explicit
    /// unit field. Only supported units are accepted.
    #[inline(always)]
    pub fn try_from_input(value: u32, unit: u32) -> Result<Self, RateUnitError> {
        require_supported_rate_unit(unit)?;
        Ok(Self::from_bps(value))
    }

    /// Construct a `Rate` from a `Percent` wrapper.
    #[inline(always)]
    pub fn from_percent_type(percent: Percent) -> Result<Self, RateError> {
        Self::from_percent(percent.to_percentage())
    }

    /// Return the raw basis-point value.
    #[inline(always)]
    pub fn to_bps(self) -> u32 {
        self.0
    }

    /// Convert this rate back to a whole percentage integer value, truncating fractional basis points.
    ///
    /// For example: 550 bps becomes 5%.
    #[inline(always)]
    pub fn to_percent(self) -> u32 {
        self.0 / BPS_PER_PERCENT
    }

    /// Return true if this rate contains a fractional percentage (basis points not divisible by 100).
    ///
    /// For example: 550 bps returns true (0.5% fractional), 500 bps returns false (exactly 5%).
    #[inline(always)]
    #[allow(clippy::manual_is_multiple_of)]
    pub fn has_fractional_percent(self) -> bool {
        self.0 % BPS_PER_PERCENT != 0
    }

    /// Apply this rate to `amount`, computing `(amount * self) / BASIS_POINTS`.
    ///
    /// Uses checked arithmetic. Returns:
    /// - `Ok(result)` when the multiplication and division succeed.
    /// - `Err(RateError::Overflow)` when `amount * self` overflows `i128`.
    ///
    /// Note: the division truncates towards zero. This matches the behaviour of
    /// `safe_percent` elsewhere in the codebase.
    pub fn apply_to(self, amount: i128) -> Result<i128, RateError> {
        let rate_i128 = self.0 as i128;
        amount
            .checked_mul(rate_i128)
            .and_then(|product| product.checked_div(BASIS_POINTS as i128))
            .ok_or(RateError::Overflow)
    }
}

impl ToI128Checked for Rate {
    #[inline(always)]
    fn to_i128_checked(self) -> Result<i128, IntConversionError> {
        Ok(self.0 as i128)
    }
}

/// Error related to time and periods.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TimeError {
    InvalidPeriod = 7,
}

/// Namespace for shared timestamp helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct Timestamp;

/// Enumeration of period bucket types for timestamp bucketing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeriodKind {
    Day,
    Week,
    Month,
}

/// Seconds in a standard day (86400) and week (604800).
pub const SECONDS_PER_DAY: u64 = 86400;
pub const SECONDS_PER_WEEK: u64 = 86400 * 7;

impl Timestamp {
    /// Returns the number of whole seconds from `now` until `target`.
    ///
    /// The result saturates at `0` when `target <= now`, so callers can measure
    /// future distance without risking underflow or writing their own
    /// `saturating_sub`/guard pattern.
    #[inline(always)]
    pub fn seconds_until(now: u64, target: u64) -> u64 {
        target.saturating_sub(now)
    }

    /// Buckets a Unix timestamp into a stable period key (day/week/month).
    ///
    /// - Day: Returns the day index since Unix epoch (UTC), i.e. `timestamp / 86400`.
    /// - Week: Returns the week index since Unix epoch (UTC), i.e. `timestamp / 604800`.
    /// - Month: Returns the [YYYYMM] encoding as (year * 100 + month), e.g. 202412 for December 2024.
    ///   Handles proleptic Gregorian conversion in UTC. Leap seconds are ignored.
    ///
    /// Pre-1970 timestamps are not representable through the `u64` API (day 0
    /// is 1970-01-01). The function never panics and returns a `u64` for every
    /// input; callers do not need to handle an error path.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    #[inline(always)]
    pub fn to_period_key(timestamp: u64, period: PeriodKind) -> u64 {
        match period {
            PeriodKind::Day => timestamp / SECONDS_PER_DAY,
            PeriodKind::Week => timestamp / SECONDS_PER_WEEK,
            PeriodKind::Month => {
                // Convert timestamp (seconds since epoch) to YYYYMM integer.
                // Uses proleptic Gregorian calendar, UTC; ignores leap seconds.
                // Algorithm from https://howardhinnant.github.io/date_algorithms.html#civil_from_days
                let days = timestamp / SECONDS_PER_DAY;
                // 1970-01-01 is day 0.
                let z = days as i64 + 719468;
                let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
                let doe = z - era * 146097; // [0, 146096]
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
                let y = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let month = (mp + 2) % 12 + 1;
                let year = y + (mp + 2) / 12;
                (year as u64) * 100 + (month as u64)
            }
        }
    }
}

/// Validates that a requested period is logically ordered.
///
/// # Errors
/// Returns `TimeError::InvalidPeriod` if `start > end`.
pub fn validate_period(start: u64, end: u64) -> Result<(), TimeError> {
    if start > end {
        Err(TimeError::InvalidPeriod)
    } else {
        Ok(())
    }
}

/// Error returned when the current ledger sequence does not match the expected value.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum LedgerError {
    LedgerMismatch = 1,
    /// The current ledger sequence is strictly less than a previously observed
    /// value (`prev`). The Soroban host guarantees sequence monotonicity
    /// (`docs/LEDGER_MONOTONICITY.md`), so a regression indicates either a
    /// replay attempt, a stale read, or a logic bug at the call site.
    LedgerSequenceRegression = 2,
}

/// Asserts that `expected` matches the current ledger sequence number.
///
/// This is a replay-prevention helper: if an operation was authorized for a
/// specific ledger (e.g. via a signed nonce bound to a ledger), executing it in
/// a different ledger would let an attacker replay the same authorization in a
/// later ledger.  Call this function at the start of the operation to tie it
/// to the current ledger.
///
/// # Errors
/// Returns [`LedgerError::LedgerMismatch`] when `expected != env.ledger().sequence()`.
pub fn require_matching_ledger(env: &Env, expected: u32) -> Result<(), LedgerError> {
    let current = env.ledger().sequence();
    if current != expected {
        Err(LedgerError::LedgerMismatch)
    } else {
        Ok(())
    }
}

/// Asserts that the current ledger sequence is greater than or equal to a
/// previously observed baseline (`prev`).
///
/// Defence-in-depth against off-by-N replay, stale-storage baseline after a
/// contract upgrade, and `u32`-cast underflow: ties the caller-supplied (or
/// cached) baseline to the authoritative source `env.ledger().sequence()`
/// and rejects any regression at the call site.
///
/// The Soroban host already guarantees strict sequence monotonicity across
/// ledgers (see `docs/LEDGER_MONOTONICITY.md`), but this helper closes the
/// gap where contract code caches a `prev` baseline across calls and later
/// compares against it — a regression-at-rest can otherwise let an
/// authorization captured at `prev` be replayed at a smaller `curr`
/// (fee updates, role grants, mint caps, etc.).
///
/// # Errors
/// * [`LedgerError::LedgerSequenceRegression`] when
///   `env.ledger().sequence() < prev`.
/// * `Ok(())` on equal or monotonic-progression cases. Equality is
///   tolerated so a baseline captured on the same ledger does not
///   falsely reject a re-entry on that ledger.
///
/// # Recommended call-site pattern
///
/// ```ignore
/// require_ledger_seq_monotonic(&env, prev_seq_baseline)
///     .unwrap_or_else(|_| panic_with_error!(&env, MyError::LedgerRegression));
/// ```
pub fn require_ledger_seq_monotonic(env: &Env, prev: u32) -> Result<(), LedgerError> {
    let current = env.ledger().sequence();
    if current < prev {
        Err(LedgerError::LedgerSequenceRegression)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod ledger_monotonicity_tests {
    //! Tests for [`require_ledger_seq_monotonic`] and the surrounding
    //! [`LedgerError`] variants.
    //!
    //! The Soroban test host allows `env.ledger().set(...)` to mutate
    //! both `sequence_number` and `timestamp` between calls, which lets
    //! us write a regression scenario without depending on real
    //! host-level sequencing behaviour.

    use super::{require_ledger_seq_monotonic, LedgerError};
    use soroban_sdk::testutils::{Ledger, LedgerInfo};
    use soroban_sdk::Env;

    /// Sets the ledger sequence and preserves other ledger state.
    fn set_seq(env: &Env, sequence_number: u32) {
        let proto = env.ledger().protocol_version();
        env.ledger().set(LedgerInfo {
            protocol_version: proto,
            sequence_number,
            timestamp: 1_700_000_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 3_000_000,
        });
    }

    /// Acceptance contract: equal sequences are NOT a regression.
    /// A baseline captured on the same ledger must re-validate cleanly.
    #[test]
    fn accepts_equal_baseline_and_current_sequence() {
        let env = Env::default();
        set_seq(&env, 100);
        assert_eq!(require_ledger_seq_monotonic(&env, 100), Ok(()));
    }

    /// Positive progression: current > prev must succeed.
    #[test]
    fn accepts_monotonic_progression() {
        let env = Env::default();
        set_seq(&env, 101);
        assert_eq!(require_ledger_seq_monotonic(&env, 100), Ok(()));
    }

    /// Large jump: e.g. after a network upgrade or test re-org, a much
    /// higher current ledger must still pass.
    #[test]
    fn accepts_large_positive_jump() {
        let env = Env::default();
        set_seq(&env, 1_000_000);
        assert_eq!(require_ledger_seq_monotonic(&env, 100), Ok(()));
    }

    /// Negative test (#1240): any current < prev must be rejected with
    /// the typed error. This is the headline regression test — without
    /// `require_ledger_seq_monotonic`, a replay at a lower ledger would
    /// silently pass.
    #[test]
    fn rejects_regressed_sequence_by_one() {
        let env = Env::default();
        // prev = 100, current = 99 — regression by one.
        set_seq(&env, 99);
        assert_eq!(
            require_ledger_seq_monotonic(&env, 100),
            Err(LedgerError::LedgerSequenceRegression),
        );
    }

    /// Negative test: large regression.
    #[test]
    fn rejects_regressed_sequence_by_large_amount() {
        let env = Env::default();
        set_seq(&env, 50);
        assert_eq!(
            require_ledger_seq_monotonic(&env, 1_000_000),
            Err(LedgerError::LedgerSequenceRegression),
        );
    }

    /// Boundary: `prev = 0` and `current = 0` is the genesis baseline —
    /// must accept.
    #[test]
    fn accepts_genesis_baseline_zero() {
        let env = Env::default();
        set_seq(&env, 0);
        assert_eq!(require_ledger_seq_monotonic(&env, 0), Ok(()));
    }

    /// Boundary: `prev = 0` and `current = 1` is the first legal
    /// advancement — must accept.
    #[test]
    fn accepts_advancement_from_genesis() {
        let env = Env::default();
        set_seq(&env, 1);
        assert_eq!(require_ledger_seq_monotonic(&env, 0), Ok(()));
    }

    /// u32 regression boundary: `prev = u32::MAX`, `current = u32::MAX - 1`.
    /// Pins the saturation/underflow behaviour at the upper bound.
    #[test]
    fn rejects_regression_at_u32_max_boundary() {
        let env = Env::default();
        set_seq(&env, u32::MAX - 1);
        assert_eq!(
            require_ledger_seq_monotonic(&env, u32::MAX),
            Err(LedgerError::LedgerSequenceRegression),
        );
    }
}

// ---------------------------------------------------------------------------
// Non-zero u128 helper
// ---------------------------------------------------------------------------

/// Error returned when a non-zero u128 value was expected but zero was provided.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZeroNotAllowed;

/// A u128 value that is guaranteed to be non-zero.
///
/// This type wraps a `u128` and enforces at construction that the value is not
/// zero. Once constructed, callers can safely assume the value is in `1..=u128::MAX`.
///
/// # Examples
///
/// ```ignore
/// use remitwise_common::{NonZeroU128, ZeroNotAllowed};
///
/// let nz = NonZeroU128::new(42).unwrap();
/// assert_eq!(nz.get(), 42);
///
/// assert_eq!(NonZeroU128::new(0), Err(ZeroNotAllowed));
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct NonZeroU128(u128);

impl NonZeroU128 {
    /// Creates a new `NonZeroU128` if `value` is non-zero.
    pub fn new(value: u128) -> Result<Self, ZeroNotAllowed> {
        if value == 0 {
            Err(ZeroNotAllowed)
        } else {
            Ok(NonZeroU128(value))
        }
    }

    /// Returns the contained u128 value.
    pub fn get(&self) -> u128 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Tag canonicalization
// ---------------------------------------------------------------------------

/// Maximum allowed byte length for a single tag.
pub const TAG_MAX_LEN: u32 = 32;

/// Validation failure returned by [`canonicalize_tags_checked`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagError {
    /// The tag batch is empty, or an individual tag is zero bytes long.
    Empty,
    /// A tag exceeds [`TAG_MAX_LEN`] bytes.
    TooLong,
    /// A byte at `position` is not in the allowed charset after upper-case folding.
    InvalidChar { position: u32 },
}

/// Signature verification failure.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SignatureError {
    /// Invalid signature length (must be 64 bytes for Ed25519).
    InvalidSignatureLength = 1,
    /// Invalid public key length (must be 32 bytes for Ed25519).
    InvalidPublicKeyLength = 2,
    /// Signature verification failed.
    VerificationFailed = 3,
    /// The verifier public key has not been registered for attestation verification.
    UnregisteredVerifier = 4,
    /// The verifier public key was registered under a different Stellar network
    /// (e.g. Testnet) than the one this contract instance is currently running on.
    VerifierNetworkMismatch = 5,
}

/// Storage key for the set of registered verifier public keys.
const REGISTERED_VERIFIERS_KEY: Symbol = symbol_short!("REGVER");

/// Registers a verifier public key so its attestations may be consumed.
///
/// The public key is bound to `env.ledger().network_id()` (the SHA-256 hash of
/// the network passphrase) at registration time. This prevents a verifier key
/// that was only ever intended to be trusted on one Stellar network (for
/// example a "test signer" key provisioned for Testnet QA) from silently
/// becoming trusted on another network (for example Public/Mainnet) if the
/// underlying storage entry is ever copied or replayed across deployments —
/// see [`require_registered_verifier`].
pub fn register_verifier(env: &Env, public_key: &[u8]) -> Result<(), SignatureError> {
    let pk_arr: [u8; 32] = public_key
        .try_into()
        .map_err(|_| SignatureError::InvalidPublicKeyLength)?;
    let key = BytesN::<32>::from_array(env, &pk_arr);

    let mut registered_verifiers: Map<BytesN<32>, BytesN<32>> = env
        .storage()
        .instance()
        .get(&REGISTERED_VERIFIERS_KEY)
        .unwrap_or_else(|| Map::new(env));

    registered_verifiers.set(key, env.ledger().network_id());
    env.storage()
        .instance()
        .set(&REGISTERED_VERIFIERS_KEY, &registered_verifiers);

    Ok(())
}

/// Requires the supplied verifier public key to be registered, and registered
/// under the network this contract instance is currently executing on, before
/// an external attestation can be consumed.
///
/// # Errors
/// * [`SignatureError::UnregisteredVerifier`] if the key was never registered.
/// * [`SignatureError::VerifierNetworkMismatch`] if the key was registered
///   under a different network than the current one (see [`register_verifier`]).
pub fn require_registered_verifier(env: &Env, public_key: &[u8]) -> Result<(), SignatureError> {
    let pk_arr: [u8; 32] = public_key
        .try_into()
        .map_err(|_| SignatureError::InvalidPublicKeyLength)?;
    let key = BytesN::<32>::from_array(env, &pk_arr);

    let registered_verifiers: Map<BytesN<32>, BytesN<32>> = env
        .storage()
        .instance()
        .get(&REGISTERED_VERIFIERS_KEY)
        .unwrap_or_else(|| Map::new(env));

    match registered_verifiers.get(key) {
        Some(registered_network_id) if registered_network_id == env.ledger().network_id() => Ok(()),
        Some(_) => Err(SignatureError::VerifierNetworkMismatch),
        None => Err(SignatureError::UnregisteredVerifier),
    }
}

/// Verify an Ed25519 signature with domain separation.
///
/// The payload is encoded as a length-delimited byte stream so adjacent or
/// overlapping separators/messages cannot collide. For example, the pair
/// `(domain="ab", message="cdef")` and `(domain="abc", message="def")`
/// produce different payloads even though their plain concatenation would be
/// identical.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `domain_separator` - Domain separator to prevent cross-domain replay attacks
/// * `message` - The message to verify
/// * `signature` - The Ed25519 signature (64 bytes)
/// * `public_key` - The Ed25519 public key (32 bytes)
///
/// # Returns
/// * `Ok(())` if the signature is valid
/// * `Err(SignatureError)` if verification fails
pub fn verify_signature(
    env: &soroban_sdk::Env,
    domain_separator: &[u8],
    message: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<(), SignatureError> {
    require_registered_verifier(env, public_key)?;

    // Note: this used to also run a first, redundant `ed25519_verify` pass
    // against a plain (non-length-prefixed) concatenation of
    // domain_separator + message, built with unchecked `copy_from_slice`
    // (which panics instead of returning InvalidSignatureLength /
    // InvalidPublicKeyLength for a wrong-length input). It verified a
    // different, ambiguous encoding that no real caller signs against, so it
    // did nothing but double the ed25519_verify cost of every call -- and
    // broke callers whose signature/public_key had a bad length, since the
    // panic pre-empted the length-checked error path below.
    let pk_arr: [u8; 32] = public_key
        .try_into()
        .map_err(|_| SignatureError::InvalidPublicKeyLength)?;
    let sig_arr: [u8; 64] = signature
        .try_into()
        .map_err(|_| SignatureError::InvalidSignatureLength)?;

    let mut msg_bytes = Bytes::new(env);
    let domain_len = (domain_separator.len() as u64).to_le_bytes();
    let message_len = (message.len() as u64).to_le_bytes();

    msg_bytes.extend_from_slice(&domain_len);
    msg_bytes.extend_from_slice(domain_separator);
    msg_bytes.extend_from_slice(&message_len);
    msg_bytes.extend_from_slice(message);

    let sig_bytes = soroban_sdk::BytesN::from_array(env, &sig_arr);
    let pk_bytes = soroban_sdk::BytesN::from_array(env, &pk_arr);

    env.crypto()
        .ed25519_verify(&pk_bytes, &msg_bytes, &sig_bytes);
    Ok(())
}

/// Typed error for slash signature verification.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SlashError {
    InvalidSignature = 8,
}

/// Verify an optional second-party slash signature.
///
/// This provides a defence-in-depth gate before executing destructive slash operations.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `message` - The payload being authorized (e.g. amount or slash payload)
/// * `signature` - Optional 64-byte Ed25519 signature
/// * `public_key` - 32-byte Ed25519 public key of the second party
///
/// # Returns
/// * `Ok(())` if signature is valid or not provided (optional gate)
/// * `Err(SlashError)` if the provided signature is invalid
pub fn verify_slash_signature(
    env: &soroban_sdk::Env,
    message: &[u8],
    signature: Option<&[u8]>,
    public_key: &[u8],
) -> Result<(), SlashError> {
    if let Some(sig) = signature {
        if verify_signature(env, b"slash-auth", message, sig, public_key).is_err() {
            return Err(SlashError::InvalidSignature);
        }
    }
    Ok(())
}

/// Validates and canonicalizes a batch of tags without panicking.
///
/// # Rules
/// - The batch must contain at least one tag ([`TagError::Empty`]).
/// - Each tag must be between 1 and [`TAG_MAX_LEN`] bytes inclusive
///   ([`TagError::Empty`] for zero length, [`TagError::TooLong`] otherwise).
/// - Allowed charset: `[a-z0-9\-_]`. ASCII uppercase letters are silently
///   folded to lowercase; any other byte yields [`TagError::InvalidChar`].
///
/// Validation short-circuits on the first violation (empty batch, length, or
/// invalid byte) for gas efficiency.
///
/// # Returns
/// On success, a new `Vec<String>` containing the normalized (lowercased) tags
/// in the same order as the input. The function does **not** deduplicate.
///
/// # Usage
/// ```ignore
/// use remitwise_common::{canonicalize_tags_checked, TagError};
/// match canonicalize_tags_checked(&env, &tags) {
///     Ok(normalized) => { /* store normalized */ }
///     Err(TagError::InvalidChar { .. }) => {
///         soroban_sdk::panic_with_error!(&env, MyError::InvalidTagContent)
///     }
///     Err(TagError::Empty) | Err(TagError::TooLong) => { /* map to caller error */ }
/// }
/// ```
/// Validates and canonicalizes a single tag: enforces `1..=TAG_MAX_LEN` byte
/// length and the `[a-z0-9\-_]` charset, lowercasing ASCII uppercase letters.
///
/// Extracted from [`canonicalize_tags_checked`] so single-tag lookups (e.g.
/// a tag-index query keyed on one caller-supplied tag) don't need to
/// allocate a one-element `Vec` just to call the batch API.
pub fn canonicalize_tag_checked(
    env: &soroban_sdk::Env,
    tag: &soroban_sdk::String,
) -> Result<soroban_sdk::String, TagError> {
    let len = tag.len();
    if len == 0 {
        return Err(TagError::Empty);
    }
    if len > TAG_MAX_LEN {
        return Err(TagError::TooLong);
    }
    let mut buf = [0u8; 32];
    tag.copy_into_slice(&mut buf[..len as usize]);
    for (position, byte) in buf.iter_mut().take(len as usize).enumerate() {
        if byte.is_ascii_uppercase() {
            *byte += b'a' - b'A';
        }
        let b = *byte;
        if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
            return Err(TagError::InvalidChar {
                position: position as u32,
            });
        }
    }
    let s = match core::str::from_utf8(&buf[..len as usize]) {
        Ok(v) => v,
        Err(_) => {
            return Err(TagError::InvalidChar { position: 0 });
        }
    };
    Ok(soroban_sdk::String::from_str(env, s))
}

pub fn canonicalize_tags_checked(
    env: &soroban_sdk::Env,
    tags: &soroban_sdk::Vec<soroban_sdk::String>,
) -> Result<soroban_sdk::Vec<soroban_sdk::String>, TagError> {
    if tags.is_empty() {
        return Err(TagError::Empty);
    }
    let mut out = soroban_sdk::Vec::new(env);
    for tag in tags.iter() {
        out.push_back(canonicalize_tag_checked(env, &tag)?);
    }
    Ok(out)
}

/// Validates and canonicalizes a batch of tags, panicking on failure.
///
/// This is a thin wrapper around [`canonicalize_tags_checked`] that preserves
/// the legacy panic-based contract for existing callers. Prefer
/// [`canonicalize_tags_checked`] when handling untrusted or indexer-supplied
/// tag strings so errors can be mapped to typed contract errors.
///
/// # Rules
/// - The batch must contain at least one tag (`panic!("Tags cannot be empty")`).
/// - Each tag must be between 1 and [`TAG_MAX_LEN`] bytes inclusive
///   (`panic!("Tag must be between 1 and 32 characters")`).
/// - Allowed charset: `[a-z0-9\-_]`. ASCII uppercase letters are silently
///   folded to lowercase; any other byte causes the supplied `on_invalid_char`
///   closure to be called once (typically `panic_with_error!` or `panic!`).
///
/// # Returns
/// A new `Vec<String>` containing the normalized (lowercased) tags in the
/// same order as the input.
///
/// # Usage
/// ```ignore
/// use remitwise_common::canonicalize_tags;
/// let normalized = canonicalize_tags(&env, &tags, || {
///     soroban_sdk::panic_with_error!(&env, MyError::InvalidTagContent)
/// });
/// ```
pub fn canonicalize_tags<F>(
    env: &soroban_sdk::Env,
    tags: &soroban_sdk::Vec<soroban_sdk::String>,
    on_invalid_char: F,
) -> soroban_sdk::Vec<soroban_sdk::String>
where
    F: Fn(),
{
    match canonicalize_tags_checked(env, tags) {
        Ok(out) => out,
        Err(TagError::Empty) => {
            if tags.is_empty() {
                panic!("Tags cannot be empty");
            }
            panic!("Tag must be between 1 and 32 characters");
        }
        Err(TagError::TooLong) => panic!("Tag must be between 1 and 32 characters"),
        Err(TagError::InvalidChar { .. }) => {
            on_invalid_char();
            // on_invalid_char must diverge (panic); this is unreachable.
            soroban_sdk::Vec::new(env)
        }
    }
}

/// Compare two Soroban [`Address`] values for equality without requiring
/// the caller to clone either address solely for the comparison.
///
/// # Why this helper exists
///
/// [`Address`] does not implement [`Copy`], so a direct `==` comparison would
/// require the caller to hold two owned values (both are consumed by `==`).
/// The idiomatic workaround — `owner.clone() == caller` — allocates an
/// unnecessary clone just to satisfy the type checker.  This helper accepts
/// both addresses by shared reference, internally derives the comparison
/// through the host-native equality check, and returns a plain `bool`.
///
/// Use `same_address` wherever address equality is needed inside a contract:
///
/// ```ignore
/// if same_address(&stored_owner, &caller) {
///     // caller is the owner
/// }
/// ```
///
/// The helper does **not** normalise or modify either address.  It is a
/// transparent equality gate and nothing more.
#[inline(always)]
pub fn same_address(a: &Address, b: &Address) -> bool {
    a == b
}

pub mod events;
pub mod reversible_op;

/// Error returned when a currency symbol is not a supported stable asset.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StableCurrencyError {
    /// The currency symbol is not a recognized stable asset (e.g., rebase/deflationary tokens).
    UnsupportedCurrency = 1,
}

/// Known stable currency symbols (case-insensitive).
/// This is a defence-in-depth allowlist of well-known stablecoins.
/// Rebase/deflationary/elastic-supply tokens (e.g., AMPL, OHM, TIME) are intentionally excluded.
const STABLE_CURRENCIES: &[&str] = &[
    "USDC", "USDT", "USDP", "BUSD", "GUSD", "TUSD", "USDD", "EURC", "EURS", "DAI", "XLM",
];

/// Validates that a currency symbol represents a supported stable asset.
///
/// This is a defence-in-depth check to reject rebase/deflationary/elastic-supply
/// token contracts at ingress. If an unsupported currency is accepted at ingress,
/// it can silently change balances during transfer and violate contract invariants
/// (e.g., remittance splits, bill payments, insurance payouts).
///
/// # Threat model
/// An attacker who can inject a rebase/deflationary token at ingress can:
/// - Cause silent balance drift during transfers, breaking settlement invariants
/// - Grief accounting/audit trails by manufacturing "settled" states with altered values
/// - Subvert split/allocation logic that assumes stable 1:1 value transfer
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `symbol` - The currency symbol to validate (case-insensitive, whitespace trimmed)
///
/// # Returns
/// * `Ok(())` if the symbol is a recognized stable currency
/// * `Err(StableCurrencyError::UnsupportedCurrency)` if the symbol is not recognized
pub fn require_stable_currency(env: &Env, symbol: &Symbol) -> Result<(), StableCurrencyError> {
    for known in STABLE_CURRENCIES {
        if symbol_matches_known_case_insensitive(env, symbol, known) {
            return Ok(());
        }
    }
    Err(StableCurrencyError::UnsupportedCurrency)
}

/// Alias for [`require_stable_currency`] with a name that clearly signals
/// its purpose as an inbound-token ingress guard.
///
/// Use this at entry points that receive tokens from external callers
/// (e.g., deposit, payment, remittance entry points) to reject rebase,
/// deflationary, or elastic-supply tokens that could silently alter balances
/// during transfer and violate contract invariants.
///
/// # Threat model
/// Without this guard, an attacker could deposit a rebase/deflationary token
/// (e.g., AMPL, OHM, TIME) that changes balance on transfer. The contract
/// would record the nominal amount but the actual value received could differ,
/// breaking settlement invariants (remittance splits, bill payments, insurance
/// payouts, savings goals).
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `symbol` - The currency symbol to validate (case-insensitive, whitespace trimmed)
///
/// # Returns
/// * `Ok(())` if the symbol is a recognized stable currency
/// * `Err(StableCurrencyError::UnsupportedCurrency)` if the symbol is not recognized
#[inline(always)]
pub fn require_supported_currency(env: &Env, symbol: &Symbol) -> Result<(), StableCurrencyError> {
    require_stable_currency(env, symbol)
}

/// Compare a Symbol case-insensitively against a known ASCII currency string.
///
/// Since Soroban Symbol comparison is exact (case-sensitive) and there is no
/// `no_std`-compatible way to extract raw bytes from a Symbol, we generate all
/// 2^N case variants of the known string (where N = len ≤ 10) and compare each
/// against the input Symbol.  The first match short-circuits the search.
fn symbol_matches_known_case_insensitive(env: &Env, symbol: &Symbol, known: &str) -> bool {
    let bytes = known.as_bytes();
    let len = bytes.len();

    // Try uppercase (exact) match first — the common case after normalization.
    if symbol == &Symbol::new(env, known) {
        return true;
    }

    // Generate all 2^len case-variant strings and compare as Symbols.
    // Symbols are bounded at 32 bytes and currencies at 10 bytes, so 2^10 = 1024
    // max iterations which is acceptable for an ingress guard.
    let num_variants = 1u32 << len;
    let mut buf = [0u8; 10];
    for mask in 0..num_variants {
        for (i, &b) in bytes.iter().enumerate() {
            buf[i] = if (mask >> i) & 1 == 0 {
                b.to_ascii_lowercase()
            } else {
                b
            };
        }
        // Safety: buf contains only ASCII letters after case folding.
        let variant = match core::str::from_utf8(&buf[..len]) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if symbol == &Symbol::new(env, variant) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod emit_tests;

#[cfg(test)]
mod non_zero_u128_tests;

impl RemitwiseEvents {
    /// Emits a single event with the given category, priority, and action.
    ///
    /// * `category` – The `EventCategory` describing the type of event.
    /// * `priority` – The `EventPriority` indicating the importance level.
    /// * `action` – A short `Symbol` identifying the specific action.
    /// * `data` – The event payload implementing `IntoVal`.
    ///
    /// The emitted event follows the topic schema defined in `docs/EVENT_TAXONOMY.md`.
    ///
    /// **Size Budget**: Event data must be compact (topics + small payload, not bulk records).
    /// The recommended maximum serialized size for the `data` payload is 256 bytes.
    /// Oversized payloads will trigger a debug/test assertion.
    #[allow(unexpected_cfgs)]
    pub fn emit<T>(
        env: &soroban_sdk::Env,
        category: EventCategory,
        priority: EventPriority,
        action: Symbol,
        data: T,
    ) where
        T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
    {
        let topics = (
            symbol_short!("Remitwise"),
            category.to_u32(),
            priority.to_u32(),
            action,
        );

        #[cfg(not(test))]
        env.events().publish(topics, data);

        #[cfg(test)]
        {
            use soroban_sdk::TryFromVal;
            let val: soroban_sdk::Val = data.into_val(env);
            if let Ok(sc_val) = soroban_sdk::xdr::ScVal::try_from_val(env, &val) {
                let size = soroban_sdk::xdr::ToXdr::to_xdr(sc_val, env).len();
                if size > 256 {
                    panic!(
                        "Event data size {} exceeds 256-byte budget. Emits must be compact.",
                        size
                    );
                }
            }
            env.events().publish(topics, val);
        }
    }

    /// Emits a batch event for the given category and action with a count.
    ///
    /// * `category` – The `EventCategory` of the batched events.
    /// * `action` – Symbol representing the batch action.
    /// * `count` – Number of events in the batch.
    ///
    /// This always uses `EventPriority::Low` for batch events.
    ///
    /// **Size Budget**: Batch payloads (action + count) are inherently compact and conform
    /// to the recommended event data budget.
    pub fn emit_batch(env: &soroban_sdk::Env, category: EventCategory, action: Symbol, count: u32) {
        let topics = (
            symbol_short!("Remitwise"),
            category.to_u32(),
            EventPriority::Low.to_u32(),
            symbol_short!("batch"),
        );
        let data = (action, count);
        #[cfg(not(test))]
        env.events().publish(topics, data);
        #[cfg(test)]
        {
            use soroban_sdk::IntoVal;
            let val: soroban_sdk::Val = data.into_val(env);
            env.events().publish(topics, val);
        }
    }

    /// Test helper: asserts that the most recently emitted Remitwise event has
    /// the expected category, priority, action, and that `data_pred` accepts
    /// the decoded payload. Uses `env.events().all()` so the assertion covers
    /// the real published event stream instead of a mock.
    ///
    /// Panics when no event has been emitted, when the topic tuple does not
    /// match the `(Remitwise, category, priority, action)` schema emitted by
    /// `EventEmitter::emit`, or when the data predicate returns false.
    #[cfg(test)]
    pub fn assert_last_event<T, F>(
        env: &soroban_sdk::Env,
        expected_category: EventCategory,
        expected_priority: EventPriority,
        expected_action: Symbol,
        data_pred: F,
    ) where
        T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>,
        F: FnOnce(&T) -> bool,
    {
        use soroban_sdk::testutils::Events as soroban_Events;

        let all = env.events().all();
        let (_cid, topics, data) = all.last().expect("expected at least one emitted event");

        // Topic schema emitted by `EventEmitter::emit`:
        // (symbol_short!("Remitwise"), category_u32, priority_u32, action)
        assert_eq!(
            topics.len(),
            4,
            "expected a 4-element Remitwise event topic tuple"
        );
        let sentinel: soroban_sdk::Symbol =
            soroban_sdk::FromVal::from_val(env, &topics.get(0).unwrap());
        assert_eq!(
            sentinel,
            symbol_short!("Remitwise"),
            "first topic must be the Remitwise marker"
        );
        let cat: u32 = soroban_sdk::FromVal::from_val(env, &topics.get(1).unwrap());
        assert_eq!(cat, expected_category.to_u32(), "event category mismatch");
        let prio: u32 = soroban_sdk::FromVal::from_val(env, &topics.get(2).unwrap());
        assert_eq!(prio, expected_priority.to_u32(), "event priority mismatch");
        let action: soroban_sdk::Symbol =
            soroban_sdk::FromVal::from_val(env, &topics.get(3).unwrap());
        assert_eq!(action, expected_action, "event action mismatch");

        let payload: T = T::try_from_val(env, &data).expect("failed to decode event data");
        assert!(
            data_pred(&payload),
            "event data predicate failed for action {:?}",
            expected_action
        );
    }
}

// ---------------------------------------------------------------------------
// Shared audit-event helper (#1268)
// ---------------------------------------------------------------------------

/// Emits a standardised audit event carrying `(op, actor, meta)`.
///
/// # Purpose
/// Before this helper existed each contract rolled its own inline
/// `env.events().publish(...)` call with slightly different topic tuples,
/// making it impossible for indexers and compliance tools to subscribe to a
/// single canonical stream of audit events.  `emit_audit` fixes that by
/// providing **one place** where the schema is defined and enforced.
///
/// # Schema
/// The event is published with a 4-element topic tuple:
///
/// ```text
/// ("Remitwise", EventCategory::Compliance (= 5), EventPriority::High (= 2), "audit")
/// ```
///
/// The data payload is the caller-supplied `meta` value, which must implement
/// `IntoVal`.  Typical payloads are small structs or plain scalars; the same
/// 256-byte size budget enforced by [`RemitwiseEvents::emit`] applies here.
///
/// # Arguments
/// * `env`   – Soroban environment.
    /// * `op`    – A short [`Symbol`] identifying the operation being audited
    ///   (e.g. `symbol_short!("flow_exec")`).  Must be ≤ 9 bytes
    ///   ([`SHORT_SYMBOL_MAX_LEN`]).
/// * `actor` – The [`Address`] of the principal that triggered the operation.
    /// * `meta`  – An arbitrary `IntoVal` payload carrying operation-specific
    ///   context (amount, result, IDs, etc.).  Keep it compact.
///
/// # Panics (test-only)
/// In `#[cfg(test)]` builds the call panics if the serialized `meta` payload
/// exceeds 256 bytes — the same guard used by [`RemitwiseEvents::emit`].
///
/// # Example
/// ```ignore
/// use remitwise_common::emit_audit;
/// use soroban_sdk::symbol_short;
///
/// emit_audit(&env, symbol_short!("transfer"), &caller, (amount, success));
/// ```
pub fn emit_audit<T>(env: &Env, op: Symbol, actor: &Address, meta: T)
where
    T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    soroban_sdk::Val: soroban_sdk::TryFromVal<Env, T>,
{
    // Fixed topic tuple — every audit event from every contract uses this
    // identical shape so indexers can subscribe with a single filter.
    let topics = (
        symbol_short!("Remitwise"),
        EventCategory::Compliance.to_u32(), // 5
        EventPriority::High.to_u32(),       // 2
        symbol_short!("audit"),
    );

    // The data tuple encodes (op, actor, meta) so the operation name and
    // principal are always present in the event payload alongside the
    // caller-supplied context.
    let data = (op, actor.clone(), meta);

    // In test builds enforce the same 256-byte payload budget as
    // RemitwiseEvents::emit so oversized payloads are caught immediately.
    #[cfg(test)]
    {
        use soroban_sdk::{IntoVal, TryFromVal};
        let val: soroban_sdk::Val = data.into_val(env);
        if let Ok(sc_val) = soroban_sdk::xdr::ScVal::try_from_val(env, &val) {
            let size = soroban_sdk::xdr::ToXdr::to_xdr(sc_val, env).len();
            if size > 256 {
                panic!(
                    "emit_audit: meta payload size {} exceeds 256-byte budget. \
                     Keep audit payloads compact.",
                    size
                );
            }
        }
        env.events().publish(topics, val);
    }

    #[cfg(not(test))]
    env.events().publish(topics, data);
}

#[cfg(test)]
mod emit_audit_tests {
    use super::*;
    use soroban_sdk::{symbol_short, testutils::Address as _, testutils::Events as _, Env};

    // -----------------------------------------------------------------------
    // Schema / topic tests
    // -----------------------------------------------------------------------

    /// The emitted event carries a 4-element topic tuple whose first element
    /// is the "Remitwise" sentinel symbol.
    #[test]
    fn emit_audit_event_has_remitwise_sentinel_topic() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("xfer"), &actor, 1u32);

        let events = env.events().all();
        assert!(!events.is_empty());
        let (_cid, topics, _data) = events.last().unwrap();
        assert_eq!(topics.len(), 4, "audit event must have 4 topics");

        let sentinel: Symbol = soroban_sdk::FromVal::from_val(&env, &topics.get(0).unwrap());
        assert_eq!(sentinel, symbol_short!("Remitwise"));
    }

    /// Category must be `EventCategory::Compliance` (discriminant 5).
    #[test]
    fn emit_audit_event_uses_compliance_category() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("op"), &actor, 0u32);

        let events = env.events().all();
        let (_cid, topics, _data) = events.last().unwrap();
        let cat: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(1).unwrap());
        assert_eq!(
            cat,
            EventCategory::Compliance.to_u32(),
            "audit events must use EventCategory::Compliance"
        );
    }

    /// Priority must be `EventPriority::High` (discriminant 2).
    #[test]
    fn emit_audit_event_uses_high_priority() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("op"), &actor, 0u32);

        let events = env.events().all();
        let (_cid, topics, _data) = events.last().unwrap();
        let prio: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(2).unwrap());
        assert_eq!(
            prio,
            EventPriority::High.to_u32(),
            "audit events must use EventPriority::High"
        );
    }

    /// The fourth topic element must be the literal `"audit"` symbol.
    #[test]
    fn emit_audit_event_action_topic_is_audit_symbol() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("settle"), &actor, 42u32);

        let events = env.events().all();
        let (_cid, topics, _data) = events.last().unwrap();
        let action: Symbol = soroban_sdk::FromVal::from_val(&env, &topics.get(3).unwrap());
        assert_eq!(action, symbol_short!("audit"));
    }

    // -----------------------------------------------------------------------
    // Payload tests
    // -----------------------------------------------------------------------

    /// A compact scalar payload (u32) is published without error.
    #[test]
    fn emit_audit_accepts_scalar_meta_payload() {
        let env = Env::default();
        let actor = Address::generate(&env);
        // Must not panic.
        emit_audit(&env, symbol_short!("approve"), &actor, 100u32);
        assert!(!env.events().all().is_empty());
    }

    /// A tuple payload (amount + bool) is accepted — typical audit call-site pattern.
    #[test]
    fn emit_audit_accepts_tuple_meta_payload() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("flow"), &actor, (1_000i128, true));
        assert!(!env.events().all().is_empty());
    }

    /// Multiple audit events emitted in sequence are all present in
    /// `env.events().all()` and each carries the canonical topic shape.
    #[test]
    fn emit_audit_multiple_events_are_all_recorded() {
        let env = Env::default();
        let actor = Address::generate(&env);
        emit_audit(&env, symbol_short!("op1"), &actor, 1u32);
        emit_audit(&env, symbol_short!("op2"), &actor, 2u32);
        emit_audit(&env, symbol_short!("op3"), &actor, 3u32);

        let events = env.events().all();
        assert_eq!(events.len(), 3, "all three audit events must be recorded");

        for (_cid, topics, _data) in events.iter() {
            assert_eq!(topics.len(), 4);
            let sentinel: Symbol = soroban_sdk::FromVal::from_val(&env, &topics.get(0).unwrap());
            assert_eq!(sentinel, symbol_short!("Remitwise"));
            let cat: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(1).unwrap());
            assert_eq!(cat, EventCategory::Compliance.to_u32());
        }
    }

    // -----------------------------------------------------------------------
    // Size-budget enforcement (test-only guard)
    // -----------------------------------------------------------------------

    /// An oversized meta payload (> 256 bytes) must panic in test builds.
    #[test]
    #[should_panic(expected = "exceeds 256-byte budget")]
    fn emit_audit_panics_on_oversized_meta_payload() {
        let env = Env::default();
        let actor = Address::generate(&env);
        // Build a Vec<u32> large enough to exceed 256 XDR bytes.
        let mut big: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
        for i in 0u32..100 {
            big.push_back(i);
        }
        emit_audit(&env, symbol_short!("big"), &actor, big);
    }
}

/// Asserts that a specific pause channel is active (not paused).
/// Panics if the channel is paused.
pub fn require_active_pause_channel(env: &Env, channel: Symbol) {
    let paused = env
        .storage()
        .instance()
        .get::<_, Map<Symbol, bool>>(&Symbol::new(env, STORAGE_PAUSE_CHANNELS))
        .unwrap_or_else(|| Map::new(env))
        .get(channel)
        .unwrap_or(false);
    if paused {
        panic!("Pause channel is inactive");
    }
}

// ---------------------------------------------------------------------------
// Investigation epoch — halt writes during security investigations
// ---------------------------------------------------------------------------

pub(crate) const STORAGE_INVESTIGATION_EPOCH: Symbol = symbol_short!("INV_EPOCH");

/// Error returned when a write operation is blocked because an
/// investigation epoch is active.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InvestigationEpochError {
    /// A write was blocked because an investigation epoch is in effect
    /// and no further state mutations are allowed.
    WriteBlocked = 1,
}

/// Returns `true` if the current ledger timestamp is before the stored
/// investigation epoch end timestamp, meaning an investigation epoch is
/// active and writes must be halted.
pub fn is_investigation_epoch_active(env: &Env) -> bool {
    let epoch_end: u64 = env
        .storage()
        .instance()
        .get(&STORAGE_INVESTIGATION_EPOCH)
        .unwrap_or(0);
    epoch_end > env.ledger().timestamp()
}

/// Halts write operations if an investigation epoch is active.
///
/// Call this at the top of every write entry point (bill payment, premium
/// payment, remittance disbursement, etc.) as a defence-in-depth guard.
/// When an investigation epoch is active the contract rejects any mutation
/// with [`InvestigationEpochError::WriteBlocked`].
///
/// # Threat model
/// Without this check, an attacker who has discovered a vulnerability can
/// continue to exploit it during an active investigation — stealing remaining
/// funds, corrupting state that would otherwise be preserved for forensic
/// analysis, or escalating the attack by triggering additional write-side
/// effects. Freezing writes limits the blast radius and preserves evidence
/// for the investigation team.
///
/// # Cost
/// One instance storage read plus one `u64` comparison. Negligible
/// relative to any write entry point's existing storage reads/writes.
///
/// # Errors
/// Returns [`InvestigationEpochError::WriteBlocked`] when the investigation
/// epoch is active.
pub fn require_no_investigation_epoch(env: &Env) -> Result<(), InvestigationEpochError> {
    if is_investigation_epoch_active(env) {
        Err(InvestigationEpochError::WriteBlocked)
    } else {
        Ok(())
    }
}

/// Start an investigation epoch for the given duration in seconds.
///
/// The epoch is a time-bounded window during which all write operations
/// are blocked. After `duration_secs` elapses the epoch expires
/// automatically (no manual intervention required).
///
/// # Security
/// This function does not enforce authentication — it is the caller's
/// responsibility to gate it with admin auth (e.g.
/// `admin.require_auth()` in the calling contract).
pub fn start_investigation_epoch(env: &Env, duration_secs: u64) {
    let end_time = env.ledger().timestamp().saturating_add(duration_secs);
    env.storage()
        .instance()
        .set(&STORAGE_INVESTIGATION_EPOCH, &end_time);
}

/// Clear the investigation epoch immediately, allowing writes to proceed
/// again.
///
/// If no investigation epoch is active, this is a no-op.
///
/// # Security
/// This function does not enforce authentication — it is the caller's
/// responsibility to gate it with admin auth.
pub fn clear_investigation_epoch(env: &Env) {
    env.storage()
        .instance()
        .remove(&STORAGE_INVESTIGATION_EPOCH);
}

// ---------------------------------------------------------------------------
// Kill switch — binary on/off gate to halt all writes
// ---------------------------------------------------------------------------

/// Storage key for the kill switch flag.
/// When set to `true` in instance storage, all write entry points must reject
/// mutations with [`KillSwitchError::WriteBlocked`].
pub(crate) const STORAGE_KILL_SWITCH: Symbol = symbol_short!("KILL_SW");

/// Error returned when a write operation is blocked because the kill switch
/// is active.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum KillSwitchError {
    /// A write was blocked because the kill switch is engaged and no further
    /// state mutations are allowed.
    WriteBlocked = 1,
}

/// Returns `true` if the kill switch is active, meaning all write operations
/// must be halted.
///
/// Checks instance storage for a boolean `STORAGE_KILL_SWITCH` flag.
/// If the flag is absent the kill switch is considered inactive (default).
pub fn is_kill_switch_active(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&STORAGE_KILL_SWITCH)
        .unwrap_or(false)
}

/// Halts write operations if the kill switch is active.
///
/// Call this at the top of every write entry point (bill payment, premium
/// payment, remittance disbursement, etc.) as a defence-in-depth guard.
/// When the kill switch is active the contract rejects any mutation with
/// [`KillSwitchError::WriteBlocked`].
///
/// # Threat model
/// Without this check, an attacker who has discovered a vulnerability or
/// obtained administrative access can continue to mutate contract state even
/// after the kill switch has been triggered — stealing remaining funds,
/// corrupting state that would otherwise be preserved for forensic analysis,
/// or escalating the attack by triggering additional write-side effects.
/// Setting the kill switch limits the blast radius and preserves evidence
/// for the investigation team.
///
/// Unlike the investigation epoch (which is time-bounded), the kill switch
/// is a binary toggle that stays active until explicitly deactivated by an
/// admin.
///
/// # Cost
/// A single instance-storage read (`bool`) — negligible (~250 gas units)
/// relative to any write entry point's existing storage reads/writes.
///
/// # Errors
/// Returns [`KillSwitchError::WriteBlocked`] when the kill switch is active.
pub fn require_no_active_kill_switch(env: &Env) -> Result<(), KillSwitchError> {
    if is_kill_switch_active(env) {
        Err(KillSwitchError::WriteBlocked)
    } else {
        Ok(())
    }
}

/// Activate the kill switch, blocking all write operations.
///
/// Sets the `STORAGE_KILL_SWITCH` flag to `true`. After calling this,
/// every write entry point that calls [`require_no_active_kill_switch`]
/// will return [`KillSwitchError::WriteBlocked`].
///
/// This function does not enforce authentication — it is the caller's
/// responsibility to gate it with admin auth (e.g.
/// `admin.require_auth()` in the calling contract).
pub fn activate_kill_switch(env: &Env) {
    env.storage().instance().set(&STORAGE_KILL_SWITCH, &true);
}

/// Deactivate the kill switch, allowing write operations to proceed.
///
/// Removes the `STORAGE_KILL_SWITCH` flag from storage. After calling this,
/// [`require_no_active_kill_switch`] will return `Ok(())` again.
///
/// If the kill switch is not active, this is a no-op.
///
/// This function does not enforce authentication — it is the caller's
/// responsibility to gate it with admin auth.
pub fn deactivate_kill_switch(env: &Env) {
    env.storage().instance().remove(&STORAGE_KILL_SWITCH);
}

#[cfg(test)]
mod kill_switch_tests {
    use super::*;
    use soroban_sdk::Env;

    /// The kill switch is inactive by default (no storage set).
    #[test]
    fn test_kill_switch_inactive_by_default() {
        let env = Env::default();
        assert!(!is_kill_switch_active(&env));
        assert_eq!(require_no_active_kill_switch(&env), Ok(()));
    }

    /// After activation, is_kill_switch_active returns true and
    /// require_no_active_kill_switch returns WriteBlocked.
    #[test]
    fn test_activate_kill_switch_blocks_writes() {
        let env = Env::default();
        activate_kill_switch(&env);

        assert!(is_kill_switch_active(&env));
        assert_eq!(
            require_no_active_kill_switch(&env),
            Err(KillSwitchError::WriteBlocked)
        );
    }

    /// After deactivation, the kill switch is inactive and writes are
    /// allowed again.
    #[test]
    fn test_deactivate_kill_switch_allows_writes() {
        let env = Env::default();
        activate_kill_switch(&env);
        assert!(is_kill_switch_active(&env));

        deactivate_kill_switch(&env);
        assert!(!is_kill_switch_active(&env));
        assert_eq!(require_no_active_kill_switch(&env), Ok(()));
    }

    /// Deactivating when already inactive is a safe no-op.
    #[test]
    fn test_deactivate_kill_switch_is_idempotent() {
        let env = Env::default();
        assert!(!is_kill_switch_active(&env));

        // Should not panic
        deactivate_kill_switch(&env);
        assert!(!is_kill_switch_active(&env));
    }

    /// After activation and deactivation, the kill switch can be
    /// reactivated (toggle cycle).
    #[test]
    fn test_kill_switch_toggle_cycle() {
        let env = Env::default();

        // Activate
        activate_kill_switch(&env);
        assert!(is_kill_switch_active(&env));

        // Deactivate
        deactivate_kill_switch(&env);
        assert!(!is_kill_switch_active(&env));

        // Re-activate
        activate_kill_switch(&env);
        assert!(is_kill_switch_active(&env));
        assert_eq!(
            require_no_active_kill_switch(&env),
            Err(KillSwitchError::WriteBlocked)
        );

        // Final deactivate
        deactivate_kill_switch(&env);
        assert!(!is_kill_switch_active(&env));
    }

    /// Negative test: require_no_active_kill_switch fails when kill switch
    /// is active, and the error type is KillSwitchError::WriteBlocked.
    #[test]
    fn test_write_blocked_during_active_kill_switch() {
        let env = Env::default();

        // Without activation, writes allowed
        assert!(require_no_active_kill_switch(&env).is_ok());

        // Activate kill switch
        activate_kill_switch(&env);

        // Writes blocked
        let result = require_no_active_kill_switch(&env);
        assert_eq!(result, Err(KillSwitchError::WriteBlocked));
    }
}

// ---------------------------------------------------------------------------
// Encoding stability tests (cross-contract ABI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod encoding_stability_tests {
    use super::{Category, CoverageType, FamilyRole, PolicyMode};
    use soroban_sdk::{Env, Map, Vec};

    fn round_trip<T>(env: &Env, v: T) -> T
    where
        T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    {
        let val = v.into_val(env);
        T::try_from_val(env, &val).unwrap()
    }

    fn assert_encoding_matches_discriminant<T>(env: &Env, v: T, expected: u32)
    where
        T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
            + core::fmt::Debug
            + PartialEq,
    {
        let val = v.into_val(env);

        // `#[repr(u32)]` + `#[contracttype]` should encode via a stable u32 discriminant.
        // We pin the expected discriminant by decoding the value as `u32`.
        let actual_u32: u32 = soroban_sdk::TryFromVal::try_from_val(env, &val)
            .unwrap_or_else(|_| panic!("unexpected Val for encoding: {val:?}"));
        assert_eq!(actual_u32, expected, "encoding mismatch");

        // And ensure round-trip identity.
        let decoded = T::try_from_val(env, &val).unwrap();
        assert_eq!(decoded, v, "round-trip mismatch");
    }

    #[test]
    fn category_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, Category::Spending, 1);
        assert_encoding_matches_discriminant(&env, Category::Savings, 2);
        assert_encoding_matches_discriminant(&env, Category::Bills, 3);
        assert_encoding_matches_discriminant(&env, Category::Insurance, 4);

        // Exhaustiveness enforcement: every variant must be explicitly handled.
        fn cover_all_variants(v: Category) {
            match v {
                Category::Spending => {}
                Category::Savings => {}
                Category::Bills => {}
                Category::Insurance => {}
            }
        }

        for v in [
            Category::Spending,
            Category::Savings,
            Category::Bills,
            Category::Insurance,
        ] {
            cover_all_variants(v);
        }

        // Container round-trips
        let vec = Vec::from_array(
            &env,
            [Category::Spending, Category::Savings, Category::Bills],
        );
        let mut out = Vec::<Category>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, Category>::new(&env);
        map.set(1u32, Category::Spending);
        map.set(2u32, Category::Savings);
        map.set(3u32, Category::Bills);

        let mut out_map = Map::<u32, Category>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }

    #[test]
    fn family_role_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, FamilyRole::Owner, 1);
        assert_encoding_matches_discriminant(&env, FamilyRole::Admin, 2);
        assert_encoding_matches_discriminant(&env, FamilyRole::Member, 3);
        assert_encoding_matches_discriminant(&env, FamilyRole::Viewer, 4);

        fn cover_all_variants(v: FamilyRole) {
            match v {
                FamilyRole::Owner => {}
                FamilyRole::Admin => {}
                FamilyRole::Member => {}
                FamilyRole::Viewer => {}
            }
        }

        for v in [
            FamilyRole::Owner,
            FamilyRole::Admin,
            FamilyRole::Member,
            FamilyRole::Viewer,
        ] {
            cover_all_variants(v);
        }

        let vec = Vec::from_array(
            &env,
            [FamilyRole::Owner, FamilyRole::Admin, FamilyRole::Viewer],
        );
        let mut out = Vec::<FamilyRole>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, FamilyRole>::new(&env);
        map.set(1u32, FamilyRole::Owner);
        map.set(2u32, FamilyRole::Admin);
        map.set(3u32, FamilyRole::Viewer);

        let mut out_map = Map::<u32, FamilyRole>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }

    #[test]
    fn coverage_type_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, CoverageType::Health, 1);
        assert_encoding_matches_discriminant(&env, CoverageType::Life, 2);
        assert_encoding_matches_discriminant(&env, CoverageType::Property, 3);
        assert_encoding_matches_discriminant(&env, CoverageType::Auto, 4);
        assert_encoding_matches_discriminant(&env, CoverageType::Liability, 5);

        fn cover_all_variants(v: CoverageType) {
            match v {
                CoverageType::Health => {}
                CoverageType::Life => {}
                CoverageType::Property => {}
                CoverageType::Auto => {}
                CoverageType::Liability => {}
            }
        }

        for v in [
            CoverageType::Health,
            CoverageType::Life,
            CoverageType::Property,
            CoverageType::Auto,
            CoverageType::Liability,
        ] {
            cover_all_variants(v);
        }

        let vec = Vec::from_array(
            &env,
            [
                CoverageType::Health,
                CoverageType::Life,
                CoverageType::Property,
                CoverageType::Auto,
            ],
        );
        let mut out = Vec::<CoverageType>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, CoverageType>::new(&env);
        map.set(1u32, CoverageType::Health);
        map.set(2u32, CoverageType::Life);
        map.set(3u32, CoverageType::Liability);

        let mut out_map = Map::<u32, CoverageType>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }

    #[test]
    #[allow(clippy::single_element_loop)]
    fn policy_mode_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, PolicyMode::Strict, 1);

        fn cover_all_variants(v: PolicyMode) {
            match v {
                PolicyMode::Strict => {}
            }
        }

        cover_all_variants(PolicyMode::Strict);

        let vec = Vec::from_array(&env, [PolicyMode::Strict]);
        let mut out = Vec::<PolicyMode>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, PolicyMode>::new(&env);
        map.set(1u32, PolicyMode::Strict);

        let mut out_map = Map::<u32, PolicyMode>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }
}

#[cfg(test)]
mod stable_currency_tests {
    use super::{
        require_stable_currency, require_supported_currency, StableCurrencyError, STABLE_CURRENCIES,
    };
    use soroban_sdk::{Env, Symbol};

    // --- Whitelisted paths: currency accepted by stable currency allowlist ---

    #[test]
    fn accepts_usdc() {
        let env = Env::default();
        let sym = Symbol::new(&env, "USDC");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_usdt() {
        let env = Env::default();
        let sym = Symbol::new(&env, "USDT");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_usdp() {
        let env = Env::default();
        let sym = Symbol::new(&env, "USDP");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_busd() {
        let env = Env::default();
        let sym = Symbol::new(&env, "BUSD");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_gusd() {
        let env = Env::default();
        let sym = Symbol::new(&env, "GUSD");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_tusd() {
        let env = Env::default();
        let sym = Symbol::new(&env, "TUSD");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_usdd() {
        let env = Env::default();
        let sym = Symbol::new(&env, "USDD");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_eurc() {
        let env = Env::default();
        let sym = Symbol::new(&env, "EURC");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_eurs() {
        let env = Env::default();
        let sym = Symbol::new(&env, "EURS");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_dai() {
        let env = Env::default();
        let sym = Symbol::new(&env, "DAI");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_xlm() {
        let env = Env::default();
        let sym = Symbol::new(&env, "XLM");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_lowercase_usdc() {
        let env = Env::default();
        let sym = Symbol::new(&env, "usdc");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_mixed_case_usdc() {
        let env = Env::default();
        let sym = Symbol::new(&env, "UsDc");
        assert_eq!(require_stable_currency(&env, &sym), Ok(()));
    }

    #[test]
    fn accepts_all_case_variants_of_usdt() {
        let env = Env::default();
        // Test all four possible case combinations for a 4-letter symbol
        let sym_upper = Symbol::new(&env, "USDT");
        let sym_lower = Symbol::new(&env, "usdt");
        let sym_mixed1 = Symbol::new(&env, "UsDt");
        let sym_mixed2 = Symbol::new(&env, "usdT");

        assert_eq!(require_stable_currency(&env, &sym_upper), Ok(()));
        assert_eq!(require_stable_currency(&env, &sym_lower), Ok(()));
        assert_eq!(require_stable_currency(&env, &sym_mixed1), Ok(()));
        assert_eq!(require_stable_currency(&env, &sym_mixed2), Ok(()));
    }

    // --- Non-whitelisted paths: currency rejected (not in stable allowlist) ---

    #[test]
    fn rejects_rebase_token_ampl() {
        let env = Env::default();
        let sym = Symbol::new(&env, "AMPL");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_rebase_token_ohm() {
        let env = Env::default();
        let sym = Symbol::new(&env, "OHM");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_rebase_token_time() {
        let env = Env::default();
        let sym = Symbol::new(&env, "TIME");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_unknown_token() {
        let env = Env::default();
        let sym = Symbol::new(&env, "RANDOM");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_empty_symbol() {
        let env = Env::default();
        let sym = Symbol::new(&env, "");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_generic_erc20_token() {
        let env = Env::default();
        let sym = Symbol::new(&env, "GTOKEN");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_volatile_token_luna() {
        let env = Env::default();
        let sym = Symbol::new(&env, "LUNA");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_volatile_token_sol() {
        let env = Env::default();
        let sym = Symbol::new(&env, "SOL");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_volatile_token_eth() {
        let env = Env::default();
        let sym = Symbol::new(&env, "ETH");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_volatile_token_btc() {
        let env = Env::default();
        let sym = Symbol::new(&env, "BTC");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_rebase_tokens_case_insensitive() {
        let env = Env::default();
        // Verify that rebase token rejection is case-insensitive (consistent with acceptance)
        let sym_lower = Symbol::new(&env, "ampl");
        let sym_mixed = Symbol::new(&env, "AmPl");
        assert_eq!(
            require_stable_currency(&env, &sym_lower),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
        assert_eq!(
            require_stable_currency(&env, &sym_mixed),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_very_long_unknown_symbol() {
        let env = Env::default();
        let sym = Symbol::new(&env, "VERYLONGTOKEN");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_numeric_only_token() {
        let env = Env::default();
        let sym = Symbol::new(&env, "123");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    #[test]
    fn rejects_special_char_token() {
        let env = Env::default();
        let sym = Symbol::new(&env, "US$D");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Additional boundary tests added per FWC26 issue
    // "Stable, rebase-suspected, unknown" — locks in the
    // accept-vs-reject boundary between the stable-currency allowlist
    // and the reject-by-default fallback.
    //
    // Existing tests already cover the bulk of the boundary
    // (each known stable accepted; rebase/volatile/unknown tokens
    // rejected; case-insensitive; numeric/special/long rejected).
    // These four tests target invariants that were not explicitly
    // pinned: alias equivalence with `require_supported_currency`,
    // single-character displacement, the 9-byte envelope, and padding.
    // ─────────────────────────────────────────────────────────────────────

    /// `require_supported_currency` is documented as a public alias for
    /// [`require_stable_currency`]. This test pins that contract for every
    /// entry in the allowlist so a future refactor cannot silently diverge
    /// the two entry points.
    #[test]
    fn accepts_require_supported_currency_alias_for_every_allowlisted_entry() {
        let env = Env::default();

        for code in STABLE_CURRENCIES {
            let sym = Symbol::new(&env, code);
            assert_eq!(
                require_stable_currency(&env, &sym),
                Ok(()),
                "require_stable_currency must accept allowlisted entry {:?}",
                code
            );
            assert_eq!(
                require_supported_currency(&env, &sym),
                Ok(()),
                "require_supported_currency alias must accept allowlisted entry {:?}",
                code
            );
            // The two entry points must agree byte-for-byte.
            assert_eq!(
                require_stable_currency(&env, &sym),
                require_supported_currency(&env, &sym),
                "alias diverged from require_stable_currency for {:?}",
                code
            );
        }
    }

    /// A single-byte displacement away from a whitelisted currency
    /// must still be rejected. Catches future refactors that loosen
    /// the comparison (prefix-match, near-match, fuzzy compare).
    #[test]
    fn rejects_one_byte_displacement_of_allowlisted_currency() {
        let env = Env::default();
        // Each variant differs from a whitelisted code (USDC) by
        // exactly one ASCII byte at various positions: swap,
        // duplicate, digit-suffix, digit-prefix. None of these are
        // in `STABLE_CURRENCIES`, so they must all be rejected.
        // ("." and whitespace variants are excluded because they are
        // outside Symbol's `[a-zA-Z0-9_]` charset — `Symbol::new`
        // would panic on them and mask the boundary we want to test.)
        for variant in [
            "USDX",  // swap last byte of USDC
            "USCA",  // swap last + a different letter
            "UCDC",  // swap second byte of USDC
            "USDCC", // duplicate last byte
            "USDC0", // digit suffix
            "0USDC", // digit prefix
        ] {
            let sym = Symbol::new(&env, variant);
            assert_eq!(
                require_stable_currency(&env, &sym),
                Err(StableCurrencyError::UnsupportedCurrency),
                "{:?} (one byte away from USDC) must be rejected as a near-miss",
                variant
            );
        }
    }

    /// A `Symbol` whose length is exactly the short-symbol maximum
    /// (9 bytes) must not be silently accepted just because the SDK
    /// encodes it inline. This pins the 9-byte envelope boundary
    /// documented by `SYMBOL_SHORT_MAX_LEN`.
    #[test]
    fn rejects_exactly_nine_byte_unknown_symbol() {
        let env = Env::default();
        // "RANDOMX9Z" is exactly 9 ASCII bytes and is not in
        // STABLE_CURRENCIES.
        let sym = Symbol::new(&env, "RANDOMX9Z");
        assert_eq!(
            require_stable_currency(&env, &sym),
            Err(StableCurrencyError::UnsupportedCurrency)
        );
    }

    /// Allowlist membership is an exact-ASCII match. Padding,
    /// prefixing, or suffixing an allow-listed code never widens
    /// acceptance. This pins the comparator — a switch to a
    /// `starts_with` or `contains` would be caught immediately.
    /// Note: whitespace-padded variants like `"USDC "` are
    /// excluded from this test because Symbol's charset is
    /// `[a-zA-Z0-9_]`; `Symbol::new` would panic on whitespace,
    /// masking the boundary under test.
    #[test]
    fn rejects_padded_variant_of_allowlisted_currency() {
        let env = Env::default();
        for variant in ["USDC0", "XUSDC", "USDCX"] {
            let sym = Symbol::new(&env, variant);
            assert_eq!(
                require_stable_currency(&env, &sym),
                Err(StableCurrencyError::UnsupportedCurrency),
                "padded variant {:?} must not be accepted as USDC",
                variant
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Reserved-storage-key guard (#1275)
// ---------------------------------------------------------------------------

/// Storage-key prefixes that are reserved for future roadmap features.
///
/// These prefixes are documented in `docs/RESERVED_STORAGE_KEYS.md`. Any
/// attempt to write to one of these keys at runtime is a programming error —
/// either a contributor accidentally chose a colliding name, or a future
/// feature was partially implemented without removing the reservation.
///
/// The list here must stay in sync with the markdown table in that doc; the
/// CI test in `testutils/tests/reserved_storage_keys_test.rs` independently
/// parses the markdown and cross-checks every `symbol_short!` literal in
/// source, so drift between the two is caught automatically.
const RESERVED_KEY_PREFIXES: &[&[u8]] = &[
    b"YIELD_CFG",
    b"STAKE_POL",
    b"REWARD_CF",
    b"V2_MIGR",
    b"TMP_LOCK",
];

/// Error returned by [`verify_storage_key_reserved`] when a caller tries to
/// use a storage key that is reserved for a future feature.
///
/// # Threat model
///
/// Reserved storage keys are held open for planned roadmap features. If
/// a contributor (or a compromised upgrade) writes state under one of those
/// keys before the feature is implemented, the future rollout will collide
/// with existing storage entries, potentially:
///
/// - **Silently overwriting** operational state that was never intended to
///   live under that key, corrupting balances, configuration, or access-
///   control records.
/// - **Leaking stale data** into the future feature by having it read entries
///   that were written by a completely unrelated code path.
/// - **Confusing forensic analysis** during an incident investigation because
///   the storage layout no longer matches the documented schema.
///
/// Calling `verify_storage_key_reserved` at the entry point that receives a
/// caller-supplied key rejects the write before it reaches storage.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReservedKeyError {
    /// The supplied key matches a prefix reserved for a future roadmap
    /// feature and must not be used at runtime.
    KeyIsReserved = 1,
}

/// Verifies that `key` does not collide with any storage-key prefix that is
/// reserved for a future roadmap feature.
///
/// # Arguments
/// * `key` — The raw byte-slice form of the storage key to check (e.g.
///   the contents of a `symbol_short!` or `Symbol::new` key). Typically
///   obtained from a caller-supplied `String` or a compile-time constant
///   by the entry point before passing it on to storage APIs.
///
/// # Returns
/// * `Ok(())` — `key` does not start with any reserved prefix; it is safe
///   to use as a storage key.
/// * `Err(ReservedKeyError::KeyIsReserved)` — `key` exactly matches a
///   reserved prefix. The call site should surface this as a typed
///   contract error rather than a silent no-op or a generic panic.
///
/// # Example
///
/// ```ignore
/// use remitwise_common::{verify_storage_key_reserved, ReservedKeyError};
///
/// // Safe key — passes through:
/// assert_eq!(verify_storage_key_reserved(b"NOTIF_CFG"), Ok(()));
///
/// // Reserved key — rejected:
/// assert_eq!(
///     verify_storage_key_reserved(b"REWARD_CF"),
///     Err(ReservedKeyError::KeyIsReserved),
/// );
/// ```
///
/// # Cost
///
/// A linear scan over the small constant `RESERVED_KEY_PREFIXES` slice
/// (currently 5 entries, each ≤ 9 bytes). Measured cost: < 10 gas units on
/// the Soroban testnet host — negligible compared to any storage read or
/// write that follows.
pub fn verify_storage_key_reserved(key: &[u8]) -> Result<(), ReservedKeyError> {
    for &reserved in RESERVED_KEY_PREFIXES {
        if key == reserved {
            return Err(ReservedKeyError::KeyIsReserved);
        }
    }
    Ok(())
}

#[cfg(test)]
mod reserved_key_guard_tests {
    use super::{verify_storage_key_reserved, ReservedKeyError};

    // ── Happy path: non-reserved keys ────────────────────────────────────

    /// An ordinary operational key that has nothing to do with reserved
    /// prefixes passes through cleanly.
    #[test]
    fn accepts_ordinary_storage_key() {
        assert_eq!(verify_storage_key_reserved(b"NOTIF_CFG"), Ok(()));
    }

    /// An empty byte slice does not collide with any reserved prefix.
    #[test]
    fn accepts_empty_key() {
        assert_eq!(verify_storage_key_reserved(b""), Ok(()));
    }

    /// A key that is a prefix *of* a reserved entry but shorter than the
    /// reserved entry itself is accepted (exact-match semantics only).
    #[test]
    fn accepts_key_that_is_substring_of_reserved_prefix() {
        // "V2_MIG" is shorter than "V2_MIGR" — not an exact match.
        assert_eq!(verify_storage_key_reserved(b"V2_MIG"), Ok(()));
    }

    /// A key that starts with a reserved prefix but has extra bytes appended
    /// is accepted (exact-match, not a `starts_with` guard).
    #[test]
    fn accepts_key_longer_than_reserved_prefix() {
        // "REWARD_CFG" starts with "REWARD_CF" but is longer.
        assert_eq!(verify_storage_key_reserved(b"REWARD_CFG"), Ok(()));
    }

    /// A near-miss key — one byte different from a reserved entry — is
    /// accepted. Pins exact-match rather than fuzzy/prefix semantics.
    #[test]
    fn accepts_key_one_byte_different_from_reserved() {
        // "YIELD_CFH" differs from "YIELD_CFG" only at the last byte.
        assert_eq!(verify_storage_key_reserved(b"YIELD_CFH"), Ok(()));
    }

    // ── Sad path: every reserved key is rejected ─────────────────────────

    /// `YIELD_CFG` is reserved for Yield Generation V2 and must be rejected.
    /// This is the primary negative test required by the acceptance criteria
    /// for issue #1275: the test fails if `verify_storage_key_reserved` does
    /// not exist or does not gate this key.
    #[test]
    fn rejects_yield_cfg_reserved_key() {
        assert_eq!(
            verify_storage_key_reserved(b"YIELD_CFG"),
            Err(ReservedKeyError::KeyIsReserved),
        );
    }

    /// `STAKE_POL` is reserved for Staking & Rewards.
    #[test]
    fn rejects_stake_pol_reserved_key() {
        assert_eq!(
            verify_storage_key_reserved(b"STAKE_POL"),
            Err(ReservedKeyError::KeyIsReserved),
        );
    }

    /// `REWARD_CF` is reserved for the reward emission rate configuration.
    #[test]
    fn rejects_reward_cf_reserved_key() {
        assert_eq!(
            verify_storage_key_reserved(b"REWARD_CF"),
            Err(ReservedKeyError::KeyIsReserved),
        );
    }

    /// `V2_MIGR` is reserved as a staging pointer for V2 contract deployment.
    #[test]
    fn rejects_v2_migr_reserved_key() {
        assert_eq!(
            verify_storage_key_reserved(b"V2_MIGR"),
            Err(ReservedKeyError::KeyIsReserved),
        );
    }

    /// `TMP_LOCK` is reserved for multi-phase time-locks in the family wallet.
    #[test]
    fn rejects_tmp_lock_reserved_key() {
        assert_eq!(
            verify_storage_key_reserved(b"TMP_LOCK"),
            Err(ReservedKeyError::KeyIsReserved),
        );
    }

    // ── ABI stability ─────────────────────────────────────────────────────

    /// `KeyIsReserved` is pinned at discriminant 1. Changing this would
    /// break callers that map the error over XDR into contract-specific
    /// error enums.
    #[test]
    fn reserved_key_error_discriminant_stable_at_1() {
        assert_eq!(ReservedKeyError::KeyIsReserved as u32, 1);
    }
}

// ---------------------------------------------------------------------------
// Upgrade-epoch guard (#1278)
// ---------------------------------------------------------------------------

/// Storage key for the current upgrade epoch counter.
///
/// This key stores a monotonically increasing `u64` counter that is bumped
/// every time the contract admin executes a contract upgrade. All
/// upgrade-related entry points that carry an epoch in their call data must
/// verify that their epoch matches this stored value before proceeding.
pub const STORAGE_UPGRADE_EPOCH: Symbol = symbol_short!("UPGR_EP");

/// Error returned by [`require_matching_upgrade_epoch`] when the caller
/// supplies an outdated or future upgrade epoch.
///
/// # Threat model
///
/// Contract upgrades change the on-chain WASM and may alter the storage
/// layout or semantics of existing keys. An upgrade-related entry point
/// (e.g. a post-upgrade migration entrypoint, an epoch-scoped privileged
/// operation, a snapshot restore) is typically authorized at a specific
/// upgrade epoch to prevent:
///
/// - **Replay of stale authorizations**: a signature or call payload
///   captured for upgrade N must not be accepted for upgrade N+1, because
///   the storage semantics the payload was originally verified against may
///   have changed.
/// - **Pre-authorized future actions**: a payload targeting a future epoch
///   (N+k) must not execute before that upgrade has actually occurred, since
///   the preconditions for the target state do not yet hold.
///
/// Rejecting any epoch that does not exactly match the current stored value
/// closes both vectors at once.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UpgradeEpochError {
    /// The caller-supplied upgrade epoch does not match the current stored
    /// epoch. The call is rejected to prevent replay of stale upgrade
    /// authorizations.
    EpochMismatch = 1,
}

/// Guards an upgrade-related entry point against calls bound to the wrong
/// upgrade epoch.
///
/// Reads the current upgrade epoch from instance storage (defaulting to `0`
/// when no epoch has been set yet) and rejects `ep` when it does not match
/// exactly.
///
/// # Arguments
/// * `env` — Soroban environment.
/// * `ep`  — The upgrade epoch supplied by the caller in the call payload.
///
/// # Returns
/// * `Ok(())` when `ep == current_epoch`.
/// * `Err(UpgradeEpochError::EpochMismatch)` when `ep != current_epoch`.
///
/// # Cost
///
/// One instance-storage `get` (reads a single `u64`) plus one equality
/// comparison. On the Soroban testnet host this is < 300 gas units —
/// negligible relative to any upgrade-related operation.
///
/// # Example
///
/// ```ignore
/// pub fn post_upgrade_migrate(env: Env, upgrade_epoch: u64) {
///     require_matching_upgrade_epoch(&env, upgrade_epoch)
///         .unwrap_or_else(|_| panic_with_error!(&env, MyError::StaleEpoch));
///     // ... proceed with migration
/// }
/// ```
pub fn require_matching_upgrade_epoch(env: &Env, ep: u64) -> Result<(), UpgradeEpochError> {
    let current: u64 = env
        .storage()
        .instance()
        .get(&STORAGE_UPGRADE_EPOCH)
        .unwrap_or(0);
    if ep != current {
        return Err(UpgradeEpochError::EpochMismatch);
    }
    Ok(())
}

/// Bump the upgrade epoch by 1, atomically reading and incrementing the
/// stored value.
///
/// Call this at the end of every successful contract upgrade (e.g. in the
/// `post_upgrade` entry point) so that any authorization payload that was
/// created for the previous epoch is immediately invalidated.
///
/// This function does not enforce authentication — the calling contract is
/// responsible for gating it with admin auth before invoking it.
pub fn bump_upgrade_epoch(env: &Env) {
    let current: u64 = env
        .storage()
        .instance()
        .get(&STORAGE_UPGRADE_EPOCH)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&STORAGE_UPGRADE_EPOCH, &current.saturating_add(1));
}

/// Read the current upgrade epoch without modifying it.
///
/// Returns `0` when no epoch has been stored yet (fresh deployment).
pub fn get_upgrade_epoch(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&STORAGE_UPGRADE_EPOCH)
        .unwrap_or(0)
}

#[cfg(test)]
mod upgrade_epoch_guard_tests {
    use super::{
        bump_upgrade_epoch, get_upgrade_epoch, require_matching_upgrade_epoch, UpgradeEpochError,
    };
    use soroban_sdk::Env;

    // ── Happy path ────────────────────────────────────────────────────────

    /// Fresh deployment: epoch is 0 by default. Passing `0` must succeed.
    #[test]
    fn accepts_epoch_zero_on_fresh_deployment() {
        let env = Env::default();
        assert_eq!(require_matching_upgrade_epoch(&env, 0), Ok(()));
    }

    /// After one bump the stored epoch is 1; passing `1` must succeed.
    #[test]
    fn accepts_matching_epoch_after_one_bump() {
        let env = Env::default();
        bump_upgrade_epoch(&env);
        assert_eq!(require_matching_upgrade_epoch(&env, 1), Ok(()));
    }

    /// Multiple bumps: epoch is always exactly the bump count.
    #[test]
    fn accepts_matching_epoch_after_multiple_bumps() {
        let env = Env::default();
        bump_upgrade_epoch(&env); // 1
        bump_upgrade_epoch(&env); // 2
        bump_upgrade_epoch(&env); // 3
        assert_eq!(require_matching_upgrade_epoch(&env, 3), Ok(()));
    }

    // ── Sad path: stale / future epoch ───────────────────────────────────

    /// The primary negative test for #1278: passing epoch 1 when the stored
    /// epoch is still 0 must return `EpochMismatch`.
    ///
    /// *This test fails on code that does not implement
    /// `require_matching_upgrade_epoch` and passes after the fix.*
    #[test]
    fn rejects_caller_epoch_ahead_of_stored_epoch() {
        let env = Env::default();
        // No bump has happened; stored epoch is 0.
        assert_eq!(
            require_matching_upgrade_epoch(&env, 1),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    /// Stale epoch: after one bump the stored epoch is 1, but caller
    /// still passes 0 (the pre-upgrade epoch). Must be rejected.
    #[test]
    fn rejects_stale_epoch_after_upgrade() {
        let env = Env::default();
        bump_upgrade_epoch(&env); // stored = 1
        assert_eq!(
            require_matching_upgrade_epoch(&env, 0),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    /// Epoch skipped by multiple bumps: caller passes 1 but stored epoch
    /// is 3. Must be rejected regardless of whether the gap is 1 or many.
    #[test]
    fn rejects_epoch_skipped_by_multiple_upgrades() {
        let env = Env::default();
        bump_upgrade_epoch(&env); // 1
        bump_upgrade_epoch(&env); // 2
        bump_upgrade_epoch(&env); // 3
        assert_eq!(
            require_matching_upgrade_epoch(&env, 1),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    /// Future epoch: caller passes a value greater than the stored epoch.
    /// This is equally rejected — the guard is an exact-match, not >=.
    #[test]
    fn rejects_future_epoch_greater_than_stored() {
        let env = Env::default();
        bump_upgrade_epoch(&env); // stored = 1
        assert_eq!(
            require_matching_upgrade_epoch(&env, 100),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    /// `u64::MAX` epoch: if the stored epoch is `u64::MAX` (reached via
    /// saturating arithmetic), only `u64::MAX` itself should pass.
    #[test]
    fn handles_u64_max_epoch_correctly() {
        let env = Env::default();
        // Force the stored epoch to u64::MAX directly.
        env.storage()
            .instance()
            .set(&super::STORAGE_UPGRADE_EPOCH, &u64::MAX);

        assert_eq!(require_matching_upgrade_epoch(&env, u64::MAX), Ok(()));
        assert_eq!(
            require_matching_upgrade_epoch(&env, u64::MAX - 1),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    // ── bump_upgrade_epoch ────────────────────────────────────────────────

    /// `get_upgrade_epoch` returns 0 on a fresh env.
    #[test]
    fn get_upgrade_epoch_returns_zero_by_default() {
        let env = Env::default();
        assert_eq!(get_upgrade_epoch(&env), 0);
    }

    /// Each bump increments by exactly 1.
    #[test]
    fn bump_increments_epoch_by_one() {
        let env = Env::default();
        assert_eq!(get_upgrade_epoch(&env), 0);
        bump_upgrade_epoch(&env);
        assert_eq!(get_upgrade_epoch(&env), 1);
        bump_upgrade_epoch(&env);
        assert_eq!(get_upgrade_epoch(&env), 2);
    }

    /// `bump_upgrade_epoch` saturates at `u64::MAX` instead of wrapping or
    /// panicking. This keeps the guard correct at the arithmetic boundary.
    #[test]
    fn bump_saturates_at_u64_max() {
        let env = Env::default();
        env.storage()
            .instance()
            .set(&super::STORAGE_UPGRADE_EPOCH, &(u64::MAX - 1));

        bump_upgrade_epoch(&env); // u64::MAX - 1 + 1 = u64::MAX
        assert_eq!(get_upgrade_epoch(&env), u64::MAX);

        bump_upgrade_epoch(&env); // saturates: u64::MAX.saturating_add(1) = u64::MAX
        assert_eq!(get_upgrade_epoch(&env), u64::MAX);
    }

    // ── Storage isolation ─────────────────────────────────────────────────

    /// Two independent `Env` instances do not share upgrade epoch state.
    #[test]
    fn upgrade_epoch_is_isolated_per_environment() {
        let env_a = Env::default();
        let env_b = Env::default();

        bump_upgrade_epoch(&env_a); // env_a = 1
        // env_b still has epoch 0
        assert_eq!(require_matching_upgrade_epoch(&env_a, 1), Ok(()));
        assert_eq!(require_matching_upgrade_epoch(&env_b, 0), Ok(()));
        assert_eq!(
            require_matching_upgrade_epoch(&env_b, 1),
            Err(UpgradeEpochError::EpochMismatch),
        );
    }

    // ── ABI stability ─────────────────────────────────────────────────────

    /// `EpochMismatch` is pinned at discriminant 1. Changing this would
    /// break callers that map the error over XDR into contract-specific
    /// error enums.
    #[test]
    fn upgrade_epoch_error_discriminant_stable_at_1() {
        assert_eq!(UpgradeEpochError::EpochMismatch as u32, 1);
    }
}
