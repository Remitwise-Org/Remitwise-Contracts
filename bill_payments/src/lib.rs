#![no_std]
//! ## Storage-key layout
//!
//! Bill and schedule state lives in **instance storage**. Request-key receipts
//! use individual persistent-storage entries so one retry record can expire or
//! be extended independently from the business state. The two primary bill
//! keys are:
//!
//! - `NEXT_ID: u32` -- a monotonically increasing counter. It only ever
//!   increases (on `create_bill`, and again on the auto-created next bill
//!   inside `pay_bill` when a recurring bill comes due); it is never
//!   decremented, even when the bill it was minted for is later removed via
//!   `cancel_bill`. This makes `NEXT_ID` an upper bound on bill ids that
//!   have ever existed, not a count of bills that currently exist.
//! - `BILLS: Map<u32, Bill>` -- every bill ever created, keyed by the id it
//!   was minted with. **Ids in this map are not contiguous**: `cancel_bill`
//!   removes an entry outright rather than tombstoning it, so `1..=NEXT_ID`
//!   is a valid *iteration range* (see `get_all_bills`, `get_unpaid_bills`,
//!   `get_overdue_bills`) but must always be probed with `Map::get` --
//!   never assumed to be present -- since cancelled ids leave gaps.
//!
//! The instance keys share one TTL, bumped together by `extend_instance_ttl` on
//! every write. There is no per-bill TTL: a single bill cannot outlive (or
//! be evicted independently of) the rest of this contract's instance
//! storage.
use remitwise_common::{
    amount::{AmountValidationError, validate_amount},
    bump_cross_contract_epoch, check_and_increment_rate_limit, clamp_limit,
    get_cross_contract_epoch, get_trusted_orchestrator, guard_cross_contract_write,
    require_no_active_kill_switch, require_stable_currency, require_within_settlement_window,
    set_trusted_orchestrator, CrossContractEpochError, TrustedOrchestratorError,
    reversible_op::{BillPaymentsReversible, ReversibleOpError},
    EventCategory, EventPriority, RemitwiseEvents, Timestamp, ARCHIVE_BUMP_AMOUNT,
    ARCHIVE_LIFETIME_THRESHOLD, DEFAULT_CURRENCY, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD, MAX_BATCH_SIZE, MAX_CURRENCY_LEN, MAX_SETTLEMENT_WINDOW_SECS,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD, SNAPSHOT_KEY, SNAPSHOT_VERSION,
};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, Map, String, Symbol, Vec,
};

mod state;

pub const CONTRACT_VERSION: u32 = 2;
pub const EVENT_VERSION: u32 = 1;


/// Validates that a currency string consists entirely of ASCII alphabetic characters.
/// This is a first-pass sanity check that rejects non-letter characters before
/// the stable-currency allowlist check in `validate_and_normalize_currency`.
fn is_valid_currency_chars(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&b| b.is_ascii_alphabetic())
}

pub mod params;
pub use params::*;

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

#[contracttype]
#[derive(Clone)]
pub struct BillSchedule {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub currency: String,
    pub next_due: u64,
    pub interval: u64,
    pub recurring: bool,
    pub active: bool,
    pub created_at: u64,
    pub last_executed: Option<u64>,
    pub missed_count: u32,
}

/// Paginated result for bill queries

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillSchedule {
    pub id: u32,
    pub owner: Address,
    pub bill_id: u32,
    pub next_due: u64,
    pub interval: u64,
    pub active: bool,
    pub missed_count: u32,
}

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

/// Paginated result for bill schedule queries.
///
/// See [Pagination Handbook](../../docs/PAGINATION_HANDBOOK.md) for cursor semantics.
#[contracttype]
#[derive(Clone)]
pub struct BillSchedulePage {
    /// The bill schedules for this page, ordered by ascending schedule ID.
    pub items: Vec<BillSchedule>,
    /// The ID to pass as `cursor` for the next page. `0` means no more pages.
    pub next_cursor: u32,
    /// Number of items returned in this page.
    pub count: u32,
}

/// An archived bill that has been moved from active storage to cold storage.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub amount: i128,
    pub paid_at: Option<u64>,
    pub archived_at: u64,
    pub tags: Vec<String>,
    pub currency: String,
}

/// Paginated result for archived bill queries.
#[contracttype]
#[derive(Clone)]
pub struct ArchivedBillPage {
    pub items: Vec<ArchivedBill>,
    pub next_cursor: u32,
    pub count: u32,
}

impl ArchivedBillPage {
    /// Returns the first archived bill in the page, or a typed error when the page is empty.
    pub fn first(&self) -> Result<ArchivedBill, BillPaymentsError> {
        match self.items.get(0) {
            Some(bill) => Ok(bill.clone()),
            None => Err(BillPaymentsError::EmptyPage),
        }
    }
}

impl BillPage {
    /// Returns the first bill in the page, or a typed error when the page is empty.
    pub fn first(&self) -> Result<Bill, BillPaymentsError> {
        match self.items.get(0) {
            Some(bill) => Ok(bill.clone()),
            None => Err(BillPaymentsError::EmptyPage),
        }
    }
}

pub mod pause_functions {
    use soroban_sdk::symbol_short;
    pub const CREATE_BILL: soroban_sdk::Symbol = symbol_short!("crt_bill");
    pub const PAY_BILL: soroban_sdk::Symbol = symbol_short!("pay_bill");
    pub const CANCEL_BILL: soroban_sdk::Symbol = symbol_short!("can_bill");
    pub const ARCHIVE: soroban_sdk::Symbol = symbol_short!("archive");
    pub const RESTORE: soroban_sdk::Symbol = symbol_short!("restore");
    pub const CREATE_BILL_SCHEDULE: soroban_sdk::Symbol = symbol_short!("crt_bsch");
    pub const MODIFY_BILL_SCHEDULE: soroban_sdk::Symbol = symbol_short!("mod_bsch");
    pub const CANCEL_BILL_SCHEDULE: soroban_sdk::Symbol = symbol_short!("can_bsch");
    pub const EXECUTE_BILL_SCHEDULES: soroban_sdk::Symbol = symbol_short!("exe_bsch");
    pub const ADD_TAGS: soroban_sdk::Symbol = symbol_short!("add_tags");
    pub const REM_TAGS: soroban_sdk::Symbol = symbol_short!("rem_tags");
    pub const SET_EXT_REF: soroban_sdk::Symbol = symbol_short!("ext_ref");
    pub const REVERSE_PAYMENT: soroban_sdk::Symbol = symbol_short!("rev_pay");
}

const STORAGE_UNPAID_TOTALS: Symbol = symbol_short!("UNPD_TOT");
const STORAGE_EXT_REF_IDX: Symbol = symbol_short!("EXTRIDX");
const STORAGE_OWNER_INDEX: Symbol = symbol_short!("OWN_IDX");
const STORAGE_ARCH_INDEX: Symbol = symbol_short!("ARCH_IDX");
const STORAGE_CURRENCY_INDEX: Symbol = symbol_short!("CUR_IDX");
const STORAGE_NEXT_BSCH: Symbol = symbol_short!("NEXT_BSCH");
const STORAGE_OWNER_BSCH_IDX: Symbol = symbol_short!("OWN_BSCH");
const STORAGE_BSCHEDS: Symbol = symbol_short!("BSCHEDS");

#[contracttype]
#[derive(Clone)]
struct RequestJournalKey {
    actor: Address,
    request_key: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBillRequest {
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub external_ref: Option<String>,
    pub currency: String,
    pub schedule_id: Option<u32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayBillRequest {
    pub caller: Address,
    pub orchestrator: Address,
    pub epoch: u64,
    pub bill_id: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillTransitionRequest {
    pub caller: Address,
    pub bill_id: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateBillScheduleRequest {
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub currency: String,
    pub next_due: u64,
    pub interval: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModifyBillScheduleRequest {
    pub caller: Address,
    pub schedule_id: u32,
    pub amount: i128,
    pub next_due: u64,
    pub interval: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BillScheduleTransitionRequest {
    pub caller: Address,
    pub schedule_id: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecuteBillSchedulesRequest {
    pub executor: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BillPaymentRequest {
    CreateBill(CreateBillRequest),
    PayBill(PayBillRequest),
    CancelBill(BillTransitionRequest),
    RestoreBill(BillTransitionRequest),
    CreateBillSchedule(CreateBillScheduleRequest),
    ModifyBillSchedule(ModifyBillScheduleRequest),
    CancelBillSchedule(BillScheduleTransitionRequest),
    ExecuteDueBillSchedules(ExecuteBillSchedulesRequest),
}

#[contracttype]
#[derive(Clone, Debug)]
pub enum BillPaymentResult {
    BillId(u32),
    Pay(AtomicPayReceipt),
    Unit,
    Bool(bool),
    ScheduleIds(Vec<u32>),
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RequestJournalEntry {
    pub request: BillPaymentRequest,
    pub result: BillPaymentResult,
}

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
    InvalidFrequency = 4,
    /// Caller is not authorized for this operation
    Unauthorized = 5,
    AdminNotInitialized = 6,
    AdminAlreadyInitialized = 7,
    NoPendingRotation = 8,
    TimelockNotElapsed = 9,
    /// Returned when a page has zero items.
    EmptyPage = 10,
    /// Currency code is invalid (too long, wrong characters, or not in allowlist).
    InvalidCurrency = 11,
    /// Currency code is not a supported stable asset.
    UnsupportedCurrency = 12,
    /// External reference string is too short, too long, or contains invalid characters.
    InvalidExternalRef = 13,
    /// External reference is already in use by another bill for this owner.
    DuplicateExternalRef = 14,
    /// Pause admin grant has expired.
    AdminGrantExpired = 15,
    /// Contract is globally paused.
    ContractPaused = 16,
    /// The requested function is paused.
    FunctionPaused = 17,
    /// Caller is not the pause admin.
    UnauthorizedPause = 18,
    /// Pre-upgrade snapshot not found.
    SnapshotNotFound = 19,
    /// A limit (pagination, cap) was out of allowed bounds.
    InvalidLimit = 20,
    /// Pre-upgrade snapshot is older than the freshness window.
    SnapshotTooOld = 21,
    /// Bill or schedule name is empty or too long.
    InvalidName = 22,
    /// Due date is 0, in the past, or would overflow on recurrence.
    InvalidDueDate = 23,
    /// Schedule interval is less than the minimum allowed.
    ScheduleIntervalTooShort = 24,
    /// Schedule lead time exceeds the maximum allowed.
    ScheduleLeadTimeTooLong = 25,
    /// Maximum number of schedules per owner exceeded.
    ScheduleCapExceeded = 26,
    /// Schedule with the given ID does not exist.
    ScheduleNotFound = 27,
    /// Schedule is not active.
    ScheduleNotActive = 28,
    /// Rate limit for this operation has been exceeded.
    RateLimitExceeded = 29,
    /// Settlement time exceeds the due date plus grace period.
    SettlementWindowExpired = 30,
    /// Per-owner bill cap has been reached.
    OwnerBillCapExceeded = 31,
    /// Tag content is invalid (too long or contains disallowed characters).
    InvalidTagContent = 32,
    /// Batch operation exceeds the maximum batch size.
    BatchTooLarge = 33,
    /// `set_upgrade_admin` was called with `new_admin` equal to the current
    /// upgrade admin — rejected so a mistyped no-op rotation is caught at the
    /// call site instead of silently doing nothing.
    SameAdmin = 34,
    /// `init_admin` was called with a `rotation_timelock_seconds` below
    /// `MIN_SCHEDULE_INTERVAL` — too short to serve its purpose of giving the
    /// legitimate admin a window to notice and react to a rotation proposal.
    RotationTimelockTooShort = 35,
    /// State transition is not allowed.
    InvalidStateTransition = 36,
    /// Invariant violation detected - bill data is inconsistent.
    InvariantViolation = 36,
    /// Amount arithmetic (per-owner unpaid totals, batch deltas) would
    /// overflow `i128`. Rejected deterministically (panic → full revert)
    /// instead of silently saturating, so balances can never be truncated.
    AmountOverflow = 37,
    /// Amount exceeds the shared maximum (`remitwise_common::MAX_AMOUNT`).
    AmountExceedsMax = 38,
    /// Schedule interval exceeds `MAX_SCHEDULE_INTERVAL` (100 years).
    ScheduleIntervalTooLong = 39,
    /// The `u32` bill/schedule id counter is exhausted (`u32::MAX`).
    IdSpaceExhausted = 40,
}

pub type Error = BillPaymentsError;

/// Receipt returned by atomic pay operations.
///
/// Captures the deterministic result of `pay_bill` so callers can verify
/// the operation completed without partial state.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AtomicPayReceipt {
    pub bill_id: u32,
    pub paid_amount: i128,
    pub child_bill_id: Option<u32>,
    pub child_due_date: Option<u64>,
}

/// Receipt returned by atomic batch pay operations.
///
/// Each entry captures the result of one bill in the batch.
/// The entire batch is atomic: either all entries succeed or
/// the entire operation returns an error with no state changes.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AtomicBatchPayReceipt {
    pub paid_count: u32,
    pub receipts: Vec<AtomicPayReceipt>,
}
#[contracttype]

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillEvent {
    Created = 0,
    Paid = 1,
    ExternalRefUpdated = 2,
    Cancelled = 3,
    Archived = 4,
    Restored = 5,
    ScheduleCreated = 6,
    ScheduleExecuted = 7,
    ScheduleMissed = 8,
    ScheduleModified = 9,
    ScheduleCancelled = 10,
    RecurringBillCreated = 11,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringControlEvent {
    pub version: u32,
    pub seq: u64,
    pub kind: Symbol,
    pub actor: Option<Address>,
    pub timestamp: u64,
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

/// Pre-upgrade snapshot for upgrade rollback protection.
///
/// Captures critical instance storage (ID counter, version, admin, pause state)
/// before a contract upgrade so state can be restored if the upgrade fails.
#[contracttype]
#[derive(Clone)]
pub struct PreUpgradeSnapshot {
    /// Snapshot schema version (`SNAPSHOT_VERSION`).
    pub schema_version: u32,
    /// Next bill ID counter.
    pub next_id: u32,
    /// Contract version at snapshot time.
    pub version: u32,
    /// Upgrade admin address, if set.
    pub upgrade_admin: Option<Address>,
    /// Pause state.
    pub paused: bool,
    /// Pause admin address, if set.
    pub pause_admin: Option<Address>,
}

/// Sane default for the admin-rotation timelock, used when a deployment
/// doesn't have its own opinion. See [`Self::init_admin`] to configure a
/// different window per deployment.
///
/// ## Why a timelock
///
/// Admin rotation is a two-step, delayed process (`propose_admin_rotation`
/// then, once the timelock has elapsed, `finalize_admin_rotation`) rather
/// than an instant one-step handoff. If the current admin's key is ever
/// compromised, an attacker who calls `propose_admin_rotation` does not
/// walk away with control -- the rotation just sits pending, publicly
/// visible via `get_pending_admin_rotation`, for this many seconds before
/// it can take effect. That window gives the legitimate admin (or anyone
/// watching `AdminEvent::RotationProposed`) time to notice the proposal
/// and respond, rather than a single signature being an irreversible,
/// instant takeover.

/// A rotation that has been proposed but not yet finalized.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PendingAdminRotation {
    pub new_admin: Address,
    /// Ledger timestamp at/after which `finalize_admin_rotation` may run.
    pub executable_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub enum AdminEvent {
    Initialized,
    RotationProposed,
    RotationFinalized,
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
        let mut ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        let len = ids.len();
        let append_at_end = match ids.get(len.saturating_sub(1)) {
            None => true,
            Some(last) => last < bill_id,
        };
        if append_at_end {
            ids.push_back(bill_id);
            idx.set(owner.clone(), ids);
            env.storage().instance().set(&STORAGE_OWNER_INDEX, &idx);
            return;
        }

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
        let mut ids = idx.get(key.clone()).unwrap_or_else(|| Vec::new(env));
        let len = ids.len();
        let append_at_end = match ids.get(len.saturating_sub(1)) {
            None => true,
            Some(last) => last < bill_id,
        };
        if append_at_end {
            ids.push_back(bill_id);
            idx.set(key, ids);
            Self::save_currency_index(env, &idx);
            return;
        }

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
    // BillSchedule owner-index helpers
    // -----------------------------------------------------------------------

    fn get_owner_bill_schedules(env: &Env, owner: &Address) -> Vec<u32> {
        let idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_BSCH_IDX)
            .unwrap_or_else(|| Map::new(env));
        idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env))
    }

    fn index_add_bill_schedule(env: &Env, owner: &Address, schedule_id: u32) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_BSCH_IDX)
            .unwrap_or_else(|| Map::new(env));
        let mut ids = idx.get(owner.clone()).unwrap_or_else(|| Vec::new(env));
        ids.push_back(schedule_id);
        idx.set(owner.clone(), ids);
        env.storage().instance().set(&STORAGE_OWNER_BSCH_IDX, &idx);
    }

    fn index_remove_bill_schedule(env: &Env, owner: &Address, schedule_id: u32) {
        let mut idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_BSCH_IDX)
            .unwrap_or_else(|| Map::new(env));
        let Some(ids) = idx.get(owner.clone()) else {
            return;
        };
        let mut new_ids: Vec<u32> = Vec::new(env);
        for id in ids.iter() {
            if id != schedule_id {
                new_ids.push_back(id);
            }
        }
        if new_ids.is_empty() {
            idx.remove(owner.clone());
        } else {
            idx.set(owner.clone(), new_ids);
        }
        env.storage().instance().set(&STORAGE_OWNER_BSCH_IDX, &idx);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn request_journal_key(actor: &Address, request_key: &BytesN<32>) -> RequestJournalKey {
        RequestJournalKey {
            actor: actor.clone(),
            request_key: request_key.clone(),
        }
    }

    fn lookup_request(
        env: &Env,
        actor: &Address,
        request_key: &BytesN<32>,
        request: &BillPaymentRequest,
    ) -> Result<Option<BillPaymentResult>, BillPaymentsError> {
        let key = Self::request_journal_key(actor, request_key);
        let entry: Option<RequestJournalEntry> = env.storage().persistent().get(&key);
        if let Some(entry) = entry {
            if entry.request == *request {
                env.storage().persistent().extend_ttl(
                    &key,
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                Ok(Some(entry.result))
            } else {
                Err(BillPaymentsError::RequestKeyConflict)
            }
        } else {
            Ok(None)
        }
    }

    fn commit_request(
        env: &Env,
        actor: &Address,
        request_key: &BytesN<32>,
        request: BillPaymentRequest,
        result: BillPaymentResult,
    ) {
        let key = Self::request_journal_key(actor, request_key);
        env.storage()
            .persistent()
            .set(&key, &RequestJournalEntry { request, result });
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

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

        // Empty string defaults to the platform default currency
        if len == 0 {
            return Ok(String::from_str(env, DEFAULT_CURRENCY));
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
            // Only whitespace - default to platform default currency
            return Ok(String::from_str(env, DEFAULT_CURRENCY));
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

        let upper_str = core::str::from_utf8(&upper[..trimmed.len()]).unwrap_or(DEFAULT_CURRENCY);

        // Defence-in-depth: reject rebase/deflationary tokens.
        // After normalizing to uppercase, verify the symbol is a recognized stable asset.
        let sym = Symbol::new(env, upper_str);
        require_stable_currency(env, &sym).map_err(|_| BillPaymentsError::UnsupportedCurrency)?;

        Ok(String::from_str(env, upper_str))
    }

    /// Legacy helper for backward compatibility - normalizes without strict validation.
    /// WARNING: This does not validate currency codes. Use validate_and_normalize_currency
    /// for new code to ensure proper currency validation.
    fn normalize_currency(env: &Env, currency: &String) -> String {
        // For backward compatibility, try validation first, fall back on error
        match Self::validate_and_normalize_currency(env, currency) {
            Ok(normalized) => normalized,
            Err(_) => String::from_str(env, DEFAULT_CURRENCY),
        }
    }

    /// Validate an amount against the shared exact-integer rules.
    ///
    /// Amounts are exact integers in the asset's smallest unit. Two rules are
    /// enforced at **every** boundary that accepts an amount:
    /// 1. **Sign** — zero and negative amounts are rejected (`InvalidAmount`).
    /// 2. **Scale / magnitude** — amounts above
    ///    [`remitwise_common::MAX_AMOUNT`] are rejected (`AmountExceedsMax`)
    ///    so that per-owner totals and batch deltas can never overflow `i128`.
    ///
    /// This check runs **before any state change**, so a rejected value can
    /// never leave partial state behind.
    fn validate_bill_amount(amount: i128) -> Result<(), BillPaymentsError> {
        match validate_amount(amount) {
            Ok(()) => Ok(()),
            Err(AmountValidationError::NonPositive) => Err(BillPaymentsError::InvalidAmount),
            Err(AmountValidationError::ExceedsMaximum) => Err(BillPaymentsError::AmountExceedsMax),
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
    fn require_admin_grant_valid(env: &Env) -> Result<(), BillPaymentsError> {
        let granted_at: Option<u64> = env.storage().instance().get(&symbol_short!("PADM_GT"));
        match granted_at {
            Some(granted) => {
                let now = env.ledger().timestamp();
                if now >= granted.saturating_add(ADMIN_GRANT_TTL) {
                    Err(BillPaymentsError::AdminGrantExpired)
                } else {
                    Ok(())
                }
            }
            None => {
                // Legacy: no grant timestamp stored. Migration path: store now so
                // the TTL clock starts from the next time the admin is read.
                env.storage()
                    .instance()
                    .set(&symbol_short!("PADM_GT"), &env.ledger().timestamp());
                Ok(())
            }
        }
    }
    fn get_next_bill_id(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
    }
    fn get_next_bill_schedule_id(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&STORAGE_NEXT_BSCH)
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

        // Defense-in-depth: Validate admin grant TTL before allowing admin changes
        // Prevents bypass of the 30-day admin grant expiration mechanism
        let current = Self::get_pause_admin(&env);
        if current.is_some() {
            // Only enforce TTL validation when there's an existing admin
            // (first-time setup when current is None is allowed to proceed)
            Self::require_admin_grant_valid(&env)?;
        }

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
        env.storage()
            .instance()
            .set(&symbol_short!("PADM_GT"), &env.ledger().timestamp());
        Ok(())
    }

    /// @notice Pause all state-changing operations.
    /// @dev Requires the pause admin to authenticate. Cancels any pending unpause schedule.
    /// @return Ok(()) on success, otherwise `Error::UnauthorizedPause`.
    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_AT"), &env.ledger().timestamp());
        // Cancel any pending unpause schedule to prevent timelock bypass
        env.storage().instance().remove(&symbol_short!("UNP_AT"));
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_PAUSED_V2),
            remitwise_common::events::PauseEvent {
                paused_at: env.ledger().timestamp(),
                paused_by: caller.clone(),
            },
        );
        Ok(())
    }

    /// @notice Unpause the contract if no time-lock is active.
    /// @dev If `schedule_unpause` set a future timestamp, unpause is blocked until then.
    /// @return Ok(()) on success, otherwise `Error::ContractPaused` or `Error::UnauthorizedPause`.
    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
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
        env.storage().instance().remove(&symbol_short!("PAUSED_AT"));
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_UNPAUSED_V2),
            remitwise_common::events::UnpauseEvent {
                unpaused_at: env.ledger().timestamp(),
                unpaused_by: caller.clone(),
            },
        );
        Ok(())
    }

    /// @notice Schedule the earliest time the contract may be unpaused.
    /// @dev Time-locks unpause to a future `at_timestamp` (ledger timestamp seconds).
    /// @return Ok(()) on success, otherwise `Error::InvalidAmount` or `Error::UnauthorizedPause`.
    pub fn schedule_unpause(env: Env, caller: Address, at_timestamp: u64) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_admin_grant_valid(&env)?;
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::UnauthorizedPause)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_AT"), &env.ledger().timestamp());
        env.storage().instance().remove(&symbol_short!("UNP_AT"));
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            soroban_sdk::Symbol::new(&env, remitwise_common::events::ACTION_PAUSED_V2),
            remitwise_common::events::PauseEvent {
                paused_at: env.ledger().timestamp(),
                paused_by: caller.clone(),
            },
        );

        let mut paused_functions: Map<Symbol, bool> = env
            .storage()
            .instance()
            .get(&symbol_short!("PAUSED_FN"))
            .unwrap_or_else(|| Map::new(&env));
        for func in [
            pause_functions::CREATE_BILL,
            pause_functions::PAY_BILL,
            pause_functions::CANCEL_BILL,
            pause_functions::ARCHIVE,
            pause_functions::RESTORE,
            pause_functions::CREATE_BILL_SCHEDULE,
            pause_functions::MODIFY_BILL_SCHEDULE,
            pause_functions::CANCEL_BILL_SCHEDULE,
            pause_functions::EXECUTE_BILL_SCHEDULES,
            pause_functions::ADD_TAGS,
            pause_functions::REM_TAGS,
            pause_functions::SET_EXT_REF,
        ] {
            paused_functions.set(func, true);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED_FN"), &paused_functions);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        Self::get_global_paused(&env)
    }
    pub fn get_paused_since(env: Env) -> Option<u64> {
        if Self::is_paused(env.clone()) {
            env.storage().instance().get(&symbol_short!("PAUSED_AT"))
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
    pub fn is_function_paused_public(env: Env, func: Symbol) -> bool {
        Self::is_function_paused(&env, func)
    }
    pub fn get_pause_admin_public(env: Env) -> Option<Address> {
        Self::get_pause_admin(&env)
    }
    pub fn refresh_admin_grant(env: Env, caller: Address) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        let admin = Self::get_pause_admin(&env).ok_or(BillPaymentsError::AdminGrantExpired)?;
        if admin != caller {
            return Err(BillPaymentsError::UnauthorizedPause);
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PADM_GT"), &env.ledger().timestamp());
        Ok(())
    }
    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("VERSION"))
            .unwrap_or(CONTRACT_VERSION)
    }

    fn next_event_seq(env: &Env) -> u64 {
        let prev: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("EV_SEQ"))
            .unwrap_or(0);
        let next = prev.saturating_add(1);
        env.storage().instance().set(&symbol_short!("EV_SEQ"), &next);
        next
    }

    fn emit_control_event(env: &Env, kind: Symbol, actor: Option<&Address>, timestamp: u64) {
        let seq = Self::next_event_seq(env);
        env.events().publish(
            (symbol_short!("recurring"), symbol_short!("control")),
            RecurringControlEvent {
                version: EVENT_VERSION,
                seq,
                kind,
                actor: actor.cloned(),
                timestamp,
            },
        );
    }
    fn get_upgrade_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("UPG_ADM"))
    }
    /// Set or transfer the upgrade admin role.
    ///
    /// # Security Requirements
    /// - If no upgrade admin exists, caller must equal new_admin (bootstrap pattern)
    /// - If upgrade admin exists, only current upgrade admin can transfer
    /// - If upgrade admin exists, `new_admin` must differ from the current upgrade
    ///   admin — unlike the pause admin, there is no TTL grant to refresh here, so
    ///   a same-admin call can only be a mistake (e.g. a copy-pasted address)
    /// - Caller must be authenticated via require_auth()
    ///
    /// # Parameters
    /// - `caller`: The address attempting to set the upgrade admin
    /// - `new_admin`: The address to become the new upgrade admin
    ///
    /// # Returns
    /// - `Ok(())` on successful admin transfer
    /// - `Err(Error::Unauthorized)` if caller lacks permission
    /// - `Err(Error::SameAdmin)` if `new_admin` is already the upgrade admin
    pub fn set_upgrade_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();

        let current_upgrade_admin = Self::get_upgrade_admin(&env);

        // Authorization logic:
        // 1. If no upgrade admin exists, caller must equal new_admin (bootstrap)
        // 2. If upgrade admin exists, only current upgrade admin can transfer,
        //    and only to a genuinely different address
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
                if *current_admin == new_admin {
                    return Err(Error::SameAdmin);
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
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

    /// Capture a pre-upgrade snapshot of critical instance storage.
    ///
    /// Call this before performing a contract upgrade. The snapshot is stored
    /// under `SNAPSHOT_KEY` in persistent storage and can be restored via
    /// `restore_from_snapshot` if the upgrade needs to be rolled back.
    ///
    /// # Authorization
    /// Only the upgrade admin may take a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the upgrade admin
    ///
    /// # Events
    /// Emits `snap_pre` event on success.
    pub fn pre_upgrade(env: Env, caller: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).ok_or(BillPaymentsError::Unauthorized)?;
        if admin != caller {
            return Err(BillPaymentsError::Unauthorized);
        }
        let snapshot = PreUpgradeSnapshot {
            schema_version: SNAPSHOT_VERSION,
            next_id: Self::get_next_bill_id(&env),
            version: Self::get_version(env.clone()),
            upgrade_admin: Self::get_upgrade_admin(&env),
            paused: Self::get_global_paused(&env),
            pause_admin: Self::get_pause_admin(&env),
        };
        env.storage().persistent().set(&SNAPSHOT_KEY, &snapshot);
        env.storage()
            .persistent()
            .set(&symbol_short!("SNAP_TS"), &env.ledger().timestamp());
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("snap_pre"),
            SNAPSHOT_VERSION,
        );
        Ok(())
    }

    /// Restore critical instance storage from a pre-upgrade snapshot.
    ///
    /// Reads the snapshot stored by `pre_upgrade` and writes the captured
    /// ID counter, version, upgrade admin, and pause state back to instance
    /// storage. The snapshot is consumed after a successful restore.
    ///
    /// # Authorization
    /// Only the upgrade admin may restore from a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the upgrade admin
    /// - `UnsupportedVersion` if the snapshot version is not supported
    ///
    /// # Events
    /// Emits `snap_rst` event on success.
    pub fn restore_from_snapshot(env: Env, caller: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).ok_or(BillPaymentsError::Unauthorized)?;
        if admin != caller {
            return Err(BillPaymentsError::Unauthorized);
        }
        let snapshot: PreUpgradeSnapshot = env
            .storage()
            .persistent()
            .get(&SNAPSHOT_KEY)
            .ok_or(BillPaymentsError::SnapshotNotFound)?;
        if snapshot.schema_version != SNAPSHOT_VERSION {
            return Err(BillPaymentsError::InvalidLimit);
        }
        let snapshot_taken_at: u64 = env
            .storage()
            .persistent()
            .get(&symbol_short!("SNAP_TS"))
            .unwrap_or(0);
        if remitwise_common::require_recent_snapshot(&env, snapshot_taken_at).is_err() {
            return Err(BillPaymentsError::SnapshotTooOld);
        }
        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &snapshot.next_id);
        env.storage()
            .instance()
            .set(&symbol_short!("VERSION"), &snapshot.version);
        match &snapshot.upgrade_admin {
            Some(addr) => env
                .storage()
                .instance()
                .set(&symbol_short!("UPG_ADM"), addr),
            None => env.storage().instance().remove(&symbol_short!("UPG_ADM")),
        }
        env.storage()
            .instance()
            .set(&symbol_short!("PAUSED"), &snapshot.paused);
        match &snapshot.pause_admin {
            Some(addr) => env
                .storage()
                .instance()
                .set(&symbol_short!("PAUSE_ADM"), addr),
            None => env.storage().instance().remove(&symbol_short!("PAUSE_ADM")),
        }
        env.storage().persistent().remove(&SNAPSHOT_KEY);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("snap_rst"),
            snapshot.version,
        );
        Ok(())
    }

    /// Discard a pre-upgrade snapshot without restoring it.
    ///
    /// Use after a successful upgrade to free persistent storage.
    ///
    /// # Authorization
    /// Only the upgrade admin may discard a snapshot.
    ///
    /// # Errors
    /// - `Unauthorized` if `caller` is not the upgrade admin
    pub fn discard_snapshot(env: Env, caller: Address) -> Result<(), Error> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        let admin = Self::get_upgrade_admin(&env).ok_or(BillPaymentsError::Unauthorized)?;
        if admin != caller {
            return Err(BillPaymentsError::Unauthorized);
        }
        env.storage().persistent().remove(&SNAPSHOT_KEY);
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("snap_dsc"),
            (),
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Bill schedule lifecycle
    // -----------------------------------------------------------------------

    /// Creates a recurring bill schedule.
    ///
    /// # Arguments
    /// * `owner` - Address of the schedule owner (must authorize)
    /// * `name` - Name template for generated bills
    /// * `amount` - Amount for each generated bill
    /// * `currency` - Currency code for generated bills
    /// * `next_due` - First execution timestamp
    /// * `interval` - Seconds between executions; 0 creates a one-off schedule
    ///
    /// # Returns
    /// ID of the new schedule
    pub fn create_bill_schedule(
        env: Env,
        owner: Address,
        name: String,
        amount: i128,
        currency: String,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_BILL_SCHEDULE)?;
        Self::create_bill_schedule_core(
            &env, owner, name, amount, currency, next_due, interval,
        )
    }

    pub fn create_bill_schedule_keyed(
        env: Env,
        owner: Address,
        request_key: BytesN<32>,
        name: String,
        amount: i128,
        currency: String,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, BillPaymentsError> {
        owner.require_auth();
        let request = BillPaymentRequest::CreateBillSchedule(CreateBillScheduleRequest {
            owner: owner.clone(),
            name: name.clone(),
            amount,
            currency: currency.clone(),
            next_due,
            interval,
        });
        if let Some(result) = Self::lookup_request(&env, &owner, &request_key, &request)? {
            return match result {
                BillPaymentResult::BillId(id) => Ok(id),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        Self::require_not_paused(&env, pause_functions::CREATE_BILL_SCHEDULE)?;
        let schedule_id = Self::create_bill_schedule_core(
            &env, owner.clone(), name, amount, currency, next_due, interval,
        )?;
        Self::commit_request(
            &env,
            &owner,
            &request_key,
            request,
            BillPaymentResult::BillId(schedule_id),
        );
        Ok(schedule_id)
    }

    fn create_bill_schedule_core(
        env: &Env,
        owner: Address,
        name: String,
        amount: i128,
        currency: String,
        next_due: u64,
        interval: u64,
    ) -> Result<u32, BillPaymentsError> {
        // Validate schedule name length
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(BillPaymentsError::InvalidName);
        }
        if amount <= 0 {
            return Err(BillPaymentsError::InvalidAmount);
        }

        // Amount must satisfy the shared exact-integer rules (positive sign,
        // bounded magnitude) BEFORE any state change. Previously the amount
        // was accepted unvalidated, letting a zero/negative/oversized value
        // propagate into generated bills and per-owner totals.
        Self::validate_bill_amount(amount)?;

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(BillPaymentsError::InvalidDueDate);
        }

        let resolved_currency = Self::validate_and_normalize_currency(env, &currency)?;

        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(BillPaymentsError::ScheduleIntervalTooShort);
        }

        // Cap the interval so execution can always make progress: a bound
        // interval guarantees the child `frequency_days` fits the per-bill
        // cap and `next_due + interval` can never overflow `u64`.
        if interval > MAX_SCHEDULE_INTERVAL {
            return Err(BillPaymentsError::ScheduleIntervalTooLong);
        }

        if Timestamp::seconds_until(current_time, next_due) > MAX_SCHEDULE_LEAD_TIME {
            return Err(BillPaymentsError::ScheduleLeadTimeTooLong);
        }

        let owner_schedule_count = Self::get_owner_bill_schedules(env, &owner).len();
        if owner_schedule_count >= MAX_BILL_SCHEDULES_PER_OWNER {
            return Err(BillPaymentsError::ScheduleCapExceeded);
        }

        Self::extend_instance_ttl(env);

        // Checked increment: id exhaustion (u32::MAX) is rejected instead of
        // silently wrapping and reusing an existing schedule id.
        let next_schedule_id = Self::get_next_bill_schedule_id(&env)
            .checked_add(1)
            .ok_or(BillPaymentsError::IdSpaceExhausted)?;

        let schedule = BillSchedule {
            id: next_schedule_id,
            owner: owner.clone(),
            name,
            amount,
            currency: resolved_currency,
            next_due,
            interval,
            recurring: interval > 0,
            active: true,
            created_at: current_time,
            last_executed: None,
            missed_count: 0,
        };

        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(env));
        schedules.set(next_schedule_id, schedule);
        env.storage().instance().set(&STORAGE_BSCHEDS, &schedules);

        env.storage()
            .instance()
            .set(&STORAGE_NEXT_BSCH, &next_schedule_id);

        Self::index_add_bill_schedule(env, &owner, next_schedule_id);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::ScheduleCreated),
            (next_schedule_id, owner.clone()),
        );

        Self::emit_control_event(env, symbol_short!("cre_sch"), Some(&owner), current_time);

        Ok(next_schedule_id)
    }

    /// Modify an existing bill schedule owned by `caller`.
    pub fn modify_bill_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::MODIFY_BILL_SCHEDULE)?;
        Self::modify_bill_schedule_core(&env, caller, schedule_id, amount, next_due, interval)
    }

    pub fn modify_bill_schedule_keyed(
        env: Env,
        caller: Address,
        request_key: BytesN<32>,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, BillPaymentsError> {
        caller.require_auth();
        let request = BillPaymentRequest::ModifyBillSchedule(ModifyBillScheduleRequest {
            caller: caller.clone(),
            schedule_id,
            amount,
            next_due,
            interval,
        });
        if let Some(result) = Self::lookup_request(&env, &caller, &request_key, &request)? {
            return match result {
                BillPaymentResult::Bool(val) => Ok(val),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        Self::require_not_paused(&env, pause_functions::MODIFY_BILL_SCHEDULE)?;
        let res = Self::modify_bill_schedule_core(&env, caller.clone(), schedule_id, amount, next_due, interval)?;
        Self::commit_request(
            &env,
            &caller,
            &request_key,
            request,
            BillPaymentResult::Bool(res),
        );
        Ok(res)
    }

    fn modify_bill_schedule_core(
        env: &Env,
        caller: Address,
        schedule_id: u32,
        amount: i128,
        next_due: u64,
        interval: u64,
    ) -> Result<bool, BillPaymentsError> {
        // Shared exact-integer amount rules (sign + magnitude), before any
        // state change. The previous `amount <= 0` guard did not bound the
        // magnitude; an oversized amount could threaten per-owner totals.
        Self::validate_bill_amount(amount)?;

        let current_time = env.ledger().timestamp();
        if next_due <= current_time {
            return Err(BillPaymentsError::InvalidDueDate);
        }

        if interval > 0 && interval < MIN_SCHEDULE_INTERVAL {
            return Err(BillPaymentsError::ScheduleIntervalTooShort);
        }

        if interval > MAX_SCHEDULE_INTERVAL {
            return Err(BillPaymentsError::ScheduleIntervalTooLong);
        }

        if Timestamp::seconds_until(current_time, next_due) > MAX_SCHEDULE_LEAD_TIME {
            return Err(BillPaymentsError::ScheduleLeadTimeTooLong);
        }

        Self::extend_instance_ttl(env);

        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(env));

        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(BillPaymentsError::ScheduleNotFound)?;

        if !schedule.active {
            return Err(BillPaymentsError::ScheduleNotActive);
        }

        if schedule.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        schedule.amount = amount;
        schedule.next_due = next_due;
        schedule.interval = interval;
        schedule.recurring = interval > 0;

        schedules.set(schedule_id, schedule);
        env.storage().instance().set(&STORAGE_BSCHEDS, &schedules);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::ScheduleModified),
            schedule_id,
        );

        Self::emit_control_event(env, symbol_short!("mod_sch"), Some(&caller), current_time);

        Ok(true)
    }

    /// Cancel an existing bill schedule owned by `caller`.
    pub fn cancel_bill_schedule(
        env: Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL_SCHEDULE)?;
        Self::cancel_bill_schedule_core(&env, caller, schedule_id)
    }

    pub fn cancel_bill_schedule_keyed(
        env: Env,
        caller: Address,
        request_key: BytesN<32>,
        schedule_id: u32,
    ) -> Result<bool, BillPaymentsError> {
        caller.require_auth();
        let request = BillPaymentRequest::CancelBillSchedule(BillScheduleTransitionRequest {
            caller: caller.clone(),
            schedule_id,
        });
        if let Some(result) = Self::lookup_request(&env, &caller, &request_key, &request)? {
            return match result {
                BillPaymentResult::Bool(val) => Ok(val),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL_SCHEDULE)?;
        let res = Self::cancel_bill_schedule_core(&env, caller.clone(), schedule_id)?;
        Self::commit_request(
            &env,
            &caller,
            &request_key,
            request,
            BillPaymentResult::Bool(res),
        );
        Ok(res)
    }

    fn cancel_bill_schedule_core(
        env: &Env,
        caller: Address,
        schedule_id: u32,
    ) -> Result<bool, BillPaymentsError> {
        check_and_increment_rate_limit(
            env,
            &caller,
            pause_functions::CANCEL_BILL_SCHEDULE,
            CANCEL_SCHEDULE_RATE_LIMIT,
        )
        .map_err(|_| BillPaymentsError::ScheduleRateLimitExceeded)?;

        let current_time = env.ledger().timestamp();
        Self::extend_instance_ttl(env);

        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(env));

        let mut schedule = schedules
            .get(schedule_id)
            .ok_or(BillPaymentsError::ScheduleNotFound)?;

        if !schedule.active {
            return Err(BillPaymentsError::ScheduleNotActive);
        }

        if schedule.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        schedule.active = false;

        schedules.set(schedule_id, schedule);
        env.storage().instance().set(&STORAGE_BSCHEDS, &schedules);

        Self::index_remove_bill_schedule(env, &caller, schedule_id);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::ScheduleCancelled),
            schedule_id,
        );

        Self::emit_control_event(env, symbol_short!("can_sch"), Some(&caller), current_time);

        Ok(true)
    }

    pub fn execute_due_bill_schedules_keyed(
        env: Env,
        executor: Address,
        request_key: BytesN<32>,
    ) -> Result<Vec<u32>, BillPaymentsError> {
        executor.require_auth();
        let request =
            BillPaymentRequest::ExecuteDueBillSchedules(ExecuteBillSchedulesRequest {
                executor: executor.clone(),
            });
        if let Some(result) = Self::lookup_request(&env, &executor, &request_key, &request)? {
            return match result {
                BillPaymentResult::ScheduleIds(ids) => Ok(ids),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        if Self::get_global_paused(&env) {
            return Err(BillPaymentsError::ContractPaused);
        }
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|_| panic!("cannot write: kill switch is active"));
        let executed = Self::execute_due_bill_schedules_core(&env, Some(&executor));
        Self::commit_request(
            &env,
            &executor,
            &request_key,
            request,
            BillPaymentResult::ScheduleIds(executed.clone()),
        );
        Ok(executed)
    }

    pub fn execute_due_bill_schedules(env: Env) -> Vec<u32> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|_| panic!("cannot write: kill switch is active"));
        Self::execute_due_bill_schedules_core(&env, None)
    }

    fn execute_due_bill_schedules_core(env: &Env, actor: Option<&Address>) -> Vec<u32> {
        Self::extend_instance_ttl(env);

        if Self::get_global_paused(env) {
            return Vec::new(env);
        }

        let current_time = env.ledger().timestamp();
        let mut executed = Vec::new(env);

        let next_schedule_id = Self::get_next_bill_schedule_id(env);

        let mut schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(env));

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));

        let mut next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        let start_schedule_id = env
            .storage()
            .instance()
            .get(&symbol_short!("EXE_CURS"))
            .unwrap_or(1u32);

        let mut schedules_checked = 0;
        let mut next_cursor = start_schedule_id;

        for schedule_id in start_schedule_id..=next_schedule_id {
            // Checked cursor increment: at `u32::MAX` the "past the end"
            // cursor is unrepresentable; reject deterministically rather than
            // wrapping back into the schedule id space.
            next_cursor = schedule_id
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(&env, BillPaymentsError::IdSpaceExhausted));
            schedules_checked += 1;

            if schedules_checked > MAX_BATCH_SIZE as u32 {
                next_cursor = schedule_id; // Will resume from this ID on the next call
                break;
            }

            let Some(mut schedule) = schedules.get(schedule_id) else {
                continue;
            };

            if !schedule.active || schedule.next_due > current_time {
                continue;
            }

            // Legacy state may contain non-positive amounts from before schedule
            // creation validated them. Leave such schedules untouched so they
            // remain inspectable and cancellable without minting invalid bills.
            if schedule.amount <= 0 {
                continue;
            }

            if let Some(last_exec) = schedule.last_executed {
                if last_exec >= schedule.next_due {
                    continue;
                }
            }

            // Defence-in-depth: a schedule stored before amount validation
            // existed (or restored from a corrupt snapshot) must not be able
            // to mint an invalid bill or move an owner's unpaid total. The
            // panic reverts the whole invocation, so nothing above is
            // committed and no partial state survives.
            Self::validate_bill_amount(schedule.amount)
                .unwrap_or_else(|e| panic_with_error!(&env, e));

            schedule.last_executed = Some(current_time);

            if schedule.recurring && schedule.interval > 0 {
                let mut missed = 0u32;
                // Checked next-due arithmetic. The first addition previously
                // used plain `+` (wrap-around risk in release builds); the
                // catch-up loop previously used `saturating_add`, which could
                // silently clamp `next_due`. Both now reject deterministically.
                let mut next = schedule
                    .next_due
                    .checked_add(schedule.interval)
                    .unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::AmountOverflow)
                    });
                while next <= current_time {
                    missed = missed.checked_add(1).unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::AmountOverflow)
                    });
                    next = next.checked_add(schedule.interval).unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::AmountOverflow)
                    });
                }
                schedule.missed_count = schedule
                    .missed_count
                    .checked_add(missed)
                    .unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::AmountOverflow)
                    });
                schedule.next_due = next;

                if missed > 0 {
                    env.events().publish(
                        (symbol_short!("bill"), BillEvent::ScheduleMissed),
                        (schedule_id, missed),
                    );
                }

                // Exact interval → frequency_days conversion. `interval` is
                // capped at `MAX_SCHEDULE_INTERVAL` on create/modify, so this
                // always fits `[1, MAX_FREQUENCY_DAYS]` for new schedules. A
                // legacy schedule with an out-of-range interval cannot be
                // represented exactly and is rejected (owner must modify or
                // cancel it) instead of silently truncating `frequency_days`.
                let freq_days_u64 = (schedule.interval / SECONDS_PER_DAY).max(1);
                let freq_days = u32::try_from(freq_days_u64).unwrap_or_else(|_| {
                    panic_with_error!(&env, BillPaymentsError::InvariantViolation)
                });
                if freq_days > MAX_FREQUENCY_DAYS {
                    panic_with_error!(&env, BillPaymentsError::InvariantViolation);
                }

                let owner_bill_count = Self::get_owner_bills(&env, &schedule.owner).len();

                if owner_bill_count < MAX_BILLS_PER_OWNER {
                    // Checked increment: at `u32::MAX` the next bill id is
                    // unrepresentable. The previous `saturating_add(1)` could
                    // silently reuse an existing bill id and overwrite a live
                    // record; reject deterministically instead (panic → the
                    // whole invocation reverts with no partial state).
                    next_id = next_id.checked_add(1).unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::IdSpaceExhausted)
                    });
                    let child = Bill {
                        id: next_id,
                        owner: schedule.owner.clone(),
                        name: schedule.name.clone(),
                        external_ref: None,
                        amount: schedule.amount,
                        due_date: schedule.next_due,
                        recurring: true,
                        frequency_days: freq_days,
                        paid: false,
                        created_at: current_time,
                        paid_at: None,
                        schedule_id: Some(schedule.id),
                        tags: Vec::new(env),
                        currency: schedule.currency.clone(),
                    };
                    bills.set(next_id, child);
                    Self::index_add_active(env, &schedule.owner, next_id);
                    Self::index_add_currency(env, &schedule.owner, &schedule.currency, next_id);
                    Self::adjust_unpaid_total(env, &schedule.owner, schedule.amount);

                        env.events().publish(
                            (symbol_short!("bill"), BillEvent::RecurringBillCreated),
                            (next_id, schedule_id, schedule.next_due),
                        );

                        bills_created_this_call = bills_created_this_call.saturating_add(1);
                    }
                }
            } else {
                schedule.active = false;
            }

            schedules.set(schedule_id, schedule);
            executed.push_back(schedule_id);

            env.events().publish(
                (symbol_short!("bill"), BillEvent::ScheduleExecuted),
                schedule_id,
            );
        }

        if next_cursor > next_schedule_id {
            env.storage().instance().remove(&symbol_short!("EXE_CURS"));
        } else {
            env.storage()
                .instance()
                .set(&symbol_short!("EXE_CURS"), &next_cursor);
        }

        env.storage().instance().set(&STORAGE_BSCHEDS, &schedules);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("NEXT_ID"), &next_id);

        if !executed.is_empty() {
            Self::emit_control_event(env, symbol_short!("exe_sch"), actor, current_time);
        }

        executed
    }

    pub fn get_bill_schedules(env: Env, owner: Address) -> Vec<BillSchedule> {
        let ids = Self::get_owner_bill_schedules(&env, &owner);
        let schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for id in ids.iter() {
            if let Some(schedule) = schedules.get(id) {
                result.push_back(schedule);
            }
        }
        result
    }

    /// Returns a deterministic, cursor-paginated page of bill schedules for `owner`.
    ///
    /// See [Pagination Handbook](../../docs/PAGINATION_HANDBOOK.md)
    /// for invariants all paginated reads must satisfy, cursor semantics, and the
    /// reviewer checklist.
    ///
    /// # Parameters
    /// - `owner` — account whose schedules are fetched (requires auth)
    /// - `cursor` — exclusive schedule ID boundary; pass `0` to start from the first
    ///   schedule. Pass `next_cursor` from the previous page to continue.
    /// - `limit` — max schedules per page. `0` is normalised to `DEFAULT_PAGE_LIMIT`
    ///   (20). Values above `MAX_PAGE_LIMIT` (50) are clamped to `MAX_PAGE_LIMIT`.
    ///
    /// # Returns
    /// `BillSchedulePage { items, next_cursor, count }`.
    /// `next_cursor == 0` means this is the final (or only) page.
    /// An out-of-range cursor or an owner with no schedules returns an empty page
    /// with `next_cursor == 0` — not an error — so callers can safely detect
    /// end-of-list without special-casing.
    ///
    /// # Ordering
    /// Results are ordered by schedule ID ascending (creation order within the
    /// owner index). This ordering is stable across repeated calls provided no
    /// schedules are created between pages, making `cursor` safe to resume with.
    ///
    /// # Cursor semantics (EXCLUSIVE)
    /// - `cursor = 0` — start from the first schedule
    /// - `cursor = N` — return only schedules whose ID is strictly greater than `N`
    /// - `next_cursor` returned is the ID of the last item on this page (or `0` on
    ///   the final page)
    ///
    /// # Security
    /// Only schedules belonging to `owner` are returned. The index is per-owner, so
    /// no cross-owner schedule leakage can occur via cursor manipulation.
    pub fn get_bill_schedules_page(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> BillSchedulePage {
        owner.require_auth();
        let effective_limit = clamp_limit(limit);

        let ids = Self::get_owner_bill_schedules(&env, &owner);

        if ids.is_empty() {
            return BillSchedulePage {
                items: Vec::new(&env),
                next_cursor: 0,
                count: 0,
            };
        }

        let schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(&env));

        // Collect up to effective_limit + 1 items to detect whether a next page exists.
        let mut staging: Vec<BillSchedule> = Vec::new(&env);
        for id in ids.iter() {
            if id <= cursor {
                continue;
            }
            if let Some(schedule) = schedules.get(id) {
                staging.push_back(schedule);
            }
            if staging.len() > effective_limit {
                break;
            }
        }

        let has_next = staging.len() > effective_limit;
        let mut next_cursor: u32 = 0;

        if has_next {
            // next_cursor is the ID of the last item on the current page (not the first skipped).
            let last_idx = effective_limit.saturating_sub(1);
            if let Some(sched) = staging.get(last_idx) {
                next_cursor = sched.id;
            }
            // Truncate to effective_limit items.
            let mut truncated: Vec<BillSchedule> = Vec::new(&env);
            for i in 0..effective_limit {
                if let Some(s) = staging.get(i) {
                    truncated.push_back(s);
                }
            }
            let count = truncated.len();
            return BillSchedulePage {
                items: truncated,
                next_cursor,
                count,
            };
        }

        let count = staging.len();
        BillSchedulePage {
            items: staging,
            next_cursor: 0,
            count,
        }
    }

    pub fn get_bill_schedule(env: Env, schedule_id: u32) -> Option<BillSchedule> {
        let schedules: Map<u32, BillSchedule> = env
            .storage()
            .instance()
            .get(&STORAGE_BSCHEDS)
            .unwrap_or_else(|| Map::new(&env));
        schedules.get(schedule_id)
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        owner.require_auth();
        Self::require_not_paused(&env, pause_functions::CREATE_BILL)?;
        Self::create_bill_core(
            &env,
            owner,
            name,
            amount,
            due_date,
            recurring,
            frequency_days,
            external_ref,
            currency,
            _schedule_id,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_bill_keyed(
        env: Env,
        owner: Address,
        request_key: BytesN<32>,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
        external_ref: Option<String>,
        currency: String,
        schedule_id: Option<u32>,
    ) -> Result<u32, BillPaymentsError> {
        owner.require_auth();
        let request = BillPaymentRequest::CreateBill(CreateBillRequest {
            owner: owner.clone(),
            name: name.clone(),
            amount,
            due_date,
            recurring,
            frequency_days,
            external_ref: external_ref.clone(),
            currency: currency.clone(),
            schedule_id,
        });
        if let Some(result) = Self::lookup_request(&env, &owner, &request_key, &request)? {
            return match result {
                BillPaymentResult::BillId(id) => Ok(id),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        Self::require_not_paused(&env, pause_functions::CREATE_BILL)?;
        let bill_id = Self::create_bill_core(
            &env,
            owner.clone(),
            name,
            amount,
            due_date,
            recurring,
            frequency_days,
            external_ref,
            currency,
            schedule_id,
        )?;
        Self::commit_request(
            &env,
            &owner,
            &request_key,
            request,
            BillPaymentResult::BillId(bill_id),
        );
        Ok(bill_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn create_bill_core(
        env: &Env,
        owner: Address,
        name: String,
        amount: i128,
        due_date: u64,
        recurring: bool,
        frequency_days: u32,
        external_ref: Option<String>,
        currency: String,
        schedule_id: Option<u32>,
    ) -> Result<u32, BillPaymentsError> {
        // Validate bill name length (defence-in-depth: matches insurance and
        // savings_goals which both validate their name parameters).
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(BillPaymentsError::InvalidName);
        }

        // Check rate limit
        check_and_increment_rate_limit(
            env,
            &owner,
            pause_functions::CREATE_BILL,
            CREATE_BILL_RATE_LIMIT,
        )
        .map_err(|_| BillPaymentsError::RateLimitExceeded)?;

        let current_time = env.ledger().timestamp();
        if due_date == 0 || due_date < current_time {
            return Err(BillPaymentsError::InvalidDueDate);
        }

        // Shared exact-integer amount rules (sign + magnitude), before any
        // state change. Rejects zero/negative (`InvalidAmount`) and amounts
        // above the shared maximum (`AmountExceedsMax`) that could overflow
        // per-owner totals.
        Self::validate_bill_amount(amount)?;
        if recurring && (frequency_days == 0 || frequency_days > MAX_FREQUENCY_DAYS) {
            return Err(Error::InvalidFrequency);
        }

        // Validate and normalize currency (strict validation - rejects invalid codes)
        let resolved_currency = Self::validate_and_normalize_currency(env, &currency)?;

        // Validate external_ref if provided
        let validated_ext_ref = Self::validate_optional_external_ref(env, &external_ref)?;

        Self::extend_instance_ttl(env);

        // Enforce per-owner bill cap before touching storage.
        let owner_bill_count = Self::get_owner_bills(env, &owner).len();
        if owner_bill_count >= MAX_BILLS_PER_OWNER {
            return Err(BillPaymentsError::OwnerBillCapExceeded);
        }

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));

        // Checked increment: id exhaustion (u32::MAX) is rejected instead of
        // silently wrapping and reusing an existing bill id (which would
        // overwrite a live record).
        let next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32)
            .checked_add(1)
            .ok_or(BillPaymentsError::IdSpaceExhausted)?;

        // Enforce uniqueness for external_ref if provided
        if let Some(ref r) = validated_ext_ref {
            Self::claim_external_ref(env, &owner, r, next_id)?;
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
            schedule_id,
            tags: Vec::new(env),
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
        Self::index_add_active(env, &bill_owner, next_id);
        // Update currency index
        Self::index_add_currency(env, &bill_owner, &bill_currency, next_id);
        Self::adjust_unpaid_total(env, &bill_owner, amount);

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

        if recurring {
            Self::emit_control_event(env, symbol_short!("cre_rec"), Some(&owner), current_time);
        }

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
    pub fn pay_bill(
        env: Env,
        orchestrator: Address,
        epoch: u64,
        caller: Address,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        guard_cross_contract_write(&env, &orchestrator, epoch)
            .unwrap_or_else(|_| panic_with_error!(&env, CrossContractEpochError::EpochMismatch));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;
        Self::pay_bill_core(&env, caller, bill_id).map(|_| ())
    }

    pub fn pay_bill_keyed(
        env: Env,
        orchestrator: Address,
        epoch: u64,
        caller: Address,
        request_key: BytesN<32>,
        bill_id: u32,
    ) -> Result<AtomicPayReceipt, BillPaymentsError> {
        caller.require_auth();
        let request = BillPaymentRequest::PayBill(PayBillRequest {
            caller: caller.clone(),
            orchestrator: orchestrator.clone(),
            epoch,
            bill_id,
        });
        if let Some(result) = Self::lookup_request(&env, &caller, &request_key, &request)? {
            return match result {
                BillPaymentResult::Pay(receipt) => Ok(receipt),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        guard_cross_contract_write(&env, &orchestrator, epoch)
            .unwrap_or_else(|_| panic_with_error!(&env, CrossContractEpochError::EpochMismatch));
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;
        let receipt = Self::pay_bill_core(&env, caller.clone(), bill_id)?;
        Self::commit_request(
            &env,
            &caller,
            &request_key,
            request,
            BillPaymentResult::Pay(receipt.clone()),
        );
        Ok(receipt)
    }

    fn pay_bill_core(
        env: &Env,
        caller: Address,
        bill_id: u32,
    ) -> Result<AtomicPayReceipt, BillPaymentsError> {
        // Check rate limit
        check_and_increment_rate_limit(
            env,
            &caller,
            pause_functions::PAY_BILL,
            PAY_BILL_RATE_LIMIT,
        )
        .map_err(|_| BillPaymentsError::RateLimitExceeded)?;

        Self::extend_instance_ttl(env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));

        let bill = bills.get(bill_id).ok_or(BillPaymentsError::BillNotFound)?;

        // Check paid status FIRST for backward compatibility
        if bill.paid {
            return Err(BillPaymentsError::BillAlreadyPaid);
        }

        // State transition validation (only for unpaid bills)
        use crate::state::{check_invariants, BillState};
        BillState::validate_transition(&bill, false, BillState::Paid, "pay_bill")?;
        check_invariants(env, &bill, false)?;

        if bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        let current_time = env.ledger().timestamp();

        // Reject settlement outside the allowed window (due date plus the
        // 30-day grace period). This keeps execution deterministic across
        // missed windows: a bill paid far too late is rejected before any
        // state change instead of silently processing a stale obligation.
        require_within_settlement_window(current_time, bill.due_date, MAX_SETTLEMENT_WINDOW_SECS)
            .map_err(|_| BillPaymentsError::SettlementWindowExpired)?;

        let mut child_bill_entry: Option<(u32, Bill)> = None;

        if bill.recurring {
            let owner_bill_count =
                Self::get_owner_bill_count((*env).clone(), bill.owner.clone());
            if owner_bill_count >= MAX_BILLS_PER_OWNER {
                return Err(BillPaymentsError::OwnerBillCapExceeded);
            }
            let period = (bill.frequency_days as u64)
                .checked_mul(SECONDS_PER_DAY)
                .ok_or(Error::InvalidFrequency)?;
            let mut next_due_date = bill
                .due_date
                .checked_add(period)
                .ok_or(Error::InvalidDueDate)?;
            // Advance forward by frequency periods until the next due date is
            // strictly in the future. Each iteration may overflow → InvalidDueDate.
            while next_due_date <= current_time {
                next_due_date = next_due_date
                    .checked_add(period)
                    .ok_or(Error::InvalidDueDate)?;
            }
            // Checked increment: id exhaustion (u32::MAX) is rejected instead
            // of silently wrapping and reusing an existing bill id.
            let next_id = env
                .storage()
                .instance()
                .get(&symbol_short!("NEXT_ID"))
                .unwrap_or(0u32)
                .checked_add(1)
                .ok_or(BillPaymentsError::IdSpaceExhausted)?;

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
            child_bill_entry = Some((next_id, next_bill));
        }

        // -----------------------------------------------------------------
        // Phase 2: All computation succeeded — apply mutations atomically.
        // -----------------------------------------------------------------
        let mut paid_bill = bill.clone();
        paid_bill.paid = true;
        paid_bill.paid_at = Some(current_time);
        bills.set(bill_id, paid_bill.clone());

        let paid_amount = paid_bill.amount;
        let bill_ext_ref = paid_bill.external_ref.clone();
        let child_bill_id = child_bill_entry.as_ref().map(|(id, _)| *id);
        let child_due_date = child_bill_entry
            .as_ref()
            .map(|(_, child)| child.due_date);

        // Net amount added to the owner's unpaid-total cache: paying a bill
        // subtracts its amount; a recurring renewal inserts a fresh unpaid
        // child of the same amount, so the two cancel out (net zero). The
        // child amount is captured before it is moved into storage below.
        let mut total_delta: i128 = -paid_amount;

        if let Some((next_id, next_bill)) = child_bill_entry {
            let child_due_date = next_bill.due_date;
            let child_amount = next_bill.amount;
            total_delta = total_delta
                .checked_add(child_amount)
                .unwrap_or_else(|| panic_with_error!(&env, BillPaymentsError::AmountOverflow));
            bills.set(next_id, next_bill);
            env.storage()
                .instance()
                .set(&symbol_short!("NEXT_ID"), &next_id);
            Self::index_add_active(env, &caller, next_id);
            Self::index_add_currency(env, &caller, &paid_bill.currency, next_id);
            Self::adjust_unpaid_total(env, &caller, child_amount);
            env.events().publish(
                (symbol_short!("bill"), BillEvent::RecurringBillCreated),
                (next_id, bill_id, child_due_date),
            );
        }

        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        // Always adjust the cached unpaid total: subtract the paid amount and,
        // for recurring renewals, add back the freshly created child so the
        // running total stays exact (net-zero for a same-amount renewal).
        Self::adjust_unpaid_total(&env, &caller, total_delta);
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Paid),
            (bill_id, caller.clone(), bill_ext_ref),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("paid"),
            (bill_id, caller.clone(), paid_amount),
        );

        if bill.recurring {
            Self::emit_control_event(env, symbol_short!("pay_rec"), Some(&caller), current_time);
        }

        Ok(AtomicPayReceipt {
            bill_id,
            paid_amount,
            child_bill_id,
            child_due_date,
        })
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|_| panic!("cannot write: kill switch is active"));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ADD_TAGS)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|_| panic!("cannot write: kill switch is active"));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::REM_TAGS)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
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
    /// See [`docs/PAGINATION_HANDBOOK.md`](../../docs/PAGINATION_HANDBOOK.md) for the invariants
    /// all paginated reads must satisfy, cursor semantics, and the reviewer checklist.
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
    /// # Overdue Semantics
    /// A bill is overdue when `!bill.paid && bill.due_date < current_ledger_time`. The
    /// comparison is strict less-than, so a bill whose `due_date == now` is **not** overdue.
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls (including across sparse
    /// IDs left by cancelled/archived bills, which are absent from the index).
    ///
    /// # Gas Complexity
    /// `O(A)` where `A` is the number of **active** (non-archived, non-cancelled) bills
    /// across all owners, *not* the global `NEXT_ID` high-water mark. This walks the
    /// per-owner active index (`OWN_IDX`) instead of scanning `1..=NEXT_ID`, so the cost
    /// no longer grows with historically created-then-removed bills. For a query scoped
    /// to a single owner whose cost tracks only that owner's bills, use
    /// [`Self::get_overdue_bills_for_owner`].
    pub fn get_overdue_bills(env: Env, cursor: u32, limit: u32) -> BillPage {
        let limit = clamp_limit(limit);
        let current_time = env.ledger().timestamp();
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        // Walk the per-owner active index (OWN_IDX) rather than the global
        // `1..=NEXT_ID` range. Each owner's ID list is ascending, so we merge the
        // matching candidates into one globally ID-ascending page using a bounded
        // staging buffer that only ever retains the smallest `limit + 1` IDs.
        let idx: Map<Address, Vec<u32>> = env
            .storage()
            .instance()
            .get(&STORAGE_OWNER_INDEX)
            .unwrap_or_else(|| Map::new(&env));

        let cap = limit + 1;
        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);

        for owner in idx.keys().iter() {
            let owner_ids = idx.get(owner).unwrap_or_else(|| Vec::new(&env));
            for id in owner_ids.iter() {
                if id <= cursor {
                    continue;
                }
                let Some(bill) = bills.get(id) else {
                    continue;
                };
                if bill.paid || bill.due_date >= current_time {
                    continue;
                }
                Self::staging_insert_bounded(&mut staging, id, bill, cap);
            }
        }

        Self::build_page(&env, staging, limit)
    }

    /// @notice Get a paginated list of overdue bills (unpaid + past due_date) for a single owner.
    /// @dev Owner-scoped counterpart to [`Self::get_overdue_bills`]. The global variant is
    /// intentionally cross-owner; this variant restricts results to `owner` and is the cheaper
    /// query when callers only care about one account.
    ///
    /// # Arguments
    /// * `owner`  - Whose overdue bills to return (must authorize the call)
    /// * `cursor` - Start after this bill ID (pass 0 for the first page)
    /// * `limit`  - Max items per page (0 -> DEFAULT_PAGE_LIMIT, capped at MAX_PAGE_LIMIT)
    ///
    /// # Returns
    /// `BillPage { items, next_cursor, count }`.
    /// When `next_cursor == 0` there are no more pages.
    ///
    /// # Overdue Semantics
    /// Identical to [`Self::get_overdue_bills`]: `!bill.paid && bill.due_date < current_ledger_time`
    /// (strict less-than; `due_date == now` is not overdue).
    ///
    /// # Canonical Ordering
    /// Results are always ordered by bill ID ascending. Pagination uses the same
    /// ordering, so `cursor` is stable across repeated calls.
    ///
    /// # Gas Complexity
    /// `O(owner_bills)` — walks only this owner's `OWN_IDX` entry and is bounded by
    /// `MAX_BILLS_PER_OWNER`, independent of the global `NEXT_ID` high-water mark.
    pub fn get_overdue_bills_for_owner(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let current_time = env.ledger().timestamp();
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

    /// Insert `(id, bill)` into `staging` keeping it sorted ascending by bill ID and
    /// capped at `cap` entries (i.e. retaining only the smallest `cap` IDs seen).
    ///
    /// Used by [`Self::get_overdue_bills`] to merge the per-owner indices into one
    /// globally ID-ascending page without materialising every candidate: the buffer
    /// never holds more than `cap` (= `limit + 1`) entries regardless of how many
    /// overdue bills exist across all owners.
    fn staging_insert_bounded(staging: &mut Vec<(u32, Bill)>, id: u32, bill: Bill, cap: u32) {
        let len = staging.len();
        // Buffer is full and already holds `cap` smaller IDs — this one can't make the page.
        if len >= cap {
            if let Some((last_id, _)) = staging.get(len - 1) {
                if id >= last_id {
                    return;
                }
            }
        }
        // Locate the ascending insertion position.
        let mut pos = len;
        for i in 0..len {
            if let Some((sid, _)) = staging.get(i) {
                if id < sid {
                    pos = i;
                    break;
                }
            }
        }
        staging.insert(pos, (id, bill));
        // Drop the largest entry if we exceeded the cap.
        if staging.len() > cap {
            staging.remove(staging.len() - 1);
        }
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
    /// Emits BillEvent::ExternalRefUpdated.
    /// Updates the external reference for a bill.
    ///
    /// # Events
    /// - Secondary topic: `(symbol_short!("bill"), BillEvent::ExternalRefUpdated)`
    /// - Action symbol: `"ext_upd"` via [`RemitwiseEvents::emit`]
    pub fn set_external_ref(
        env: Env,
        caller: Address,
        bill_id: u32,
        external_ref: Option<String>,
    ) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::SET_EXT_REF)?;

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
            // Claim new ref first if provided
            if let Some(ref new_ref) = validated_ext_ref {
                Self::claim_external_ref(&env, &caller, new_ref, bill_id)?;
            }
            // Release old ref only after new ref is successfully claimed
            if let Some(ref old_ref) = bill.external_ref {
                Self::release_external_ref(&env, &caller, old_ref);
            }
        }

        bill.external_ref = validated_ext_ref.clone();
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::ExternalRefUpdated),
            (bill_id, caller.clone(), validated_ext_ref.clone()),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("ext_upd"),
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
    ///
    /// # Security
    /// Requires `owner.require_auth()`. The archived-bill index is per-owner, so
    /// results are scoped to `owner` and no cross-owner leakage can occur via
    /// cursor manipulation.
    pub fn get_archived_bills(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> ArchivedBillPage {
        owner.require_auth();
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
    ///
    /// # Security
    /// Requires `owner.require_auth()`. The archived-bill index is per-owner, so
    /// results are scoped to `owner` and no cross-owner leakage can occur via
    /// cursor manipulation.
    pub fn get_archived_bills_page(
        env: Env,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> ArchivedBillPage {
        owner.require_auth();
        let effective_limit = clamp_limit(limit);
        let archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));

        // Use the archived owner index for O(owner_archived) traversal.
        let owner_ids = Self::get_owner_archived_bills(&env, &owner);

        let mut staging: Vec<ArchivedBill> = Vec::new(&env);
        for id in owner_ids.iter() {
            if id <= cursor {
                continue;
            }
            if let Some(bill) = archived.get(id) {
                staging.push_back(bill);
            }
            if staging.len() > effective_limit {
                break;
            }
        }

        let has_next = staging.len() > effective_limit;
        let mut next_cursor: u32 = 0;

        if has_next {
            // next_cursor = last item on the current page (before truncation)
            let last_idx = effective_limit - 1;
            if let Some(bill) = staging.get(last_idx) {
                next_cursor = bill.id;
            }
            // Truncate to effective_limit
            let mut truncated: Vec<ArchivedBill> = Vec::new(&env);
            for i in 0..effective_limit {
                if let Some(bill) = staging.get(i) {
                    truncated.push_back(bill);
                }
            }
            staging = truncated;
        }

        let count = staging.len();
        ArchivedBillPage {
            items: staging,
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

    /// Emits BillEvent::Cancelled.
    pub fn cancel_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL)?;
        Self::cancel_bill_core(&env, caller, bill_id)
    }

    pub fn cancel_bill_keyed(
        env: Env,
        caller: Address,
        request_key: BytesN<32>,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        let request = BillPaymentRequest::CancelBill(BillTransitionRequest {
            caller: caller.clone(),
            bill_id,
        });
        if let Some(result) = Self::lookup_request(&env, &caller, &request_key, &request)? {
            return match result {
                BillPaymentResult::Unit => Ok(()),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        Self::require_not_paused(&env, pause_functions::CANCEL_BILL)?;
        Self::cancel_bill_core(&env, caller.clone(), bill_id)?;
        Self::commit_request(
            &env,
            &caller,
            &request_key,
            request,
            BillPaymentResult::Unit,
        );
        Ok(())
    }

    fn cancel_bill_core(
        env: &Env,
        caller: Address,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        // Check rate limit
        check_and_increment_rate_limit(
            env,
            &caller,
            pause_functions::CANCEL_BILL,
            CANCEL_BILL_RATE_LIMIT,
        )
        .map_err(|_| BillPaymentsError::RateLimitExceeded)?;
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));
        let bill = bills.get(bill_id).ok_or(BillPaymentsError::BillNotFound)?;

        // State transition validation
        // Check invariants before deletion
        use crate::state::check_invariants;
        check_invariants(env, &bill, false)?;

        if bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }
        // A paid bill is a terminal, audited record: cancelling it here would
        // silently delete that record (and its paid_at trail) without going
        // through reverse_payment, the dedicated typed reversal path that
        // preserves the bill and correctly restores the unpaid total.
        if bill.paid {
            return Err(BillPaymentsError::BillAlreadyPaid);
        }

        // State transition validation: only Active → Cancelled (deletion) is legal.
        // Paid → Cancelled is blocked above; Archived → Cancelled is impossible
        // (archived bills live in ARCH_BILL, not BILLS).
        use crate::state::BillState;
        BillState::validate_transition(&bill, false, BillState::Paid, "cancel_bill")?;

        // Release external_ref if it exists
        if let Some(ref r) = bill.external_ref {
            Self::release_external_ref(env, &caller, r);
        }

        let removed_unpaid_amount = if bill.paid { 0 } else { bill.amount };
        let bill_currency = bill.currency.clone();
        bills.remove(bill_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        if removed_unpaid_amount > 0 {
            Self::adjust_unpaid_total(env, &caller, -removed_unpaid_amount);
        }
        // Remove from owner index
        Self::index_remove_active(env, &caller, bill_id);
        // Remove from currency index
        Self::index_remove_currency(env, &caller, &bill_currency, bill_id);
        env.events().publish(
            (symbol_short!("bill"), BillEvent::Cancelled),
            (bill_id, caller.clone(), env.ledger().timestamp()),
        );
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("cancelled"),
            bill_id,
        );
        Ok(())
    }

    /// Cancels (removes) an unpaid bill by `bill_id`.
    ///
    /// # Events
    /// - Secondary topic: `(symbol_short!("bill"), BillEvent::Cancelled)`
    /// - Action symbol: `"cancelled"` via [`RemitwiseEvents::emit`]
    ///
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::ARCHIVE)?;
        Self::extend_instance_ttl(&env);

        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();

        // -----------------------------------------------------------------
        // Phase 1: Scan and collect all qualifying bills into staging buffers.
        // No storage is modified during this scan. External refs are NOT
        // released yet — that happens in Phase 2 after all computation
        // succeeds, ensuring the operation is fully atomic.
        // -----------------------------------------------------------------
        // staging_archived: bill_id -> ArchivedBill (bills to archive)
        let mut staging_archived: Map<u32, ArchivedBill> = Map::new(&env);
        let mut owner_to_archived: Map<Address, Vec<u32>> = Map::new(&env);
        let mut owner_currency_to_removed: Map<(Address, String), Vec<u32>> = Map::new(&env);

        for (id, bill) in bills.iter() {
            if let Some(paid_at) = bill.paid_at {
                if bill.paid && paid_at < before_timestamp {
                    // State transition validation: only Paid → Archived is legal.
                    // Active → Archived is blocked (must be paid first).
                    // The scan filter guarantees `bill.paid && bill.paid_at.is_some()`,
                    // so BillState::from_bill returns Paid and the transition is valid.
                    use crate::state::BillState;
                    BillState::validate_transition(&bill, false, BillState::Archived, "archive_paid_bills")?;

                    let archived_bill = ArchivedBill {
                        id: bill.id,
                        owner: bill.owner.clone(),
                        name: bill.name.clone(),
                        external_ref: bill.external_ref.clone(),
                        amount: bill.amount,
                        paid_at: Some(paid_at),
                        archived_at: current_time,
                        tags: bill.tags.clone(),
                        currency: bill.currency.clone(),
                    };
                    staging_archived.set(id, archived_bill);

                    let mut list = owner_to_archived
                        .get(bill.owner.clone())
                        .unwrap_or_else(|| Vec::new(&env));
                    list.push_back(id);
                    owner_to_archived.set(bill.owner.clone(), list);

                    let currency_key = (bill.owner.clone(), bill.currency.clone());
                    let mut currency_list = owner_currency_to_removed
                        .get(currency_key.clone())
                        .unwrap_or_else(|| Vec::new(&env));
                    currency_list.push_back(id);
                    owner_currency_to_removed.set(currency_key, currency_list);
                }
            }
        }

        let archived_count = staging_archived.len();
        if archived_count == 0 {
            return Ok(0);
        }

        // -----------------------------------------------------------------
        // Phase 2: All qualifying bills identified — apply mutations atomically.
        // -----------------------------------------------------------------
        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(&env));

        for (id, staged_bill) in staging_archived.iter() {
            // Release external_ref from the active index
            if let Some(ref r) = staged_bill.external_ref {
                Self::release_external_ref(&env, &staged_bill.owner, r);
            }

            archived.set(id, staged_bill);
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

        env.events().publish(
            (symbol_short!("bill"), BillEvent::Archived),
            (archived_count, current_time),
        );
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::System,
            symbol_short!("archived"),
            archived_count,
        );

        Ok(archived_count)
    }

    /// Emits BillEvent::Restored.
    /// Restores a previously archived bill back to active storage.
    ///
    /// # Events
    /// - Secondary topic: `(symbol_short!("bill"), BillEvent::Restored)`
    /// - Action symbol: `"restored"` via [`RemitwiseEvents::emit`]
    pub fn restore_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::RESTORE)?;
        Self::restore_bill_core(&env, caller, bill_id)
    }

    pub fn restore_bill_keyed(
        env: Env,
        caller: Address,
        request_key: BytesN<32>,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        caller.require_auth();
        let request = BillPaymentRequest::RestoreBill(BillTransitionRequest {
            caller: caller.clone(),
            bill_id,
        });
        if let Some(result) = Self::lookup_request(&env, &caller, &request_key, &request)? {
            return match result {
                BillPaymentResult::Unit => Ok(()),
                _ => Err(BillPaymentsError::RequestKeyConflict),
            };
        }
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        Self::require_not_paused(&env, pause_functions::RESTORE)?;
        Self::restore_bill_core(&env, caller.clone(), bill_id)?;
        Self::commit_request(
            &env,
            &caller,
            &request_key,
            request,
            BillPaymentResult::Unit,
        );
        Ok(())
    }

    fn restore_bill_core(
        env: &Env,
        caller: Address,
        bill_id: u32,
    ) -> Result<(), BillPaymentsError> {
        Self::extend_instance_ttl(env);

        let mut archived: Map<u32, ArchivedBill> = env
            .storage()
            .instance()
            .get(&symbol_short!("ARCH_BILL"))
            .unwrap_or_else(|| Map::new(env));
        let archived_bill = archived
            .get(bill_id)
            .ok_or(BillPaymentsError::BillNotFound)?;

        if archived_bill.owner != caller {
            return Err(BillPaymentsError::Unauthorized);
        }

        // State transition validation: Archived → Active is the only legal
        // path for restore. A bill in ARCH_BILL is always in the Archived
        // state by construction.
        use crate::state::BillState;
        BillState::validate_transition(
            &Bill {
                id: archived_bill.id,
                owner: archived_bill.owner.clone(),
                name: archived_bill.name.clone(),
                external_ref: None,
                amount: archived_bill.amount,
                due_date: 0,
                recurring: false,
                frequency_days: 0,
                paid: false,
                created_at: 0,
                paid_at: archived_bill.paid_at,
                schedule_id: None,
                tags: Vec::new(env),
                currency: archived_bill.currency.clone(),
            },
            true, // is_archived = true (bill lives in ARCH_BILL)
            BillState::Active,
            "restore_bill",
        )?;

        if let Some(ref r) = archived_bill.external_ref {
            Self::claim_external_ref(env, &caller, r, bill_id)?;
        }

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(env));

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
            paid_at: archived_bill.paid_at,
            schedule_id: None,
            tags: archived_bill.tags.clone(),
            currency: archived_bill.currency.clone(),
        };

        bills.set(bill_id, restored_bill);
        archived.remove(bill_id);

        Self::index_remove_archived(env, &caller, bill_id);
        Self::index_add_active(env, &caller, bill_id);
        // Add back to currency index
        Self::index_add_currency(env, &caller, &archived_bill.currency, bill_id);

        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);
        env.storage()
            .instance()
            .set(&symbol_short!("ARCH_BILL"), &archived);

        Self::update_storage_stats(env);

        env.events().publish(
            (symbol_short!("bill"), BillEvent::Restored),
            (bill_id, caller.clone(), env.ledger().timestamp()),
        );
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
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
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
    /// @dev Atomic batch execution: all eligible bills in `bill_ids` are paid in a
    /// single commit. Non-existent, cross-owner, and already-paid IDs are skipped
    /// (not failed). Any failure during computation (recurring due-date overflow,
    /// id exhaustion, unpaid-total overflow, invariant violation) returns an error
    /// with **no partial state** — computation is fully staged in Phase 1 and only
    /// committed in Phase 2.
    ///
    /// @param caller Authenticated owner attempting the batch payment.
    /// @param bill_ids Candidate bill IDs to process.
    /// @return `Ok(())` after processing the requested bill IDs.
    /// @security Cross-owner payments are skipped per item; oversized batches are
    /// rejected before iteration. Unpaid-total arithmetic is checked, never
    /// saturating, so balances cannot be silently truncated.
    pub fn batch_pay_bills(
        env: Env,
        caller: Address,
        bill_ids: Vec<u32>,
    ) -> Result<(), BillPaymentsError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        caller.require_auth();
        Self::require_not_paused(&env, pause_functions::PAY_BILL)?;

        if bill_ids.len() > MAX_BATCH_SIZE {
            return Err(BillPaymentsError::BatchTooLarge);
        }

        Self::extend_instance_ttl(&env);
        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let current_time = env.ledger().timestamp();
        let current_next_id = env
            .storage()
            .instance()
            .get(&symbol_short!("NEXT_ID"))
            .unwrap_or(0u32);

        // -----------------------------------------------------------------
        // Phase 1: Validate and compute ALL side-effects in staging buffers.
        // No storage is modified during this phase. If any bill's recurring
        // computation overflows (due-date arithmetic, id exhaustion, unpaid-
        // total arithmetic), the entire batch returns an error with no
        // partial state — the caller can retry safely.
        // -----------------------------------------------------------------
        // staging_paid: bill_id -> paid Bill
        let mut staging_paid: Map<u32, Bill> = Map::new(&env);
        // staging_child: next_bill_id -> next Bill (recurring children)
        let mut staging_child: Map<u32, Bill> = Map::new(&env);
        // parent_to_child: parent_bill_id -> child_bill_id
        let mut parent_to_child: Map<u32, u32> = Map::new(&env);
        let mut running_next_id = current_next_id;
        let mut total_unpaid_delta: i128 = 0;

        for bill_id in bill_ids.iter() {
            let Some(bill) = bills.get(bill_id) else {
                continue; // Non-existent bill: skip, do not fail
            };

            if bill.owner != caller {
                continue; // Cross-owner bill: skip, do not fail
            }
            if bill.paid {
                continue; // Already-paid bill: skip, do not fail
            }

            // State-transition + invariant validation (also enforces the
            // shared amount bounds, rejecting a corrupted/oversized stored
            // bill before any state change).
            use crate::state::{check_invariants, BillState};
            BillState::validate_transition(&bill, false, BillState::Paid, "batch_pay_bills")?;
            check_invariants(&env, &bill, false)?;

            // Reject settlement outside the allowed window (due date plus the
            // 30-day grace period): a stale obligation in the batch fails the
            // whole batch deterministically rather than being silently paid.
            require_within_settlement_window(
                current_time,
                bill.due_date,
                MAX_SETTLEMENT_WINDOW_SECS,
            )
            .map_err(|_| BillPaymentsError::SettlementWindowExpired)?;

            // Compute the child bill (if recurring) — may fail on overflow.
            let mut child_entry: Option<(u32, Bill)> = None;
            let mut unpaid_delta_item: i128 = 0;

            if bill.recurring {
                // Checked increment: id exhaustion (u32::MAX) is rejected
                // instead of silently wrapping and reusing a bill id.
                running_next_id = running_next_id
                    .checked_add(1)
                    .ok_or(BillPaymentsError::IdSpaceExhausted)?;

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
                    id: running_next_id,
                    owner: bill.owner.clone(),
                    name: bill.name.clone(),
                    external_ref: None, // do not clone: uniqueness is per-bill
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
                child_entry = Some((running_next_id, next_bill));
            } else {
                // Paying a non-recurring bill removes its amount from the
                // owner's unpaid total. Recurring bills are net-zero: the
                // parent's amount is removed but the child bill adds it back.
                unpaid_delta_item = bill.amount;
            }

            let mut paid_bill = bill.clone();
            paid_bill.paid = true;
            paid_bill.paid_at = Some(current_time);

            // Checked, never saturating: a delta overflow would silently
            // truncate the owner's unpaid total.
            total_unpaid_delta = total_unpaid_delta
                .checked_sub(unpaid_delta_item)
                .ok_or(BillPaymentsError::AmountOverflow)?;

            staging_paid.set(bill_id, paid_bill);
            if let Some((child_id, child_bill)) = child_entry {
                parent_to_child.set(bill_id, child_id);
                staging_child.set(child_id, child_bill);
            }
        }

        // -----------------------------------------------------------------
        // Phase 2: All computation succeeded — commit all mutations at once.
        // -----------------------------------------------------------------
        for (bill_id, paid_bill) in staging_paid.iter() {
            let external_ref = paid_bill.external_ref.clone();
            let amount = paid_bill.amount;

            if let Some(child_id) = parent_to_child.get(bill_id) {
                if let Some(child_bill) = staging_child.get(child_id) {
                    bills.set(child_id, child_bill.clone());
                    Self::index_add_active(&env, &caller, child_id);
                    Self::index_add_currency(&env, &caller, &child_bill.currency, child_id);
                    env.events().publish(
                        (symbol_short!("bill"), BillEvent::RecurringBillCreated),
                        (child_id, bill_id, child_bill.due_date),
                    );
                }
            }

            bills.set(bill_id, paid_bill);
            env.events().publish(
                (symbol_short!("bill"), BillEvent::Paid),
                (bill_id, caller.clone(), external_ref),
            );
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
            .set(&symbol_short!("NEXT_ID"), &running_next_id);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        if total_unpaid_delta != 0 {
            Self::adjust_unpaid_total(&env, &caller, total_unpaid_delta);
        }

        Self::update_storage_stats(&env);
        Ok(())
    }

    /// Sum of all **unpaid** bill amounts for the given `owner`.
    ///
    /// # Overflow Behavior
    /// Since Issue #1737 every stored amount is bounded by `MAX_AMOUNT` and
    /// every owner is capped at `MAX_BILLS_PER_OWNER` unpaid bills, the sum
    /// can never exceed `1000 × 10³⁰ = 10³³`, far below `i128::MAX`. The
    /// cold-path aggregation below therefore uses **checked** addition: if a
    /// corrupted/legacy bill ever made this unreachable case fire, the panic
    /// reverts the read rather than silently returning a truncated total
    /// (identical behavior to the cached-path `adjust_unpaid_total`).
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
                // Checked, never saturating: overflow here would mean a
                // stored bill exceeds MAX_AMOUNT; reject deterministically.
                total = total
                    .checked_add(bill.amount)
                    .unwrap_or_else(|| panic_with_error!(&env, BillPaymentsError::AmountOverflow));
            }
        }
        total
    }

    /// Returns the total unpaid amount for `owner` filtered by `currency`.
    ///
    /// The currency string is normalized (uppercased, whitespace trimmed)
    /// for consistent lookup against the currency index.
    pub fn get_total_unpaid_by_currency(env: Env, owner: Address, currency: String) -> i128 {
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let currency_ids = Self::get_bills_by_owner_currency(&env, &owner, &normalized_currency);
        let mut total = 0i128;
        for id in currency_ids.iter() {
            if let Some(bill) = bills.get(id) {
                if !bill.paid {
                    // Checked, never saturating (see `get_total_unpaid`).
                    total = total.checked_add(bill.amount).unwrap_or_else(|| {
                        panic_with_error!(&env, BillPaymentsError::AmountOverflow)
                    });
                }
            }
        }
        total
    }

    /// Returns a page of unpaid bills for `owner` filtered by `currency`.
    ///
    /// The currency string is normalized for consistent lookup.
    /// Pagination uses the existing currency index for O(currency_bills) traversal.
    ///
    /// # Security
    /// Requires `owner.require_auth()`. The per-`(owner, currency)` index is
    /// scoped to `owner`, so no cross-owner leakage can occur via cursor
    /// manipulation.
    pub fn get_unpaid_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        owner.require_auth();
        let limit = clamp_limit(limit);
        let normalized_currency = Self::normalize_currency(&env, &currency);
        let bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));
        let currency_ids = Self::get_bills_by_owner_currency(&env, &owner, &normalized_currency);
        let mut staging: Vec<(u32, Bill)> = Vec::new(&env);
        for id in currency_ids.iter() {
            if id <= cursor {
                continue;
            }
            let Some(bill) = bills.get(id) else {
                continue;
            };
            if !bill.paid {
                staging.push_back((id, bill));
                if staging.len() > limit {
                    break;
                }
            }
        }
        Self::build_page(&env, staging, limit)
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
    ///
    /// # Security
    /// Requires `owner.require_auth()`. The per-`(owner, currency)` index is
    /// scoped to `owner`, so no cross-owner leakage can occur via cursor
    /// manipulation.
    pub fn get_bills_by_currency(
        env: Env,
        owner: Address,
        currency: String,
        cursor: u32,
        limit: u32,
    ) -> BillPage {
        owner.require_auth();
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

    /// One-time admin setup.
    ///
    /// `rotation_timelock_seconds` configures how long a future admin
    /// rotation must sit proposed before `finalize_admin_rotation` can
    /// complete it -- see [`DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS`] for the
    /// sane default and the rationale. Pass that constant to keep the
    /// previous fixed behavior, or a different value to tune it per
    /// deployment (e.g. a short window on a test network).
    ///
    /// # Errors
    /// * `AdminAlreadyInitialized` - If an admin has already been set
    /// * `RotationTimelockTooShort` - If `rotation_timelock_seconds` is
    ///   below `MIN_SCHEDULE_INTERVAL`
    pub fn init_admin(
        env: Env,
        admin: Address,
        rotation_timelock_seconds: u64,
    ) -> Result<(), Error> {
        admin.require_auth();

        if env.storage().instance().has(&symbol_short!("ADMIN")) {
            return Err(Error::AdminAlreadyInitialized);
        }

        if rotation_timelock_seconds < MIN_SCHEDULE_INTERVAL {
            return Err(Error::RotationTimelockTooShort);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);
        env.storage()
            .instance()
            .set(&symbol_short!("ROT_TL"), &rotation_timelock_seconds);
        env.events()
            .publish((symbol_short!("admin"), AdminEvent::Initialized), admin);

        Ok(())
    }

    /// Get the configured admin-rotation timelock window, in seconds.
    /// Returns [`DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS`] if `init_admin`
    /// hasn't run yet.
    pub fn get_admin_rotation_timelock(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&symbol_short!("ROT_TL"))
            .unwrap_or(DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS)
    }

    /// Propose rotating the admin to `new_admin`. Does not take effect
    /// immediately -- uses the timelock window configured at `init_admin`
    /// (see [`Self::get_admin_rotation_timelock`]). Call
    /// `finalize_admin_rotation` after the timelock elapses to complete it.
    /// A second call before finalization overwrites the still-pending
    /// proposal (and restarts its timelock) rather than stacking.
    ///
    /// # Errors
    /// * `AdminNotInitialized` - If no admin has been set yet
    /// * `Unauthorized` - If caller is not the current admin
    pub fn propose_admin_rotation(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), Error> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(Error::AdminNotInitialized)?;

        if admin != caller {
            return Err(Error::Unauthorized);
        }

        let timelock: u64 = env
            .storage()
            .instance()
            .get(&symbol_short!("ROT_TL"))
            .unwrap_or(DEFAULT_ADMIN_ROTATION_TIMELOCK_SECONDS);
        let executable_at = env.ledger().timestamp() + timelock;
        let pending = PendingAdminRotation {
            new_admin: new_admin.clone(),
            executable_at,
        };
        env.storage()
            .instance()
            .set(&symbol_short!("PENDROT"), &pending);

        env.events().publish(
            (symbol_short!("admin"), AdminEvent::RotationProposed),
            (new_admin, executable_at),
        );

        Ok(())
    }

    /// Finalize a previously proposed admin rotation, once its timelock
    /// has elapsed. Callable by anyone -- the timelock, not the caller
    /// identity, is what gates this taking effect.
    ///
    /// # Errors
    /// * `NoPendingRotation` - If no rotation has been proposed
    /// * `TimelockNotElapsed` - If called before `executable_at`
    pub fn finalize_admin_rotation(env: Env) -> Result<(), Error> {
        let pending: PendingAdminRotation = env
            .storage()
            .instance()
            .get(&symbol_short!("PENDROT"))
            .ok_or(Error::NoPendingRotation)?;

        if env.ledger().timestamp() < pending.executable_at {
            return Err(Error::TimelockNotElapsed);
        }

        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &pending.new_admin);
        env.storage().instance().remove(&symbol_short!("PENDROT"));

        env.events().publish(
            (symbol_short!("admin"), AdminEvent::RotationFinalized),
            pending.new_admin,
        );

        Ok(())
    }

    /// Get the current admin, or `None` if `init_admin` hasn't run yet.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("ADMIN"))
    }

    /// Get the pending rotation, if one has been proposed and not yet
    /// finalized.
    pub fn get_pending_admin_rotation(env: Env) -> Option<PendingAdminRotation> {
        env.storage().instance().get(&symbol_short!("PENDROT"))
    }

    /// Extend the TTL of instance storage
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
            active_count = active_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(env, BillPaymentsError::AmountOverflow));
            if !bill.paid {
                // Checked, never saturating: bounded by MAX_AMOUNT per bill.
                unpaid_amount = unpaid_amount.checked_add(bill.amount).unwrap_or_else(|| {
                    panic_with_error!(env, BillPaymentsError::AmountOverflow)
                });
            }
        }

        let mut archived_count = 0u32;
        let mut archived_amount = 0i128;
        for (_, bill) in archived.iter() {
            archived_count = archived_count
                .checked_add(1)
                .unwrap_or_else(|| panic_with_error!(env, BillPaymentsError::AmountOverflow));
            archived_amount = archived_amount.checked_add(bill.amount).unwrap_or_else(|| {
                panic_with_error!(env, BillPaymentsError::AmountOverflow)
            });
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
        // Checked, never saturating: an overflow here would silently truncate
        // an owner's unpaid balance. All amounts are bounded by MAX_AMOUNT at
        // every boundary, so this is unreachable in practice; if it ever fires
        // the panic reverts the entire invocation (no partial state).
        let next = current
            .checked_add(delta)
            .unwrap_or_else(|| panic_with_error!(env, BillPaymentsError::AmountOverflow));
        totals.set(owner.clone(), next);
        env.storage()
            .instance()
            .set(&STORAGE_UNPAID_TOTALS, &totals);
    }

    /// Configure the trusted orchestrator address used by the cross-contract
    /// epoch guard. Only the contract admin may set this. Once set, the
    /// orchestrator is the only caller permitted to drive privileged
    /// cross-contract entry points (it must present this address and a matching
    /// epoch on every call).
    pub fn set_trusted_orchestrator(env: Env, caller: Address, orchestrator: Address) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .unwrap_or_else(|| panic!("Contract not initialized"));
        if caller != admin {
            panic_with_error!(&env, BillPaymentsError::Unauthorized);
        }
        set_trusted_orchestrator(&env, &orchestrator);
        env.events()
            .publish((symbol_short!("bp"), symbol_short!("orch_set")), orchestrator.clone());
    }

    /// Bump the cross-contract epoch by 1. Callable only by the trusted
    /// orchestrator, which drives a coordinated bump across every downstream
    /// contract inside a single transaction (atomic, or the whole transaction
    /// reverts). Returns the new epoch.
    pub fn bump_cross_contract_epoch(env: Env, orchestrator: Address) -> u64 {
        remitwise_common::require_trusted_orchestrator(&env, &orchestrator)
            .unwrap_or_else(|_| panic_with_error!(&env, TrustedOrchestratorError::Unauthorized));
        let new_epoch = bump_cross_contract_epoch(&env);
        env.events()
            .publish((symbol_short!("bp"), symbol_short!("epch_bump")), new_epoch);
        new_epoch
    }

    /// View the current cross-contract epoch for off-chain reconciliation.
    pub fn get_cross_contract_epoch(env: Env) -> u64 {
        get_cross_contract_epoch(&env)
    }
}

// -----------------------------------------------------------------------
// ReversibleOp (compensation) trait implementation
// -----------------------------------------------------------------------
#[contractimpl]
impl BillPaymentsReversible for BillPayments {
    /// Reverse a previous `pay_bill` call for the given bill.
    ///
    /// Marks the bill as unpaid and restores the unpaid-total tracker.
    /// Returns `Ok(false)` when the bill was already unpaid (idempotent).
    fn reverse_payment(
        env: Env,
        orchestrator: Address,
        epoch: u64,
        user: Address,
        bill_id: u32,
        _amount: i128,
    ) -> Result<bool, ReversibleOpError> {
        remitwise_common::require_no_active_kill_switch(&env)
            .unwrap_or_else(|e| soroban_sdk::panic_with_error!(&env, e));
        guard_cross_contract_write(&env, &orchestrator, epoch)
            .unwrap_or_else(|_| panic_with_error!(&env, CrossContractEpochError::EpochMismatch));
        Self::require_not_paused(&env, pause_functions::REVERSE_PAYMENT)
            .map_err(|_| ReversibleOpError::InvalidState)?;
        Self::extend_instance_ttl(&env);

        let mut bills: Map<u32, Bill> = env
            .storage()
            .instance()
            .get(&symbol_short!("BILLS"))
            .unwrap_or_else(|| Map::new(&env));

        let mut bill = bills.get(bill_id).ok_or(ReversibleOpError::NotFound)?;

        if bill.owner != user {
            return Err(ReversibleOpError::Unauthorized);
        }

        if !bill.paid {
            return Ok(false);
        }

        bill.paid = false;
        bill.paid_at = None;

        let reversed_amount = bill.amount;
        bills.set(bill_id, bill);
        env.storage()
            .instance()
            .set(&symbol_short!("BILLS"), &bills);

        Self::adjust_unpaid_total(&env, &user, reversed_amount);

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("reverse"),
            (bill_id, user, reversed_amount),
        );

        Ok(true)
    }
}

#[cfg(test)]
mod events_schema_test;

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_state_invariants;

#[cfg(test)]
mod tests_amount_precision;
