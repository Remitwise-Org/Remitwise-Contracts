#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token::TokenClient, vec,
    Address, Env, Map, Symbol, Vec,
};

// Event topics
const SPLIT_INITIALIZED: Symbol = symbol_short!("init");
const SPLIT_CALCULATED: Symbol = symbol_short!("calc");

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
    NotInitialized = 2,
    PercentagesDoNotSumTo100 = 3,
    InvalidAmount = 4,
    Overflow = 5,
    Unauthorized = 6,
    InvalidNonce = 7,
    UnsupportedVersion = 8,
    ChecksumMismatch = 9,
    InvalidDueDate = 10,
    ScheduleNotFound = 11,
    /// The supplied token contract address does not match the trusted USDC contract.
    UntrustedTokenContract = 12,
    /// A destination account is the same as the sender, which would be a no-op transfer.
    SelfTransferNotAllowed = 13,
}

#[derive(Clone)]
#[contracttype]
pub struct Allocation {
    pub category: Symbol,
    pub amount: i128,
}

#[derive(Clone)]
#[contracttype]
pub struct AccountGroup {
    pub spending: Address,
    pub savings: Address,
    pub bills: Address,
    pub insurance: Address,
}

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

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

/// Events emitted by the contract for audit trail
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SplitEvent {
    Initialized,
    Updated,
    Calculated,
    /// Emitted when distribute_usdc successfully completes all transfers.
    DistributionCompleted,
}

/// Snapshot for data export/import (migration).
///
/// # Schema Version Tag
/// `schema_version` carries the explicit snapshot format version.
/// Importers **must** validate this field against the supported range
/// (`MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION`) before applying the
/// snapshot. Snapshots with an unknown future version must be rejected to
/// guarantee forward/backward compatibility.
/// `checksum` is a simple numeric digest for on-chain integrity verification.
#[contracttype]
#[derive(Clone)]
pub struct ExportSnapshot {
    /// Explicit schema version tag for this snapshot format.
    /// Supported range: MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION.
    pub schema_version: u32,
    pub checksum: u64,
    pub config: SplitConfig,
    pub schedules: Vec<RemittanceSchedule>,
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

/// Current snapshot schema version. Bump this when the ExportSnapshot format changes.
const SCHEMA_VERSION: u32 = 1;
/// Oldest snapshot schema version this contract can import. Enables backward compat.
const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;
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

    pub fn set_pause_admin(
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
        if config.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }
    pub fn pause(env: Env, caller: Address) -> Result<(), RemittanceSplitError> {
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
            .set(&symbol_short!("PAUSED"), &true);
        env.events()
            .publish((symbol_short!("split"), symbol_short!("paused")), ());
        Ok(())
    }
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
        env.events()
            .publish((symbol_short!("split"), symbol_short!("unpaused")), ());
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
                // Initial admin setup - only owner can set
                if config.owner != caller {
                    return Err(RemittanceSplitError::Unauthorized);
                }
            }
            Some(current_admin) => {
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
            (current_upgrade_admin, new_admin.clone()),
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
    pub fn set_version(
        env: Env,
        caller: Address,
        new_version: u32,
    ) -> Result<(), RemittanceSplitError> {
        caller.require_auth();
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
        env.events().publish(
            (symbol_short!("split"), symbol_short!("upgraded")),
            (prev, new_version),
        );
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
        owner.require_auth();
        Self::require_not_paused(&env)?;
        Self::require_nonce(&env, &owner, nonce)?;

        let existing: Option<SplitConfig> = env.storage().instance().get(&symbol_short!("CONFIG"));
        if existing.is_some() {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            return Err(RemittanceSplitError::AlreadyInitialized);
        }

        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("init"), &owner, false);
            return Err(RemittanceSplitError::PercentagesDoNotSumTo100);
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
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Initialized), owner);

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

        let total = spending_percent + savings_percent + bills_percent + insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("update"), &caller, false);
            return Err(RemittanceSplitError::PercentagesDoNotSumTo100);
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
        env.events()
            .publish((symbol_short!("split"), SplitEvent::Updated), caller);

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
        let s0 = split.get(0).unwrap() as i128;
        let s1 = split.get(1).unwrap() as i128;
        let s2 = split.get(2).unwrap() as i128;

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
        env.events().publish((SPLIT_CALCULATED,), event);
        env.events().publish(
            (symbol_short!("split"), SplitEvent::Calculated),
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
        Self::require_nonce(&env, &from, nonce)?;

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

        // 10. Advance nonce, record audit, emit event.
        Self::increment_nonce(&env, &from)?;
        Self::append_audit(&env, symbol_short!("distrib"), &from, true);
        env.events().publish(
            (symbol_short!("split"), SplitEvent::DistributionCompleted),
            (from, total_amount),
        );

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
        }))
    }

    pub fn import_snapshot(
        env: Env,
        caller: Address,
        nonce: u64,
        snapshot: ExportSnapshot,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();
        Self::require_nonce(&env, &caller, nonce)?;

        // Accept any schema_version within the supported range for backward/forward compat.
        if snapshot.schema_version < MIN_SUPPORTED_SCHEMA_VERSION
            || snapshot.schema_version > SCHEMA_VERSION
        {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::UnsupportedVersion);
        }
        let expected = Self::compute_checksum(snapshot.schema_version, &snapshot.config, &snapshot.schedules);
        if snapshot.checksum != expected {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::ChecksumMismatch);
        }

        let existing: SplitConfig = env
            .storage()
            .instance()
            .get(&symbol_short!("CONFIG"))
            .ok_or(RemittanceSplitError::NotInitialized)?;
        if existing.owner != caller {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::Unauthorized);
        }

        let total = snapshot.config.spending_percent
            + snapshot.config.savings_percent
            + snapshot.config.bills_percent
            + snapshot.config.insurance_percent;
        if total != 100 {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::PercentagesDoNotSumTo100);
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
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::Schedule(schedule.id), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        }

        // Reconstruct owner index
        let mut owner_ids = Vec::new(&env);
        for schedule in snapshot.schedules.iter() {
            owner_ids.push_back(schedule.id);
        }
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedules(caller.clone()), &owner_ids);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OwnerSchedules(caller.clone()), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        Self::increment_nonce(&env, &caller)?;
        Self::append_audit(&env, symbol_short!("import"), &caller, true);
        Ok(true)
    }

    pub fn get_audit_log(env: Env, from_index: u32, limit: u32) -> Vec<AuditEntry> {
        let log: Option<Vec<AuditEntry>> = env.storage().instance().get(&symbol_short!("AUDIT"));
        let log = log.unwrap_or_else(|| Vec::new(&env));
        let len = log.len();
        let cap = MAX_AUDIT_ENTRIES.min(limit);
        let mut out = Vec::new(&env);
        if from_index >= len {
            return out;
        }
        let end = (from_index + cap).min(len);
        for i in from_index..end {
            if let Some(entry) = log.get(i) {
                out.push_back(entry);
            }
        }
        out
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

    fn increment_nonce(env: &Env, address: &Address) -> Result<(), RemittanceSplitError> {
        let current = Self::get_nonce_value(env, address);
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

    fn compute_checksum(version: u32, config: &SplitConfig, schedules: &Vec<RemittanceSchedule>) -> u64 {
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
            env.events().publish((SPLIT_CALCULATED,), event);
            env.events().publish(
                (symbol_short!("split"), SplitEvent::Calculated),
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

    pub fn create_remittance_schedule(
        env: Env,
        owner: Address,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, RemittanceSplitError> {
        owner.require_auth();

        if amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(RemittanceSplitError::InvalidDueDate);
        }

        let next_schedule_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_RSCH"))
            .unwrap_or(0u32)
            + 1;

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
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Schedule(next_schedule_id), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        // 2. Update owner's schedule index
        let mut owner_schedules: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerSchedules(owner.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        owner_schedules.push_back(next_schedule_id);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSchedules(owner.clone()), &owner_schedules);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::OwnerSchedules(owner.clone()), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_RSCH"), &next_schedule_id);

        env.events().publish(
            (symbol_short!("schedule"), ScheduleEvent::Created),
            (next_schedule_id, owner),
        );

        Ok(next_schedule_id)
    }

    pub fn modify_remittance_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();

        if amount <= 0 {
            return Err(RemittanceSplitError::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(RemittanceSplitError::InvalidDueDate);
        }

        let mut schedule: RemittanceSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

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
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Schedule(schedule_id), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        env.events().publish(
            (symbol_short!("schedule"), ScheduleEvent::Modified),
            (schedule_id, caller),
        );

        Ok(true)
    }

    pub fn cancel_remittance_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, RemittanceSplitError> {
        caller.require_auth();

        let mut schedule: RemittanceSchedule = env
            .storage()
            .persistent()
            .get(&DataKey::Schedule(schedule_id))
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

        if schedule.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        schedule.active = false;

        env.storage()
            .persistent()
            .set(&DataKey::Schedule(schedule_id), &schedule);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::Schedule(schedule_id), INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        env.events().publish(
            (symbol_short!("schedule"), ScheduleEvent::Cancelled),
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

        let mut result = Vec::new(&env);
        for id in schedule_ids.iter() {
            if let Some(schedule) = env.storage().persistent().get(&DataKey::Schedule(id)) {
                result.push_back(schedule);
            }
        }
        result
    }

    pub fn get_remittance_schedule(env: Env, schedule_id: u32) -> Option<RemittanceSchedule> {
        env.storage().persistent().get(&DataKey::Schedule(schedule_id))
    }
}

#[cfg(test)]
mod test;
