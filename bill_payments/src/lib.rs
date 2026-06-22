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

fn is_valid_currency_chars(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&b| b.is_ascii_alphabetic())
}

const MAX_FREQUENCY_DAYS: u32 = 36_500; // 100 years
const SECONDS_PER_DAY: u64 = 86_400;
const MAX_CURRENCY_LEN: u32 = 10;
pub const MAX_BILLS_PER_OWNER: u32 = 1_000;
const MIN_EXTERNAL_REF_LEN: u32 = 1;
const MAX_EXTERNAL_REF_LEN: u32 = 64;

#[contracttype]
#[derive(Clone, Debug)]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub amount: i128,
    /// Unix timestamp (seconds) when this bill is due.
    ///
    /// Acceptance rule: `due_date >= env.ledger().timestamp()` at creation time.
    /// `due_date == 0` is always rejected. `due_date == now` is accepted.
    pub due_date: u64,
    pub recurring: bool,
    /// Recurrence interval in days. Valid range: `[1, MAX_FREQUENCY_DAYS]` (1–36_500).
    ///
    /// Ignored when `recurring == false`. A value of `0` on a recurring bill
    /// returns `BillPaymentsError::InvalidFrequency`.
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
const STORAGE_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");
const STORAGE_ARCH_INDEX: Symbol = symbol_short!("ARCH_IDX");
const STORAGE_CURRENCY_INDEX: Symbol = symbol_short!("CUR_IDX");
const ARCH_IDX_KEY: Symbol = STORAGE_ARCH_INDEX;

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
    /// Recurring frequency is invalid (error code 4).
    ///
    /// Triggered when `recurring == true` and `frequency_days == 0` or
    /// `frequency_days > MAX_FREQUENCY_DAYS` (36_500). Valid range: `[1, 36_500]`.
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
    /// Due date is in the past or otherwise invalid (error code 12).
    ///
    /// Triggered when `due_date == 0` OR `due_date < env.ledger().timestamp()`.
    /// Boundary: `due_date == now` is **accepted** (strict less-than comparison).
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
    /// Owner has reached the maximum number of allowed active bills.
    OwnerBillCapExceeded = 18,
    /// Tag content contains invalid characters (must be [a-z0-9-_])
    InvalidTagContent = 19,
}

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
    RecurringBillCreated,
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
    // Owner-index helpers
    // -----------------------------------------------------------------------

    /// Return the active-bill ID list for `owner` (ID-ascending, no gaps).
    fn get_owner_bills(env: &Env, owner: &Address) -> Vec<u32> {
        let idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(env));
        idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env))
    }

    /// Return the archived-bill ID list for `owner`.
    fn get_owner_archived_bills(env: &Env, owner: &Address) -> Vec<u32> {
        let idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_ARCH_INDEX)
            .unwrap_or_else(|| Map::new(env));
        idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env))
    }

    /// Insert `bill_id` into the active index for `owner` in ascending order.
    fn index_add_active(env: &Env, owner: &Address, bill_id: u32) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let mut new_ids: Vec<u32> = Vec::new(env);
        let mut inserted = false;
        for id in ids.iter() {
            if !inserted {
                if bill_id == id {
                    inserted = true;
                } else if bill_id < id {
                    new_ids.push_back(bill_id);
                    inserted = true;
                }
            }
            new_ids.push_back(id);
        }
        if !inserted {
            new_ids.push_back(bill_id);
        }
        idx.set(owner.clone(), new_ids);
        env.storage().instance().set(&STORAGE_OWNER_INDEX, &idx);
    }

    /// Remove `bill_id` from the active index for `owner`.
    fn index_remove_active(env: &Env, owner: &Address, bill_id: u32) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let mut new_ids: Vec<u32> = Vec::new(env);
        for id in ids.iter() {
            if id != bill_id {
                new_ids.push_back(id);
            }
        }
        idx.set(owner.clone(), new_ids);
        env.storage().instance().set(&STORAGE_OWNER_INDEX, &idx);
    }

    /// Remove multiple `bill_ids` from the active index for `owner`.
    fn index_remove_active_batch(env: &Env, owner: &Address, bill_ids: &Vec<u32>) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let mut new_ids: Vec<u32> = Vec::new(env);
        for id in ids.iter() {
            let mut removed = false;
            for b_id in bill_ids.iter() {
                if id == b_id {
                    removed = true;
                    break;
                }
            }
            if !removed {
                new_ids.push_back(id);
            }
        }
        idx.set(owner.clone(), new_ids);
        env.storage().instance().set(&STORAGE_OWNER_INDEX, &idx);
    }

    /// Add multiple `bill_ids` to the archived index for `owner`.
    fn index_add_archived_batch(env: &Env, owner: &Address, bill_ids: &Vec<u32>) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_ARCH_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let mut owner_ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));

        for bill_id in bill_ids.iter() {
            let mut new_ids: Vec<u32> = Vec::new(env);
            let mut inserted = false;
            for id in owner_ids.iter() {
                if !inserted {
                    if bill_id == id {
                        inserted = true;
                    } else if bill_id < id {
                        new_ids.push_back(bill_id);
                        inserted = true;
                    }
                }
                new_ids.push_back(id);
            }
            if !inserted {
                new_ids.push_back(bill_id);
            }
            owner_ids = new_ids;
        }

        idx.set(owner.clone(), owner_ids);
        env.storage().instance().set(&STORAGE_ARCH_INDEX, &idx);
    }

    /// Remove `bill_id` from the archived index for `owner`.
    fn index_remove_archived(env: &Env, owner: &Address, bill_id: u32) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_ARCH_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let mut new_ids: Vec<u32> = Vec::new(env);
        for id in ids.iter() {
            if id != bill_id {
                new_ids.push_back(id);
            }
        }
        idx.set(owner.clone(), new_ids);
        env.storage().instance().set(&STORAGE_ARCH_INDEX, &idx);
    }

    /// Remove multiple `bill_ids` from the archived index for `owner`.
    fn index_remove_archived_batch(env: &Env, owner: &Address, bill_ids: &Vec<u32>) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_ARCH_INDEX)
            .unwrap_or_else(|| Map::new(env));
        let ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let mut new_ids: Vec<u32> = Vec::new(env);
        for id in ids.iter() {
            let mut removed = false;
            for b_id in bill_ids.iter() {
                if id == b_id {
                    removed = true;
                    break;
                }
            }
            if !removed {
                new_ids.push_back(id);
            }
        }
        idx.set(owner.clone(), new_ids);
        env.storage().instance().set(&STORAGE_ARCH_INDEX, &idx);
    }

    // -----------------------------------------------------------------------
    // Currency-index helpers
    // -----------------------------------------------------------------------

    /// Load the currency index: Map<(Address, String), Vec<u32>>
    /// Maps (owner, currency) pairs to their bill IDs in ascending order
    fn get_currency_index(env: &Env) -> Map<(Address, String), Vec<u32>> {
        env.storage()
            .instance()
            .get(&STORAGE_CURRENCY_INDEX)
            .unwrap_or_else(|| Map::new(env))
    }

    fn save_currency_index(env: &Env, idx: &Map<(Address, String), Vec<u32>>) {
        env.storage().instance().set(&STORAGE_CURRENCY_INDEX, idx);
    }

    /// Get bill IDs for a specific owner and currency
    fn get_bills_by_owner_currency(env: &Env, owner: &Address, currency: &String) -> Vec<u32> {
        let idx = Self::get_currency_index(env);
        idx.get((owner.clone(), currency.clone()))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add a bill ID to the currency index for (owner, currency)
    fn index_add_currency(env: &Env, owner: &Address, currency: &String, bill_id: u32) {
        let mut idx = Self::get_currency_index(env);
        let key = (owner.clone(), currency.clone());
        let ids = idx.get(key.clone()).unwrap_or_else(|| Vec::new(env));
        
        // Insert in ascending order
        let mut new_ids: Vec<u32> = Vec::new(env);
        let mut inserted = false;
        for id in ids.iter() {
            if !inserted {
                if bill_id == id {
                    inserted = true;
                } else if bill_id < id {
                    new_ids.push_back(bill_id);
                    inserted = true;
                }
            }
            new_ids.push_back(id);
        }
        if !inserted {
            new_ids.push_back(bill_id);
        }

        idx.set(key, new_ids);
        Self::save_currency_index(env, &idx);
    }

    /// Remove a bill ID from the currency index for (owner, currency)
    fn index_remove_currency(env: &Env, owner: &Address, currency: &String, bill_id: u32) {
        let mut idx = Self::get_currency_index(env);
        let key = (owner.clone(), currency.clone());
        if let Some(ids) = idx.get(key.clone()) {
            let mut new_ids: Vec<u32> = Vec::new(env);
            for id in ids.iter() {
                if id != bill_id {
                    new_ids.push_back(id);
                }
            }
            if new_ids.is_empty() {
                idx.remove(key);
            } else {
                idx.set(key, new_ids);
            }
            Self::save_currency_index(env, &idx);
        }
    }

    /// Remove multiple bill IDs from the currency index for (owner, currency)
    fn index_remove_currency_batch(
        env: &Env,
        owner: &Address,
        currency: &String,
        bill_ids: &Vec<u32>,
    ) {
        let mut idx = Self::get_currency_index(env);
        let key = (owner.clone(), currency.clone());
        if let Some(ids) = idx.get(key.clone()) {
            let mut new_ids: Vec<u32> = Vec::new(env);
            for id in ids.iter() {
                let mut removed = false;
                for b_id in bill_ids.iter() {
                    if id == b_id {
                        removed = true;
                        break;
                    }
                }
                if !removed {
                    new_ids.push_back(id);
                }
            }
            if new_ids.is_empty() {
                idx.remove(key);
            } else {
                idx.set(key, new_ids);
            }
            Self::save_currency_index(env, &idx);
        }
    }

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
    fn validate_external_ref(_env: &Env, ext_ref: &String) -> Result<String, BillPaymentsError> {
        let len = ext_ref.len();
        if !(MIN_EXTERNAL_REF_LEN..=MAX_EXTERNAL_REF_LEN).contains(&len) {
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
            Option::None => Ok(None),
            Option::Some(r) => Ok(Some(Self::validate_external_ref(env, r)?)),
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
        env.storage().instance().set(&STORAGE_EXT_REF_IDX, idx);
    }

    /// Claim `ext_ref` for `owner` → `bill_id`. Fails if already claimed by another bill.
    fn claim_external_ref(
        env: &Env,
        owner: &Address,
        ext_ref: &String,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        let mut idx = Self::get_ext_ref_index(env);
        let mut owner_map: Map<String, u32> =
            idx.get(owner.clone()).unwrap_or_else(|| Map::new(env));

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
            Option::None => {
                if caller != new_admin {
                    return Err(BillPaymentsError::UnauthorizedPause);
                }
            }
            Option::Some(admin) if admin != caller => {
                return Err(BillPaymentsError::UnauthorizedPause)
            }
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }

    /// @notice Pause all state-changing operations.
    /// @dev Requires the pause admin to authenticate. Cancels any pending unpause schedule.
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
        // Cancel any pending unpause schedule to prevent timelock bypass
        env.storage().instance().remove(&symbol_short!("UNP_AT"));
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
            Option::Some(ref current_admin) => {
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
    /// * `due_date` - Due date as Unix timestamp (seconds). Must satisfy
    ///   `due_date >= env.ledger().timestamp()`. `due_date == now` is **accepted**
    ///   (strict less-than comparison). `due_date == 0` is always rejected.
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Recurrence interval in days. Must be in `[1, MAX_FREQUENCY_DAYS]`
    ///   when `recurring == true`; ignored otherwise.
    /// * `external_ref` - Optional external system reference ID
    /// * `currency` - Currency code (e.g., "XLM", "USDC", "NGN"). Case-insensitive, whitespace trimmed.
    ///
    /// # Due Date Rule
    /// `due_date` must satisfy `due_date >= current_ledger_timestamp`.
    /// A `due_date` strictly in the past (`due_date < now`) returns `InvalidDueDate`.
    /// Boundary: `due_date == now` is **accepted**.
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
        _schedule_id: Option<u32>,
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

        // Enforce per-owner bill cap before touching storage.
        let owner_bill_count = Self::get_owner_bills(&env, &owner).len();
        if owner_bill_count >= MAX_BILLS_PER_OWNER {
            return Err(BillPaymentsError::OwnerBillCapExceeded);
        }

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
        let bill_currency = bill.currency.clone();
        let bill_ext_ref = bill.external_ref.clone();
        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        // Update owner index
        Self::index_add_active(&env, &bill_owner, next_id);
        // Update currency index
        Self::index_add_currency(&env, &bill_owner, &bill_currency, next_id);
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

    /// Mark a bill as paid. If `bill.recurring == true`, spawns a child bill with:
    ///
    /// ```text
    /// child.due_date = bill.due_date + frequency_days * 86_400
    /// ```
    ///
    /// If the computed `child.due_date` is still `<= current_time` (extremely late payment),
    /// the formula advances by one additional period repeatedly until the child is strictly
    /// in the future. This guarantees the child is **never born overdue**.
    ///
    /// # Recurring Invariant
    /// The child due date is computed relative to the **parent's** `due_date`, not the
    /// payment timestamp (`paid_at`). This ensures the billing schedule is independent
    /// of when payment actually occurs.
    ///
    /// # Errors
    /// * `BillNotFound` - If no bill with `bill_id` exists
    /// * `BillAlreadyPaid` - If the bill is already marked paid
    /// * `Unauthorized` - If `caller != bill.owner`
    /// * `InvalidDueDate` - If child due_date arithmetic overflows `u64`
    /// * `InvalidFrequency` - If period arithmetic overflows `u64`
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
        let _bill_external_ref = bill.external_ref.clone();

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
            let period = (bill.frequency_days as u64)
                .checked_mul(SECONDS_PER_DAY)
                .ok_or(Error::InvalidFrequency)?;
            let mut next_due_date = bill
                .due_date
                .checked_add(period)
                .ok_or(Error::InvalidDueDate)?;
            // Advance forward by frequency periods until the next due date is strictly in the future
            while next_due_date <= current_time {
                next_due_date = next_due_date
                    .checked_add(period)
                    .ok_or(Error::InvalidDueDate)?;
            }
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
            // Update owner index for the newly created recurring bill
            Self::index_add_active(&env, &caller, next_id);
            // Update currency index for the newly created recurring bill
            Self::index_add_currency(&env, &caller, &bill.currency, next_id);
            env.events().publish(
                (symbol_short!("bill"), BillEvent::RecurringBillCreated),
                (next_id, bill_id, next_due_date),
            );
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

    // -----------------------------------------------------------------------
    // Tag management
    // -----------------------------------------------------------------------

    /// Validates and canonicalizes a tag batch for metadata operations.
    ///
    /// Delegates to the shared [`remitwise_common::canonicalize_tags`] helper.
    /// Invalid characters are reported as [`BillPaymentsError::InvalidTagContent`].
    fn validate_and_normalize_tags(env: &Env, tags: &Vec<String>) -> Vec<String> {
        remitwise_common::canonicalize_tags(env, tags, || {
            soroban_sdk::panic_with_error!(env, BillPaymentsError::InvalidTagContent)
        })
    }

    /// Adds tags to a bill's metadata.
    ///
    /// Security:
    /// - `caller` must authorize the invocation.
    /// - Only the bill owner can add tags.
    ///
    /// Notes:
    /// - Tags are validated and normalized (lowercase, trimmed charset).
    /// - Emits `(bill, tags_add)` with `(bill_id, caller, tags)`.
    pub fn add_tags_to_bill(env: Env, caller: Address, bill_id: u32, tags: Vec<String>) {
        caller.require_auth();
        let normalized_tags = Self::validate_and_normalize_tags(&env, &tags);
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).unwrap_or_else(|| {
            panic!("Bill not found");
        });

        if bill.owner != caller {
            panic!("Only the bill owner can add tags");
        }

        for tag in normalized_tags.iter() {
            bill.tags.push_back(tag);
        }

        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("tags_add"),
            (bill_id, caller.clone(), tags.clone()),
        );
        env.events().publish(
            (symbol_short!("bill"), symbol_short!("tags_add")),
            (bill_id, caller.clone(), tags.clone()),
        );
    }

    /// Removes tags from a bill's metadata.
    ///
    /// Security:
    /// - `caller` must authorize the invocation.
    /// - Only the bill owner can remove tags.
    ///
    /// Notes:
    /// - Removing a tag that is not present is a no-op.
    /// - Emits `(bill, tags_rem)` with `(bill_id, caller, tags)`.
    pub fn remove_tags_from_bill(env: Env, caller: Address, bill_id: u32, tags: Vec<String>) {
        caller.require_auth();
        let normalized_tags = Self::validate_and_normalize_tags(&env, &tags);
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).unwrap_or_else(|| {
            panic!("Bill not found");
        });

        if bill.owner != caller {
            panic!("Only the bill owner can remove tags");
        }

        // Remove matching tags (first occurrence only for each tag in the removal list)
        let mut remaining_tags = Vec::new(&env);
        for existing_tag in bill.tags.iter() {
            let mut should_remove = false;
            for tag_to_remove in normalized_tags.iter() {
                if existing_tag == tag_to_remove {
                    should_remove = true;
                    break;
                }
            }
            if !should_remove {
                remaining_tags.push_back(existing_tag);
            }
        }
        bill.tags = remaining_tags;

        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("tags_rem"),
            (bill_id, caller.clone(), tags.clone()),
        );
        env.events().publish(
            (symbol_short!("bill"), symbol_short!("tags_rem")),
            (bill_id, caller.clone(), tags.clone()),
        );
    }

    pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        bills.get(bill_id)
    }

    /// Return the number of active (non-archived) bills owned by `owner`.
    ///
    /// This is an O(1) read from the owner index and does not scan the full
    /// bill map.  The count is bounded by `MAX_BILLS_PER_OWNER`.
    pub fn get_owner_bill_count(env: Env, owner: Address) -> u32 {
        Self::get_owner_bills(&env, &owner).len()
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

        // Use the owner index for O(owner_bills) traversal instead of O(NEXT_ID).
        let owner_ids = Self::get_owner_bills(&env, &owner);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in owner_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.paid {
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

        // Use the owner index for O(owner_bills) traversal instead of O(NEXT_ID).
        let owner_ids = Self::get_owner_bills(&env, &owner);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in owner_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = bills.get(id) else {
                continue;
            };
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

        // Use the archived owner index for O(owner_archived) traversal.
        let owner_ids = Self::get_owner_archived_bills(&env, &owner);

        let mut staging: Vec<(u32, ArchivedBill)> = Vec::new(&env);
        for id in owner_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = archived.get(id) else {
                continue;
            };
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

    /// Returns a page of archived bills for `owner` using the `ARCH_IDX` per-owner index.
    ///
    /// # Parameters
    /// - `owner`: The address whose archived bills are queried.
    /// - `cursor`: Exclusive lower bound on bill ID. Pass `0` to start from the beginning.
    ///   The next page starts after the last returned ID (use `next_cursor` from the previous page).
    /// - `limit`: Maximum items to return per page. `0` defaults to `DEFAULT_PAGE_LIMIT` (20).
    ///   Values above `MAX_PAGE_LIMIT` (50) are clamped to `MAX_PAGE_LIMIT`.
    ///
    /// # Returns
    /// `ArchivedBillPage` with:
    /// - `items`: Up to `clamp_limit(limit)` archived bills in strictly ascending bill ID order.
    /// - `next_cursor`: ID of the last item on this page if more pages exist; `0` if this is the last page.
    /// - `count`: Number of items in `items`.
    ///
    /// # Ordering
    /// Items are returned in strictly ascending bill ID order, matching the order maintained in `ARCH_IDX`.
    ///
    /// # Gas Complexity
    /// O(clamp_limit(limit)) `ARCH_BILL` map lookups regardless of total archive size, because
    /// only the owner's index entry is read rather than scanning the full `ARCH_BILL` map.
    pub fn get_archived_bills_page(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> ArchivedBillPage {
        let effective_limit = clamp_limit(limit);
        let archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));

        let ids = Self::get_owner_index(&env, &owner);
        let mut items: Vec<ArchivedBill> = Vec::new(&env);

        for id in ids.iter() {
            if id <= cursor {
                continue;
            }
            if let Some(bill) = archived.get(id) {
                items.push_back(bill);
            }
            if items.len() > effective_limit {
                break;
            }
        }

        let has_next = items.len() > effective_limit;
        let mut next_cursor: u32 = 0;

        if has_next {
            // next_cursor = last item on the current page (before truncation)
            let last_idx = effective_limit - 1;
            if let Some(bill) = items.get(last_idx) {
                next_cursor = bill.id;
            }
            // Truncate to effective_limit
            let mut truncated: Vec<ArchivedBill> = Vec::new(&env);
            for i in 0..effective_limit {
                if let Some(bill) = items.get(i) {
                    truncated.push_back(bill);
                }
            }
            items = truncated;
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
        let bill_currency = bill.currency.clone();
        bills.remove(bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if removed_unpaid_amount > 0 {
            Self::adjust_unpaid_total(&env, &caller, -removed_unpaid_amount);
        }
        // Remove from owner index
        Self::index_remove_active(&env, &caller, bill_id);
        // Remove from currency index
        Self::index_remove_currency(&env, &caller, &bill_currency, bill_id);
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
        let mut owner_to_archived: Map<Address, Vec<u32>> = Map::new(&env);
        let mut owner_currency_to_removed: Map<(Address, String), Vec<u32>> = Map::new(&env);

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

                    let mut list = owner_to_archived
                        .get(bill.owner.clone())
                        .unwrap_or_else(|| Vec::new(&env));
                    list.push_back(id);
                    owner_to_archived.set(bill.owner.clone(), list);

                    // Track currency for index removal
                    let currency_key = (bill.owner.clone(), bill.currency.clone());
                    let mut currency_list = owner_currency_to_removed
                        .get(currency_key.clone())
                        .unwrap_or_else(|| Vec::new(&env));
                    currency_list.push_back(id);
                    owner_currency_to_removed.set(currency_key, currency_list);

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

        // Update owner indexes in batch per owner
        for (owner, ids) in owner_to_archived.iter() {
            Self::index_remove_active_batch(&env, &owner, &ids);
            Self::index_add_archived_batch(&env, &owner, &ids);
        }

        // Update currency indexes in batch per (owner, currency)
        for ((owner, currency), ids) in owner_currency_to_removed.iter() {
            Self::index_remove_currency_batch(&env, &owner, &currency, &ids);
        }

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
            name: archived_bill.name,
            external_ref: archived_bill.external_ref,
            amount: archived_bill.amount,
            due_date: env.ledger().timestamp() + SECONDS_PER_DAY,
            recurring: false,
            frequency_days: 0,
            paid: true,
            created_at: env.ledger().timestamp(),
            paid_at: Some(archived_bill.paid_at),
            schedule_id: None,
            tags: archived_bill.tags.clone(),
            currency: archived_bill.currency.clone(),
        };

        bills.set(bill_id, restored_bill);
        archived.remove(bill_id);

        Self::index_remove_archived(&env, &caller, bill_id);
        Self::index_add_active(&env, &caller, bill_id);
        // Add back to currency index
        Self::index_add_currency(&env, &caller, &archived_bill.currency, bill_id);

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
        let mut owner_to_removed: Map<Address, Vec<u32>> = Map::new(&env);

        for (id, bill) in archived.iter() {
            if bill.archived_at < before_timestamp {
                if let Some(ref r) = bill.external_ref {
                    Self::release_external_ref(&env, &bill.owner, r);
                }

                let mut list = owner_to_removed
                    .get(bill.owner.clone())
                    .unwrap_or_else(|| Vec::new(&env));
                list.push_back(id);
                owner_to_removed.set(bill.owner.clone(), list);

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

        // Update owner indexes in batch per owner
        for (owner, ids) in owner_to_removed.iter() {
            Self::index_remove_archived_batch(&env, &owner, &ids);
        }
        Self::update_storage_stats(&env);

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

        if bill_ids.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut success_count = 0u32;
        let mut unpaid_delta = 0i128;
        let current_time = env.ledger().timestamp();
        let mut next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        for bill_id in bill_ids.iter() {
            let mut bill = match bills.get(bill_id) {
                Some(b) => b,
                None => continue,
            };

            if bill.owner != caller || bill.paid {
                continue;
            }

            let amount = bill.amount;
            bill.paid = true;
            bill.paid_at = Some(current_time);

            if bill.recurring {
                next_id = next_id.saturating_add(1);
                let period = (bill.frequency_days as u64)
                    .checked_mul(SECONDS_PER_DAY)
                    .ok_or(Error::InvalidFrequency)?;
                let mut next_due_date = bill
                    .due_date
                    .checked_add(period)
                    .ok_or(Error::InvalidDueDate)?;
                while next_due_date <= current_time {
                    next_due_date = next_due_date
                        .checked_add(period)
                        .ok_or(Error::InvalidDueDate)?;
                }
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
                // Update owner index for the newly spawned recurring bill
                Self::index_add_active(&env, &caller, next_id);
                // Update currency index for the newly spawned recurring bill
                Self::index_add_currency(&env, &caller, &bill.currency, next_id);
                env.events().publish(
                    (symbol_short!("bill"), BillEvent::RecurringBillCreated),
                    (next_id, bill_id, next_due_date),
                );
            } else {
                unpaid_delta = unpaid_delta.saturating_sub(amount);
            }

            bills.set(bill_id, bill);
            success_count += 1;

            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                EventPriority::High,
                symbol_short!("paid"),
                (bill_id, caller.clone(), amount),
            );
        }

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

        Ok(success_count)
    }

    /// Sum of all **unpaid** bill amounts for the given `owner`.
    ///
    /// # Overflow Behavior
    /// Uses **saturating addition** to prevent panic on overflow. If the total would
    /// exceed i128::MAX, returns i128::MAX instead. This ensures the aggregation is
    /// always bounded and predictable, even with arbitrarily many large bills.
    ///
    /// # Performance Note
    /// Results are cached in an unpaid-totals map for faster repeated queries.
    /// The cache is invalidated on bill creation/payment.
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
                // Use saturating_add to prevent overflow panics
                total = total.saturating_add(bill.amount);
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

        // Use the currency index for O(owner_currency_bills) traversal instead of O(owner_bills).
        let currency_ids = Self::get_bills_by_owner_currency(&env, &owner, &normalized_currency);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in currency_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = bills.get(id) else {
                continue;
            };
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
        let limit = clamp_limit(limit);
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        // Use the currency index for O(owner_currency_bills) traversal instead of O(owner_bills).
        let currency_ids = Self::get_bills_by_owner_currency(&env, &owner, &normalized_currency);

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in currency_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if bill.paid {
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
    /// # Overflow Behavior
    /// Uses **saturating addition** to prevent panic on overflow. If the total would
    /// exceed i128::MAX, returns i128::MAX instead. This ensures the aggregation is
    /// always bounded and predictable, even with arbitrarily many large bills.
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
                // Use saturating_add to prevent overflow panics
                total = total.saturating_add(bill.amount);
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

    /// Read the owner's archived bill ID list from ARCH_IDX.
    /// Returns an empty Vec if no entry exists for this owner.
    fn get_owner_index(env: &Env, owner: &Address) -> Vec<u32> {
        let idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&ARCH_IDX_KEY)
            .unwrap_or_else(|| Map::new(env));
        idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env))
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
        let next = current.saturating_add(delta);
        totals.set(owner.clone(), next);
        env.storage()
            .instance()
            .set(&STORAGE_UNPAID_TOTALS, &totals);
    }
}

#[cfg(test)]
mod test;
