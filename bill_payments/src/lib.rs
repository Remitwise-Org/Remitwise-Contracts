#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::{
    clamp_limit, EventCategory, EventPriority, RemitwiseEvents, ARCHIVE_BUMP_AMOUNT,
    ARCHIVE_LIFETIME_THRESHOLD, CONTRACT_VERSION, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD, MAX_BATCH_SIZE,
};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

const MAX_FREQUENCY_DAYS: u32 = 36_500; // 100 years
const SECONDS_PER_DAY: u64 = 86_400;

/// Maximum length for currency codes (ISO 4217 is 3 letters)
const MAX_CURRENCY_LEN: u32 = 10;

/// Minimum length for external reference strings
const MIN_EXTERNAL_REF_LEN: u32 = 1;
/// Maximum length for external reference strings
const MAX_EXTERNAL_REF_LEN: u32 = 64;

/// Validates that a currency string contains only ASCII alphabetic characters.
/// Returns true if the string is valid (all ASCII letters A-Z or a-z).
fn is_valid_currency_chars(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&b| b.is_ascii_alphabetic())
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
    pub schedule_id: Option<u32>,
    pub tags: Vec<String>,
    /// Intended currency/asset for this bill (e.g. "XLM", "USDC", "NGN").
    /// Defaults to "XLM" for entries created before this field was introduced.
    pub currency: String,
}

/// Paginated result for bill queries
#[contracttype]
#[derive(Clone)]
pub struct BillPage {
    /// The bills for this page
    pub items: Vec<Bill>,
    /// The ID to pass as `cursor` for the next page. 0 means no more pages.
    pub next_cursor: u32,
    /// Total items returned in this page
    pub count: u32,
}

pub mod pause_functions {
    use soroban_sdk::symbol_short;
    pub const CREATE_BILL: soroban_sdk::Symbol = symbol_short!("crt_bill");
    pub const PAY_BILL: soroban_sdk::Symbol = symbol_short!("pay_bill");
    pub const CANCEL_BILL: soroban_sdk::Symbol = symbol_short!("can_bill");
    pub const ARCHIVE: soroban_sdk::Symbol = symbol_short!("archive");
    pub const RESTORE: soroban_sdk::Symbol = symbol_short!("restore");
}

const STORAGE_UNPAID_TOTALS: Symbol = symbol_short!("UNPD_TOT");
const STORAGE_EXT_REF_IDX: Symbol = symbol_short!("EXTRIDX");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BillPaymentsError {
    /// Bill with the given ID does not exist
    BillNotFound = 1,
    /// Bill has already been paid
    BillAlreadyPaid = 2,
    /// Amount is zero or negative
    InvalidAmount = 3,
    /// Recurring frequency is invalid
    InvalidFrequency = 4,
    /// Caller is not authorized for this operation
    Unauthorized = 5,
    /// The entire contract is paused
    ContractPaused = 6,
    /// Caller is not authorized to pause/unpause
    UnauthorizedPause = 7,
    /// This specific function is paused
    FunctionPaused = 8,
    /// Batch exceeds maximum allowed size
    BatchTooLarge = 9,
    /// One or more bills in the batch failed validation
    BatchValidationFailed = 10,
    /// Pagination limit is out of allowed range
    InvalidLimit = 11,
    /// Due date is in the past or otherwise invalid
    InvalidDueDate = 12,
    /// Tag string is invalid (empty or too long)
    InvalidTag = 13,
    /// Tags list is empty
    EmptyTags = 14,
    /// Currency code is invalid (empty, too long, or contains non-alphanumeric)
    InvalidCurrency = 15,
    /// External reference is invalid (empty, too long, or contains disallowed chars)
    InvalidExternalRef = 16,
    /// External reference already used by another active bill for this owner
    DuplicateExternalRef = 17,
}

// Back-compat alias: large parts of this crate (and tests) still refer to `Error`.
pub type Error = BillPaymentsError;

#[contracttype]
#[derive(Clone)]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub amount: i128,
    pub paid_at: u64,
    pub archived_at: u64,
    pub tags: Vec<String>,
    /// Intended currency/asset carried over from the originating `Bill`.
    pub currency: String,
}

/// Paginated result for archived bill queries
#[contracttype]
#[derive(Clone)]
pub struct ArchivedBillPage {
    pub items: Vec<ArchivedBill>,
    /// 0 means no more pages
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub enum BillEvent {
    Created,
    Paid,
    ExternalRefUpdated,
    Cancelled,
    Archived,
    Restored,
    ScheduleCreated,
    ScheduleExecuted,
    ScheduleMissed,
    ScheduleModified,
    ScheduleCancelled,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct StorageStats {
    pub active_bills: u32,
    pub archived_bills: u32,
    pub total_unpaid_amount: i128,
    pub total_archived_amount: i128,
    pub last_updated: u64,
}

#[contract]
pub struct BillPayments;

#[contractimpl]
impl BillPayments {
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Validate and normalize a currency string for consistent storage and comparison.
    ///
    /// # Arguments
    /// * `env` - The Soroban environment
    /// * `currency` - Currency code string to validate and normalize
    ///
    /// # Returns
    /// `Ok(normalized_currency)` on success with:
    /// 1. Empty strings default to "XLM"
    /// 2. Whitespace trimmed
    /// 3. Converted to uppercase
    ///
    /// # Errors
    /// * `InvalidCurrency` - If currency is too long or contains non-alphanumeric characters
    fn validate_and_normalize_currency(
        env: &Env,
        currency: &String,
    ) -> Result<String, BillPaymentsError> {
        let len = currency.len();

        // Empty string defaults to "XLM"
        if len == 0 {
            return Ok(String::from_str(env, "XLM"));
        }

        // Check length constraint
        if len > MAX_CURRENCY_LEN {
            return Err(BillPaymentsError::InvalidCurrency);
        }

        let mut buf = [0u8; 32];
        let copy_len = (len as usize).min(buf.len());
        currency.copy_into_slice(&mut buf[..copy_len]);
        let s = &buf[..copy_len];

        // Trim leading/trailing ASCII spaces
        let start = s.iter().position(|&b| b != b' ').unwrap_or(copy_len);
        let end = s
            .iter()
            .rposition(|&b| b != b' ')
            .map(|i| i + 1)
            .unwrap_or(0);

        if start >= end {
            // Only whitespace - default to XLM
            return Ok(String::from_str(env, "XLM"));
        }

        let trimmed = &s[start..end];

        // Validate: must be only ASCII alphabetic characters (A-Z or a-z)
        if !is_valid_currency_chars(trimmed) {
            return Err(BillPaymentsError::InvalidCurrency);
        }

        // Uppercase the validated string
        let mut upper = [0u8; 32];
        for (i, &b) in trimmed.iter().enumerate() {
            upper[i] = b.to_ascii_uppercase();
        }

        let upper_str = core::str::from_utf8(&upper[..trimmed.len()]).unwrap_or("XLM");
        Ok(String::from_str(env, upper_str))
    }

    /// Legacy helper for backward compatibility - normalizes without strict validation.
    /// WARNING: This does not validate currency codes. Use validate_and_normalize_currency
    /// for new code to ensure proper currency validation.
    fn normalize_currency(env: &Env, currency: &String) -> String {
        // For backward compatibility, try validation first, fall back on error
        match Self::validate_and_normalize_currency(env, currency) {
            Ok(normalized) => normalized,
            Err(_) => String::from_str(env, "XLM"),
        }
    }

    // -----------------------------------------------------------------------
    // external_ref validation & per-owner uniqueness index
    // -----------------------------------------------------------------------

    /// Validate an `external_ref` string.
    ///
    /// Allowed characters: ASCII alphanumeric, hyphens, underscores, dots, colons.
    /// Length must be within `[MIN_EXTERNAL_REF_LEN, MAX_EXTERNAL_REF_LEN]`.
    fn validate_external_ref(
        env: &Env,
        ext_ref: &String,
    ) -> Result<String, BillPaymentsError> {
        let len = ext_ref.len();
        if len < MIN_EXTERNAL_REF_LEN || len > MAX_EXTERNAL_REF_LEN {
            return Err(BillPaymentsError::InvalidExternalRef);
        }

        let mut buf = [0u8; 64];
        let copy_len = (len as usize).min(buf.len());
        ext_ref.copy_into_slice(&mut buf[..copy_len]);
        let s = &buf[..copy_len];

        for &b in s {
            if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':') {
                return Err(BillPaymentsError::InvalidExternalRef);
            }
        }

        // Return as-is (case-sensitive for reconciliation fidelity)
        Ok(ext_ref.clone())
    }

    /// Optionally validate an external_ref. `None` passes through.
    fn validate_optional_external_ref(
        env: &Env,
        ext_ref: &Option<String>,
    ) -> Result<Option<String>, BillPaymentsError> {
        match ext_ref {
            None => Ok(None),
            Some(r) => Ok(Some(Self::validate_external_ref(env, r)?)),
        }
    }

    /// Load the owner-scoped external_ref index: `Map<Address, Map<String, u32>>`
    fn get_ext_ref_index(env: &Env) -> Map<Address, Map<String, u32>> {
        env.storage()
            .instance()
            .get(&STORAGE_EXT_REF_IDX)
            .unwrap_or_else(|| Map::new(env))
    }

    fn save_ext_ref_index(env: &Env, idx: &Map<Address, Map<String, u32>>) {
        env.storage()
            .instance()
            .set(&STORAGE_EXT_REF_IDX, idx);
    }

    /// Claim `ext_ref` for `owner` → `bill_id`. Fails if already claimed by another bill.
    fn claim_external_ref(
        env: &Env,
        owner: &Address,
        ext_ref: &String,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        let mut idx = Self::get_ext_ref_index(env);
        let mut owner_map: Map<String, u32> = idx
            .get(owner.clone())
            .unwrap_or_else(|| Map::new(env));

        if let Some(existing_id) = owner_map.get(ext_ref.clone()) {
            if existing_id != bill_id {
                return Err(BillPaymentsError::DuplicateExternalRef);
            }
            // Same bill re-claiming its own ref — no-op
            return Ok(());
        }

        owner_map.set(ext_ref.clone(), bill_id);
        idx.set(owner.clone(), owner_map);
        Self::save_ext_ref_index(env, &idx);
        Ok(())
    }

    /// Release a previously claimed `ext_ref` for `owner`.
    fn release_external_ref(env: &Env, owner: &Address, ext_ref: &String) {
        let mut idx = Self::get_ext_ref_index(env);
        if let Some(mut owner_map) = idx.get(owner.clone()) {
            owner_map.remove(ext_ref.clone());
            idx.set(owner.clone(), owner_map);
            Self::save_ext_ref_index(env, &idx);
        }
    }

    fn get_pause_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
    }
    fn get_next_bill_id(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
    }
    fn get_global_paused(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("PAUSED"))
            .unwrap_or(false)
    }
    fn is_function_paused(env: &Env, func: Symbol) -> bool {
        env.storage()
            .instance()
            .get::<_, Map<Symbol, bool>>(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(env))
            .get(func)
            .unwrap_or(false)
    }
    fn require_not_paused(env: &Env, func: Symbol) -> Result<(), BillPaymentsError> {
        if Self::get_global_paused(env) {
            return Err(BillPaymentsError::ContractPaused);
        }
        if Self::is_function_paused(env, func) {
            return Err(BillPaymentsError::FunctionPaused);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Pause / upgrade
    // -----------------------------------------------------------------------

    pub fn set_pause_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        let current = Self::get_pause_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    return Err(BillPaymentsError::UnauthorizedPause);
                }
            }
            Some(admin) if admin != caller => return Err(BillPaymentsError::UnauthorizedPause),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }

    /// @notice Pause all state-changing operations.
    /// @dev Requires the pause admin to authenticate.
    /// @return Ok(()) on success, otherwise `Error::UnauthorizedPause`.
    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
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

    /// @notice Unpause the contract if no time-lock is active.
    /// @dev If `schedule_unpause` set a future timestamp, unpause is blocked until then.
    /// @return Ok(()) on success, otherwise `Error::ContractPaused` or `Error::UnauthorizedPause`.
    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        let unpause_at: Option<u64> = env.storage().instance().get(&symbol_short!("UNP_AT"));
        if let Some(at) = unpause_at {
            if env.ledger().timestamp() < at {
                return Err(BillPaymentsError::ContractPaused);
            }
            env.storage().instance().remove(&symbol_short!("UNP_AT"));
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

    /// @notice Schedule the earliest time the contract may be unpaused.
    /// @dev Time-locks unpause to a future `at_timestamp` (ledger timestamp seconds).
    /// @return Ok(()) on success, otherwise `Error::InvalidAmount` or `Error::UnauthorizedPause`.
    pub fn schedule_unpause(env: Env, caller: Address, at_timestamp: u64) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        if at_timestamp <= env.ledger().timestamp() {
            return Err(BillPaymentsError::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UNP_AT"), &at_timestamp);
        Ok(())
    }

    /// @notice Pause a specific function without pausing the entire contract.
    /// @dev Uses `func` symbols defined in `pause_functions`.
    /// @return Ok(()) on success, otherwise `Error::UnauthorizedPause`.
    pub fn pause_function(env: Env, caller: Address, func: Symbol) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        let mut m: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(&env));
        m.set(func, true);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_FN"), &m);
        Ok(())
    }

    /// @notice Unpause a previously paused function.
    /// @dev Uses `func` symbols defined in `pause_functions`.
    /// @return Ok(()) on success, otherwise `Error::UnauthorizedPause`.
    pub fn unpause_function(env: Env, caller: Address, func: Symbol) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        let mut m: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(&env));
        m.set(func, false);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_FN"), &m);
        Ok(())
    }

    /// @notice Emergency pause both global state and all function-level flags.
    /// @dev Equivalent to calling `pause` plus pausing all supported functions.
    /// @return Ok(()) on success, otherwise the underlying pause errors.
    pub fn emergency_pause_all(env: Env, caller: Address) -> Result<(), Error> {
        Self::pause(env.clone(), caller.clone())?;
        for func in [
            pause_functions::CREATE_BILL,
            pause_functions::PAY_BILL,
            pause_functions::CANCEL_BILL,
            pause_functions::ARCHIVE,
            pause_functions::RESTORE,
        ] {
            let _ = Self::pause_function(env.clone(), caller.clone(), func);
        }
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        Self::get_global_paused(&env)
    }
    pub fn is_function_paused_public(env: Env, func: Symbol) -> bool {
        Self::is_function_paused(&env, func)
    }
    pub fn get_pause_admin_public(env: Env) -> Option<Address> {
        Self::get_pause_admin(&env)
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
    /// - If no upgrade admin exists, caller must equal new_admin (bootstrap pattern)
    /// - If upgrade admin exists, only current upgrade admin can transfer
    /// - Caller must be authenticated via require_auth()
    ///
    /// # Parameters
    /// - `caller`: The address attempting to set the upgrade admin
    /// - `new_admin`: The address to become the new upgrade admin
    ///
    /// # Returns
    /// - `Ok(())` on successful admin transfer
    /// - `Err(Error::Unauthorized)` if caller lacks permission
    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        caller.require_auth();

        let current_upgrade_admin = Self::get_upgrade_admin(&env);

        // Authorization logic:
        // 1. If no upgrade admin exists, caller must equal new_admin (bootstrap)
        // 2. If upgrade admin exists, only current upgrade admin can transfer
        match &current_upgrade_admin {
            None => {
                // Bootstrap pattern - caller must be setting themselves as admin
                if caller != new_admin {
                    return Err(Error::Unauthorized);
                }
            }
            Some(ref current_admin) => {
                // Admin transfer - only current admin can transfer
                if *current_admin != caller {
                    return Err(Error::Unauthorized);
                }
            }
        }

        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);

        // Emit admin transfer event for audit trail
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("adm_xfr"),
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
    pub fn set_version(env: Env, caller: Address, new_version: u32) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).ok_or(BillPaymentsError::Unauthorized)?;
        if admin != caller {
            return Err(BillPaymentsError::Unauthorized);
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

    // -----------------------------------------------------------------------
    // Core bill operations
    // -----------------------------------------------------------------------

    /// Create a new bill with currency specification.
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner (must authorize)
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay (must be positive)
    /// * `due_date` - Due date as Unix timestamp (must be in the future)
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills (must be > 0 if recurring)
    /// * `external_ref` - Optional external system reference ID
    /// * `currency` - Currency code (e.g., "XLM", "USDC", "NGN"). Case-insensitive, whitespace trimmed.
    ///
    /// # Returns
    /// The ID of the created bill
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount is zero or negative
    /// * `InvalidFrequency` - If recurring is true but frequency_days is 0 or exceeds MAX_FREQUENCY_DAYS
    /// * `InvalidDueDate` - If due_date is 0, in the past, or would overflow on recurrence
    /// * `InvalidCurrency` - If currency code is invalid (non-alphanumeric or wrong length)
    /// * `ContractPaused` - If contract is globally paused
    /// * `FunctionPaused` - If create_bill function is paused
    ///
    /// # Currency Normalization
    /// - Empty string defaults to "XLM"
    #[allow(clippy::too_many_arguments)]
    pub fn create_bill(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
        external_ref: Option<String>,
        currency: String,
    ) -> Result<u32, BillPaymentsError> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_BILL)?;

        let current_time = env.ledger().timestamp();
        if due_date == 0 || due_date < current_time {
            return Err(BillPaymentsError::InvalidDueDate);
        }

        if amount <= 0 {
            return Err(BillPaymentsError::InvalidAmount);
        }
        if recurring && (frequency_days == 0 || frequency_days > MAX_FREQUENCY_DAYS) {
            return Err(Error::InvalidFrequency);
        }

        // Validate and normalize currency (strict validation - rejects invalid codes)
        let resolved_currency = Self::validate_and_normalize_currency(&env, &currency)?;

        // Validate external_ref if provided
        let validated_ext_ref = Self::validate_optional_external_ref(&env, &external_ref)?;

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            + 1;

        // Enforce uniqueness for external_ref if provided
        if let Some(ref r) = validated_ext_ref {
            Self::claim_external_ref(&env, &owner, r, next_id)?;
        }

        let current_time = env.ledger().timestamp();
        let bill_external_ref = validated_ext_ref.clone();
        let bill = Bill {
            id: next_id,
            owner: owner.clone(),
            name: name.clone(),
            external_ref: validated_ext_ref,
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
            created_at: current_time,
            paid_at: None,
            schedule_id: None,
            tags: Vec::new(&env),
            currency: resolved_currency,
        };

        let bill_owner = bill.owner.clone();
        let bill_ext_ref = bill.external_ref.clone();
        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::adjust_unpaid_total(&env, &bill_owner, amount);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Created),
            (next_id, bill_owner.clone(), bill_ext_ref),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("created"),
            (next_id, bill_owner, amount, due_date),
        );

        Ok(next_id)
    }

    pub fn pay_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(BillPaymentsError::BillNotFound)?;
        let bill_external_ref = bill.external_ref.clone();

        if bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }
        if bill.paid {
            return Err(BillPaymentsError::BillAlreadyPaid);
        }

        let current_time = env.ledger().timestamp();
        bill.paid = true;
        bill.paid_at = Some(current_time);

        if bill.recurring {
            let next_due_date = bill
                .due_date
                .checked_add(
                    (bill.frequency_days as u64)
                        .checked_mul(SECONDS_PER_DAY)
                        .ok_or(Error::InvalidFrequency)?,
                )
                .ok_or(Error::InvalidDueDate)?;
            let next_id = env
                .storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
                + 1;

            let next_bill = Bill {
                id: next_id,
                owner: bill.owner.clone(),
                name: bill.name.clone(),
                external_ref: None, // Do not clone ref to avoid uniqueness conflict
                amount: bill.amount,
                due_date: next_due_date,
                recurring: true,
                frequency_days: bill.frequency_days,
                paid: false,
                created_at: current_time,
                paid_at: None,
                schedule_id: bill.schedule_id,
                tags: bill.tags.clone(),
                currency: bill.currency.clone(),
            };
            bills.set(next_id, next_bill);
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
        }

        let paid_amount = bill.amount;
        let was_recurring = bill.recurring;
        let bill_ext_ref = bill.external_ref.clone();
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if !was_recurring {
            Self::adjust_unpaid_total(&env, &caller, -paid_amount);
        }
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Paid),
            (bill_id, caller.clone(), bill_ext_ref),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("paid"),
            (bill_id, caller, paid_amount),
        );

        Ok(())
    }

    pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        bills.get(bill_id)
    }

    // -----------------------------------------------------------------------
    // PAGINATED LIST QUERIES
    // -----------------------------------------------------------------------

    /// Get a page of unpaid bills for `owner`.
    ///
    /// # Arguments
    /// * `owner`  – whose bills to return
    /// * `cursor` – start after this bill ID (pass 0 for the first page)
    /// * `limit`  – max items per page (0 → DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`.
    /// When `next_cursor == 0` there are no more pages.
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_unpaid_bills(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.owner != owner || bill.paid {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// Get a page of ALL bills (paid + unpaid) for `owner`.
    ///
    /// Same cursor/limit semantics as `get_unpaid_bills`.
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.owner != owner {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// @notice Get a paginated list of overdue bills (unpaid + past due_date) across all owners.
    /// @dev Canonical ordering is bill ID ascending and is preserved across pages.
    /// Security assumption: Overdue bill retrieval is public since it does not reveal sensitive
    /// off-chain PII (only on-chain bill state). Bounded by pagination `limit` to prevent
    /// exceeding maximum compute or memory limits on large datasets.
    ///
    /// # Arguments
    /// * `cursor` - Start after this bill ID (pass 0 for the first page)
    /// * `limit`  - Max items per page (0 -> DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`.
    /// When `next_cursor == 0` there are no more pages.
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_overdue_bills(env: Env, cursor: u32, limit: u32) -> BillPage {
        let limit = clamp_limit(limit);
        let current_time = env.ledger().timestamp();
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.paid || bill.due_date >= current_time {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// Admin-only: get ALL bills (any owner), paginated.
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_all_bills_page(
        env: Env,
        caller: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<BillPage, BillPaymentsError> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::Unauthorized)?;
        if admin != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Ok(Self::build_page(&env, staging, limit))
    }

    /// Build a `BillPage` from a staging buffer of up to `limit+1` matching items.
    /// `next_cursor` is set to the last *returned* item's ID so the next call's
    /// `id <= cursor` filter correctly skips past it.
    fn build_page(env: &Env, staging: Vec<(u32, Bill)>, limit: u32) -> BillPage {
        let n = staging.len();
        let has_next = n > limit;
        let mut items = Vec::new(env);
        let mut next_cursor: u32 = 0;

        // Emit all items, or all-but-last if there is a next page
        let take = if has_next { n - 1 } else { n };

        for i in 0..take {
            if let Some((_, bill)) = staging.get(i) {
                items.push_back(bill);
            }
        }

        // next_cursor = last returned item's ID (NOT the first skipped item)
        if has_next {
            if let Some((id, _)) = staging.get(take - 1) {
                next_cursor = id;
            }
        }

        let count = items.len();
        BillPage {
            items,
            next_cursor,
            count,
        }
    }

    /// Set or clear an external reference ID for a bill
    ///
    /// # Arguments
    /// * `caller` - Address of the caller (must be the bill owner)
    /// * `bill_id` - ID of the bill to update
    /// * `external_ref` - Optional external system reference ID
    ///
    /// # Returns
    /// Ok(()) if update was successful
    ///
    /// # Errors
    /// * `BillNotFound` - If bill with given ID doesn't exist
    /// * `Unauthorized` - If caller is not the bill owner
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        bill_id: u32,
        external_ref: Option<String>,
    ) -> Result<(), BillPaymentsError> {
        caller.require_auth();

        // Validate the new ref if provided
        let validated_ext_ref = Self::validate_optional_external_ref(&env, &external_ref)?;

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(BillPaymentsError::BillNotFound)?;
        if bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        // Handle index updates
        if bill.external_ref != validated_ext_ref {
            // Release old ref if it existed
            if let Some(ref old_ref) = bill.external_ref {
                Self::release_external_ref(&env, &caller, old_ref);
            }
            // Claim new ref if provided
            if let Some(ref new_ref) = validated_ext_ref {
                Self::claim_external_ref(&env, &caller, new_ref, bill_id)?;
            }
        }

        bill.external_ref = validated_ext_ref.clone();
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("ext_ref"),
            (bill_id, caller, validated_ext_ref),
        );

        Ok(())
    }

    /// Get all bills (paid and unpaid).
    ///
    /// Returned order is canonical bill ID ascending.
    ///
    /// # Returns
    /// Vec of all Bill structs
    pub fn get_all_bills(env: Env, caller: Address) -> Result<Vec<Bill>, Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::Unauthorized)?;
        if admin != caller {
            return Err(Error::Unauthorized);
        }

        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);
        let mut result = Vec::new(&env);
        for id in 1..=max_id {
            if let Some(bill) = bills.get(id) {
                result.push_back(bill);
            }
        }
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Backward-compat helpers
    // -----------------------------------------------------------------------

    /// Legacy helper: returns ALL unpaid bills for owner in one Vec.
    /// Only safe for owners with a small number of bills. Prefer the
    /// paginated `get_unpaid_bills` for production use.
    ///
    /// Returned order is canonical bill ID ascending.
    pub fn get_all_unpaid_bills_legacy(env: Env, owner: Address) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);
        let mut result = Vec::new(&env);
        for id in 1..=max_id {
            if let Some(bill) = bills.get(id) {
                if !bill.paid && bill.owner == owner {
                    result.push_back(bill);
                }
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // Archived bill queries (paginated)
    // -----------------------------------------------------------------------

    /// Get a page of archived bills for `owner`.
    ///
    /// Returned order is canonical bill ID ascending across pages.
    pub fn get_archived_bills(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> ArchivedBillPage {
        let limit = clamp_limit(limit);
        let archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));
        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, ArchivedBill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = archived.get(id) else {
                continue;
            };
            if bill.owner != owner {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        let has_next = staging.len() > limit;
        let mut items = Vec::new(&env);
        let mut next_cursor: u32 = 0;
        let take = if has_next {
            staging.len() - 1
        } else {
            staging.len()
        };

        for i in 0..take {
            if let Some((_, bill)) = staging.get(i) {
                items.push_back(bill);
            }
        }
        if has_next {
            if let Some((id, _)) = staging.get(take - 1) {
                next_cursor = id;
            }
        }

        let count = items.len();
        ArchivedBillPage {
            items,
            next_cursor,
            count,
        }
    }

    pub fn get_archived_bill(env: Env, bill_id: u32) -> Option<ArchivedBill> {
        let archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));
        archived.get(bill_id)
    }

    // -----------------------------------------------------------------------
    // Remaining operations
    // -----------------------------------------------------------------------

    pub fn cancel_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL)?;
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let bill = bills.get(bill_id).ok_or(BillPaymentsError::BillNotFound)?;
        if bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        // Release external_ref if it exists
        if let Some(ref r) = bill.external_ref {
            Self::release_external_ref(&env, &caller, r);
        }

        let removed_unpaid_amount = if bill.paid { 0 } else { bill.amount };
        bills.remove(bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if removed_unpaid_amount > 0 {
            Self::adjust_unpaid_total(&env, &caller, -removed_unpaid_amount);
        }
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("canceled"),
            bill_id,
        );
        Ok(())
    }

    /// @notice Archive paid bills with `paid_at < before_timestamp`.
    /// @dev Permissionless maintenance operation. Caller must authenticate, but does not need to
    /// own each archived bill. Only paid bills with a historical payment timestamp are moved from
    /// active storage into archival storage.
    /// @param caller Authenticated caller executing archive maintenance.
    /// @param before_timestamp Exclusive upper bound for `paid_at`.
    /// @return Number of bills archived in this call.
    /// @security Unpaid bills are never archived; owner data is preserved on archived records.
    pub fn archive_paid_bills(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, BillPaymentsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ARCHIVE)?;
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut archived_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, bill) in bills.iter() {
            if let Some(paid_at) = bill.paid_at {
                    if bill.paid && paid_at < before_timestamp {
                        // Release external_ref from the active index during archival
                        if let Some(ref r) = bill.external_ref {
                            Self::release_external_ref(&env, &bill.owner, r);
                        }

                        let archived_bill = ArchivedBill {
                            id: bill.id,
                            owner: bill.owner.clone(),
                            name: bill.name.clone(),
                            external_ref: bill.external_ref.clone(),
                            amount: bill.amount,
                            paid_at,
                            archived_at: current_time,
                            tags: bill.tags.clone(),
                            currency: bill.currency.clone(),
                        };
                    archived.set(id, archived_bill);
                    to_remove.push_back(id);
                    archived_count += 1;
                }
            }
        }

        for id in to_remove.iter() {
            bills.remove(id);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_BILL"), &archived);

        Self::extend_archive_ttl(&env);
        Self::update_storage_stats(&env);

        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::System,
            symbol_short!("archived"),
            archived_count,
        );

        Ok(archived_count)
    }

    pub fn restore_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::RESTORE)?;
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));
        let archived_bill = archived
            .get(bill_id)
            .ok_or(BillPaymentsError::BillNotFound)?;

        if archived_bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        // Reclaim external_ref in the active index. 
        // Fails if another bill now uses this ref.
        if let Some(ref r) = archived_bill.external_ref {
            Self::claim_external_ref(&env, &caller, r, bill_id)?;
        }

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let restored_bill = Bill {
            id: archived_bill.id,
            owner: archived_bill.owner.clone(),
            name: archived_bill.name.clone(),
            external_ref: archived_bill.external_ref.clone(),
            amount: archived_bill.amount,
            due_date: env.ledger().timestamp() + 2592000,
            recurring: false,
            frequency_days: 0,
            paid: true,
            created_at: archived_bill.paid_at,
            paid_at: Some(archived_bill.paid_at),
            schedule_id: None,
            tags: archived_bill.tags.clone(),
            currency: archived_bill.currency.clone(),
        };

        bills.set(bill_id, restored_bill);
        archived.remove(bill_id);

        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_BILL"), &archived);

        Self::update_storage_stats(&env);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("restored"),
            bill_id,
        );
        Ok(())
    }

    /// @notice Permanently delete archived bills with `archived_at < before_timestamp`.
    /// @dev Permissionless maintenance operation for archive compaction.
    /// @param caller Authenticated caller executing cleanup.
    /// @param before_timestamp Exclusive upper bound for `archived_at`.
    /// @return Number of archived records removed.
    /// @security Only archived data is touched; active bills are unaffected.
    pub fn bulk_cleanup_bills(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, BillPaymentsError> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ARCHIVE)?;
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));
        let mut deleted_count = 0u32;
        let mut to_remove: Vec<u32> = Vec::new(&env);

        for (id, bill) in archived.iter() {
            if bill.archived_at < before_timestamp {
                // Release external_ref if it exists
                if let Some(ref r) = bill.external_ref {
                    Self::release_external_ref(&env, &bill.owner, r);
                }
                to_remove.push_back(id);
                deleted_count += 1;
            }
        }

        for id in to_remove.iter() {
            archived.remove(id);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_BILL"), &archived);
        Self::update_storage_stats(&env);

        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::System,
            symbol_short!("cleaned"),
            deleted_count,
        );
        Ok(deleted_count)
    }

    /// @notice Pay multiple bills in one call.
    ///
    /// @dev Partial-success semantics are deterministic: invalid bill IDs are skipped and reported,
    /// while valid IDs continue processing.
    ///
    /// @param caller Authenticated owner attempting the batch payment.
    /// @param bill_ids Candidate bill IDs to process.
    /// @return Number of successfully paid bills.
    /// @security Cross-owner payments are rejected per item; oversized batches are rejected
    /// before iteration.
    pub fn batch_pay_bills(env: Env, caller: Address, bill_ids: Vec<u32>) -> Result<u32, Error> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;

        if bill_ids.len() > (MAX_BATCH_SIZE as usize).try_into().unwrap_or(u32::MAX) {
            return Err(BillPaymentsError::BatchTooLarge);
        }

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let mut next_id: u32 = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        let mut paid_count = 0u32;
        let mut failed_count = 0u32;
        let mut unpaid_delta = 0i128;

        for id in bill_ids.iter() {
            let bill_result = bills.get(id);

            // Validation logic for each bill
            let mut bill = match bill_result {
                Some(b) => b,
                None => {
                    failed_count += 1;
                    RemitwiseEvents::emit(
                        &env,
                        EventCategory::Transaction,
                        EventPriority::Medium,
                        symbol_short!("f_pay_id"), // fail_pay_id
                        (id, Error::BillNotFound as u32),
                    );
                    continue;
                }
            };

            if bill.owner != caller {
                failed_count += 1;
                RemitwiseEvents::emit(
                    &env,
                    EventCategory::Transaction,
                    EventPriority::Medium,
                    symbol_short!("fpay_auth"), // fail_pay_auth
                    (id, Error::Unauthorized as u32),
                );
                continue;
            }

            if bill.paid {
                failed_count += 1;
                RemitwiseEvents::emit(
                    &env,
                    EventCategory::Transaction,
                    EventPriority::Medium,
                    symbol_short!("f_pay_pd"), // fail_pay_paid
                    (id, Error::BillAlreadyPaid as u32),
                );
                continue;
            }

            // Process payment
            let amount = bill.amount;
            bill.paid = true;
            bill.paid_at = Some(current_time);

            if bill.recurring {
                next_id = next_id.saturating_add(1);
                let next_due_date = bill
                    .due_date
                    .checked_add(
                        (bill.frequency_days as u64)
                            .checked_mul(SECONDS_PER_DAY)
                            .ok_or(Error::InvalidFrequency)?,
                    )
                    .ok_or(Error::InvalidDueDate)?;
                let next_bill = Bill {
                    id: next_id,
                    owner: bill.owner.clone(),
                    name: bill.name.clone(),
                    external_ref: None, // Do not clone ref to avoid uniqueness conflict
                    amount: bill.amount,
                    due_date: next_due_date,
                    recurring: true,
                    frequency_days: bill.frequency_days,
                    paid: false,
                    created_at: current_time,
                    paid_at: None,
                    schedule_id: bill.schedule_id,
                    tags: bill.tags.clone(),
                    currency: bill.currency.clone(),
                };
                bills.set(next_id, next_bill);
            } else {
                unpaid_delta = unpaid_delta.saturating_sub(amount);
            }

            bills.set(id, bill);
            paid_count += 1;

            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::High,
                symbol_short!("paid"),
                (id, caller.clone(), amount),
            );
        }

        // Final storage updates
        if paid_count > 0 || failed_count > 0 {
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
            env.storage()
                .instance()
                .set(&symbol_short!("BILLS"), &bills);

            if unpaid_delta != 0 {
                Self::adjust_unpaid_total(&env, &caller, unpaid_delta);
            }
            Self::update_storage_stats(&env);
        }

        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::Medium,
            symbol_short!("batch_res"), // batch_result
            (paid_count, failed_count),
        );

        Ok(paid_count)
    }

    pub fn get_total_unpaid(env: Env, owner: Address) -> i128 {
        if let Some(totals) = Self::get_unpaid_totals_map(&env) {
            if let Some(total) = totals.get(owner.clone()) {
                return total;
            }
        }

        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut total = 0i128;
        for (_, bill) in bills.iter() {
            if !bill.paid && bill.owner == owner {
                total += bill.amount;
            }
        }
        total
    }

    pub fn get_storage_stats(env: Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&symbol_short!("STOR_STAT"))
            .unwrap_or(StorageStats {
                active_bills: 0,
                archived_bills: 0,
                total_unpaid_amount: 0,
                total_archived_amount: 0,
                last_updated: 0,
            })
    }

    // -----------------------------------------------------------------------
    // Currency-filter helper queries
    // -----------------------------------------------------------------------

    /// Get a page of ALL bills (paid + unpaid) for `owner` that match `currency`.
    ///
    /// # Arguments
    /// * `owner`    – Address of the bill owner
    /// * `currency` – Currency code to filter by, e.g. `"USDC"`, `"XLM"`
    /// * `cursor`   – Start after this bill ID (pass 0 for the first page)
    /// * `limit`    – Max items per page (0 → DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`. `next_cursor == 0` means no more pages.
    ///
    /// # Currency Comparison
    /// Currency comparison is case-insensitive and whitespace-insensitive:
    /// - "usdc", "USDC", "UsDc", " usdc " all match
    /// - Empty currency defaults to "XLM" for comparison
    ///
    /// # Examples
    /// ```rust,ignore
    /// // Get all USDC bills for owner
    /// let page = client.get_bills_by_currency(&owner, &"USDC".into(), &0, &10);
    /// ```
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        let limit = clamp_limit(limit);
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let max_id = Self::get_next_bill_id(&env);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.owner != owner || bill.currency != normalized_currency {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// Get a page of **unpaid** bills for `owner` that match `currency`.
    ///
    /// # Arguments
    /// * `owner`    – Address of the bill owner
    /// * `currency` – Currency code to filter by, e.g. `"USDC"`, `"XLM"`
    /// * `cursor`   – Start after this bill ID (pass 0 for the first page)
    /// * `limit`    – Max items per page (0 → DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`. `next_cursor == 0` means no more pages.
    ///
    /// # Currency Comparison
    /// Currency comparison is case-insensitive and whitespace-insensitive:
    /// - "usdc", "USDC", "UsDc", " usdc " all match
    /// - Empty currency defaults to "XLM" for comparison
    ///
    /// # Examples
    /// ```rust,ignore
    /// // Get unpaid USDC bills for owner
    /// let page = client.get_unpaid_bills_by_currency(&owner, &"USDC".into(), &0, &10);
    /// ```
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    pub fn get_unpaid_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let max_id = Self::get_next_bill_id(&env);
        for id in (cursor.saturating_add(1))..=max_id {
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.owner != owner || bill.paid || bill.currency != normalized_currency {
                continue;
            }
            staging.push_back((id, bill));
            if staging.len() > limit {
                break;
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// Sum of all **unpaid** bill amounts for `owner` denominated in `currency`.
    ///
    /// # Arguments
    /// * `owner`    – Address of the bill owner
    /// * `currency` – Currency code to filter by, e.g. `"USDC"`, `"XLM"`
    ///
    /// # Returns
    /// Total unpaid amount in the specified currency
    ///
    /// # Currency Comparison
    /// Currency comparison is case-insensitive and whitespace-insensitive:
    /// - "usdc", "USDC", "UsDc", " usdc " all match
    /// - Empty currency defaults to "XLM" for comparison
    ///
    /// # Examples
    /// ```rust,ignore
    /// // Get total unpaid amount in USDC
    /// let total_usdc = client.get_total_unpaid_by_currency(&owner, &"USDC".into());
    /// // Get total unpaid amount in XLM
    /// let total_xlm = client.get_total_unpaid_by_currency(&owner, &"XLM".into());
    /// ```
    pub fn get_total_unpaid_by_currency(env: Env, owner: Address, currency: String) -> i128 {
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut total = 0i128;
        for (_, bill) in bills.iter() {
            if !bill.paid && bill.owner == owner && bill.currency == normalized_currency {
                total += bill.amount;
            }
        }
        total
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn extend_archive_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    fn update_storage_stats(env: &Env) {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));
        let archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(env));

        let mut active_count = 0u32;
        let mut unpaid_amount = 0i128;
        for (_, bill) in bills.iter() {
            active_count += 1;
            if !bill.paid {
                unpaid_amount = unpaid_amount.saturating_add(bill.amount);
            }
        }

        let mut archived_count = 0u32;
        let mut archived_amount = 0i128;
        for (_, bill) in archived.iter() {
            archived_count += 1;
            archived_amount = archived_amount.saturating_add(bill.amount);
        }

        let stats = StorageStats {
            active_bills: active_count,
            archived_bills: archived_count,
            total_unpaid_amount: unpaid_amount,
            total_archived_amount: archived_amount,
            last_updated: env.ledger().timestamp(),
        };

        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }
    fn get_unpaid_totals_map(env: &Env) -> Option<Map<Address, i128>> {
        env.storage().instance().get(&STORAGE_UNPAID_TOTALS)
    }

    fn adjust_unpaid_total(env: &Env, owner: &Address, delta: i128) {
        if delta == 0 {
            return;
        }
        let mut totals: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&STORAGE_UNPAID_TOTALS)
            .unwrap_or_else(|| Map::new(env));
        let current = totals.get(owner.clone()).unwrap_or(0);
        let next = match current.checked_add(delta) {
            Some(n) => n,
            None => panic!("overflow"),
        };
        totals.set(owner.clone(), next);
        env.storage()
            .instance()
            .set(&STORAGE_UNPAID_TOTALS, &totals);
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;
    use remitwise_common::MAX_PAGE_LIMIT;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env, String,
    };

    fn make_env() -> Env {
        Env::default()
    }

    /// Create `count` bills with a static name. Returns their IDs.
    /// Due dates are set in the future so they are NOT overdue.
    fn setup_bills(
        env: &Env,
        client: &BillPaymentsClient,
        owner: &Address,
        count: u32,
    ) -> Vec<u32> {
        let mut ids = Vec::new(env);
        for i in 0..count {
            let id = client.create_bill(
                owner,
                &String::from_str(env, "Test Bill"),
                &(100i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
                &false,
                &0,
                &None,
                &String::from_str(env, "XLM"),
            );
            ids.push_back(id);
        }
        ids
    }

    // --- get_unpaid_bills ---

    #[test]
    fn test_get_unpaid_bills_empty() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let page = client.get_unpaid_bills(&owner, &0, &0);
        assert_eq!(page.count, 0);
        assert_eq!(page.next_cursor, 0);
        assert_eq!(page.items.len(), 0);
    }

    #[test]
    fn test_get_unpaid_bills_single_page() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        setup_bills(&env, &client, &owner, 5);

        let page = client.get_unpaid_bills(&owner, &0, &10);
        assert_eq!(page.count, 5);
        assert_eq!(page.next_cursor, 0);
    }

    #[test]
    fn test_get_unpaid_bills_multiple_pages() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        setup_bills(&env, &client, &owner, 7);

        let page1 = client.get_unpaid_bills(&owner, &0, &3);
        assert_eq!(page1.count, 3);
        assert!(page1.next_cursor > 0, "expected a next cursor");

        let page2 = client.get_unpaid_bills(&owner, &page1.next_cursor, &3);
        assert_eq!(page2.count, 3);
        assert!(page2.next_cursor > 0);

        let page3 = client.get_unpaid_bills(&owner, &page2.next_cursor, &3);
        assert_eq!(page3.count, 1);
        assert_eq!(page3.next_cursor, 0);
    }

    #[test]
    fn test_get_unpaid_bills_excludes_paid() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let ids = setup_bills(&env, &client, &owner, 4);
        let second_id = ids.get(1).unwrap();
        client.pay_bill(&owner, &second_id);

        let page = client.get_unpaid_bills(&owner, &0, &10);
        assert_eq!(page.count, 3);
    }

    #[test]
    fn test_get_unpaid_bills_excludes_other_owner() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        setup_bills(&env, &client, &owner_a, 3);
        setup_bills(&env, &client, &owner_b, 2);

        let page = client.get_unpaid_bills(&owner_a, &0, &10);
        assert_eq!(page.count, 3);
        for bill in page.items.iter() {
            assert_eq!(bill.owner, owner_a);
        }
    }

    #[test]
    fn test_get_unpaid_bills_owner_isolation_bidirectional() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        setup_bills(&env, &client, &owner_a, 2);
        setup_bills(&env, &client, &owner_b, 3);

        // owner_a sees only their own bills
        let page_a = client.get_unpaid_bills(&owner_a, &0, &10);
        assert_eq!(page_a.count, 2);
        for bill in page_a.items.iter() {
            assert_eq!(
                bill.owner, owner_a,
                "owner_a page must not contain owner_b bills"
            );
        }

        // owner_b sees only their own bills
        let page_b = client.get_unpaid_bills(&owner_b, &0, &10);
        assert_eq!(page_b.count, 3);
        for bill in page_b.items.iter() {
            assert_eq!(
                bill.owner, owner_b,
                "owner_b page must not contain owner_a bills"
            );
        }
    }

    #[test]
    fn test_get_unpaid_bills_owner_isolation_after_one_pays() {
        // If owner_a pays their bill, owner_b's unpaid bills are unaffected
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        let ids_a = setup_bills(&env, &client, &owner_a, 2);
        setup_bills(&env, &client, &owner_b, 2);

        // owner_a pays one of their bills
        client.pay_bill(&owner_a, &ids_a.get(0).unwrap());

        // owner_a now has 1 unpaid
        let page_a = client.get_unpaid_bills(&owner_a, &0, &10);
        assert_eq!(page_a.count, 1);
        for bill in page_a.items.iter() {
            assert_eq!(bill.owner, owner_a, "Should only see owner_a bills");
            assert!(!bill.paid, "Should only see unpaid bills");
        }

        // owner_b still has 2 unpaid — unaffected by owner_a's payment
        let page_b = client.get_unpaid_bills(&owner_b, &0, &10);
        assert_eq!(page_b.count, 2);
        for bill in page_b.items.iter() {
            assert_eq!(bill.owner, owner_b, "Should only see owner_b bills");
        }
    }

    #[test]
    fn test_get_unpaid_bills_owner_isolation_one_owner_no_bills() {
        // owner_b has bills but owner_a has none — owner_a gets empty page
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        // Only owner_b creates bills
        setup_bills(&env, &client, &owner_b, 3);

        let page_a = client.get_unpaid_bills(&owner_a, &0, &10);
        assert_eq!(page_a.count, 0, "owner_a should see no bills");
        assert_eq!(page_a.next_cursor, 0);

        let page_b = client.get_unpaid_bills(&owner_b, &0, &10);
        assert_eq!(page_b.count, 3, "owner_b should see all their bills");
    }

    #[test]
    fn test_get_unpaid_bills_owner_isolation_all_paid_other_owner_unpaid() {
        // owner_a pays all their bills — owner_b's unpaid still isolated correctly
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        let ids_a = setup_bills(&env, &client, &owner_a, 3);
        setup_bills(&env, &client, &owner_b, 2);

        // owner_a pays all their bills
        for id in ids_a.iter() {
            client.pay_bill(&owner_a, &id);
        }

        // owner_a has zero unpaid
        let page_a = client.get_unpaid_bills(&owner_a, &0, &10);
        assert_eq!(page_a.count, 0, "owner_a should have no unpaid bills left");

        // owner_b still has 2 unpaid — not polluted by owner_a's paid bills
        let page_b = client.get_unpaid_bills(&owner_b, &0, &10);
        assert_eq!(page_b.count, 2);
        for bill in page_b.items.iter() {
            assert_eq!(bill.owner, owner_b);
            assert!(!bill.paid);
        }
    }

    #[test]
    fn test_get_unpaid_bills_owner_isolation_pagination_does_not_leak() {
        // With many owners, paginating through owner_a's results never leaks owner_b's bills
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);

        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        // Interleave bills: a, b, a, b, a, b ...
        for i in 0..4u32 {
            // Added the 'currency' argument at the end to match the new signature
            client.create_bill(
                &owner_a,
                &String::from_str(&env, "Bill A"),
                &(100i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
            client.create_bill(
                &owner_b,
                &String::from_str(&env, "Bill B"),
                &(200i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        // Paginate through owner_a with small page size
        let mut all_a_bills: soroban_sdk::Vec<Bill> = soroban_sdk::Vec::new(&env);
        let mut cursor = 0u32;

        loop {
            // Assuming your get_unpaid_bills function returns a struct with 'items' and 'next_cursor'
            let page = client.get_unpaid_bills(&owner_a, &cursor, &2);

            for bill in page.items.iter() {
                assert_eq!(
                    bill.owner, owner_a,
                    "Paginated result must never contain owner_b's bill"
                );
                // Verification: ensure the default currency logic worked
                assert_eq!(bill.currency, String::from_str(&env, "XLM"));

                all_a_bills.push_back(bill);
            }

            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        assert_eq!(
            all_a_bills.len(),
            4,
            "owner_a should have exactly 4 bills across all pages"
        );
    }

    // --- get_overdue_bills ---

    #[test]
    fn test_get_overdue_bills_not_overdue() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        setup_bills(&env, &client, &owner, 3);
        let page = client.get_overdue_bills(&owner, &0, &10);
        assert_eq!(page.count, 0);
    }

    #[test]
    fn test_get_overdue_bills_pagination() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        // 1. Set initial time so create_bill succeeds
        // The contract requires: due_date >= current_time
        env.ledger().set_timestamp(10000);

        let due_date = 20000;

        for _ in 0..6u32 {
            client.create_bill(
                &owner,
                &String::from_str(&env, "Overdue Bill"),
                &100,
                &due_date, // 20000
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        // 2. Advance time PAST the due date to make them "Overdue"
        // current_time (25000) > due_date (20000)
        env.ledger().set_timestamp(25000);

        // Now get_overdue_bills will actually find the 6 bills
        let page1 = client.get_overdue_bills(&owner, &0, &4);
        assert_eq!(page1.count, 4);
        assert!(page1.next_cursor > 0);

        let page2 = client.get_overdue_bills(&owner, &page1.next_cursor, &4);
        assert_eq!(page2.count, 2);
        assert_eq!(page2.next_cursor, 0);
    }

    // --- get_all_bills_for_owner ---

    #[test]
    fn test_get_all_bills_for_owner_includes_paid() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let ids = setup_bills(&env, &client, &owner, 5);
        let first_id = ids.get(0).unwrap();
        client.pay_bill(&owner, &first_id);

        let page = client.get_all_bills_for_owner(&owner, &0, &10);
        assert_eq!(page.count, 5);
    }

    // --- limit clamping ---

    #[test]
    fn test_limit_zero_uses_default() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        setup_bills(&env, &client, &owner, 3);
        let page = client.get_unpaid_bills(&owner, &0, &0);
        assert_eq!(page.count, 3);
    }

    #[test]
    fn test_limit_clamped_to_max() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        setup_bills(&env, &client, &owner, 55);
        let page = client.get_unpaid_bills(&owner, &0, &9999);
        assert_eq!(page.count, MAX_PAGE_LIMIT);
        assert!(page.next_cursor > 0);
    }

    // --- archived bill pagination ---

    #[test]
    fn test_get_archived_bills_pagination() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.set_pause_admin(&owner, &owner);

        let ids = setup_bills(&env, &client, &owner, 6);
        for bill_id in ids.iter() {
            client.pay_bill(&owner, &bill_id);
        }
        client.archive_paid_bills(&owner, &u64::MAX);

        let page1 = client.get_archived_bills(&owner, &0, &4);
        assert_eq!(page1.count, 4);
        assert!(page1.next_cursor > 0);

        let page2 = client.get_archived_bills(&owner, &page1.next_cursor, &4);
        assert_eq!(page2.count, 2);
        assert_eq!(page2.next_cursor, 0);
    }

    fn assert_bill_ids_strictly_ascending(page: &BillPage, last_seen: &mut u32) {
        for bill in page.items.iter() {
            assert!(
                bill.id > *last_seen,
                "expected ascending bill ids, got {} after {}",
                bill.id,
                *last_seen
            );
            *last_seen = bill.id;
        }
    }

    fn assert_archived_bill_ids_strictly_ascending(page: &ArchivedBillPage, last_seen: &mut u32) {
        for bill in page.items.iter() {
            assert!(
                bill.id > *last_seen,
                "expected ascending archived bill ids, got {} after {}",
                bill.id,
                *last_seen
            );
            *last_seen = bill.id;
        }
    }

    #[test]
    fn test_canonical_order_get_unpaid_bills_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        let mut owner_a_ids: Vec<u32> = Vec::new(&env);
        for i in 0..4u32 {
            let id_a = client.create_bill(
                &owner_a,
                &String::from_str(&env, "A"),
                &(100 + i as i128),
                &(env.ledger().timestamp() + 10_000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
            owner_a_ids.push_back(id_a);

            client.create_bill(
                &owner_b,
                &String::from_str(&env, "B"),
                &(200 + i as i128),
                &(env.ledger().timestamp() + 20_000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        client.pay_bill(&owner_a, &owner_a_ids.get(1).unwrap());

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut collected: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_unpaid_bills(&owner_a, &cursor, &2);
            assert_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                collected.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let expected = soroban_sdk::vec![&env, 1u32, 5u32, 7u32];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_canonical_order_get_all_bills_for_owner_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        let mut ids: Vec<u32> = Vec::new(&env);
        for i in 0..4u32 {
            let id = client.create_bill(
                &owner,
                &String::from_str(&env, "Owner Bill"),
                &(500 + i as i128),
                &(env.ledger().timestamp() + 30_000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
            ids.push_back(id);
            client.create_bill(
                &other,
                &String::from_str(&env, "Other Bill"),
                &(900 + i as i128),
                &(env.ledger().timestamp() + 40_000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        client.pay_bill(&owner, &ids.get(1).unwrap());

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut collected: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_all_bills_for_owner(&owner, &cursor, &2);
            assert_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                collected.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let expected = soroban_sdk::vec![&env, 1u32, 3u32, 5u32, 7u32];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_canonical_order_get_overdue_bills_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        env.ledger().set_timestamp(1000);
        for i in 0..6u32 {
            client.create_bill(
                &owner,
                &String::from_str(&env, "Overdue"),
                &(100 + i as i128),
                &(2000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        client.pay_bill(&owner, &2);
        env.ledger().set_timestamp(10_000);

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut collected: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_overdue_bills(&cursor, &2);
            assert_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                collected.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let expected = soroban_sdk::vec![&env, 1u32, 3u32, 4u32, 5u32, 6u32];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_canonical_order_get_all_bills_page_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let admin = Address::generate(&env);
        let owner = Address::generate(&env);

        client.set_pause_admin(&admin, &admin);

        for i in 0..6u32 {
            client.create_bill(
                &owner,
                &String::from_str(&env, "Admin List"),
                &(1000 + i as i128),
                &(env.ledger().timestamp() + 50_000 + i as u64),
                &false,
                &0,
                &None,
                &String::from_str(&env, "XLM"),
            );
        }

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut collected: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_all_bills_page(&admin, &cursor, &2);
            assert_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                collected.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let expected = soroban_sdk::vec![&env, 1u32, 2u32, 3u32, 4u32, 5u32, 6u32];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_canonical_order_get_archived_bills_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.set_pause_admin(&owner, &owner);

        let ids = setup_bills(&env, &client, &owner, 5);
        for bill_id in ids.iter() {
            client.pay_bill(&owner, &bill_id);
        }
        client.archive_paid_bills(&owner, &u64::MAX);

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut collected: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_archived_bills(&owner, &cursor, &2);
            assert_archived_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                collected.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let expected = soroban_sdk::vec![&env, 1u32, 2u32, 3u32, 4u32, 5u32];
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_canonical_order_currency_endpoints_id_ascending() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.create_bill(
            &owner,
            &String::from_str(&env, "C1"),
            &100,
            &(env.ledger().timestamp() + 1_000),
            &false,
            &0,
            &None,
            &String::from_str(&env, "USDC"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "C2"),
            &200,
            &(env.ledger().timestamp() + 2_000),
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );
        let paid_usdc_id = client.create_bill(
            &owner,
            &String::from_str(&env, "C3"),
            &300,
            &(env.ledger().timestamp() + 3_000),
            &false,
            &0,
            &None,
            &String::from_str(&env, "usdc"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "C4"),
            &400,
            &(env.ledger().timestamp() + 4_000),
            &false,
            &0,
            &None,
            &String::from_str(&env, "USDC"),
        );
        client.create_bill(
            &owner,
            &String::from_str(&env, "C5"),
            &500,
            &(env.ledger().timestamp() + 5_000),
            &false,
            &0,
            &None,
            &String::from_str(&env, " usdc "),
        );

        client.pay_bill(&owner, &paid_usdc_id);

        let mut cursor = 0u32;
        let mut last_seen = 0u32;
        let mut all_currency_ids: Vec<u32> = Vec::new(&env);
        loop {
            let page =
                client.get_bills_by_currency(&owner, &String::from_str(&env, "USDC"), &cursor, &2);
            assert_bill_ids_strictly_ascending(&page, &mut last_seen);
            for bill in page.items.iter() {
                all_currency_ids.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        let expected_all = soroban_sdk::vec![&env, 1u32, 3u32, 4u32, 5u32];
        assert_eq!(all_currency_ids, expected_all);

        let mut cursor = 0u32;
        let mut last_seen_unpaid = 0u32;
        let mut unpaid_currency_ids: Vec<u32> = Vec::new(&env);
        loop {
            let page = client.get_unpaid_bills_by_currency(
                &owner,
                &String::from_str(&env, "USDC"),
                &cursor,
                &2,
            );
            assert_bill_ids_strictly_ascending(&page, &mut last_seen_unpaid);
            for bill in page.items.iter() {
                unpaid_currency_ids.push_back(bill.id);
            }
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        let expected_unpaid = soroban_sdk::vec![&env, 1u32, 4u32, 5u32];
        assert_eq!(unpaid_currency_ids, expected_unpaid);
    }

    // -----------------------------------------------------------------------
    // RECURRING BILLS DATE MATH TESTS
    // -----------------------------------------------------------------------
    // These tests verify the core date math for recurring bills:
    // next_due_date = due_date + (frequency_days * 86400)
    // Ensures paid_at does not affect next bill's due_date calculation.

    #[test]
    fn test_recurring_date_math_frequency_1_day() {
        // Test: frequency_days = 1 → next due date is +1 day (86400 seconds)
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Daily Bill"),
            &100,
            &base_due_date,
            &true, // recurring
            &1,    // frequency_days = 1
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay the bill
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (1 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        assert_eq!(
            next_bill.due_date,
            base_due_date + 86400,
            "Next due date should be exactly 1 day later"
        );
        assert_eq!(next_bill.frequency_days, 1, "Frequency should be preserved");
    }

    #[test]
    fn test_recurring_date_math_frequency_30_days() {
        // Test: frequency_days = 30 → next due date is +30 days (2,592,000 seconds)
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Monthly Bill"),
            &500,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay the bill
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (30 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be exactly 30 days later"
        );
        assert_eq!(
            next_bill.frequency_days, 30,
            "Frequency should be preserved"
        );
    }

    #[test]
    fn test_recurring_date_math_frequency_365_days() {
        // Test: frequency_days = 365 → next due date is +365 days (31,536,000 seconds)
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Annual Bill"),
            &1200,
            &base_due_date,
            &true, // recurring
            &365,  // frequency_days = 365
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay the bill
        client.pay_bill(&owner, &bill_id);

        // Verify next bill's due_date = base_due_date + (365 * 86400)
        let next_bill = client.get_bill(&2).unwrap();
        assert!(!next_bill.paid, "Next bill should be unpaid");
        let expected_due_date = base_due_date + (365u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be exactly 365 days later"
        );
        assert_eq!(
            next_bill.frequency_days, 365,
            "Frequency should be preserved"
        );
    }

    #[test]
    fn test_recurring_date_math_paid_at_does_not_affect_next_due() {
        let env = Env::default();

        // FORCE reset to a very small number first
        env.ledger().set_timestamp(100);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        // Now current_time (100) is definitely < base_due_date (1,000,000)
        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Late Payment Test"),
            &300,
            &base_due_date,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Warp to late payment time
        env.ledger().set_timestamp(1_000_500);
        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(next_bill.due_date, expected_due_date);
    }

    #[test]
    fn test_recurring_date_math_multiple_pay_cycles_2nd_bill() {
        // Test: Multiple pay cycles - verify 2nd bill's due date advances correctly
        // Bill 1: due_date=1000000, frequency=30
        // Bill 2: due_date=1000000 + (30*86400)
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Multi-Cycle Bill"),
            &250,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        client.pay_bill(&owner, &bill_id);

        // Verify second bill
        let bill2 = client.get_bill(&2).unwrap();
        let expected_bill2_due = base_due_date + (30u64 * 86400);
        assert_eq!(bill2.due_date, expected_bill2_due);
        assert!(!bill2.paid);

        // Pay second bill
        client.pay_bill(&owner, &2);

        // Verify second bill is now paid
        let bill2_paid = client.get_bill(&2).unwrap();
        assert!(bill2_paid.paid);

        // Verify third bill was created with correct due_date
        let bill3 = client.get_bill(&3).unwrap();
        let expected_bill3_due = expected_bill2_due + (30u64 * 86400);
        assert_eq!(
            bill3.due_date, expected_bill3_due,
            "Bill 3 due_date should be Bill 2 due_date + (30*86400)"
        );
        assert!(!bill3.paid);
    }

    #[test]
    fn test_recurring_date_math_multiple_pay_cycles_3rd_bill() {
        // Test: Multiple pay cycles - verify 3rd bill's due date advances correctly
        // Bill 1: due_date=1000000, frequency=30
        // Bill 2: due_date=1000000 + (30*86400)
        // Bill 3: due_date=1000000 + (60*86400)
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Three-Cycle Bill"),
            &150,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        client.pay_bill(&owner, &2);

        // Pay third bill
        client.pay_bill(&owner, &3);

        // Verify third bill is now paid
        let bill3_paid = client.get_bill(&3).unwrap();
        assert!(bill3_paid.paid);

        // Verify fourth bill was created with correct due_date
        let bill4 = client.get_bill(&4).unwrap();
        let expected_bill4_due = base_due_date + (90u64 * 86400); // 3 * 30 days
        assert_eq!(
            bill4.due_date, expected_bill4_due,
            "Bill 4 due_date should be base + (90*86400)"
        );
        assert!(!bill4.paid);
    }

    #[test]
    fn test_recurring_date_math_early_payment_does_not_affect_schedule() {
        // Test: Paying a bill EARLY should not affect the next bill's due_date
        // Bill 1: due_date=1000000, paid at time=500000 (paid 500000 seconds early)
        // Bill 2: due_date should still be 1000000 + (30*86400)
        let env = make_env();
        env.ledger().set_timestamp(500_000); // Set time BEFORE due date
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Early Payment Test"),
            &200,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay the bill early (at time 500_000)
        client.pay_bill(&owner, &bill_id);

        // Verify original bill has paid_at set to early time
        let paid_bill = client.get_bill(&bill_id).unwrap();
        assert!(paid_bill.paid);
        assert_eq!(paid_bill.paid_at, Some(500_000));

        // Verify next bill's due_date is still based on original due_date
        let next_bill = client.get_bill(&2).unwrap();
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should not be affected by early payment"
        );
    }

    #[test]
    fn test_recurring_date_math_preserves_frequency_across_cycles() {
        // Test: frequency_days is preserved across all recurring cycles
        // Verify that Bill 1, 2, 3 all have the same frequency_days value
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let frequency = 7u32; // Weekly
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Weekly Bill"),
            &50,
            &1_000_000,
            &true,
            &frequency,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        client.pay_bill(&owner, &2);

        // Verify all bills have the same frequency_days
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.frequency_days, frequency);
        assert_eq!(bill2.frequency_days, frequency);
        assert_eq!(bill3.frequency_days, frequency);
    }

    #[test]
    fn test_recurring_date_math_amount_preserved_across_cycles() {
        // Test: Bill amount is preserved across all recurring cycles
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let amount = 999i128;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Fixed Amount Bill"),
            &amount,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        client.pay_bill(&owner, &2);

        // Verify all bills have the same amount
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.amount, amount);
        assert_eq!(bill2.amount, amount);
        assert_eq!(bill3.amount, amount);
    }

    #[test]
    fn test_recurring_date_math_owner_preserved_across_cycles() {
        // Test: Bill owner is preserved across all recurring cycles
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Owner Test"),
            &100,
            &1_000_000,
            &true,
            &30,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // Pay first bill
        client.pay_bill(&owner, &bill_id);

        // Pay second bill
        client.pay_bill(&owner, &2);

        // Verify all bills have the same owner
        let bill1 = client.get_bill(&1).unwrap();
        let bill2 = client.get_bill(&2).unwrap();
        let bill3 = client.get_bill(&3).unwrap();

        assert_eq!(bill1.owner, owner);
        assert_eq!(bill2.owner, owner);
        assert_eq!(bill3.owner, owner);
    }

    #[test]
    fn test_recurring_date_math_exact_calculation_verification() {
        // Test: Verify exact date math calculation with known values
        // due_date = 1_000_000
        // frequency_days = 14
        // Expected: 1_000_000 + (14 * 86400) = 1_000_000 + 1_209_600 = 2_209_600
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due = 1_000_000u64;
        let freq = 14u32;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Math Verification"),
            &100,
            &base_due,
            &true,
            &freq,
            &None,
            &String::from_str(&env, "XLM"),
        );

        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        let expected = 1_000_000u64 + (14u64 * 86400);
        assert_eq!(next_bill.due_date, expected);
        assert_eq!(next_bill.due_date, 2_209_600);
    }

    // -----------------------------------------------------------------------
    // Property-based tests: time-dependent behavior
    // -----------------------------------------------------------------------

    proptest! {
        /// All bills returned by get_overdue_bills must have due_date < now,
        /// and every bill created with due_date < now must appear in the result.
        #[test]
        fn prop_overdue_bills_all_have_due_before_now(
            now in 2_000_000u64..10_000_000u64,
            n_overdue in 1usize..6usize,
            n_future in 0usize..6usize,
        ) {
            let env = make_env();
            let create_time = now.saturating_sub(10_000);
            env.ledger().set_timestamp(create_time);
            env.mock_all_auths();
            let cid = env.register_contract(None, BillPayments);
            let client = BillPaymentsClient::new(&env, &cid);
            let owner = Address::generate(&env);

            // Set initial ledger time to 0 to bypass "due_date >= now" check,
            // then fast-forward to the target 'now' value.
            env.ledger().set_timestamp(0);

            // Create bills with due_date < now (overdue)
            for i in 0..n_overdue {
                client.create_bill(
                    &owner,
                    &String::from_str(&env, "Overdue"),
                    &100,
                    &(now - 1 - i as u64), // due_date < now; created while time=1 so it's "future"
                    &false,
                    &0,
                    &None,

                    &String::from_str(&env, "XLM"),
                );
            }

            // Create bills that will remain not overdue at time=now
            for i in 0..n_future {
                client.create_bill(
                    &owner,
                    &String::from_str(&env, "Future"),
                    &100,
                    &(now + 1 + i as u64),
                    &false,
                    &0,
                    &None,

                    &String::from_str(&env, "XLM"),
                );
            }

            // Fast-forward to 'now' so they become overdue
            env.ledger().set_timestamp(now);

            let page = client.get_overdue_bills(&owner, &0, &50);
            for bill in page.items.iter() {
                prop_assert!(bill.due_date < now, "returned bill must be past due");
            }
            prop_assert_eq!(page.count as usize, n_overdue);
        }
    }

    proptest! {
        /// Bills with due_date >= now must never appear in get_overdue_bills.
        #[test]
        fn prop_future_bills_not_in_overdue_set(
            now in 1_000_000u64..5_000_000u64,
            n in 1usize..6usize,
        ) {
            let env = make_env();
            env.ledger().set_timestamp(now);
            env.mock_all_auths();
            let cid = env.register_contract(None, BillPayments);
            let client = BillPaymentsClient::new(&env, &cid);
            let owner = Address::generate(&env);

            for i in 0..n {
                client.create_bill(
                    &owner,
                    &String::from_str(&env, "NotOverdue"),
                    &100,
                    &(now + i as u64), // due_date >= now — strict less-than is required to be overdue
                    &false,
                    &0,
                    &None,

                    &String::from_str(&env, "XLM"),
                );
            }

            let page = client.get_overdue_bills(&owner, &0, &50);
            prop_assert_eq!(
                page.count,
                0u32,
                "bills with due_date >= now must not appear as overdue"
            );
        }
    }

    proptest! {
        /// After paying a recurring bill, the next bill's due_date equals
        /// the original due_date + frequency_days * 86400, regardless of
        /// when payment is made.
        #[test]
        fn prop_recurring_next_bill_due_date_follows_original(
            _base_due in 1_000_000u64..5_000_000u64,
            base_due_offset in 1_000_000u64..5_000_000u64,
            pay_offset in 1u64..100_000u64,
            freq_days in 1u32..366u32,
        ) {
            let env = make_env();
            env.mock_all_auths();
            let cid = env.register_contract(None, BillPayments);
            let client = BillPaymentsClient::new(&env, &cid);
            let owner = Address::generate(&env);

            // Set base due date far in the future relative to 0
            let base_due = 1_000_000 + base_due_offset;
            env.ledger().set_timestamp(0);

            let bill_id = client.create_bill(
                &owner,
                &String::from_str(&env, "Recurring"),
                &100,
                &base_due,
                &true,
                &freq_days,
                &None,

                &String::from_str(&env, "XLM"),
            );

            // Fast-forward to the payment time
            let now = base_due + pay_offset;
            env.ledger().set_timestamp(now);

            client.pay_bill(&owner, &bill_id);

            let next_bill = client.get_bill(&2).unwrap();
            let expected_due = base_due + (freq_days as u64 * 86400);
            prop_assert_eq!(
                next_bill.due_date,
                expected_due,
                "next recurring bill due_date must equal original due_date + freq_days * 86400"
            );
            prop_assert!(!next_bill.paid, "next recurring bill must be unpaid");
        }
    }

    /// Issue #102 – When pay_bill is called on a recurring bill, the contract
    /// creates the next occurrence.  This test asserts every cloned field
    /// individually so that a regression in the clone logic (e.g. paid left
    /// true, wrong due_date, wrong owner) is caught immediately.
    #[test]
    fn test_create_bill_invalid_due_date() {
        // 1. Setup
        let env = make_env();
        env.mock_all_auths();

        // Explicitly set the ledger time
        let current_ledger_time = 1_700_000_000;
        env.ledger().with_mut(|info| {
            info.timestamp = current_ledger_time;
        });

        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        // 2. Scenario Data
        let past_due_date = 946684800; // Year 1999
        let zero_due_date = 0u64;
        let name = String::from_str(&env, "Electricity");
        let currency = String::from_str(&env, ""); // New required parameter

        // 3. Execution: Attempt to create bills with invalid dates
        // Added '&currency' as the final argument to both calls
        let result_past = client.try_create_bill(
            &owner,
            &name,
            &1000,
            &past_due_date,
            &false,
            &0,
            &None,
            &currency,
        );

        let result_zero = client.try_create_bill(
            &owner,
            &name,
            &1000,
            &zero_due_date,
            &false,
            &0,
            &None,
            &currency,
        );

        // 4. Assertions
        assert!(
            result_past.is_err(),
            "Creation should have failed for a past date"
        );
        assert!(
            result_zero.is_err(),
            "Creation should have failed for a zero date"
        );

        // Check that the error code matches InvalidDueDate
        match result_past {
            Err(Ok(err)) => assert_eq!(err, BillPaymentsError::InvalidDueDate),
            _ => panic!("Expected contract error InvalidDueDate for past date"),
        }

        match result_zero {
            Err(Ok(err)) => assert_eq!(err, BillPaymentsError::InvalidDueDate),
            _ => panic!("Expected contract error InvalidDueDate for zero date"),
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Time & Ledger Drift Resilience Tests (#158)
    //
    // Assumptions:
    //  - A bill is overdue when due_date < current_time (strict less-than).
    //  - At exactly due_date the bill is NOT yet overdue.
    //  - Stellar ledger timestamps are monotonically increasing in production.
    // ══════════════════════════════════════════════════════════════════════

    /// Bill is NOT overdue when ledger timestamp == due_date (inclusive boundary).
    #[test]
    fn test_time_drift_bill_not_overdue_at_exact_due_date() {
        let due_date = 1_000_000u64;
        let env = make_env();
        env.mock_all_auths();
        env.ledger().set_timestamp(due_date);

        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.create_bill(
            &owner,
            &String::from_str(&env, "Power"),
            &200,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let page = client.get_overdue_bills(&owner, &0, &100);
        assert_eq!(
            page.count, 0,
            "Bill must not appear overdue when current_time == due_date"
        );
    }

    /// Bill becomes overdue exactly one second after due_date.
    #[test]
    fn test_time_drift_bill_overdue_one_second_after_due_date() {
        let due_date = 1_000_000u64;
        let env = make_env();
        env.mock_all_auths();
        env.ledger().set_timestamp(due_date);

        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.create_bill(
            &owner,
            &String::from_str(&env, "Internet"),
            &150,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let page = client.get_overdue_bills(&owner, &0, &100);
        assert_eq!(page.count, 0);

        env.ledger().set_timestamp(due_date + 1);
        let page = client.get_overdue_bills(&owner, &0, &100);
        assert_eq!(
            page.count, 1,
            "Bill must appear overdue exactly one second past due_date"
        );
    }
    #[test]
    /// Mix of past-due, exactly-due, and future bills: only past-due one appears.
    fn test_time_drift_overdue_boundary_mixed_bills() {
        let env = Env::default();
        // 1. Set time to long ago
        env.ledger().set_timestamp(1_000_000);

        let contract_id = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        env.mock_all_auths();

        // 2. Create bills with due dates in the "future" (relative to 1,000_000)
        // This one will be our "Overdue" bill later
        let overdue_target = 1_500_000u64;
        client.create_bill(
            &owner,
            &String::from_str(&env, "Overdue"),
            &100,
            &overdue_target,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // This one will be "DueNow" later
        let due_now_target = 2_000_000u64;
        client.create_bill(
            &owner,
            &String::from_str(&env, "DueNow"),
            &200,
            &due_now_target,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // 3. WARP to the "Present" (2,000_000)
        env.ledger().set_timestamp(2_000_000);

        let page = client.get_overdue_bills(&owner, &0, &100);

        // Now overdue_target (1.5M) is < current (2M) -> OVERDUE
        // due_now_target (2M) is NOT < current (2M) -> NOT OVERDUE
        assert_eq!(page.count, 1);
        assert_eq!(page.items.get(0).unwrap().amount, 100);
    }

    /// Full-day boundary (86400 s): bill created at due_date, queried one day later, is overdue.
    #[test]
    fn test_time_drift_overdue_full_day_boundary() {
        let day = 86400u64;
        let due_date = 1_000_000u64;
        let env = make_env();
        env.mock_all_auths();
        env.ledger().set_timestamp(due_date);

        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        client.create_bill(
            &owner,
            &String::from_str(&env, "Monthly Rent"),
            &5000,
            &due_date,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let page = client.get_overdue_bills(&owner, &0, &100);
        assert_eq!(page.count, 0);

        env.ledger().set_timestamp(due_date + day);
        let page = client.get_overdue_bills(&owner, &0, &100);
        assert_eq!(
            page.count, 1,
            "Bill must be overdue one full day past due_date"
        );
    }

    // -----------------------------------------------------------------------
    // Strict Owner Authorization Lifecycle Tests
    // -----------------------------------------------------------------------

    /// ### Test: `test_create_bill_no_auth_fails`
    /// **Objective**: Verify that `create_bill` reverts if the owner doesn't authorize the call.
    /// **Expected**: Reverts with a Soroban AuthError.
    #[test]
    #[should_panic(expected = "Error(Auth, InvalidAction)")]
    fn test_create_bill_no_auth_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        // Attempting to create a bill without mocking auth should fail on owner.require_auth()
        client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );
    }

    /// ### Test: `test_pay_bill_wrong_owner_fails`
    /// **Objective**: Verify that `pay_bill` reverts if a caller attempts to pay a bill they don't own.
    /// **Authorized Caller**: `bill.owner`
    /// **Unauthorized Caller**: `other`
    /// **Expected**: Returns `Error::Unauthorized`.
    #[test]
    fn test_pay_bill_wrong_owner_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // 'other' attempts to pay owner's bill
        let result = client.try_pay_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    /// ### Test: `test_pay_bill_no_auth_fails`
    /// **Objective**: Verify that `pay_bill` reverts if the caller is the owner but does not authorize the call.
    /// **Expected**: Reverts with a Soroban AuthError.
    #[test]
    #[should_panic(expected = "Error(Auth, InvalidAction)")]
    fn test_pay_bill_no_auth_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        // Use mock_auths specifically for creation so it doesn't affect the pay_bill call
        env.mock_all_auths();
        let _bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Water"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        // This will panic as expected because we are NOT mocking auths for this call
        // and 'owner.require_auth()' will fail.
        // We set mock_all_auths to false to disable the global mock.
        env.set_auths(&[]);
        client.pay_bill(&owner, &_bill_id);
    }

    #[test]
    fn test_cancel_bill_wrong_owner_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Cancel"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let result = client.try_cancel_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_external_ref_wrong_owner_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "ExtRef"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let result =
            client.try_set_external_ref(&other, &bill_id, &Some(String::from_str(&env, "REF")));
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_restore_bill_wrong_owner_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);
        let other = Address::generate(&env);

        env.mock_all_auths();
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Restore"),
            &500,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );
        client.pay_bill(&owner, &bill_id);

        // Archive it
        client.archive_paid_bills(&owner, &2000000);

        // Other tries to restore
        let result = client.try_restore_bill(&other, &bill_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_batch_pay_bills_mixed_ownership_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        env.mock_all_auths();
        let alice_bill = client.create_bill(
            &alice,
            &String::from_str(&env, "Alice"),
            &100,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );
        let bob_bill = client.create_bill(
            &bob,
            &String::from_str(&env, "Bob"),
            &200,
            &1000000,
            &false,
            &0,
            &None,
            &String::from_str(&env, "XLM"),
        );

        let mut ids = Vec::new(&env);
        ids.push_back(alice_bill);
        ids.push_back(bob_bill);

        // Alice tries to batch pay both: her bill is paid, Bob's is skipped.
        let result = client.try_batch_pay_bills(&alice, &ids);
        assert_eq!(result, Ok(Ok(1)));

        assert!(client.get_bill(&alice_bill).unwrap().paid);
        assert!(!client.get_bill(&bob_bill).unwrap().paid);
    }

    #[test]
    #[should_panic(expected = "Error(Auth, InvalidAction)")]
    fn test_archive_paid_bills_no_auth_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let caller = Address::generate(&env);

        // No sign, should fail on caller.require_auth()
        client.archive_paid_bills(&caller, &1000000);
    }

    #[test]
    #[should_panic(expected = "Error(Auth, InvalidAction)")]
    fn test_bulk_cleanup_bills_no_auth_fails() {
        let env = make_env();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let admin = Address::generate(&env);

        client.bulk_cleanup_bills(&admin, &1000000);
    }

    #[test]
    fn test_external_ref_validation() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Test");
        let currency = String::from_str(&env, "XLM");

        // Valid chars: alphanumeric, -, _, ., :
        let valid_ref = String::from_str(&env, "ABC-123_abc.def:000");
        let res = client.try_create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(valid_ref), &currency);
        assert!(res.is_ok());

        // Invalid char: space
        let invalid_ref = String::from_str(&env, "REF 1");
        let res = client.try_create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(invalid_ref), &currency);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidExternalRef)));

        // Invalid char: @
        let invalid_ref2 = String::from_str(&env, "ref@123");
        let res = client.try_create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(invalid_ref2), &currency);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidExternalRef)));

        // Length limits
        let too_short = String::from_str(&env, "");
        let res = client.try_create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(too_short), &currency);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidExternalRef)));

        let too_long_str = "a".repeat(65);
        let too_long = String::from_str(&env, &too_long_str);
        let res = client.try_create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(too_long), &currency);
        assert_eq!(res, Err(Ok(BillPaymentsError::InvalidExternalRef)));
    }

    #[test]
    fn test_external_ref_uniqueness_per_owner() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner_a = Address::generate(&env);
        let owner_b = Address::generate(&env);

        let name = String::from_str(&env, "Test");
        let currency = String::from_str(&env, "XLM");
        let ext_ref = String::from_str(&env, "REF-001");

        // Owner A creates bill with REF-001
        client.create_bill(&owner_a, &name, &100, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);

        // Owner A tries to create ANOTHER bill with SAME ref -> Fails
        let res = client.try_create_bill(&owner_a, &name, &200, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);
        assert_eq!(res, Err(Ok(BillPaymentsError::DuplicateExternalRef)));

        // Owner B tries to create bill with SAME ref -> Success (isolated)
        let res_b = client.try_create_bill(&owner_b, &name, &300, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);
        assert!(res_b.is_ok());
    }

    #[test]
    fn test_external_ref_reuse_after_clear_or_cancel() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Test");
        let currency = String::from_str(&env, "XLM");
        let ext_ref = String::from_str(&env, "REF-001");

        let id1 = client.create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);

        // Clear ref on ID1
        client.set_external_ref(&owner, &id1, &None);

        // Now REF-001 is free
        let id2 = client.create_bill(&owner, &name, &200, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);
        assert!(id2 > id1);

        // Cancel ID2
        client.cancel_bill(&owner, &id2);

        // Now REF-001 is free again
        let id3 = client.create_bill(&owner, &name, &300, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);
        assert!(id3 > id2);
    }

    #[test]
    fn test_restore_conflict_fails() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Test");
        let currency = String::from_str(&env, "XLM");
        let ext_ref = String::from_str(&env, "REF-001");

        // 1. Create and Pay Bill 1
        let id1 = client.create_bill(&owner, &name, &100, &2000000, &false, &0, &Some(ext_ref.clone()), &currency);
        client.pay_bill(&owner, &id1);

        // 2. Archive Bill 1 (frees REF-001 for active bills)
        client.archive_paid_bills(&owner, &u64::MAX);

        // 3. Create Bill 2 with SAME ref
        let _id2 = client.create_bill(&owner, &name, &200, &3000000, &false, &0, &Some(ext_ref.clone()), &currency);

        // 4. Try to Restore Bill 1 -> Conflicts with Bill 2
        let res = client.try_restore_bill(&owner, &id1);
        assert_eq!(res, Err(Ok(BillPaymentsError::DuplicateExternalRef)));
    }

    #[test]
    fn test_recurring_avoids_conflict_on_auto_create() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let name = String::from_str(&env, "Recurring");
        let currency = String::from_str(&env, "XLM");
        let ext_ref = String::from_str(&env, "REF-RECUR");

        // Create recurring bill with ref
        let id1 = client.create_bill(&owner, &name, &100, &2000000, &true, &30, &Some(ext_ref.clone()), &currency);

        // Pay it. This creates the next instance.
        // If we cloned the ref, it would fail.
        client.pay_bill(&owner, &id1);

        // Check the new bill
        let id2 = 2u32;
        let bill2 = client.get_bill(&id2).unwrap();
        assert_eq!(bill2.external_ref, None, "Next recurring instance should have None ref to avoid conflict");

        // Now we can reuse the ref if we clear it from the old one or archive it.
        client.archive_paid_bills(&owner, &u64::MAX); // Frees the ref from id1

        // Now we can set it on id2
        client.set_external_ref(&owner, &id2, &Some(ext_ref.clone()));
        let bill2_updated = client.get_bill(&id2).unwrap();
        assert_eq!(bill2_updated.external_ref, Some(ext_ref));
    }
}
