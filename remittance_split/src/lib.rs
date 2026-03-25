#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
mod test;
// test module declared above

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
}

/// Snapshot for data export/import (migration). Checksum is a simple numeric digest for on-chain verification.
#[contracttype]
#[derive(Clone)]
pub struct ExportSnapshot {
    pub version: u32,
    pub checksum: u64,
    pub config: SplitConfig,
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

const SNAPSHOT_VERSION: u32 = 1;
const MAX_AUDIT_ENTRIES: u32 = 100;
const CONTRACT_VERSION: u32 = 1;

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
        if config.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);
        Ok(())
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
    /// * `spending_percent` - Percentage for spending (0-100)
    /// * `savings_percent` - Percentage for savings (0-100)
    /// * `bills_percent` - Percentage for bills (0-100)
    /// * `insurance_percent` - Percentage for insurance (0-100)
    ///
    /// # Returns
    /// True if initialization was successful
    ///
    /// # Panics
    /// - If owner doesn't authorize the transaction
    /// - If nonce is invalid (replay)
    /// - If percentages don't sum to 100
    /// - If split is already initialized (use update_split instead)
    pub fn initialize_split(
        env: Env,
        owner: Address,
        nonce: u64,
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

    pub fn distribute_usdc(
        env: Env,
        usdc_contract: Address,
        from: Address,
        nonce: u64,
        accounts: AccountGroup,
        total_amount: i128,
    ) -> Result<bool, RemittanceSplitError> {
        if total_amount <= 0 {
            Self::append_audit(&env, symbol_short!("distrib"), &from, false);
            return Err(RemittanceSplitError::InvalidAmount);
        }

        from.require_auth();
        Self::require_nonce(&env, &from, nonce)?;

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

        Self::increment_nonce(&env, &from)?;
        Self::append_audit(&env, symbol_short!("distrib"), &from, true);
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
        let checksum = Self::compute_checksum(SNAPSHOT_VERSION, &config);
        Ok(Some(ExportSnapshot {
            version: SNAPSHOT_VERSION,
            checksum,
            config,
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

        if snapshot.version != SNAPSHOT_VERSION {
            Self::append_audit(&env, symbol_short!("import"), &caller, false);
            return Err(RemittanceSplitError::UnsupportedVersion);
        }
        let expected = Self::compute_checksum(snapshot.version, &snapshot.config);
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

    fn compute_checksum(version: u32, config: &SplitConfig) -> u64 {
        let v = version as u64;
        let s = config.spending_percent as u64;
        let g = config.savings_percent as u64;
        let b = config.bills_percent as u64;
        let i = config.insurance_percent as u64;
        v.wrapping_add(s)
            .wrapping_add(g)
            .wrapping_add(b)
            .wrapping_add(i)
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

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, RemittanceSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("REM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

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

        schedules.set(next_schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("REM_SCH"), &schedules);
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

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, RemittanceSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("REM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

        if schedule.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        schedule.amount = amount;
        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("REM_SCH"), &schedules);

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

        Self::extend_instance_ttl(&env);

        let mut schedules: Map<u32, RemittanceSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("REM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(RemittanceSplitError::ScheduleNotFound)?;

        if schedule.owner != caller {
            return Err(RemittanceSplitError::Unauthorized);
        }

        schedule.active = false;

        schedules.set(schedule_id, schedule);
        env.storage()
            .instance()
            .set(&symbol_short!("REM_SCH"), &schedules);

        env.events().publish(
            (symbol_short!("schedule"), ScheduleEvent::Cancelled),
            (schedule_id, caller),
        );

        Ok(true)
    }

    pub fn get_remittance_schedules(env: Env, owner: Address) -> Vec<RemittanceSchedule> {
        let schedules: Map<u32, RemittanceSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("REM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        let mut result = Vec::new(&env);
        for (_, schedule) in schedules.iter() {
            if schedule.owner == owner {
                result.push_back(schedule);
            }
        }
        result
    }

    pub fn get_remittance_schedule(env: Env, schedule_id: u32) -> Option<RemittanceSchedule> {
        let schedules: Map<u32, RemittanceSchedule> = env
            .storage()
            .instance()
            .get(&symbol_short!("REM_SCH"))
            .unwrap_or_else(|| Map::new(&env));

        schedules.get(schedule_id)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::storage::Instance as _;
    use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
    use soroban_sdk::TryFromVal;

    #[test]
    fn test_initialize_split_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Initialize split
        let result = client.initialize_split(&owner, &0, &50, &30, &15, &5);
        assert!(result);

        // Verify event was emitted
        let events = env.events().all();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_calculate_split_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Initialize split first
        client.initialize_split(&owner, &0, &40, &30, &20, &10);

        // Get events before calculating
        let events_before = env.events().all().len();

        // Calculate split
        let result = client.calculate_split(&1000);
        assert_eq!(result.len(), 4);
        assert_eq!(result.get(0).unwrap(), 400); // 40% of 1000
        assert_eq!(result.get(1).unwrap(), 300); // 30% of 1000
        assert_eq!(result.get(2).unwrap(), 200); // 20% of 1000
        assert_eq!(result.get(3).unwrap(), 100); // 10% of 1000

        // Verify 2 new events were emitted (SplitCalculated + audit event)
        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_multiple_operations_emit_multiple_events() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Initialize split
        client.initialize_split(&owner, &0, &50, &25, &15, &10);

        // Calculate split twice
        client.calculate_split(&2000);
        client.calculate_split(&3000);

        // Should have 5 events total (1 init + 2*2 calc)
        let events = env.events().all();
        assert_eq!(events.len(), 5);
    }

    // ====================================================================
    // Storage TTL Extension Tests
    //
    // Verify that instance storage TTL is properly extended on
    // state-changing operations, preventing unexpected data expiration.
    //
    // Contract TTL configuration:
    //   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
    //   INSTANCE_BUMP_AMOUNT        = 518,400 ledgers (~30 days)
    //
    // Operations extending instance TTL:
    //   initialize_split, update_split, import_snapshot,
    //   create_remittance_schedule, modify_remittance_schedule,
    //   cancel_remittance_schedule
    // ====================================================================

    /// Verify that initialize_split extends instance storage TTL.
    #[test]
    fn test_instance_ttl_extended_on_initialize_split() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // initialize_split calls extend_instance_ttl
        let result = client.initialize_split(&owner, &0, &50, &30, &15, &5);
        assert!(result);

        // Inspect instance TTL — must be at least INSTANCE_BUMP_AMOUNT
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= INSTANCE_BUMP_AMOUNT (518,400) after initialize_split",
            ttl
        );
    }

    /// Verify that update_split refreshes instance TTL after ledger advancement.
    ///
    /// extend_ttl(threshold, extend_to) only extends when TTL <= threshold.
    /// We advance the ledger far enough for TTL to drop below 17,280.
    #[test]
    fn test_instance_ttl_refreshed_on_update_split() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // Advance ledger so TTL drops below threshold (17,280)
        // After init: live_until = 518,500. At seq 510,000: TTL = 8,500
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 500_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // update_split calls extend_instance_ttl → re-extends TTL to 518,400
        let result = client.update_split(&owner, &1, &40, &30, &20, &10);
        assert!(result);

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl >= 518_400,
            "Instance TTL ({}) must be >= 518,400 after update_split",
            ttl
        );
    }

    /// Verify data persists across repeated operations spanning multiple
    /// ledger advancements, proving TTL is continuously renewed.
    #[test]
    fn test_split_data_persists_across_ledger_advancements() {
        let env = Env::default();
        env.mock_all_auths();

        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 100,
            timestamp: 1000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Phase 1: Initialize at seq 100. live_until = 518,500
        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // Phase 2: Advance to seq 510,000 (TTL = 8,500 < 17,280)
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 510_000,
            timestamp: 510_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        client.update_split(&owner, &1, &40, &25, &20, &15);

        // Phase 3: Advance to seq 1,020,000 (TTL = 8,400 < 17,280)
        env.ledger().set(LedgerInfo {
            protocol_version: 20,
            sequence_number: 1_020_000,
            timestamp: 1_020_000,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 100,
            min_persistent_entry_ttl: 100,
            max_entry_ttl: 700_000,
        });

        // Calculate split to exercise read path
        let result = client.calculate_split(&1000);
        assert_eq!(result.len(), 4);

        // Config should be accessible with updated values
        let config = client.get_config();
        assert!(
            config.is_some(),
            "Config must persist across ledger advancements"
        );
        let config = config.unwrap();
        assert_eq!(config.spending_percent, 40);
        assert_eq!(config.savings_percent, 25);

        // TTL is still valid (within the second extension window)
        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert!(
            ttl > 0,
            "Instance TTL ({}) must be > 0 — data is still live",
            ttl
        );
    }

    // ============================================================================
    // Issue #60 – Full Test Suite for Remittance Split Contract
    // ============================================================================

    /// 1. test_initialize_split_success
    /// Owner authorizes the call, percentages sum to 100, config is stored correctly.
    #[test]
    fn test_initialize_split_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        let result = client.initialize_split(&owner, &0, &50, &30, &15, &5);
        assert!(result, "initialize_split should return true on success");

        let config = client
            .get_config()
            .expect("config should be stored after init");
        assert_eq!(config.owner, owner);
        assert_eq!(config.spending_percent, 50);
        assert_eq!(config.savings_percent, 30);
        assert_eq!(config.bills_percent, 15);
        assert_eq!(config.insurance_percent, 5);
        assert!(config.initialized);
    }

    /// 2. test_initialize_split_requires_auth
    /// Calling initialize_split without the owner authorizing should panic.
    #[test]
    #[should_panic]
    fn test_initialize_split_requires_auth() {
        let env = Env::default();
        // Intentionally NOT calling env.mock_all_auths()
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Should panic because owner has not authorized
        client.initialize_split(&owner, &0, &50, &30, &15, &5);
    }

    /// 3. test_initialize_split_percentages_must_sum_to_100
    /// Percentages that do not sum to 100 must return PercentagesDoNotSumTo100.
    #[test]
    fn test_initialize_split_percentages_must_sum_to_100() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // 40 + 30 + 15 + 5 = 90, not 100
        let result = client.try_initialize_split(&owner, &0, &40, &30, &15, &5);
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100))
        );

        // 50 + 50 + 10 + 0 = 110, not 100
        let result2 = client.try_initialize_split(&owner, &0, &50, &50, &10, &0);
        assert_eq!(
            result2,
            Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100))
        );
    }

    /// 4. test_initialize_split_already_initialized_panics
    /// Calling initialize_split a second time should return AlreadyInitialized.
    #[test]
    fn test_initialize_split_already_initialized_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // First init succeeds
        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // Second init must fail with AlreadyInitialized
        let result = client.try_initialize_split(&owner, &1, &50, &30, &15, &5);
        assert_eq!(result, Err(Ok(RemittanceSplitError::AlreadyInitialized)));
    }

    /// 5. test_update_split_owner_only
    /// Only the owner can call update_split; any other address must get Unauthorized.
    #[test]
    fn test_update_split_owner_only() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // other address is not the owner — must fail
        let result = client.try_update_split(&other, &0, &40, &40, &10, &10);
        assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));

        // owner can update just fine
        let ok = client.update_split(&owner, &1, &40, &40, &10, &10);
        assert!(ok);
    }

    /// 6. test_update_split_percentages_must_sum_to_100
    /// update_split must reject percentages that do not sum to 100.
    #[test]
    fn test_update_split_percentages_must_sum_to_100() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // 60 + 30 + 15 + 5 = 110 — invalid
        let result = client.try_update_split(&owner, &1, &60, &30, &15, &5);
        assert_eq!(
            result,
            Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100))
        );

        // 10 + 10 + 10 + 10 = 40 — invalid
        let result2 = client.try_update_split(&owner, &1, &10, &10, &10, &10);
        assert_eq!(
            result2,
            Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100))
        );
    }

    /// 7. test_get_split_returns_default_before_init
    /// Before initialize_split is called, get_split must return the hardcoded
    /// default of [50, 30, 15, 5].
    #[test]
    fn test_get_split_returns_default_before_init() {
        let env = Env::default();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let split = client.get_split();
        assert_eq!(split.len(), 4);
        assert_eq!(split.get(0).unwrap(), 50);
        assert_eq!(split.get(1).unwrap(), 30);
        assert_eq!(split.get(2).unwrap(), 15);
        assert_eq!(split.get(3).unwrap(), 5);
    }

    /// 8. test_get_config_returns_none_before_init
    /// Before initialize_split is called, get_config must return None.
    #[test]
    fn test_get_config_returns_none_before_init() {
        let env = Env::default();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);

        let config = client.get_config();
        assert!(config.is_none(), "get_config should be None before init");
    }

    /// 9. test_get_config_returns_some_after_init
    /// After initialize_split, get_config must return Some with correct owner.
    #[test]
    fn test_get_config_returns_some_after_init() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        let config = client.get_config();
        assert!(config.is_some(), "get_config should be Some after init");

        let config = config.unwrap();
        assert_eq!(
            config.owner, owner,
            "config owner must match the initializer"
        );
        assert_eq!(config.spending_percent, 50);
        assert_eq!(config.savings_percent, 30);
        assert_eq!(config.bills_percent, 15);
        assert_eq!(config.insurance_percent, 5);
    }

    /// 10. test_calculate_split_positive_amount
    /// Correct amounts for a positive total; insurance receives the remainder.
    #[test]
    fn test_calculate_split_positive_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // 50 / 30 / 15 / 5
        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        let amounts = client.calculate_split(&1000);
        assert_eq!(amounts.len(), 4);
        // spending: 50% of 1000 = 500
        assert_eq!(amounts.get(0).unwrap(), 500);
        // savings: 30% of 1000 = 300
        assert_eq!(amounts.get(1).unwrap(), 300);
        // bills: 15% of 1000 = 150
        assert_eq!(amounts.get(2).unwrap(), 150);
        // insurance: remainder = 1000 - 500 - 300 - 150 = 50
        assert_eq!(amounts.get(3).unwrap(), 50);
    }

    /// 11. test_calculate_split_zero_or_negative_panics
    /// total_amount of 0 or any negative value must return InvalidAmount.
    #[test]
    fn test_calculate_split_zero_or_negative_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        // Zero
        let result_zero = client.try_calculate_split(&0);
        assert_eq!(result_zero, Err(Ok(RemittanceSplitError::InvalidAmount)));

        // Negative
        let result_neg = client.try_calculate_split(&-1);
        assert_eq!(result_neg, Err(Ok(RemittanceSplitError::InvalidAmount)));

        // Large negative
        let result_large_neg = client.try_calculate_split(&-9999);
        assert_eq!(
            result_large_neg,
            Err(Ok(RemittanceSplitError::InvalidAmount))
        );
    }

    /// 12. test_calculate_split_rounding
    /// The sum of all split amounts must always equal total_amount exactly
    /// (insurance absorbs any integer division remainder).
    #[test]
    fn test_calculate_split_rounding() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Use percentages that cause integer division remainders: 33/33/33/1
        client.initialize_split(&owner, &0, &33, &33, &33, &1);

        // total = 100: 33+33+33 = 99, insurance gets remainder = 1
        let amounts = client.calculate_split(&100);
        let sum: i128 = amounts.iter().sum();
        assert_eq!(sum, 100, "split amounts must sum to total_amount");

        // total = 7: each of 33% = 2 (floor), remainder = 7 - 2 - 2 - 2 = 1
        let amounts2 = client.calculate_split(&7);
        let sum2: i128 = amounts2.iter().sum();
        assert_eq!(sum2, 7, "split amounts must sum to total_amount");

        // total = 1000
        let amounts3 = client.calculate_split(&1000);
        let sum3: i128 = amounts3.iter().sum();
        assert_eq!(sum3, 1000, "split amounts must sum to total_amount");
    }

    /// 13. test_event_emitted_on_initialize_and_update
    /// Events must be published when initialize_split and update_split are called.
    #[test]
    fn test_event_emitted_on_initialize_and_update() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, RemittanceSplit);
        let client = RemittanceSplitClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // --- initialize_split event ---
        client.initialize_split(&owner, &0, &50, &30, &15, &5);

        let events_after_init = env.events().all();
        assert!(
            !events_after_init.is_empty(),
            "at least one event should be emitted on initialize_split"
        );

        // The last event topic should be (symbol_short!("split"), SplitEvent::Initialized)
        let init_event = events_after_init.last().unwrap();
        let topic0: Symbol = Symbol::try_from_val(&env, &init_event.1.get(0).unwrap()).unwrap();
        let topic1: SplitEvent =
            SplitEvent::try_from_val(&env, &init_event.1.get(1).unwrap()).unwrap();
        assert_eq!(topic0, symbol_short!("split"));
        assert_eq!(topic1, SplitEvent::Initialized);

        // --- update_split event ---
        client.update_split(&owner, &1, &40, &40, &10, &10);

        let events_after_update = env.events().all();
        let update_event = events_after_update.last().unwrap();
        let upd_topic0: Symbol =
            Symbol::try_from_val(&env, &update_event.1.get(0).unwrap()).unwrap();
        let upd_topic1: SplitEvent =
            SplitEvent::try_from_val(&env, &update_event.1.get(1).unwrap()).unwrap();
        assert_eq!(upd_topic0, symbol_short!("split"));
        assert_eq!(upd_topic1, SplitEvent::Updated);
    }
}
