#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use remitwise_common::{
    clamp_limit, CreateBillConfig, EventCategory, EventPriority, RemitwiseEvents,
    ARCHIVE_BUMP_AMOUNT, ARCHIVE_LIFETIME_THRESHOLD, CONTRACT_VERSION, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD, MAX_BATCH_SIZE,
};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

#[derive(Clone, Debug)]
#[contracttype]
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

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    BillNotFound = 1,
    BillAlreadyPaid = 2,
    InvalidAmount = 3,
    InvalidFrequency = 4,
    Unauthorized = 5,
    ContractPaused = 6,
    UnauthorizedPause = 7,
    FunctionPaused = 8,
    BatchTooLarge = 9,
    BatchValidationFailed = 10,
    InvalidLimit = 11,
    InvalidDueDate = 12,
    InvalidTag = 13,
    EmptyTags = 14,
    InvalidRecurrenceCombination = 15,
    FrequencyOverflow = 16,
}

#[derive(Clone)]
#[contracttype]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
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
}

#[derive(Clone)]
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
    /// @notice Validate create-bill inputs before any state write.
    /// @dev Fails fast with explicit error variants for each invalid case.
    fn validate_create_bill_inputs(
        env: &Env,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
    ) -> Result<(), Error> {
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let current_time = env.ledger().timestamp();
        if due_date == 0 || due_date <= current_time {
            return Err(Error::InvalidDueDate);
        }

        if recurring {
            if frequency_days == 0 {
                return Err(Error::InvalidFrequency);
            }

            let cadence_secs = (frequency_days as u64)
                .checked_mul(86_400)
                .ok_or(Error::FrequencyOverflow)?;
            due_date
                .checked_add(cadence_secs)
                .ok_or(Error::FrequencyOverflow)?;
        } else if frequency_days != 0 {
            return Err(Error::InvalidRecurrenceCombination);
        }

        Ok(())
    }

    /// Create a new bill
    ///
    /// # Arguments
    /// * `owner` - Address of the bill owner (must authorize)
    /// * `name` - Name of the bill (e.g., "Electricity", "School Fees")
    /// * `amount` - Amount to pay (must be positive)
    /// * `due_date` - Due date as Unix timestamp
    /// * `recurring` - Whether this is a recurring bill
    /// * `frequency_days` - Frequency in days for recurring bills (must be > 0 if recurring)
    /// * `external_ref` - Optional external system reference ID
    ///
    /// # Returns
    /// The ID of the created bill
    ///
    /// # Errors
    /// * `InvalidAmount` - If amount is zero or negative
    /// * `InvalidDueDate` - If due_date is zero or not strictly in the future
    /// * `InvalidFrequency` - If recurring is true but frequency_days is 0
    /// * `InvalidRecurrenceCombination` - If recurring is false and frequency_days is non-zero
    /// * `FrequencyOverflow` - If recurrence cadence can overflow due date arithmetic
    /// @notice Creates a bill only when all input constraints are valid.
    /// @dev Validation executes before any storage mutation (fail-fast semantics).
    fn get_pause_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("PAUSE_ADM"))
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
    fn require_not_paused(env: &Env, func: Symbol) -> Result<(), Error> {
        if Self::get_global_paused(env) {
            return Err(Error::ContractPaused);
        }
        if Self::is_function_paused(env, func) {
            return Err(Error::FunctionPaused);
        }
        Ok(())
    }

    /// Clamp a caller-supplied limit to [1, MAX_PAGE_LIMIT].
    /// A value of 0 is treated as DEFAULT_PAGE_LIMIT.
    pub fn set_pause_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        caller.require_auth();
        let current = Self::get_pause_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    return Err(Error::UnauthorizedPause);
                }
            }
            Some(admin) if admin != caller => return Err(Error::UnauthorizedPause),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSE_ADM"), &new_admin);
        Ok(())
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::UnauthorizedPause)?;
        if admin != caller {
            return Err(Error::UnauthorizedPause);
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

    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::UnauthorizedPause)?;
        if admin != caller {
            return Err(Error::UnauthorizedPause);
        }
        let unpause_at: Option<u64> = env.storage().instance().get(&symbol_short!("UNP_AT"));
        if let Some(at) = unpause_at {
            if env.ledger().timestamp() < at {
                return Err(Error::ContractPaused);
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

    pub fn schedule_unpause(env: Env, caller: Address, at_timestamp: u64) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::UnauthorizedPause)?;
        if admin != caller {
            return Err(Error::UnauthorizedPause);
        }
        if at_timestamp <= env.ledger().timestamp() {
            return Err(Error::InvalidAmount);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UNP_AT"), &at_timestamp);
        Ok(())
    }

    pub fn pause_function(env: Env, caller: Address, func: Symbol) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::UnauthorizedPause)?;
        if admin != caller {
            return Err(Error::UnauthorizedPause);
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

    pub fn unpause_function(env: Env, caller: Address, func: Symbol) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::UnauthorizedPause)?;
        if admin != caller {
            return Err(Error::UnauthorizedPause);
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
    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        caller.require_auth();
        let current = Self::get_upgrade_admin(&env);
        match current {
            None => {
                if caller != new_admin {
                    return Err(Error::Unauthorized);
                }
            }
            Some(adm) if adm != caller => return Err(Error::Unauthorized),
            _ => {}
        }
        env.storage()
            .instance()
            .set(&symbol_short!("UPG_ADM"), &new_admin);
        Ok(())
    }
    pub fn set_version(env: Env, caller: Address, new_version: u32) -> Result<(), Error> {
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).ok_or(Error::Unauthorized)?;
        if admin != caller {
            return Err(Error::Unauthorized);
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

    pub fn create_bill(env: Env, owner: Address, config: CreateBillConfig) -> Result<u32, Error> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_BILL)?;
        Self::validate_create_bill_inputs(
            &env,
            config.amount,
            config.due_date,
            config.recurring,
            config.frequency_days,
        )?;

        // Resolve default currency: blank input → "XLM"
        let resolved_currency = if config.currency.is_empty() {
            String::from_str(&env, "XLM")
        } else {
            config.currency
        };

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

        let current_time = env.ledger().timestamp();
        let bill = Bill {
            id: next_id,
            owner: owner.clone(),
            name: config.name.clone(),
            external_ref: config.external_ref,
            amount: config.amount,
            due_date: config.due_date,
            recurring: config.recurring,
            frequency_days: config.frequency_days,
            paid: false,
            created_at: current_time,
            paid_at: None,
            schedule_id: None,
            tags: Vec::new(&env),
            currency: resolved_currency,
        };

        let bill_owner = bill.owner.clone();
        let bill_external_ref = bill.external_ref.clone();
        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::adjust_unpaid_total(&env, &bill_owner, config.amount);

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Created),
            (next_id, bill_owner.clone(), bill_external_ref),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("created"),
            (next_id, bill_owner, config.amount, config.due_date),
        );

        Ok(next_id)
    }

    pub fn pay_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), Error> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;

        if bill.owner != caller {
            return Err(Error::Unauthorized);
        }
        if bill.paid {
            return Err(Error::BillAlreadyPaid);
        }

        let current_time = env.ledger().timestamp();
        bill.paid = true;
        bill.paid_at = Some(current_time);

        if bill.recurring {
            let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
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
                external_ref: bill.external_ref.clone(),
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

        let bill_external_ref = bill.external_ref.clone();
        let paid_amount = bill.amount;
        let was_recurring = bill.recurring;
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if !was_recurring {
            Self::adjust_unpaid_total(&env, &caller, -paid_amount);
        }

        // Emit event for audit trail
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Paid),
            (bill_id, caller.clone(), bill_external_ref),
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
    pub fn get_unpaid_bills(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage {
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
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
    pub fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
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

    /// Get a page of overdue (unpaid + past due_date) bills across all owners.
    ///
    /// Same cursor/limit semantics.
    pub fn get_overdue_bills(env: Env, cursor: u32, limit: u32) -> BillPage {
        let limit = clamp_limit(limit);
        let current_time = env.ledger().timestamp();
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
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
    pub fn get_all_bills(
        env: Env,
        caller: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<BillPage, Error> {
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(Error::Unauthorized)?;
        if admin != caller {
            return Err(Error::Unauthorized);
        }

        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
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
    ) -> Result<(), Error> {
        caller.require_auth();

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;
        if bill.owner != caller {
            return Err(Error::Unauthorized);
        }

        bill.external_ref = external_ref.clone();
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::ExternalRefUpdated),
            (bill_id, caller, external_ref),
        );

        Ok(())
    }

    /// Get all bills (paid and unpaid)
    ///
    /// # Returns
    /// Vec of all Bill structs
    pub fn get_all_bills_legacy(env: Env) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for (_, bill) in bills.iter() {
            result.push_back(bill);
        }
        result
    }

    // -----------------------------------------------------------------------
    // Backward-compat helpers
    // -----------------------------------------------------------------------

    /// Legacy helper: returns ALL unpaid bills for owner in one Vec.
    /// Only safe for owners with a small number of bills. Prefer the
    /// paginated `get_unpaid_bills` for production use.
    pub fn get_all_unpaid_bills_legacy(env: Env, owner: Address) -> Vec<Bill> {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for (_, bill) in bills.iter() {
            if !bill.paid && bill.owner == owner {
                result.push_back(bill);
            }
        }
        result
    }

    // -----------------------------------------------------------------------
    // Archived bill queries (paginated)
    // -----------------------------------------------------------------------

    /// Get a page of archived bills for `owner`.
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

        let mut staging: Vec<(u32, ArchivedBill)> = Vec::new(&env);
        for (id, bill) in archived.iter() {
            if id <= cursor {
                continue;
            }
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

    pub fn cancel_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), Error> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL)?;
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let bill = bills.get(bill_id).ok_or(Error::BillNotFound)?;
        if bill.owner != caller {
            return Err(Error::Unauthorized);
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

    pub fn archive_paid_bills(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, Error> {
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
                    let archived_bill = ArchivedBill {
                        id: bill.id,
                        owner: bill.owner.clone(),
                        name: bill.name.clone(),
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

    pub fn restore_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), Error> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::RESTORE)?;
        Self::extend_instance_ttl(&env);

        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));
        let archived_bill = archived.get(bill_id).ok_or(Error::BillNotFound)?;

        if archived_bill.owner != caller {
            return Err(Error::Unauthorized);
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
            external_ref: None,
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

    pub fn bulk_cleanup_bills(
        env: Env,
        caller: Address,
        before_timestamp: u64,
    ) -> Result<u32, Error> {
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

    pub fn batch_pay_bills(env: Env, caller: Address, bill_ids: Vec<u32>) -> Result<u32, Error> {
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;
        if bill_ids.len() > (MAX_BATCH_SIZE as usize).try_into().unwrap_or(u32::MAX) {
            return Err(Error::BatchTooLarge);
        }
        let bills_map: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        for id in bill_ids.iter() {
            let bill = bills_map.get(id).ok_or(Error::BillNotFound)?;
            if bill.owner != caller {
                return Err(Error::Unauthorized);
            }
            if bill.paid {
                return Err(Error::BillAlreadyPaid);
            }
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
        let mut unpaid_delta = 0i128;
        for id in bill_ids.iter() {
            let mut bill = bills.get(id).ok_or(Error::BillNotFound)?;
            if bill.owner != caller || bill.paid {
                return Err(Error::BatchValidationFailed);
            }
            let amount = bill.amount;
            bill.paid = true;
            bill.paid_at = Some(current_time);
            if bill.recurring {
                next_id = next_id.saturating_add(1);
                let next_due_date = bill.due_date + (bill.frequency_days as u64 * 86400);
                let next_bill = Bill {
                    id: next_id,
                    owner: bill.owner.clone(),
                    name: bill.name.clone(),
                    external_ref: bill.external_ref.clone(),
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
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::Medium,
            symbol_short!("batch_pay"),
            (paid_count, caller),
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
    /// * `owner`    – whose bills to return
    /// * `currency` – currency code to filter by, e.g. `"USDC"`, `"XLM"`
    /// * `cursor`   – start after this bill ID (pass 0 for the first page)
    /// * `limit`    – max items per page (0 → DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`. `next_cursor == 0` means no more pages.
    pub fn get_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
            if bill.owner != owner || bill.currency != currency {
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
    /// Same cursor/limit semantics as `get_bills_by_currency`.
    pub fn get_unpaid_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        let limit = clamp_limit(limit);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for (id, bill) in bills.iter() {
            if id <= cursor {
                continue;
            }
            if bill.owner != owner || bill.paid || bill.currency != currency {
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
    /// # Example
    /// ```text
    /// let usdc_owed = client.get_total_unpaid_by_currency(&owner, &String::from_str(&env, "USDC"));
    /// ```
    pub fn get_total_unpaid_by_currency(env: Env, owner: Address, currency: String) -> i128 {
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let mut total = 0i128;
        for (_, bill) in bills.iter() {
            if !bill.paid && bill.owner == owner && bill.currency == currency {
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
        let next = current.checked_add(delta).unwrap_or_else(|| {
            panic!("Total unpaid amount overflow for owner");
        });
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
mod test;
