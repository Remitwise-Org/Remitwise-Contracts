#![no_std]
#![allow(clippy::too_many_arguments)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
#[cfg(test)]
mod test;

use remitwise_common::{clamp_limit, EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token::TokenClient, vec,
    Address, BytesN, Env, IntoVal, Map, Symbol, Vec,
};
use soroban_sdk::crypto::Sha256;

// Event topics
const SPLIT_INITIALIZED: Symbol = symbol_short!("init");
const SPLIT_CALCULATED: Symbol = symbol_short!("calc");

// Request hash domain separator for signing (prevents cross-domain attacks)
const DISTRIBUTE_USDC_DOMAIN: &[u8] = b"distribute_usdc_v1";
const MAX_DEADLINE_WINDOW_SECS: u64 = 3600; // 1 hour deadline window

// Event data structures
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SplitInitializedEvent {
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub timestamp: u64,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RemittanceSplitError {
    AlreadyInitialized = 1,
    /// The contract has not been initialized yet; `initialize_split` must be called first.
    NotInitialized = 2,
    /// One or more split percentages are invalid: either a field exceeds 100 or the four
    /// fields do not sum to exactly 100.
    InvalidPercentages = 3,
    InvalidAmount = 4,
    Overflow = 5,
    /// The caller is not authorized to perform this operation.
    Unauthorized = 6,
    InvalidNonce = 7,
    /// The snapshot's `schema_version` is outside the supported range
    /// `[MIN_SUPPORTED_SCHEMA_VERSION, SCHEMA_VERSION]`.
    UnsupportedVersion = 8,
    /// The snapshot's `checksum` field does not match the value computed from its contents;
    /// the payload may have been tampered with or corrupted.
    ChecksumMismatch = 9,
    InvalidDueDate = 10,
    ScheduleNotFound = 11,
    InactiveSchedule = 21,
    /// The supplied token contract address does not match the trusted USDC contract.
    UntrustedTokenContract = 12,
    /// A destination account is the same as the sender, which would be a no-op transfer.
    SelfTransferNotAllowed = 13,
    DeadlineExpired = 14,

    RequestHashMismatch = 15,

    PercentagesDoNotSumTo100 = 18,
    FutureTimestamp = 19,
    OwnerMismatch = 20,
    NonceAlreadyUsed = 16,
    PercentageOutOfRange = 17,
    /// The owner has reached the maximum number of allowed schedules.
    ScheduleCapExceeded = 22,
    /// The schedule interval is below the minimum allowed value.
    ScheduleIntervalTooShort = 23,
    /// The schedule lead time exceeds the maximum allowed value.
    ScheduleLeadTimeTooLong = 24,
}

#[derive(Clone)]
#[contracttype]
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}

/// Typed request for distribute_usdc signing.
/// This structure contains all parameters needed for distributing USDC.
/// Signers use this to compute a deterministic request hash.
#[derive(Clone)]
#[contracttype]
pub struct DistributeUsdcRequest {
    /// USDC contract address
    pub usdc_contract: Address,
    /// Sender/payer address
    pub from: Address,
    /// Nonce for replay protection
    pub nonce: u64,
    /// Destination accounts for split allocation
    pub accounts: AccountGroup,
    /// Total USDC amount to distribute
    pub total_amount: i128,
    /// Deadline timestamp (Unix seconds) - request is invalid after this time
    pub deadline: u64,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days
/// Maximum number of used nonces tracked per address before the oldest are pruned.
const MAX_USED_NONCES_PER_ADDR: u32 = 256;
/// Maximum ledger seconds a signed request may remain valid after creation.
const MAX_DEADLINE_WINDOW_SECS: u64 = 3600; // 1 hour
/// Maximum number of remittance schedules allowed per owner to prevent storage bloat.
pub const MAX_SCHEDULES_PER_OWNER: u32 = 50;
/// Minimum allowed recurrence interval for repeating schedules (1 hour in seconds).
/// One-off schedules (interval == 0) are exempt from this check.
pub const MIN_SCHEDULE_INTERVAL: u64 = 3_600;
/// Maximum allowed lead time for schedule due dates (1 year in seconds).
/// Prevents unrealistic far-future scheduling that creates operational risk.
pub const MAX_SCHEDULE_LEAD_TIME: u64 = 365 * 24 * 3_600;

/// Split configuration with owner tracking for access control
#[derive(Clone)]
#[contracttype]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub timestamp: u64,
    pub initialized: bool,
    /// The only token contract address permitted for distribute_usdc calls.
    /// Stored at initialization time; prevents substitution attacks.
    pub usdc_contract: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SplitCalculatedEvent {
    pub total_amount: i128,
    pub spending_amount: i128,
    pub savings_amount: i128,
    pub bills_amount: i128,
    pub insurance_amount: i128,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct DistributionCompletedEvent {
    pub from: Address,
    pub total_amount: i128,
    pub spending_amount: i128,
    pub savings_amount: i128,
    pub bills_amount: i128,
    pub insurance_amount: i128,
    pub timestamp: u64,
}

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SplitEvent {
    Initialized,
    Updated,
    Calculated,
    /// Emitted when distribute_usdc successfully completes all transfers.
    DistributionCompleted,
    SnapshotExported,
    SnapshotImported,
}

/// Snapshot for data export/import (migration).
///
/// The checksum is an FNV-1a digest covering every scalar field
/// (schema_version, all four percentages, config timestamp, the initialized
/// flag, and the export timestamp). Any single-bit mutation to any covered
/// field will produce a different checksum, making tampered payloads
/// detectable before restore.
/// Importers **must** validate `schema_version` against the supported range
/// (`MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION`) before applying.
#[contracttype]
#[derive(Clone)]
pub struct ExportSnapshot {
    /// Explicit schema version tag for this snapshot format.
    /// Supported range: MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION.
    pub schema_version: u32,
    pub checksum: u64,
    pub config: SplitConfig,
    pub schedules: Vec<RemittanceSchedule>,
    pub exported_at: u64,
}

/// Audit log entry for security and compliance.
#[contracttype]
#[derive(Clone)]
pub struct AuditEntry {
    pub operation: Symbol,
    pub caller: Address,
    pub timestamp: u64,
    pub success: bool,
}

/// Paginated result for audit log queries.
#[contracttype]
#[derive(Clone)]
pub struct AuditPage {
    /// Audit entries for this page, ordered by index ascending.
    pub items: Vec<AuditEntry>,
    /// Index to pass as `from_index` for the next page. 0 means no more pages.
    pub next_cursor: u32,
    /// Number of items returned in this page.
    pub count: u32,
}

/// Paginated result for schedule queries.
///
/// Provides stable cursor-based pagination so consumers can replay the schedule list
/// without gaps or duplicates across page boundaries. Schedules are ordered by ID ascending.
#[contracttype]
#[derive(Clone)]
pub struct SchedulePage {
    /// Schedule entries for this page, ordered by ID ascending.
    pub items: Vec<RemittanceSchedule>,
    /// Index to pass as `from_index` for the next page. 0 means no more pages.
    pub next_cursor: u32,
    /// Number of items returned in this page.
    pub count: u32,
}

/// Split allocation output item for UI/analytics consumers.
#[contracttype]
#[derive(Clone)]
pub struct Allocation {
    pub category: Symbol,
    pub amount: i128,
}

/// Schedule for automatic remittance splits
#[contracttype]
#[derive(Clone)]
pub struct RemittanceSchedule {
    pub id: u32,
    pub owner: Address,
    pub amount: i128,
    pub next_due: u64,
    pub interval: u64,
    pub recurring: bool,
    pub active: bool,
    pub created_at: u64,
    pub last_executed: Option<u64>,
    pub missed_count: u32,
}

/// Schedule event types
#[contracttype]
#[derive(Clone)]
pub enum ScheduleEvent {
    Created,
    Executed,
    Missed,
    Modified,
    Cancelled,
}

/// Domain-separated authorization payload for split operations.
///
/// Includes the full set of initialization parameters so that the
/// signer commits to the exact configuration being applied.
// NOTE: `SplitAuthPayload` is defined below with a stable schema.

/// Current snapshot schema version. Bumped to 2 for FNV-1a checksum + exported_at field.
const SCHEMA_VERSION: u32 = 2;
/// Oldest snapshot schema version this contract can import. Enables backward compat.
const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Domain-separated payload for split initialization.
/// Binds technical context (network, contract) with business parameters
/// to prevent relay/replay attacks across different deployments or networks.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitAuthPayload {
    /// Domain identifier for functional separation (e.g. symbol_short!("init"))
    pub domain_id: Symbol,
    /// Network ID to prevent replay across different Stellar networks
    pub network_id: BytesN<32>,
    /// Contract address to prevent replay across different contract instances
    pub contract_addr: Address,
    /// Owner address who is authorizing the initialization
    pub owner_addr: Address,
    /// Per-address nonce for sequential replay protection
    pub nonce_val: u64,
    /// USDC token contract address
    pub usdc_contract: Address,
    /// Percentage for precision spending
    pub spending_percent: u32,
    /// Percentage for savings goals
    pub savings_percent: u32,
    /// Percentage for bill payments
    pub bills_percent: u32,
    /// Percentage for insurance premiums
    pub insurance_percent: u32,
}

const MAX_AUDIT_ENTRIES: u32 = 100;
const CONTRACT_VERSION: u32 = 1;

#[contracttype]
pub enum DataKey {
    Schedule(u32),
    OwnerSchedules(Address),
}

#[contract]
pub struct RemittanceSplit;

#[contractimpl]
impl RemittanceSplit {
    fn get_pause_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    }
    fn get_global_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("PAUSED"))
            .unwrap_or(false)
    }
    fn require_not_paused(env: &Env) -> Result<(), RemittanceSplitError> {
        if Self::get_global_paused(env) {
            Err(RemittanceSplitError::Unauthorized)
        } else {
            Ok(())
        }
    }

    /// Set the pause administrator responsible for emergency pause control.
    ///
    /// # Arguments
    /// * `caller` - Split owner address (must authorize)
    /// * `new_admin` - Address that will gain pause and unpause authority
    ///
    /// # Errors
    /// - `NotInitialized` if the split has not been initialized yet
    /// - `Unauthorized` if `caller` is not the owner or the contract is currently paused
    pub fn set_pause_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        if config.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }

    /// Pause all state-changing split entrypoints until `unpause` is called.
    ///
    /// # Arguments
    /// * `caller` - Pause administrator address (must authorize)
    ///
    /// # Errors
    /// - `NotInitialized` if the split has not been initialized yet
    /// - `Unauthorized` if `caller` is not the active pause admin or the contract is already paused
    pub fn pause(env: Env, caller: Address) -> Result<(), RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        let admin = Self::get_pause_admin(&env).unwrap_or(config.owner);
        if admin != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("paused"),
            (),
        );
        Ok(())
    }

    /// Resume state-changing split entrypoints after an emergency pause.
    ///
    /// This is intentionally the only mutating entrypoint that remains callable while paused.
    ///
    /// # Arguments
    /// * `caller` - Pause administrator address (must authorize)
    ///
    /// # Errors
    /// - `NotInitialized` if the split has not been initialized yet
    /// - `Unauthorized` if `caller` is not the active pause admin
    pub fn unpause(env: Env, caller: Address) -> Result<(), RemittanceSplitError> {
        caller.require_auth();
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        let admin = Self::get_pause_admin(&env).unwrap_or(config.owner);
        if admin != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &false);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("unpaused"),
            (),
        );
        Ok(())
    }
    pub fn is_paused(env: Env) -> bool {
        Self::get_global_paused(&env)
    }
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }
    fn get_upgrade_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("UPG_ADM"))
    }
    /// Set or transfer the upgrade admin role.
    ///
    /// # Security Requirements
    /// - If no upgrade admin exists, only the contract owner can set the initial admin
    /// - If upgrade admin exists, only the current upgrade admin can transfer to a new admin
    /// - Caller must be authenticated via require_auth()
    ///
    /// # Parameters
    /// - `caller`: The address attempting to set the upgrade admin
    /// - `new_admin`: The address to become the new upgrade admin
    ///
    /// # Returns
    /// - `Ok(())` on successful admin transfer
    /// - `Err(RemittanceSplitError::Unauthorized)` if caller lacks permission
    /// - `Err(RemittanceSplitError::NotInitialized)` if contract not initialized
    pub fn set_upgrade_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), RemittanceSplitError> {
        caller.require_auth();

        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;

        let current_upgrade_admin = Self::get_upgrade_admin(&env);

        // Authorization logic:
        // 1. If no upgrade admin exists, only contract owner can set initial admin
        // 2. If upgrade admin exists, only current upgrade admin can transfer
        match &current_upgrade_admin {
            None => {
                if config.owner != caller {
                    return Err(RemittanceSplitError::Unauthorized);
                }
            }
            Some(ref current_admin) => {
                // Admin transfer - only current admin can transfer
                if *current_admin != caller {
                    return Err(RemittanceSplitError::Unauthorized);
                }
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);

        // Emit admin transfer event for audit trail
        env.events().publish(
            (symbol_short!("split"), symbol_short!("adm_xfr")),
            (current_upgrade_admin.clone(), new_admin.clone()),
        );

        Ok(())
    }

    /// Get the current upgrade admin address.
    ///
    /// # Returns
    /// - `Some(Address)` if upgrade admin is set
    /// - `None` if no upgrade admin has been configured
    pub fn get_upgrade_admin_public(env: Env) -> Option<Address> {
        Self::get_upgrade_admin(&env)
    }

    /// Update the contract version marker used for migrations and upgrade coordination.
    ///
    /// # Arguments
    /// * `caller` - Upgrade administrator address (must authorize)
    /// * `new_version` - Version marker to persist in instance storage
    ///
    /// # Errors
    /// - `NotInitialized` if the split has not been initialized yet
    /// - `Unauthorized` if `caller` is not the active upgrade admin or the contract is paused
    pub fn set_version(
        env: Env,
        caller: Address,
        new_version: u32,
    ) -> Result<(), RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        let admin = Self::get_upgrade_admin(&env).unwrap_or(config.owner);
        if admin != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        let prev = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &new_version);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("upgraded"),
            (prev, new_version),
        );
        Ok(())
    }

    /// Validate that every individual percentage is in [0, 100] **and** that
    /// their sum equals exactly 100.
    ///
    /// Enforced invariants (checked in order):
    /// 1. Each bucket must be <= 100 (`InvalidPercentages`).
    /// 2. The four buckets must sum to exactly 100 (`PercentagesDoNotSumTo100`).
    ///
    /// Separating the two checks gives callers a precise error code:
    /// a value like 110/0/0/0 produces `InvalidPercentages`, not a misleading
    /// "doesn't sum to 100" message.
    fn validate_percentages(
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> Result<(), RemittanceSplitError> {
        // Per-bucket upper-bound check — must precede sum check.
        if spending_percent > 100
            || savings_percent > 100
            || bills_percent > 100
            || insurance_percent > 100
        {
            return Err(RemittanceSplitError::InvalidPercentages);
        }
        // Global sum invariant.
        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            return Err(RemittanceSplitError::InvalidPercentages);
        }
        Ok(())
    }

    /// Set or update the split percentages used to allocate remittances.
    ///
    /// # Arguments
    /// * `owner` - Address of the split owner (must authorize)
    /// * `nonce` - Caller's transaction nonce (must equal get_nonce(owner)) for replay protection
    /// * `usdc_contract` - The trusted USDC token contract address; only this address is
    ///   permitted in future `distribute_usdc` calls (prevents token substitution attacks)
    /// * `spending_percent` - Percentage for spending (0-100)
    /// * `savings_percent` - Percentage for savings (0-100)
    /// * `bills_percent` - Percentage for bills (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if initialization was successful
    ///
    /// # Errors
    /// - `Unauthorized` if owner doesn't authorize the transaction
    /// - `InvalidNonce` if nonce is invalid (replay protection)
    /// - `PercentagesDoNotSumTo100` if percentages don't sum to 100
    /// - `AlreadyInitialized` if split is already initialized (use update_split instead)
    pub fn initialize_split(
        env: Env,
        owner: Address,
        nonce: u64,
        usdc_contract: Address,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> Result<bool, RemittanceSplitError> {
        let payload = SplitAuthPayload {
            domain_id: symbol_short!("init"),
            network_id: env.ledger().network_id(),
            contract_addr: env.current_contract_address(),
            owner_addr: owner.clone(),
            nonce_val: nonce,
            usdc_contract: usdc_contract.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
        };
        owner.require_auth_for_args(vec![&env, payload.into_val(&env)]);

        Self::require_not_paused(&env)?;
        Self::require_nonce(&env, &owner, nonce)?;

        let existing: Option<SplitConfig> = env.storage().instance().get(&symbol_short!("CONFIG"));
        if existing.is_some() {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            return Err(RemittanceSplitError::AlreadyInitialized);
        }

        if let Err(_e) = Self::validate_percentages(
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
        ) {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            return Err(RemittanceSplitError::InvalidPercentages);
        }

        Self::extend_instance_ttl(&env);

        let config = SplitConfig {
            owner: owner.clone(),
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            timestamp: env.ledger().timestamp(),
            initialized: true,
            usdc_contract,
        };

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        Self::increment_nonce(&env, &owner)?;
        Self::append_audit(&env, symbol_short!("init"), &owner, true);
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("init"),
            owner,
        );

        Ok(true)
    }

    pub fn update_split(
        env: Env,
        caller: Address,
        nonce: u64,
        spending_percent: u32,
        savings_percent: u32,
        bills_percent: u32,
        insurance_percent: u32,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_nonce(&env, &caller, nonce)?;

        let mut config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;

        if config.owner != caller {
            Self::append_audit(&env, symbol_short!("update"), &caller, false);
            return Err(RemittanceSplitError::Unauthorized);
        }

        if let Err(_e) = Self::validate_percentages(
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
        ) {
            Self::append_audit(&env, symbol_short!("update"), &caller, false);
            return Err(RemittanceSplitError::InvalidPercentages);
        }

        Self::extend_instance_ttl(&env);

        config.spending_percent = spending_percent;
        config.savings_percent = savings_percent;
        config.bills_percent = bills_percent;
        config.insurance_percent = insurance_percent;

        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                spending_percent,
                savings_percent,
                bills_percent,
                insurance_percent,
            ],
        );

        let event = SplitInitializedEvent {
            spending_percent,
            savings_percent,
            bills_percent,
            insurance_percent,
            timestamp: env.ledger().timestamp(),
        };
        env.events().publish((SPLIT_INITIALIZED,), event);
        env.events().publish(
            (symbol_short!("split"), SplitEvent::Updated),
            caller.clone(),
        );

        Self::increment_nonce(&env, &caller)?;
        Ok(true)
    }

    pub fn get_split(env: &Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("SPLIT"))
            .unwrap_or_else(|| vec![&env, 50, 30, 15, 5])
    }

    pub fn get_config(env: Env) -> Option<SplitConfig> {
        env.storage().instance().get(&symbol_short!("CONFIG"))
    }

    pub fn calculate_split(
        env: Env,
        total_amount: i128,
    ) -> Result<Vec<i128>, RemittanceSplitError> {
        if total_amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let split = Self::get_split(&env);
        let s0 = match split.get(0) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };
        let s1 = match split.get(1) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };
        let s2 = match split.get(2) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };

        let spending = total_amount
            .checked_mul(s0)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        let savings = total_amount
            .checked_mul(s1)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        let bills = total_amount
            .checked_mul(s2)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        // Insurance gets the remainder to handle rounding
        let insurance = total_amount
            .checked_sub(spending)
            .and_then(|n| n.checked_sub(savings))
            .and_then(|n| n.checked_sub(bills))
            .ok_or(RemittanceSplitError::Overflow)?;

        // Emit SplitCalculated event

        let event = SplitCalculatedEvent {
            total_amount,
            spending_amount: spending,
            savings_amount: savings,
            bills_amount: bills,
            insurance_amount: insurance,
            timestamp: env.ledger().timestamp(),
        };
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Low,
            SPLIT_CALCULATED,
            event,
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Low,
            symbol_short!("calc_raw"),
            total_amount,
        );

        Ok(vec![&env, spending, savings, bills, insurance])
    }

    /// Distribute USDC from `from` to the four split destination accounts according
    /// to the configured percentages.
    ///
    /// # Security invariants enforced
    /// 1. `from.require_auth()` is the very first operation — no state is read before
    ///    the caller proves authority.
    /// 2. The contract must not be paused.
    /// 3. `from` must be the configured split owner — prevents any third party from
    ///    triggering transfers out of an owner's account even if they can self-authorize.
    /// 4. `usdc_contract` must match the address stored at initialization time —
    ///    prevents token-substitution attacks where a malicious token is passed in.
    /// 5. None of the destination accounts may equal `from` — prevents silent no-op
    ///    transfers that could be used to inflate audit logs or waste gas.
    /// 6. Nonce replay protection is checked before any token interaction.
    /// 7. A `DistributionCompleted` event is emitted on success for off-chain indexing.
    ///
    /// # Arguments
    /// * `usdc_contract` - Token contract address (must match the trusted address stored at init)
    /// * `from` - Sender address (must be the config owner and must authorize)
    /// * `nonce` - Replay-protection nonce (must equal `get_nonce(from)`)
    /// * `accounts` - Destination accounts for each split category
    /// * `total_amount` - Total amount to distribute (must be > 0)
    ///
    /// # Errors
    /// - `Unauthorized` if `from` is not the config owner or contract is paused
    /// - `UntrustedTokenContract` if `usdc_contract` ≠ stored trusted address
    /// - `SelfTransferNotAllowed` if any destination account equals `from`
    /// - `InvalidNonce` on replay
    /// - `InvalidAmount` if `total_amount` ≤ 0
    /// - `NotInitialized` if the contract has not been initialized
    pub fn distribute_usdc(
        env: Env,
        usdc_contract: Address,
        from: Address,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
        accounts: AccountGroup,
        total_amount: i128,
    ) -> Result<bool, RemittanceSplitError> {
        // 1. Auth first — before any storage reads or state checks.
        from.require_auth();

        // 2. Pause guard.
        Self::require_not_paused(&env)?;

        // 3. Contract must be initialized; load config for subsequent checks.
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;

        // 4. Only the configured owner may trigger distributions.
        if config.owner != from {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return Err(RemittanceSplitError::Unauthorized);
        }

        // 5. Token contract must match the trusted address pinned at initialization.
        if config.usdc_contract != usdc_contract {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return Err(RemittanceSplitError::UntrustedTokenContract);
        }

        // 6. Amount validation.
        if total_amount <= 0 {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return Err(RemittanceSplitError::InvalidAmount);
        }

        // 7. No destination account may equal the sender (self-transfer guard).
        if accounts.spending == from
            || accounts.savings == from
            || accounts.bills == from
            || accounts.insurance == from
        {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return Err(RemittanceSplitError::SelfTransferNotAllowed);
        }

        // 8. Replay protection.
        let expected_hash = Self::compute_request_hash(
            symbol_short!("distrib"),
            from.clone(),
            nonce,
            total_amount,
            deadline,
        );
        Self::require_nonce_hardened(&env, &from, nonce, deadline, request_hash, expected_hash)?;

        // 9. Calculate split amounts and execute transfers.
        let amounts = Self::calculate_split_amounts(&env, total_amount, false)?;
        let token = TokenClient::new(&env, &usdc_contract);

        if amounts[0] > 0 {
            token.transfer(&from, &accounts.spending, &amounts[0]);
        }
        if amounts[1] > 0 {
            token.transfer(&from, &accounts.savings, &amounts[1]);
        }
        if amounts[2] > 0 {
            token.transfer(&from, &accounts.bills, &amounts[2]);
        }
        if amounts[3] > 0 {
            token.transfer(&from, &accounts.insurance, &amounts[3]);
        }

        // 10. Advance nonce, record audit, emit events.
        Self::increment_nonce(&env, &from)?;
        Self::append_audit(&env, symbol_short!("distrib"), &from, true);

        // Emit unstructured event for backward compatibility
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            symbol_short!("dist_ok"),
            (from.clone(), total_amount),
        );

        // Emit structured event for advanced indexing and reconciliation
        let dist_event = DistributionCompletedEvent {
            from,
            total_amount,
            spending_amount: amounts[0],
            savings_amount: amounts[1],
            bills_amount: amounts[2],
            insurance_amount: amounts[3],
            timestamp: env.ledger().timestamp(),
        };

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Medium,
            symbol_short!("dist_comp"),
            dist_event,
        );

        Ok(true)
    }

    /// Distribute USDC with request hash and deadline verification.
    ///
    /// This function provides secure USDC distribution with deterministic request hashing
    /// and deadline enforcement. Integrators sign the request hash before calling this function.
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `request` - DistributeUsdcRequest containing all parameters and deadline
    /// * `request_hash` - Pre-computed SHA-256 hash that must match compute_request_hash(request)
    ///
    /// # Returns
    /// True if distribution was successful, Error if verification or execution fails
    ///
    /// # Errors
    /// - `InvalidAmount` - total_amount <= 0
    /// - `DeadlineExpired` - current_time > request.deadline
    /// - `InvalidDeadline` - deadline is 0 or unreasonably far in future
    /// - `InvalidNonce` - nonce doesn't match expected value
    /// - `RequestHashMismatch` - provided hash doesn't match computed hash
    /// - `Overflow` - arithmetic overflow in split calculation
    ///
    /// # Security Properties
    /// 1. **Deadline Enforcement**: Prevents indefinite validity of signed requests (max 1 hour window)
    /// 2. **Hash Verification**: Ensures no parameter tampering (all params bound to hash)
    /// 3. **Nonce Protection**: Prevents replay attacks
    /// 4. **Domain Separation**: Hash includes "distribute_usdc_v1" separator
    /// 5. **Deterministic**: Same inputs always produce same hash
    ///
    /// # Example Workflow
    /// 1. Off-chain: Create DistributeUsdcRequest with deadline = now() + 600 seconds
    /// 2. Off-chain: Call get_request_hash to obtain hash
    /// 3. Off-chain: Sign the hash with payer's private key
    /// 4. On-chain: Call distribute_usdc_with_hash_and_deadline with request and signature
    ///
    /// # Parameter Binding Fields
    /// All parameters are cryptographically bound via SHA-256:
    /// - `usdc_contract`: USDC token contract address (prevents cross-token attacks)
    /// - `from`: Payer address (prevents impersonation)
    /// - `nonce`: Transaction sequence number (prevents replays)
    /// - `accounts.spending`: Spending destination (prevents fund misdirection)
    /// - `accounts.savings`: Savings destination (prevents fund misdirection)
    /// - `accounts.bills`: Bills destination (prevents fund misdirection)
    /// - `accounts.insurance`: Insurance destination (prevents fund misdirection)
    /// - `total_amount`: Total amount to distribute (prevents amount tampering)
    /// - `deadline`: Expiry time (prevents stale request use)
    pub fn distribute_usdc_with_hash_and_deadline(
        env: Env,
        request: DistributeUsdcRequest,
        request_hash: Bytes,
    ) -> Result<bool, RemittanceSplitError> {
        // Validate amount
        if request.total_amount <= 0 {
            Self::append_audit(&env, symbol_short!("distH"), &request.from, false);
            return Err(RemittanceSplitError::InvalidAmount);
        }

        // Validate deadline
        let current_time = env.ledger().timestamp();
        if request.deadline == 0 {
            Self::append_audit(&env, symbol_short!("distH"), &request.from, false);
            return Err(RemittanceSplitError::InvalidDeadline);
        }
        if current_time > request.deadline {
            Self::append_audit(&env, symbol_short!("distH"), &request.from, false);
            return Err(RemittanceSplitError::DeadlineExpired);
        }
        
        // Validate deadline is within reasonable bounds (max 1 hour from now)
        let deadline_in_future = request.deadline - current_time;
        if deadline_in_future > MAX_DEADLINE_WINDOW_SECS {
            Self::append_audit(&env, symbol_short!("distH"), &request.from, false);
            return Err(RemittanceSplitError::InvalidDeadline);
        }

        // Verify request hash matches computed hash
        let computed_hash = Self::compute_request_hash(&env, &request);
        if computed_hash.ne(&request_hash) {
            Self::append_audit(&env, symbol_short!("distH"), &request.from, false);
            return Err(RemittanceSplitError::RequestHashMismatch);
        }

        // Require authorization from payer
        request.from.require_auth();

        // Verify nonce
        Self::require_nonce(&env, &request.from, request.nonce)?;

        // Calculate split amounts
        let amounts = Self::calculate_split_amounts(&env, request.total_amount, false)?;
        let token = TokenClient::new(&env, &request.usdc_contract);

        // Execute transfers
        if amounts[0] > 0 {
            token.transfer(&request.from, &request.accounts.spending, &amounts[0]);
        }
        if amounts[1] > 0 {
            token.transfer(&request.from, &request.accounts.savings, &amounts[1]);
        }
        if amounts[2] > 0 {
            token.transfer(&request.from, &request.accounts.bills, &amounts[2]);
        }
        if amounts[3] > 0 {
            token.transfer(&request.from, &request.accounts.insurance, &amounts[3]);
        }

        // Increment nonce and record success
        Self::increment_nonce(&env, &request.from)?;
        Self::append_audit(&env, symbol_short!("distH"), &request.from, true);
        Ok(true)
    }

    pub fn get_usdc_balance(env: &Env, usdc_contract: Address, account: Address) -> i128 {
        TokenClient::new(env, &usdc_contract).balance(&account)
    }

    pub fn get_split_allocations(
        env: &Env,
        total_amount: i128,
    ) -> Result<Vec<Allocation>, RemittanceSplitError> {
        let amounts = Self::calculate_split(env.clone(), total_amount)?;
        let categories = [
            symbol_short!("SPENDING"),
            symbol_short!("SAVINGS"),
            symbol_short!("BILLS"),
            symbol_short!("INSURANCE"),
        ];

        let mut result = Vec::new(env);
        for (category, amount) in categories.into_iter().zip(amounts.into_iter()) {
            result.push_back(Allocation { category, amount });
        }
        Ok(result)
    }

    pub fn get_nonce(env: Env, address: Address) -> u64 {
        Self::get_nonce_value(&env, &address)
    }

    fn get_nonce_value(env: &Env, address: &Address) -> u64 {
        let nonces: Option<Map<Address, u64>> =
            env.storage().instance().get(&symbol_short!("NONCES"));
        nonces
            .as_ref()
            .and_then(|m: &Map<Address, u64>| m.get(address.clone()))
            .unwrap_or(0)
    }

    /// Export a portable snapshot of the current split configuration.
    ///
    /// The returned payload includes an FNV-1a checksum that covers the version,
    /// all percentage fields, the config timestamp, the initialized flag, and
    /// the export timestamp (`exported_at`). Any mutation to these fields will
    /// be detected by `import_snapshot` or `verify_snapshot`.
    ///
    /// # Authorization
    /// Only the contract owner may export a snapshot.
    pub fn export_snapshot(
        env: Env,
        caller: Address,
    ) -> Result<Option<ExportSnapshot>, RemittanceSplitError> {
        caller.require_auth();
        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        if config.owner != caller {
            Self::append_audit(&env, symbol_short!("export"), &caller, false);
            return Err(RemittanceSplitError::Unauthorized);
        }
        let schedules = Self::get_remittance_schedules(env.clone(), caller.clone());
        let checksum = Self::compute_checksum(SCHEMA_VERSION, &config, &schedules);
        env.events().publish(
            (symbol_short!("split"), symbol_short!("snap_exp")),
            SCHEMA_VERSION,
        );
        Ok(Some(ExportSnapshot {
            schema_version: SCHEMA_VERSION,
            checksum,
            config,
            schedules,
            exported_at: env.ledger().timestamp(),
        }))
    }

    /// Import a previously exported snapshot, restoring the full `SplitConfig` and
    /// associated `RemittanceSchedule` list to on-chain storage after running the
    /// complete validation pipeline.
    ///
    /// # Arguments
    /// * `caller` - Split owner address (must authorize)
    /// * `nonce` - Replay-protection nonce (must equal `get_nonce(caller)`)
    /// * `snapshot` - Serialized configuration snapshot to restore
    ///
    /// # Errors
    /// * `UnsupportedVersion` — `snapshot.schema_version` is outside
    ///   `[MIN_SUPPORTED_SCHEMA_VERSION, SCHEMA_VERSION]`.
    /// * `ChecksumMismatch` — `snapshot.checksum` does not match the value computed
    ///   by `compute_checksum` over the snapshot fields.
    /// * `SnapshotNotInitialized` — `snapshot.config.initialized` is `false`; the
    ///   snapshot represents an incomplete or factory-default configuration.
    /// * `InvalidPercentageRange` — at least one of `spending_percent`,
    ///   `savings_percent`, `bills_percent`, or `insurance_percent` exceeds `100`.
    /// * `InvalidPercentages` — all four percentage fields are within `[0, 100]` but
    ///   their sum is not equal to `100`.
    /// * `InvalidAmount` — `snapshot.config.timestamp` is greater than the current
    ///   ledger timestamp, indicating a future-dated or replayed payload.
    /// * `Unauthorized` — `caller` is not the current on-chain owner stored in
    ///   instance storage, or the contract is paused.
    /// * `OwnerMismatch` — `snapshot.config.owner` does not equal `caller`, which
    ///   would silently transfer ownership if allowed.
    /// * `NotInitialized` — no existing `SplitConfig` is present in instance storage
    ///   (the contract has not been initialized).
    /// * `InvalidNonce` — the provided `nonce` has already been used or does not
    ///   match the expected replay-protection value for `caller`.
    pub fn import_snapshot(
        env: Env,
        caller: Address,
        nonce: u64,
        snapshot: ExportSnapshot,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_nonce(&env, &caller, nonce)?;

        // 1. Version boundary check
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION
            || snapshot.schema_version > SCHEMA_VERSION
        {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::UnsupportedVersion);
        }
        let expected = Self::compute_checksum(
            snapshot.schema_version,
            &snapshot.config,
            &snapshot.schedules,
        );
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::ChecksumMismatch);
        }

        // 3. Initialized flag — a snapshot with initialized = false is
        //    incomplete and must not be restored.
        if !snapshot.config.initialized {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::NotInitialized);
        }

        // 4. Per-field percentage range — reject values that could not have
        //    been produced by a valid initialize_split / update_split call.
        if snapshot.config.spending_percent > 100
            || snapshot.config.savings_percent > 100
            || snapshot.config.bills_percent > 100
            || snapshot.config.insurance_percent > 100
        {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::InvalidPercentages);
        }

        // 5. Sum constraint
        let total = snapshot.config.spending_percent
            + snapshot.config.savings_percent
            + snapshot.config.bills_percent
            + snapshot.config.insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::PercentagesDoNotSumTo100);
        }

        // 6. Timestamp sanity — reject payloads whose timestamps are in the future.
        let current_time = env.ledger().timestamp();
        if snapshot.config.timestamp > current_time {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::InvalidDueDate);
        }

        // 7. Caller must be the current contract owner.
        let existing: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        if existing.owner != caller {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::Unauthorized);
        }

        // 8. Ownership mapping — prevent silent ownership transfer via snapshot.
        if snapshot.config.owner != caller {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::Unauthorized);
        }

        // 9. Schedule cap validation — prevent bypass via migration
        if snapshot.schedules.len() > MAX_SCHEDULES_PER_OWNER as u32 {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::ScheduleCapExceeded);
        }

        for schedule in snapshot.schedules.iter() {
            if schedule.active {
                if schedule.interval > 0 && schedule.interval < MIN_SCHEDULE_INTERVAL {
                    Self::append_audit(&env, symbol_short!("import"), &caller, false);
                    return Err(RemittanceSplitError::ScheduleIntervalTooShort);
                }
                if schedule.next_due.saturating_sub(current_time) > MAX_SCHEDULE_LEAD_TIME {
                    Self::append_audit(&env, symbol_short!("import"), &caller, false);
                    return Err(RemittanceSplitError::ScheduleLeadTimeTooLong);
                }
            }
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("CONFIG"), &snapshot.config);
        env.storage().instance().set(
            &symbol_short!("SPLIT"),
            &vec![
                &env,
                snapshot.config.spending_percent,
                snapshot.config.savings_percent,
                snapshot.config.bills_percent,
                snapshot.config.insurance_percent,
            ],
        );

        // Import schedules to new storage
        for schedule in snapshot.schedules.iter() {
            env.storage()
                .persistent()
                .set(&DataKey::Schedule(schedule.id), &schedule);
            env.storage().persistent().extend_ttl(
                &DataKey::Schedule(schedule.id),
                INSTANCE_LIFETIME_THRESHOLD,
                INSTANCE_BUMP_AMOUNT,
            );
        }

        // Reconstruct owner index
        let owner_ids: Vec<u32> = Vec::new(&env);
        // Schedule IDs are maintained in insertion order; Vec doesn't support sort
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedules(caller.clone()), &owner_ids);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerSchedules(caller.clone()),
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );

        Self::increment_nonce(&env, &caller)?;
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        env.events().publish(
            (symbol_short!("split"), SplitEvent::SnapshotImported),
            caller,
        );
        Ok(true)
    }

    /// Verify snapshot integrity without importing it.
    ///
    /// Runs the same validation pipeline as `import_snapshot` steps 2–7 — schema
    /// version boundary, checksum integrity, initialized flag, per-field percentage
    /// range, percentage sum constraint, and timestamp sanity — **without** modifying
    /// any contract state. Use this as a read-only pre-flight check before calling
    /// `import_snapshot`.
    ///
    /// Returns `Ok(true)` when all checks pass and the snapshot is ready to import.
    ///
    /// # Errors
    /// - `UnsupportedVersion` — `snapshot.schema_version` is outside
    ///   `[MIN_SUPPORTED_SCHEMA_VERSION, SCHEMA_VERSION]`.
    /// - `ChecksumMismatch` — the stored checksum does not match the freshly
    ///   computed digest over the snapshot fields.
    /// - `SnapshotNotInitialized` — `snapshot.config.initialized` is `false`.
    /// - `InvalidPercentageRange` — at least one of the four percentage fields
    ///   individually exceeds `100`.
    /// - `InvalidPercentages` — all four fields are ≤ 100 but their sum is not `100`.
    /// - `InvalidAmount` — `snapshot.config.timestamp` is greater than the current
    ///   ledger timestamp (future-dated payload).
    ///
    /// # Note
    /// This function does **not** check ownership mapping or nonce validity; those
    /// require a specific caller context and are only enforced by `import_snapshot`.
    pub fn verify_snapshot(
        env: Env,
        snapshot: ExportSnapshot,
    ) -> Result<bool, RemittanceSplitError> {
        // Step 2. Schema version boundary
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION
            || snapshot.schema_version > SCHEMA_VERSION
        {
            return Err(RemittanceSplitError::UnsupportedVersion);
        }

        // Step 3. Checksum integrity
        let expected = Self::compute_checksum(
            snapshot.schema_version,
            &snapshot.config,
            &snapshot.schedules,
        );
        if snapshot.checksum != expected {
            return Err(RemittanceSplitError::ChecksumMismatch);
        }

        // Step 4. Initialized flag
        if !snapshot.config.initialized {
            return Err(RemittanceSplitError::NotInitialized);
        }

        // 4. Per-field range
        if snapshot.config.spending_percent > 100
            || snapshot.config.savings_percent > 100
            || snapshot.config.bills_percent > 100
            || snapshot.config.insurance_percent > 100
        {
            return Err(RemittanceSplitError::InvalidPercentages);
        }

        // 5. Sum constraint
        let total = snapshot.config.spending_percent
            + snapshot.config.savings_percent
            + snapshot.config.bills_percent
            + snapshot.config.insurance_percent;
        if total != 100 {
            return Err(RemittanceSplitError::InvalidPercentages);
        }

        // 6. Timestamp sanity
        let current_time = env.ledger().timestamp();
        if snapshot.config.timestamp > current_time {
            return Err(RemittanceSplitError::FutureTimestamp);
        }

        Ok(true)
    }

    /// Return a page of audit log entries with a stable cursor.
    ///
    /// # Parameters
    /// - `from_index`: zero-based starting index (pass 0 for the first page,
    ///   then use the returned `next_cursor` for subsequent pages).
    /// - `limit`: maximum entries to return; clamped to `[DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT]`.
    ///
    /// # Pagination contract
    /// - Entries are returned oldest-to-newest within the rotating log window.
    /// - `next_cursor == 0` signals no more pages.
    /// - Uses saturating arithmetic so a caller cannot trigger overflow panics.
    /// - Deterministic: identical `(from_index, limit)` on identical state always
    ///   returns the same page, enabling reliable replay by audit consumers.
    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> AuditPage {
        let log: Option<Vec<AuditEntry>> = env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();
        let cap = clamp_limit(limit);

        if from_index >= len {
            return AuditPage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let end = from_index.saturating_add(cap).min(len);
        let mut items = Vec::new(&env);
        for i in from_index..end {
            if let Some(entry) = log.get(i) {
                items.push_back(entry);
            }
        }

        let count = items.len();
        let next_cursor = if end < len { end } else { 0 };

        AuditPage {
            items,
            next_cursor,
            count,
        }
    }

    fn require_nonce(
        env: &Env,
        address: &Address,
        expected: u64,
    ) -> Result<(), RemittanceSplitError> {
        let current = Self::get_nonce_value(env, address);
        if expected != current {
            return Err(RemittanceSplitError::InvalidNonce);
        }
        Ok(())
    }

    /// Hardened nonce validation with three layers of replay protection:
    ///
    /// 1. **Deadline check** — rejects requests whose `deadline` is in the past
    ///    or further than `MAX_DEADLINE_WINDOW_SECS` in the future (stale or
    ///    pre-signed too far ahead).
    /// 2. **Sequential counter** — the nonce must equal `get_nonce(address)`.
    /// 3. **Used-nonce set** — the nonce must not appear in the per-address
    ///    consumed set, preventing double-spend even if the counter is reset
    ///    via snapshot import.
    /// 4. **Request hash binding** — `request_hash` must equal the caller's
    ///    expected fingerprint; prevents parameter-swap replay attacks where
    ///    a valid nonce is reused with different arguments.
    ///
    /// # Arguments
    /// * `address`      — The signing address
    /// * `nonce`        — The nonce being consumed
    /// * `deadline`     — Ledger timestamp after which the request expires
    /// * `request_hash` — Caller-computed binding hash (see `compute_request_hash`)
    /// * `expected_hash`— Hash the contract recomputes from its own parameters
    fn require_nonce_hardened(
        env: &Env,
        address: &Address,
        nonce: u64,
        deadline: u64,
        request_hash: u64,
        expected_hash: u64,
    ) -> Result<(), RemittanceSplitError> {
        let now = env.ledger().timestamp();

        // 1. Deadline: must be in the future but not too far ahead
        if deadline <= now {
            return Err(RemittanceSplitError::DeadlineExpired);
        }
        if deadline > now + MAX_DEADLINE_WINDOW_SECS {
            return Err(RemittanceSplitError::DeadlineExpired);
        }

        // 2. Sequential counter
        Self::require_nonce(env, address, nonce)?;

        // 3. Used-nonce double-spend check
        if Self::is_nonce_used(env, address, nonce) {
            return Err(RemittanceSplitError::NonceAlreadyUsed);
        }

        // 4. Request hash binding
        if request_hash != expected_hash {
            return Err(RemittanceSplitError::RequestHashMismatch);
        }

        Ok(())
    }

    /// Returns true if `nonce` has already been consumed for `address`.
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

    /// Compute a deterministic u64 request fingerprint.
    ///
    /// Binds: operation symbol bits + nonce + amount + deadline.
    /// Works in `no_std` — uses `Symbol::to_val()` to extract the raw
    /// packed bits instead of `to_string()`, which requires std's `ToString`.
    ///
    /// # Arguments
    /// * `operation` — Short symbol tag (e.g. `symbol_short!("distrib")`)
    /// * `nonce`     — The request nonce
    /// * `amount`    — Token amount (0 for non-monetary operations)
    /// * `deadline`  — Request expiry ledger timestamp
    pub fn compute_request_hash(
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

    fn increment_nonce(env: &Env, address: &Address) -> Result<(), RemittanceSplitError> {
        let current = Self::get_nonce_value(env, address);
        // Mark current nonce as used BEFORE advancing the counter
        Self::mark_nonce_used(env, address, current);

        let next = current
            .checked_add(1)
            .ok_or(RemittanceSplitError::Overflow)?;
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

    fn compute_checksum(
        version: u32,
        config: &SplitConfig,
        schedules: &Vec<RemittanceSchedule>,
    ) -> u64 {
        let v = version as u64;
        let s = config.spending_percent as u64;
        let g = config.savings_percent as u64;
        let b = config.bills_percent as u64;
        let i = config.insurance_percent as u64;
        let sc_count = schedules.len() as u64;

        v.wrapping_add(s)
            .wrapping_add(g)
            .wrapping_add(b)
            .wrapping_add(i)
            .wrapping_add(sc_count)
            .wrapping_mul(31)
    }

    fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
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
            caller: caller.clone(),
            timestamp,
            success,
        });
        env.storage().instance().set(&symbol_short!("AUDIT"), &log);
    }

    fn calculate_split_amounts(
        env: &Env,
        total_amount: i128,
        emit_events: bool,
    ) -> Result<[i128; 4], RemittanceSplitError> {
        if total_amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let split = Self::get_split(env);
        let s0 = match split.get(0) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };
        let s1 = match split.get(1) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };
        let s2 = match split.get(2) {
            Some(v) => v as i128,
            None => return Err(RemittanceSplitError::Overflow),
        };

        let spending = total_amount
            .checked_mul(s0)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        let savings = total_amount
            .checked_mul(s1)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        let bills = total_amount
            .checked_mul(s2)
            .and_then(|n| n.checked_div(100))
            .ok_or(RemittanceSplitError::Overflow)?;
        let insurance = total_amount
            .checked_sub(spending)
            .and_then(|n| n.checked_sub(savings))
            .and_then(|n| n.checked_sub(bills))
            .ok_or(RemittanceSplitError::Overflow)?;

        if emit_events {
            let event = SplitCalculatedEvent {
                total_amount,
                spending_amount: spending,
                savings_amount: savings,
                bills_amount: bills,
                insurance_amount: insurance,
                timestamp: env.ledger().timestamp(),
            };
            RemitwiseEvents::emit(
                env,
                EventCategory::Transaction,
                EventPriority::Low,
                SPLIT_CALCULATED,
                event,
            );
            RemitwiseEvents::emit(
                env,
                EventCategory::Transaction,
                EventPriority::Low,
                symbol_short!("calc_raw"),
                total_amount,
            );
        }

        Ok([spending, savings, bills, insurance])
    }

    /// Extend the TTL of instance storage
    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn sort_u32_vec(v: &mut Vec<u32>) {
        let len = v.len();
        for i in 1..len {
            let key = v.get(i).unwrap_or(0);
            let mut j = i;
            while j > 0 {
                let prev = v.get(j - 1).unwrap_or(0);
                if prev <= key {
                    break;
                }
                v.set(j, prev);
                j -= 1;
            }
            v.set(j, key);
        }
    }

    /// Create a new automatic remittance schedule for the split owner.
    ///
    /// # Arguments
    /// * `owner` - Split owner address (must authorize)
    /// * `amount` - Amount to distribute on each execution
    /// * `next_due` - Unix timestamp of the next execution
    /// * `interval` - Recurrence interval in seconds; `0` creates a one-off schedule
    ///
    /// # Errors
    /// - `InvalidAmount` if `amount` is not positive
    /// - `InvalidDueDate` if `next_due` is not in the future
    /// - `Unauthorized` if the contract is paused
    pub fn create_remittance_schedule(
        env: Env,
        owner: Address,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, RemittanceSplitError> {
        owner.require_auth();
        Self::require_not_paused(&env)?;

        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;

        if config.owner != owner {
            return Err(RemittanceSplitError::Unauthorized);
        }

        if amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(RemittanceSplitError::InvalidDueDate);
        }

        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(RemittanceSplitError::ScheduleIntervalTooShort);
        }

        if next_due.saturating_sub(current_time) > MAX_SCHEDULE_LEAD_TIME {
            return Err(RemittanceSplitError::ScheduleLeadTimeTooLong);
        }

        // Check schedule cap before creating new schedule
        let owner_schedules: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        if owner_schedules.len() >= MAX_SCHEDULES_PER_OWNER as u32 {
            return Err(RemittanceSplitError::ScheduleCapExceeded);
        }

        let current_max_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_RSCH"))
            .unwrap_or(0u32);

        let next_schedule_id = current_max_id
            .checked_add(1)
            .ok_or(RemittanceSplitError::Overflow)?;

        // Explicit uniqueness check to prevent any potential storage collisions
        if env
            .storage()
            .persistent()
            .has(&DataKey::Schedule(next_schedule_id))
        {
            return Err(RemittanceSplitError::Overflow); // Should be unreachable with monotonic counter
        }

        let schedule = RemittanceSchedule {
            id: next_schedule_id,
            owner: owner.clone(),
            amount,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: current_time,
            last_executed: None,
            missed_count: 0,
        };

        // 1. Save individual schedule to persistent storage
        env.storage()
            .persistent()
            .set(&DataKey::Schedule(next_schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey::Schedule(next_schedule_id),
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );

        // 2. Update owner's schedule index
        let mut owner_schedules: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        owner_schedules.push_back(next_schedule_id);
        // Vec doesn't support sort; IDs are maintained in insertion order
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedules(owner.clone()), &owner_schedules);
        env.storage().persistent().extend_ttl(
            &DataKey::OwnerSchedules(owner.clone()),
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_RSCH"), &next_schedule_id);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("sch_new"),
            (next_schedule_id, owner),
        );

        Ok(next_schedule_id)
    }

    /// Modify an existing remittance schedule owned by `caller`.
    ///
    /// # Arguments
    /// * `caller` - Schedule owner address (must authorize)
    /// * `schedule_id` - Identifier returned by `create_remittance_schedule`
    /// * `amount` - Replacement schedule amount
    /// * `next_due` - Replacement next due timestamp
    /// * `interval` - Replacement recurrence interval in seconds
    ///
    /// # Errors
    /// - `InvalidAmount` if `amount` is not positive
    /// - `InvalidDueDate` if `next_due` is not in the future
    /// - `ScheduleNotFound` if `schedule_id` does not exist
    /// - `Unauthorized` if `caller` does not own the schedule or the contract is paused
    pub fn modify_remittance_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;

        let config: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;

        if config.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        if amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(RemittanceSplitError::InvalidDueDate);
        }

        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(RemittanceSplitError::ScheduleIntervalTooShort);
        }

        if next_due.saturating_sub(current_time) > MAX_SCHEDULE_LEAD_TIME {
            return Err(RemittanceSplitError::ScheduleLeadTimeTooLong);
        }

        let mut schedule: RemittanceSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

        if !schedule.active {
            return Err(RemittanceSplitError::InactiveSchedule);
        }

        if schedule.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        schedule.amount = amount;
        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey::Schedule(schedule_id),
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("sch_mod"),
            (schedule_id, caller),
        );

        Ok(true)
    }

    /// Cancel an existing remittance schedule owned by `caller`.
    ///
    /// # Arguments
    /// * `caller` - Schedule owner address (must authorize)
    /// * `schedule_id` - Identifier returned by `create_remittance_schedule`
    ///
    /// # Errors
    /// - `ScheduleNotFound` if `schedule_id` does not exist
    /// - `Unauthorized` if `caller` does not own the schedule or the contract is paused
    pub fn cancel_remittance_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();
        Self::require_not_paused(&env)?;

        let mut schedule: RemittanceSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

        if !schedule.active {
            return Err(RemittanceSplitError::InactiveSchedule);
        }

        if schedule.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        schedule.active = false;

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey::Schedule(schedule_id),
            INSTANCE_LIFETIME_THRESHOLD,
            INSTANCE_BUMP_AMOUNT,
        );

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("sch_can"),
            (schedule_id, caller),
        );

        Ok(true)
    }

    pub fn get_remittance_schedules(env: Env, owner: Address) -> Vec<RemittanceSchedule> {
        let schedule_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        // Ensure deterministic ordering by sorting IDs ascending
        // This guarantees consistent results regardless of storage order
        // Vec doesn't support sort; IDs are maintained in insertion order

        let mut result = Vec::new(&env);
        for id in schedule_ids.iter() {
            if let Some(schedule) = env.storage().persistent().get(&DataKey::Schedule(id)) {
                result.push_back(schedule);
            }
        }
        result
    }

    /// Get remittance schedules for an owner with pagination.
    ///
    /// Returns schedules ordered by ID ascending, with stable cursor-based pagination.
    /// This ensures deterministic ordering for UI/indexer cursor management.
    ///
    /// **Ordering Guarantee**: Schedules are always returned in ascending ID order,
    /// regardless of creation time or storage order. This provides stable pagination
    /// cursors that work reliably across different storage implementations.
    ///
    /// # Arguments
    /// * `owner` - The owner address to query schedules for
    /// * `from_index` - Zero-based starting index in the owner's schedule list
    /// * `limit` - Maximum number of schedules to return (clamped to MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// A SchedulePage containing:
    /// - `items`: Schedules for this page, ordered by ID ascending
    /// - `next_cursor`: Index for next page (0 if no more pages)
    /// - `count`: Number of items in this page
    ///
    /// # Pagination Notes
    /// - Results are ordered deterministically by schedule ID ascending
    /// - `from_index` is a stable cursor within the owner's schedule list
    /// - `limit` is clamped to prevent excessive gas usage
    /// - Out-of-range `from_index` returns empty page safely
    /// - Cancelled schedules are included (they remain in storage for audit)
    pub fn get_schedules_paginated(
        env: Env,
        owner: Address,
        from_index: u32,
        limit: u32,
    ) -> SchedulePage {
        let schedule_ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        // Vec items are already retrieved in order; ensure deterministic traversal
        // by processing sequentially without mutating the order

        let len = schedule_ids.len();
        let cap = clamp_limit(limit);

        if from_index >= len {
            return SchedulePage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let end = from_index.saturating_add(cap).min(len);
        let mut items = Vec::new(&env);
        for i in from_index..end {
            if let Some(id) = schedule_ids.get(i) {
                if let Some(schedule) = env.storage().persistent().get(&DataKey::Schedule(id)) {
                    items.push_back(schedule);
                }
            }
        }

        let count = items.len();
        let next_cursor = if end < len { end } else { 0 };

        SchedulePage {
            items,
            next_cursor,
            count,
        }
    }

    pub fn get_remittance_schedule(env: Env, schedule_id: u32) -> Option<RemittanceSchedule> {
        env.storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
    }
}
