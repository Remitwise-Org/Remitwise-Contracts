#![no_std]

mod events;
use events::{EventCategory, EventPriority, RemitwiseEvents};

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, Map, String,
    Symbol, Vec,
};

// Storage TTL constants
const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280;
const INSTANCE_BUMP_AMOUNT: u32 = 518400;
const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280;
const ARCHIVE_BUMP_AMOUNT: u32 = 2592000;

/// Pagination limits
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

#[derive(Clone, Debug)]
#[contracttype]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
    pub schedule_id: Option<u32>,
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

const CONTRACT_VERSION: u32 = 1;
const MAX_BATCH_SIZE: u32 = 50;
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
}

#[contracttype]
#[derive(Clone)]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub paid_at: u64,
    pub archived_at: u64,
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
    fn clamp_limit(limit: u32) -> u32 {
        if limit == 0 {
            DEFAULT_PAGE_LIMIT
        } else if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        }
    }

    // -----------------------------------------------------------------------
    // Pause / upgrade
    // -----------------------------------------------------------------------

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

    pub fn create_bill(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
        currency: String,
    ) -> Result<u32, Error> {
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_BILL)?;

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if recurring && frequency_days == 0 {
            return Err(Error::InvalidFrequency);
        }

        // Resolve default currency: blank input → "XLM"
        let resolved_currency = if currency.len() == 0 {
            String::from_str(&env, "XLM")
        } else {
            currency
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
            name: name.clone(),
            amount,
            due_date,
            recurring,
            frequency_days,
            paid: false,
            created_at: current_time,
            paid_at: None,
            schedule_id: None,
            currency: resolved_currency,
        };

        let bill_owner = bill.owner.clone();
        bills.set(next_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);
        Self::adjust_unpaid_total(&env, &bill_owner, amount);

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("created"),
            (next_id, bill_owner, amount, due_date),
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
                amount: bill.amount,
                due_date: next_due_date,
                recurring: true,
                frequency_days: bill.frequency_days,
                paid: false,
                created_at: current_time,
                paid_at: None,
                schedule_id: bill.schedule_id,
                currency: bill.currency.clone(),
            };
            bills.set(next_id, next_bill);
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
        }

        let paid_amount = bill.amount;
        let was_recurring = bill.recurring;
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if !was_recurring {
            Self::adjust_unpaid_total(&env, &caller, -paid_amount);
        }

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
        let limit = Self::clamp_limit(limit);
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
        let limit = Self::clamp_limit(limit);
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
        let limit = Self::clamp_limit(limit);
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

        let limit = Self::clamp_limit(limit);
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
        let limit = Self::clamp_limit(limit);
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
            amount: archived_bill.amount,
            due_date: env.ledger().timestamp() + 2592000,
            recurring: false,
            frequency_days: 0,
            paid: true,
            created_at: archived_bill.paid_at,
            paid_at: Some(archived_bill.paid_at),
            schedule_id: None,
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
        if bill_ids.len() > (MAX_BATCH_SIZE as usize).try_into().unwrap() {
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
                    amount: bill.amount,
                    due_date: next_due_date,
                    recurring: true,
                    frequency_days: bill.frequency_days,
                    paid: false,
                    created_at: current_time,
                    paid_at: None,
                    schedule_id: bill.schedule_id,
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
        let limit = Self::clamp_limit(limit);
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
        let limit = Self::clamp_limit(limit);
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
        let next = if delta >= 0 {
            current.saturating_add(delta)
        } else {
            current.saturating_sub(delta.saturating_abs())
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
                &String::from_str(&env, "XLM"),
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
            client.create_bill(
                &owner_a,
                &String::from_str(&env, "Bill A"),
                &(100i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
                &false,
                &0,
            );
            client.create_bill(
                &owner_b,
                &String::from_str(&env, "Bill B"),
                &(200i128 * (i as i128 + 1)),
                &(env.ledger().timestamp() + 86400 * (i as u64 + 1)),
                &false,
                &0,
            );
        }

        // Paginate through owner_a with small page size
        let mut all_a_bills: soroban_sdk::Vec<Bill> = soroban_sdk::Vec::new(&env);
        let mut cursor = 0u32;
        loop {
            let page = client.get_unpaid_bills(&owner_a, &cursor, &2);
            for bill in page.items.iter() {
                assert_eq!(
                    bill.owner, owner_a,
                    "Paginated result must never contain owner_b's bill"
                );
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
        let page = client.get_overdue_bills(&0, &10);
        assert_eq!(page.count, 0);
    }

    #[test]
    fn test_get_overdue_bills_pagination() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        for _ in 0..6u32 {
            client.create_bill(
                &owner,
                &String::from_str(&env, "Overdue Bill"),
                &100,
                &0,
                &false,
                &0,
                &String::from_str(&env, "XLM"),
            );
        }

        env.ledger().set_timestamp(1);

        let page1 = client.get_overdue_bills(&0, &4);
        assert_eq!(page1.count, 4);
        assert!(page1.next_cursor > 0);

        let page2 = client.get_overdue_bills(&page1.next_cursor, &4);
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
        // Test: paid_at timestamp does NOT affect next bill's due_date calculation
        // Bill 1: due_date=1000000, paid_at=1000500 (paid 500 seconds late)
        // Bill 2: due_date should be 1000000 + (30*86400), NOT 1000500 + (30*86400)
        let env = make_env();
        env.ledger().set_timestamp(1_000_500); // Set current time to 500 seconds after due date
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let base_due_date = 1_000_000u64;
        let bill_id = client.create_bill(
            &owner,
            &String::from_str(&env, "Late Payment Test"),
            &300,
            &base_due_date,
            &true, // recurring
            &30,   // frequency_days = 30
            &String::from_str(&env, "XLM"),
        );

        // Pay the bill (at time 1_000_500, which is 500 seconds after due_date)
        client.pay_bill(&owner, &bill_id);

        // Verify original bill has paid_at set
        let paid_bill = client.get_bill(&bill_id).unwrap();
        assert!(paid_bill.paid, "Bill should be marked as paid");
        assert_eq!(
            paid_bill.paid_at,
            Some(1_000_500),
            "paid_at should be set to current time"
        );

        // Verify next bill's due_date is based on original due_date, NOT paid_at
        let next_bill = client.get_bill(&2).unwrap();
        let expected_due_date = base_due_date + (30u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Next due date should be based on original due_date, not paid_at"
        );
        assert!(!next_bill.paid, "Next bill should be unpaid");
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
            &String::from_str(&env, "XLM"),
        );

        client.pay_bill(&owner, &bill_id);

        let next_bill = client.get_bill(&2).unwrap();
        let expected = 1_000_000u64 + (14u64 * 86400);
        assert_eq!(next_bill.due_date, expected);
        assert_eq!(next_bill.due_date, 2_209_600);
    }

    /// Issue #102 – When pay_bill is called on a recurring bill, the contract
    /// creates the next occurrence.  This test asserts every cloned field
    /// individually so that a regression in the clone logic (e.g. paid left
    /// true, wrong due_date, wrong owner) is caught immediately.
    #[test]
    fn test_recurring_bill_clone_fields() {
        let env = make_env();
        env.mock_all_auths();
        let cid = env.register_contract(None, BillPayments);
        let client = BillPaymentsClient::new(&env, &cid);
        let owner = Address::generate(&env);

        let original_due_date: u64 = 1_000_000;
        let frequency: u32 = 30;
        let amount: i128 = 10_000;
        let bill_name = String::from_str(&env, "Rent");

        let bill_id = client.create_bill(
            &owner,
            &bill_name,
            &amount,
            &original_due_date,
            &true,      // recurring
            &frequency, // frequency_days
        );

        // Pay the recurring bill – this should create the next occurrence.
        client.pay_bill(&owner, &bill_id);

        // The next bill should have ID = bill_id + 1.
        let next_id = bill_id + 1;
        let next_bill = client
            .get_bill(&next_id)
            .expect("Next recurring bill should exist after paying the original");

        // --- Assert every cloned field ---
        assert_eq!(
            next_bill.name, bill_name,
            "Cloned bill must preserve the original name"
        );
        assert_eq!(
            next_bill.amount, amount,
            "Cloned bill must preserve the original amount"
        );
        assert_eq!(
            next_bill.recurring, true,
            "Cloned bill must remain recurring"
        );
        assert_eq!(
            next_bill.frequency_days, frequency,
            "Cloned bill must preserve frequency_days"
        );
        assert_eq!(
            next_bill.owner, owner,
            "Cloned bill must preserve the original owner"
        );
        assert_eq!(next_bill.paid, false, "Cloned bill must start as unpaid");
        assert_eq!(
            next_bill.paid_at, None,
            "Cloned bill must have paid_at = None"
        );

        let expected_due_date = original_due_date + (frequency as u64 * 86400);
        assert_eq!(
            next_bill.due_date, expected_due_date,
            "Cloned bill due_date must be original_due_date + frequency_days * 86400"
        );
    }
}
