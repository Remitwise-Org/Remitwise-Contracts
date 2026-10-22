#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    Env, IntoVal, Map, Symbol, TryFromVal, Val, Vec,
};
mod utils;
use utils::u64_to_u32;

pub use remitwise_common::{Category, CoverageType, ToI128Checked, DEFAULT_PAGE_LIMIT, MAX_TOP_N};

// Storage TTL constants
const DAY_IN_LEDGERS: u32 = 17280;

pub const PERSISTENT_BUMP_AMOUNT: u32 = 60 * DAY_IN_LEDGERS; // 60 days
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS; // 15 days

pub const INSTANCE_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;

pub const ARCHIVE_BUMP_AMOUNT: u32 = 150 * DAY_IN_LEDGERS; // ~150 days
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = DAY_IN_LEDGERS; // 1 day

/// Maximum number of pages fetched from any single dependency per report call.
/// Loops that reach this cap mark the result `DataAvailability::Partial` so
/// callers know the aggregate may be incomplete.
pub const MAX_DEP_PAGES: u32 = 20;

/// Page size for dependency queries. This is the maximum number of items
/// fetched per page from bill-payments and insurance contracts.
///
/// The aggregation loops fetch up to MAX_DEP_PAGES pages, allowing for
/// up to MAX_DEP_PAGES * DEP_PAGE_LIMIT items to be aggregated.
/// If the cap is reached, DataAvailability is set to Partial to indicate
/// the report may be incomplete.
pub const DEP_PAGE_LIMIT: u32 = 50;

/// Maximum number of items included in top-N reports.
///
/// Alias for [`remitwise_common::MAX_TOP_N`] so the invariance is
/// compile-time enforced.  Validated at the top of every top-N endpoint
/// via [`remitwise_common::require_bounded_top_n`] as defence-in-depth.
pub const MAX_ITEMS_PER_REPORT: u32 = remitwise_common::MAX_TOP_N;

/// Financial health score (0-100), composed of three weighted components.
///
/// - `savings_score`: 0-40, from aggregate savings-goal completion
/// - `bills_score`: 0-40, from bill-payment compliance (tiered 40/35/20)
/// - `insurance_score`: 0-20, binary on having an active policy
/// - `score`: `clamp(savings + bills + insurance, 0, 100)`
///
/// See `docs/HEALTH_SCORE.md` (at the repository root) for the full scoring
/// model, inputs, and worked examples.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct HealthScore {
    /// Overall score, clamped to `0..=100`.
    pub score: u32,
    /// Savings component, `0..=40` (goal completion percentage scaled to 40).
    pub savings_score: u32,
    /// Bills component, `0..=40` (40 none unpaid, 35 unpaid none overdue, 20 overdue).
    pub bills_score: u32,
    /// Insurance component, `0..=20` (20 if any active policy, else 0).
    pub insurance_score: u32,
}

/// Category breakdown with amount and percentage
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CategoryBreakdown {
    pub category: Category,
    pub amount: i128,
    pub percentage: u32,
}

/// Trend data comparing two periods
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TrendData {
    pub current_amount: i128,
    pub previous_amount: i128,
    pub change_amount: i128,
    pub change_percentage: i32, // Can be negative
}

/// Indicates the completeness of the data retrieved from external contracts
#[contracttype]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataAvailability {
    /// All external calls succeeded and data is complete
    Complete = 0,
    /// Some external calls failed or returned partial data
    Partial = 1,
    /// Critical external calls failed or addresses not configured, data is missing/default
    Missing = 2,
}

/// Remittance summary report
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RemittanceSummary {
    pub total_received: i128,
    pub total_allocated: i128,
    pub category_breakdown: Vec<CategoryBreakdown>,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Savings progress report
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SavingsReport {
    pub total_goals: u32,
    pub completed_goals: u32,
    pub total_target: i128,
    pub total_saved: i128,
    pub completion_percentage: u32,
    pub period_start: u64,
    pub period_end: u64,
}

/// Bill payment compliance report
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BillComplianceReport {
    pub total_bills: u32,
    pub paid_bills: u32,
    pub unpaid_bills: u32,
    pub overdue_bills: u32,
    pub total_amount: i128,
    pub paid_amount: i128,
    pub unpaid_amount: i128,
    pub compliance_percentage: u32,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Insurance coverage report
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct InsuranceReport {
    pub active_policies: u32,
    pub total_coverage: i128,
    pub monthly_premium: i128,
    pub annual_premium: i128,
    pub coverage_to_premium_ratio: u32,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Family spending report aggregated from the configured `family_wallet` dependency.
///
/// See `reporting/docs/FAMILY_SPENDING_REPORT.md` for the full schema and
/// `DataAvailability` degradation rules.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FamilySpendingReport {
    pub member_breakdown: Vec<FamilyMemberSpending>,
    pub total_members: u32,
    pub total_spending: i128,
    pub average_per_member: i128,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Per-member family spending breakdown entry.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FamilyMemberSpending {
    /// Family-wallet member address.
    pub member: Address,
    /// Aggregated spending fetched from the family wallet's `SpendingTracker`.
    ///
    /// This is `0` when no tracker exists yet or when the per-member spending
    /// read was unavailable.
    pub total_spending: i128,
    /// `true` when `total_spending` reflects a successful downstream read.
    pub data_available: bool,
}

/// Overall financial health report
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FinancialHealthReport {
    pub health_score: HealthScore,
    pub remittance_summary: RemittanceSummary,
    pub savings_report: SavingsReport,
    pub bill_compliance: BillComplianceReport,
    pub insurance_report: InsuranceReport,
    /// Worst-case availability across component reports that expose
    /// [`DataAvailability`].
    pub data_availability: DataAvailability,
    pub generated_at: u64,
}

/// Top-N bills sorted deterministically.
///
/// Ordering contract (reproducible across calls/networks):
/// - Primary sort: `amount` descending
/// - Tie-break (when `amount` is equal): `id` ascending
///
/// Returned `items` are capped to [`MAX_ITEMS_PER_REPORT`] (no padding).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TopNBillsReport {
    pub items: Vec<Bill>,
    pub total_amount: i128,
    pub total_count: u32,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Top-N savings goals sorted deterministically.
///
/// Ordering contract (reproducible across calls/networks):
/// - Primary sort: `target_amount` descending
/// - Tie-break (when `target_amount` is equal): `id` ascending
///
/// Returned `items` are capped to [`MAX_ITEMS_PER_REPORT`] (no padding).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TopNSavingsReport {
    pub items: Vec<SavingsGoal>,
    pub total_target: i128,
    pub total_saved: i128,
    pub total_count: u32,
    pub period_start: u64,
    pub period_end: u64,
    pub data_availability: DataAvailability,
}

/// Contract addresses configuration
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractAddresses {
    pub remittance_split: Address,
    pub savings_goals: Address,
    pub bill_payments: Address,
    pub insurance: Address,
    pub family_wallet: Address,
}

/// Errors returned by the reporting contract (`Result` arms and `try_` client helpers).
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReportingError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    AddressesNotConfigured = 4,
    NotAdminProposed = 5,
    /// Dependency address set is not usable: duplicates or self-reference to this reporting contract.
    InvalidDependencyAddressConfiguration = 6,
    /// Report period range is invalid (`period_start` is greater than `period_end`).
    InvalidPeriod = 7,
    /// Invalid percentage split summing to > 100 or != 100
    InvalidPercentageSplit = 8,
    /// u64 to u32 overflow guard
    Overflow = 9,
    /// Proposed new admin is the same as the current admin.
    SameAdmin = 10,
    /// The requested top-N size exceeds the global cap.
    TopNTooLarge = 11,
    /// Proposed new admin is this contract's own address — accepting it would
    /// brick admin control, since the contract cannot sign `require_auth()`
    /// for itself as an external caller. Mirrors `emergency_killswitch`'s
    /// `InvalidAdmin` guard on `transfer_admin`.
    InvalidAdmin = 12,
    /// A grant expiry is in the past or is not strictly after the current ledger time.
    InvalidGrantExpiry = 13,
    /// A viewer grant does not exist or is no longer active.
    ViewerNotGranted = 14,
    /// A viewer cannot be granted access to the owner identity itself.
    InvalidViewer = 15,
    /// `store_report` would exceed [`MAX_ACTIVE_REPORTS_PER_USER`] active
    /// reports for this user. Archive old reports or overwrite an existing
    /// period key instead.
    TooManyActiveReports = 16,
    /// Legacy instance-storage data has not been fully migrated yet; call
    /// `migrate_reports_to_persistent` until it completes.
    MigrationPending = 17,
    /// The user's archive index already holds [`MAX_ARCHIVE_PER_USER`]
    /// entries; run `cleanup_old_reports` for this user first.
    ArchiveIndexFull = 18,
}

/// Explicit report data scopes that can be delegated independently.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportScope {
    /// Stored full financial-health reports.
    Stored = 0,
    /// Archived health-score summaries.
    Archived = 1,
}

/// The subject and viewer identity recorded in an ACL event.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerGrant {
    pub owner: Address,
    pub viewer: Address,
    pub scope: ReportScope,
    /// Zero means no expiry; otherwise access is valid while now < expires_at.
    pub expires_at: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportEvent {
    ReportGenerated,
    ReportStored,
    AddressesConfigured,
    ReportsArchived,
    ArchivesCleaned,
    ViewerGranted,
    ViewerRevoked,
    ArchiveIndexRepaired,
}

/// Archived report - compressed summary
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedReport {
    pub user: Address,
    pub period_key: u64,
    pub health_score: u32,
    pub generated_at: u64,
    pub archived_at: u64,
}

/// Paginated result for archived reports
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedPage {
    pub items: Vec<ArchivedReport>,
    pub next_cursor: u32,
    pub count: u32,
}

/// Storage statistics for monitoring
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorageStats {
    pub active_reports: u32,
    pub archived_reports: u32,
    pub last_updated: u64,
}

// ---------------------------------------------------------------------------
// Persistent per-record storage layout
// ---------------------------------------------------------------------------

/// Maximum number of *active* (non-archived) reports a single user may hold.
///
/// `store_report` only requires the user's own auth, so without a cap any
/// account could grow its own index without bound. 120 = 10 years of monthly
/// reports. Overwriting an existing `(user, period_key)` never counts against
/// the cap.
pub const MAX_ACTIVE_REPORTS_PER_USER: u32 = 120;

/// Maximum number of entries in a single user's archive index.
///
/// `archive_old_reports` stops appending once a user's index reaches this
/// size; `cleanup_old_reports` frees room again.
pub const MAX_ARCHIVE_PER_USER: u32 = 500;

/// Upper bound (and default for `limit == 0`) on the number of records
/// processed by one call to `archive_old_reports`, `cleanup_old_reports` or
/// `migrate_reports_to_persistent`.
///
/// Operators should pick a `limit` that also fits the network's per-transaction
/// read/write ledger-entry limits; this constant is only an absolute ceiling.
pub const MAX_STORAGE_BATCH: u32 = 100;

/// Legacy instance-storage keys (pre-migration layout). They are only read
/// as a fallback and drained by [`ReportingContract::migrate_reports_to_persistent`].
const LEGACY_REPORTS: Symbol = symbol_short!("REPORTS");
const LEGACY_ARCH_RPT: Symbol = symbol_short!("ARCH_RPT");
const LEGACY_ARCH_IDX: Symbol = symbol_short!("ARCH_IDX");
const LEGACY_VIEW_ACL: Symbol = symbol_short!("VIEW_ACL");

/// Per-record persistent storage keys.
///
/// Every per-user record lives under its own persistent ledger entry with its
/// own TTL, so the cost of touching one user's data is independent of how
/// many other users exist. Only small, contract-wide configuration
/// (`ADMIN`, `PEND_ADM`, `ADDRS`, `STOR_STAT`) stays in instance storage.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    /// Active report for `(user, period_key)` → [`FinancialHealthReport`].
    Report(Address, u64),
    /// Archived summary for `(user, period_key)` → [`ArchivedReport`].
    Archived(Address, u64),
    /// Per-user archive index (`Vec<u64>` of period keys, append order),
    /// capped at [`MAX_ARCHIVE_PER_USER`].
    ArchiveIndex(Address),
    /// Per-user list of active reports (`Vec<ReportIndexEntry>`), capped at
    /// [`MAX_ACTIVE_REPORTS_PER_USER`]. Needed so archiving can enumerate one
    /// user's reports without scanning global state.
    ReportIndex(Address),
    /// Viewer grant `(owner, viewer, scope)` → expiry (`u64`, `0` = never).
    ViewerGrant(Address, Address, ReportScope),
}

/// One entry of [`DataKey::ReportIndex`].
///
/// `generated_at` is denormalised from the report so `archive_old_reports`
/// can select candidates without loading every report entry.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportIndexEntry {
    pub period_key: u64,
    pub generated_at: u64,
}

/// Health-check result for one configured downstream reporting dependency.
///
/// `ReportingContract::check_dependencies` returns one `DependencyStatus`
/// for each address slot in `ContractAddresses`, in fixed slot order. A
/// healthy status means the lightweight probe call completed successfully.
/// A failed status means the dependency probe failed; operators should fix the
/// dependency before trusting reports that read from that contract.
#[contracttype]
#[derive(Clone)]
pub struct DependencyStatus {
    /// Stable dependency slot name from `ContractAddresses`.
    ///
    /// Current values are `remittance_split`, `savings_goals`,
    /// `bill_payments`, `insurance`, and `family_wallet`.
    pub name: soroban_sdk::String,
    /// `true` when the lightweight probe call succeeds.
    pub ok: bool,
    /// `None` when `ok` is true; otherwise the probe failure category.
    ///
    /// Current values are `get_split_failed`, `get_all_goals_failed`,
    /// `get_total_unpaid_failed`, `get_total_monthly_premium_failed`, and
    /// `get_owner_failed`.
    pub error_category: Option<soroban_sdk::String>,
}

/// Mirror of the real `remittance_split::SplitConfig`.
///
/// Field names, order and types MUST match `SplitConfig` in
/// `remittance_split/src/lib.rs` exactly so the value returned by
/// `get_config` decodes without a `ConversionError`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub timestamp: u64,
    pub initialized: bool,
    pub usdc_contract: Address,
}

// Client traits for cross-contract calls

#[contractclient(name = "RemittanceSplitClient")]
pub trait RemittanceSplitTrait {
    fn get_split(env: &Env, orchestrator: Address, epoch: u64) -> Vec<u32>;
    fn get_config(env: Env) -> Option<SplitConfig>;
}

#[contractclient(name = "SavingsGoalsClient")]
pub trait SavingsGoalsTrait {
    fn get_all_goals(env: Env, owner: Address) -> Vec<SavingsGoal>;
    fn get_goals(env: Env, owner: Address, cursor: u32, limit: u32) -> GoalPage;
    fn is_goal_completed(env: Env, goal_id: u32) -> bool;
}

#[contractclient(name = "BillPaymentsClient")]
pub trait BillPaymentsTrait {
    fn get_unpaid_bills(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage;
    fn get_total_unpaid(env: Env, owner: Address) -> i128;
    fn get_all_bills_for_owner(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage;
}

#[contractclient(name = "InsuranceClient")]
pub trait InsuranceTrait {
    fn get_active_policies(env: Env, owner: Address, cursor: u32, limit: u32) -> PolicyPage;
    fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy>;
    fn get_total_monthly_premium(env: Env, owner: Address) -> i128;
}

#[contractclient(name = "FamilyWalletClient")]
pub trait FamilyWalletTrait {
    fn get_owner(env: &Env) -> Address;
    fn get_member_addresses_page(env: Env, cursor: u32, limit: u32) -> MemberAddressPage;
    fn get_spending_tracker(env: Env, member: Address) -> Option<SpendingTracker>;
}

// Data structures from other contracts (needed for client traits)
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct SavingsGoal {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
    pub unlock_date: Option<u64>,
    pub tags: Vec<soroban_sdk::String>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GoalPage {
    pub items: Vec<SavingsGoal>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub external_ref: Option<soroban_sdk::String>,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
    pub schedule_id: Option<u32>,
    pub tags: Vec<soroban_sdk::String>,
    pub currency: soroban_sdk::String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BillPage {
    pub items: Vec<Bill>,
    pub next_cursor: u32,
    pub count: u32,
}

/// Mirror of the real `insurance::Policy` struct.
///
/// Field order and types MUST match `insurance::Policy` exactly so a real
/// `Policy` returned by `get_policy` deserializes into this type without a
/// `ConversionError`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct InsurancePolicy {
    pub id: u32,
    pub owner: Address,
    pub name: soroban_sdk::String,
    pub coverage_type: CoverageType,
    pub monthly_premium: i128,
    pub coverage_amount: i128,
    pub external_ref: Option<soroban_sdk::String>,
    pub active: bool,
    pub created_at: u64,
    pub last_payment_at: u64,
    pub next_payment_date: u64,
}

/// Mirror of the real `insurance::PolicyPage`.
///
/// `items` is a list of policy IDs (`Vec<u32>`); fetch each full policy with
/// `InsuranceClient::get_policy`.
#[contracttype]
#[derive(Clone)]
pub struct PolicyPage {
    pub items: Vec<u32>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct MemberAddressPage {
    pub items: Vec<Address>,
    pub next_cursor: u32,
    pub count: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct SpendingPeriod {
    pub period_type: u32,
    pub period_start: u64,
    pub period_duration: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct SpendingTracker {
    pub current_spent: i128,
    pub last_tx_timestamp: u64,
    pub tx_count: u32,
    pub period: SpendingPeriod,
}

/// Compute `(numerator * scale) / denominator` using checked arithmetic.
///
/// Returns `0` when `denominator <= 0` (safe default for percentage/ratio math).
/// Intermediate arithmetic uses `i128` to avoid overflow on large inputs.
pub(crate) fn safe_percent(numerator: i128, denominator: i128, scale: i128) -> i128 {
    if denominator <= 0 {
        return 0;
    }
    // checked_mul returns None on overflow
    match numerator.checked_mul(scale) {
        Some(scaled) => scaled / denominator,
        None => {
            if numerator > 0 {
                scale
            } else {
                -scale
            }
        }
    }
}

fn trend_from_amounts(current_amount: i128, previous_amount: i128) -> TrendData {
    let change_amount = current_amount.checked_sub(previous_amount).unwrap_or(
        if current_amount >= previous_amount {
            i128::MAX
        } else {
            i128::MIN
        },
    );
    let change_percentage = if previous_amount > 0 {
        safe_percent(change_amount, previous_amount, 100).clamp(i32::MIN as i128, i32::MAX as i128)
            as i32
    } else if current_amount > 0 {
        100
    } else {
        0
    };

    TrendData {
        current_amount,
        previous_amount,
        change_amount,
        change_percentage,
    }
}

/// Result of a generic paginated dependency fetch.
pub(crate) struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub data_availability: DataAvailability,
}

/// Generic paginated fetch helper.
///
/// Repeatedly calls `fetch_page(cursor)` to accumulate items across pages
/// up to [`MAX_DEP_PAGES`] (20). The loop **always terminates**:
/// - After `MAX_DEP_PAGES` iterations (cap reached — returns `Partial`).
/// - When `fetch_page` returns `next_cursor == 0` (end sentinel).
///
/// Cursor monotonicity is guaranteed because every iteration increments
/// `pages_fetched` and the closure is the sole source of the next cursor;
/// a buggy closure that never returns 0 is still bounded by `MAX_DEP_PAGES`.
///
/// # DataAvailability contract
/// | Return value  | Condition |
/// |---|---|
/// | `Complete`    | All pages drained within the page cap (`next_cursor == 0` seen). |
/// | `Partial`     | The page cap was reached — result may be truncated. |
/// | `Missing`     | The first page contained zero items. |
pub(crate) fn paginate_dependency<T>(
    env: &Env,
    mut fetch_page: impl FnMut(u32) -> (Vec<T>, u32),
) -> PaginatedResult<T>
where
    T: IntoVal<Env, Val> + TryFromVal<Env, Val> + Clone,
{
    let mut items: Vec<T> = Vec::new(env);
    let mut cursor = 0u32;
    let mut pages_fetched = 0u32;

    loop {
        let (page_items, next_cursor) = fetch_page(cursor);
        for item in page_items.iter() {
            items.push_back(item);
        }
        pages_fetched += 1;

        if next_cursor == 0 {
            break;
        }
        if pages_fetched >= MAX_DEP_PAGES {
            return PaginatedResult {
                items,
                data_availability: DataAvailability::Partial,
            };
        }
        cursor = next_cursor;
    }

    let data_availability = if items.is_empty() {
        DataAvailability::Missing
    } else {
        DataAvailability::Complete
    };

    PaginatedResult {
        items,
        data_availability,
    }
}

#[contract]
pub struct ReportingContract;

#[contractimpl]
impl ReportingContract {
    // ---------------------------------------------------------------------
    // Dependency address integrity
    // ---------------------------------------------------------------------

    /// Validates the five downstream contract addresses before they are persisted or used.
    ///
    /// # Security assumptions
    ///
    /// - **Self-reference**: No slot may equal [`Env::current_contract_address`]. Routing a role
    ///   back to this reporting contract would make cross-contract calls ambiguous and can break
    ///   tooling that assumes unique callees.
    /// - **Pairwise uniqueness**: Each of `remittance_split`, `savings_goals`, `bill_payments`,
    ///   `insurance`, and `family_wallet` must refer to a **different** contract ID. Duplicate IDs
    ///   mean two logical roles silently talk to the same deployment (data integrity / audit risk).
    /// Complexity: constant time (five slots, fixed number of equality checks).
    fn validate_dependency_address_set(
        env: &Env,
        addrs: &ContractAddresses,
    ) -> Result<(), ReportingError> {
        let reporting = env.current_contract_address();
        let slots = [
            &addrs.remittance_split,
            &addrs.savings_goals,
            &addrs.bill_payments,
            &addrs.insurance,
            &addrs.family_wallet,
        ];

        for slot in slots {
            if *slot == reporting {
                return Err(ReportingError::InvalidDependencyAddressConfiguration);
            }
        }

        for i in 0..slots.len() {
            for j in (i + 1)..slots.len() {
                if *slots[i] == *slots[j] {
                    return Err(ReportingError::InvalidDependencyAddressConfiguration);
                }
            }
        }

        Ok(())
    }

    /// Verify a [`ContractAddresses`] bundle using the same rules as [`ReportingContract::configure_addresses`].
    ///
    /// Does **not** write storage and does **not** require authorization. Intended for admin UIs and
    /// offline checks before submitting a configuration transaction.
    ///
    /// # Errors
    ///
    /// * [`ReportingError::InvalidDependencyAddressConfiguration`] — duplicates or self-reference.
    pub fn verify_dependency_address_set(
        env: Env,
        addrs: ContractAddresses,
    ) -> Result<(), ReportingError> {
        Self::validate_dependency_address_set(&env, &addrs)
    }

    /// Initialize the reporting contract with an admin address.
    ///
    /// This function must be called only once. The provided admin address will
    /// have full control over contract configuration and maintenance.
    ///
    /// # Arguments
    /// * `admin` - Address of the initial contract administrator
    ///
    /// # Returns
    /// `Ok(())` on successful initialization
    ///
    /// # Errors
    /// * `AlreadyInitialized` - If the contract has already been initialized
    pub fn init(env: Env, admin: Address) -> Result<(), ReportingError> {
        let existing: Option<Address> = env.storage().instance().get(&symbol_short!("ADMIN"));
        if existing.is_some() {
            return Err(ReportingError::AlreadyInitialized);
        }

        admin.require_auth();

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &admin);

        Ok(())
    }

    // ---------------------------------------------------------------------
    // Delegated report access
    // ---------------------------------------------------------------------

    /// Look up the raw expiry for an explicit subject-to-viewer grant.
    ///
    /// The owner is part of the key, so a viewer grant for one subject can
    /// never authorize access to another subject's records. Reads the
    /// per-grant persistent entry first and falls back to the legacy
    /// `VIEW_ACL` instance map until migration has drained it.
    fn viewer_grant_expiry(
        env: &Env,
        owner: &Address,
        viewer: &Address,
        scope: ReportScope,
    ) -> Option<u64> {
        let key = DataKey::ViewerGrant(owner.clone(), viewer.clone(), scope);
        if let Some(expires_at) = Self::read_persistent::<u64>(env, &key) {
            return Some(expires_at);
        }
        env.storage()
            .instance()
            .get::<_, Map<(Address, Address, ReportScope), u64>>(&LEGACY_VIEW_ACL)
            .and_then(|grants| grants.get((owner.clone(), viewer.clone(), scope)))
    }

    fn viewer_is_authorized(
        env: &Env,
        caller: &Address,
        owner: &Address,
        scope: ReportScope,
    ) -> bool {
        if caller == owner {
            return true;
        }
        match Self::viewer_grant_expiry(env, owner, caller, scope) {
            Some(expires_at) => expires_at == 0 || env.ledger().timestamp() < expires_at,
            None => false,
        }
    }

    fn require_viewer(
        env: &Env,
        caller: &Address,
        owner: &Address,
        scope: ReportScope,
    ) -> Result<(), ReportingError> {
        caller.require_auth();
        if Self::viewer_is_authorized(env, caller, owner, scope) {
            Ok(())
        } else {
            // Every denied subject/scope pair returns the same error. This
            // prevents callers from probing whether a report exists.
            Err(ReportingError::Unauthorized)
        }
    }

    /// Grant a viewer access to one explicit report scope.
    ///
    /// `expires_at == 0` creates a non-expiring grant. Any non-zero expiry
    /// must be in the future. Only the subject can grant or revoke access;
    /// viewers cannot delegate the subject's authority onward.
    pub fn grant_viewer(
        env: Env,
        owner: Address,
        viewer: Address,
        scope: ReportScope,
        expires_at: u64,
    ) -> Result<(), ReportingError> {
        owner.require_auth();
        let admin: Option<Address> = env.storage().instance().get(&symbol_short!("ADMIN"));
        if admin.is_none() {
            return Err(ReportingError::NotInitialized);
        }
        if viewer == owner || viewer == env.current_contract_address() {
            return Err(ReportingError::InvalidViewer);
        }
        if expires_at != 0 && expires_at <= env.ledger().timestamp() {
            return Err(ReportingError::InvalidGrantExpiry);
        }

        Self::write_persistent(
            &env,
            &DataKey::ViewerGrant(owner.clone(), viewer.clone(), scope),
            &expires_at,
        );
        Self::extend_instance_ttl(&env);
        env.events().publish(
            (symbol_short!("report"), ReportEvent::ViewerGranted),
            ViewerGrant {
                owner,
                viewer,
                scope,
                expires_at,
            },
        );
        Ok(())
    }

    /// Revoke a viewer's access immediately for future queries.
    pub fn revoke_viewer(
        env: Env,
        owner: Address,
        viewer: Address,
        scope: ReportScope,
    ) -> Result<(), ReportingError> {
        owner.require_auth();
        let _: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;
        env.storage()
            .persistent()
            .remove(&DataKey::ViewerGrant(owner.clone(), viewer.clone(), scope));
        // Security: until migration completes, reads fall back to the legacy
        // map. Remove the legacy entry too so a revoked grant cannot be
        // resurrected by the fallback path.
        if let Some(mut legacy) = env
            .storage()
            .instance()
            .get::<_, Map<(Address, Address, ReportScope), u64>>(&LEGACY_VIEW_ACL)
        {
            let legacy_key = (owner.clone(), viewer.clone(), scope);
            if legacy.contains_key(legacy_key.clone()) {
                legacy.remove(legacy_key);
                Self::store_legacy_map(&env, &LEGACY_VIEW_ACL, &legacy);
            }
        }
        Self::extend_instance_ttl(&env);
        env.events().publish(
            (symbol_short!("report"), ReportEvent::ViewerRevoked),
            ViewerGrant {
                owner,
                viewer,
                scope,
                expires_at: 0,
            },
        );
        Ok(())
    }

    /// Read the effective grant without exposing report existence.
    pub fn get_viewer_grant(
        env: Env,
        caller: Address,
        owner: Address,
        viewer: Address,
        scope: ReportScope,
    ) -> Result<Option<u64>, ReportingError> {
        caller.require_auth();
        if caller != owner && caller != viewer {
            return Err(ReportingError::Unauthorized);
        }
        Ok(Self::viewer_grant_expiry(&env, &owner, &viewer, scope)
            .filter(|expiry| *expiry == 0 || env.ledger().timestamp() < *expiry))
    }

    /// Propose a new administrator for the contract.
    ///
    /// This is the first step of a two-step admin rotation process. The proposed
    /// admin must then call `accept_admin_rotation` to complete the transfer.
    ///
    /// # Arguments
    /// * `caller` - Current administrator (must authorize)
    /// * `new_admin` - Address of the proposed successor
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the current admin
    /// * `InvalidAdmin` - If `new_admin` is this contract's own address
    /// * `SameAdmin` - If `new_admin` is the same as the current admin
    pub fn propose_new_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        // Rejecting the contract's own address prevents an unrecoverable brick:
        // this contract can never produce a `require_auth()` signature for
        // itself as an external caller, so `accept_admin_rotation` could never
        // be completed and admin control would be permanently lost.
        if new_admin == env.current_contract_address() {
            return Err(ReportingError::InvalidAdmin);
        }

        if new_admin == admin {
            return Err(ReportingError::SameAdmin);
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("PEND_ADM"), &new_admin);

        Ok(())
    }

    /// Accept the role of contract administrator.
    ///
    /// This is the second step of a two-step admin rotation process. Only the
    /// address currently proposed via `propose_new_admin` can call this.
    ///
    /// # Arguments
    /// * `caller` - The proposed administrator (must authorize)
    ///
    /// # Errors
    /// * `NotAdminProposed` - If no admin rotation is currently in progress
    /// * `Unauthorized` - If caller is not the proposed admin
    pub fn accept_admin_rotation(env: Env, caller: Address) -> Result<(), ReportingError> {
        caller.require_auth();

        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("PEND_ADM"))
            .ok_or(ReportingError::NotAdminProposed)?;

        if caller != pending_admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);
        env.storage()
            .instance()
            .set(&symbol_short!("ADMIN"), &pending_admin);
        env.storage().instance().remove(&symbol_short!("PEND_ADM"));

        Ok(())
    }

    /// Configure addresses for all related contracts (admin only).
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `remittance_split` - Address of the remittance split contract
    /// * `savings_goals` - Address of the savings goals contract
    /// * `bill_payments` - Address of the bill payments contract
    /// * `insurance` - Address of the insurance contract
    /// * `family_wallet` - Address of the family wallet contract
    ///
    /// # Returns
    /// `Ok(())` on successful configuration
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    /// * [`ReportingError::InvalidDependencyAddressConfiguration`] - Duplicate addresses or
    ///   self-reference (this reporting contract used as a dependency).
    ///
    /// # Panics
    /// * If `caller` does not authorize the transaction
    pub fn configure_addresses(
        env: Env,
        caller: Address,
        remittance_split: Address,
        savings_goals: Address,
        bill_payments: Address,
        insurance: Address,
        family_wallet: Address,
    ) -> Result<(), ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let addresses = ContractAddresses {
            remittance_split,
            savings_goals,
            bill_payments,
            insurance,
            family_wallet,
        };

        Self::validate_dependency_address_set(&env, &addresses)?;

        env.storage()
            .instance()
            .set(&symbol_short!("ADDRS"), &addresses);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::AddressesConfigured),
            caller,
        );

        Ok(())
    }

    /// Check health of all configured dependencies (admin only).
    ///
    /// Performs minimal `try_*` calls against each configured contract to
    /// verify that the address is responsive and exposes the expected method.
    /// The call is side-effect free: it reads admin/address configuration and
    /// does not write contract storage.
    ///
    /// Returns exactly one `DependencyStatus` per `ContractAddresses` slot, in
    /// this order:
    ///
    /// 1. `remittance_split`
    /// 2. `savings_goals`
    /// 3. `bill_payments`
    /// 4. `insurance`
    /// 5. `family_wallet`
    ///
    /// Status values are encoded as `ok == true` with `error_category == None`
    /// for a healthy probe, or `ok == false` with a concrete
    /// `error_category` when the probe fails.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    ///
    /// # Returns
    /// Vec of DependencyStatus for each configured contract.
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    /// * `AddressesNotConfigured` - If dependency addresses have not been configured
    pub fn check_dependencies(
        env: Env,
        caller: Address,
    ) -> Result<Vec<DependencyStatus>, ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let mut statuses = Vec::new(&env);

        // Check remittance_split
        let split_client = RemittanceSplitClient::new(&env, &addresses.remittance_split);
        let split_ok = matches!(split_client.try_get_config(), Ok(Ok(_)));
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "remittance_split"),
            ok: split_ok,
            error_category: if split_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_split_failed"))
            },
        });

        // Check savings_goals
        let savings_client = SavingsGoalsClient::new(&env, &addresses.savings_goals);
        let savings_ok = matches!(
            savings_client.try_get_all_goals(&env.current_contract_address()),
            Ok(Ok(_))
        );
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "savings_goals"),
            ok: savings_ok,
            error_category: if savings_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_all_goals_failed"))
            },
        });

        // Check bill_payments
        let bill_client = BillPaymentsClient::new(&env, &addresses.bill_payments);
        let bill_ok = matches!(
            bill_client.try_get_total_unpaid(&env.current_contract_address()),
            Ok(Ok(_))
        );
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "bill_payments"),
            ok: bill_ok,
            error_category: if bill_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(
                    &env,
                    "get_total_unpaid_failed",
                ))
            },
        });

        // Check insurance
        let insurance_client = InsuranceClient::new(&env, &addresses.insurance);
        let insurance_ok = matches!(
            insurance_client.try_get_total_monthly_premium(&env.current_contract_address()),
            Ok(Ok(_))
        );
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "insurance"),
            ok: insurance_ok,
            error_category: if insurance_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(
                    &env,
                    "get_total_monthly_premium_failed",
                ))
            },
        });

        // Check family_wallet
        let family_client = FamilyWalletClient::new(&env, &addresses.family_wallet);
        let family_ok = matches!(family_client.try_get_owner(), Ok(Ok(_)));
        statuses.push_back(DependencyStatus {
            name: soroban_sdk::String::from_str(&env, "family_wallet"),
            ok: family_ok,
            error_category: if family_ok {
                None
            } else {
                Some(soroban_sdk::String::from_str(&env, "get_owner_failed"))
            },
        });

        Ok(statuses)
    }

    /// Generate remittance summary report.
    ///
    /// Fetches split configuration and calculates amounts for a specific period.
    pub fn get_remittance_summary(
        env: Env,
        user: Address,
        total_amount: i128,
        period_start: u64,
        period_end: u64,
    ) -> Result<RemittanceSummary, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        Self::get_remittance_summary_internal(&env, total_amount, period_start, period_end)
    }

    fn get_remittance_summary_internal(
        env: &Env,
        total_amount: i128,
        period_start: u64,
        period_end: u64,
    ) -> Result<RemittanceSummary, ReportingError> {
        let addresses: Option<ContractAddresses> =
            env.storage().instance().get(&symbol_short!("ADDRS"));

        let addresses = match addresses {
            Some(a) => a,
            None => {
                return Ok(RemittanceSummary {
                    total_received: total_amount,
                    total_allocated: total_amount,
                    category_breakdown: Vec::new(env),
                    period_start,
                    period_end,
                    data_availability: DataAvailability::Missing,
                })
            }
        };
        let split_client = RemittanceSplitClient::new(env, &addresses.remittance_split);
        let mut availability = DataAvailability::Complete;

        let split_percentages = match split_client.try_get_config() {
            Ok(Ok(Some(cfg))) => soroban_sdk::vec![
                env,
                cfg.spending_percent,
                cfg.savings_percent,
                cfg.bills_percent,
                cfg.insurance_percent
            ],
            // `None` (split_not_configured) or any call/decode failure.
            _ => {
                availability = DataAvailability::Partial;
                Vec::new(env)
            }
        };

        let mut split_amounts = Vec::new(env);
        if availability == DataAvailability::Complete {
            let mut sum = 0u32;
            // Percentages are basis points that must sum to 10_000 (10000 = 100.00%)
            for i in 0..split_percentages.len() {
                let p = split_percentages.get(i).unwrap_or(0);
                sum = sum
                    .checked_add(p)
                    .ok_or(ReportingError::InvalidPercentageSplit)?;
                if sum > 10_000 {
                    return Err(ReportingError::InvalidPercentageSplit);
                }

                // Percentages are basis points; divide by 10_000 (100.00%).
                let amount = total_amount
                    .checked_mul(p as i128)
                    .ok_or(ReportingError::Overflow)?
                    / 10_000;
                split_amounts.push_back(amount);
            }

            if sum != 10_000 {
                return Err(ReportingError::InvalidPercentageSplit);
            }
        }

        let mut breakdown = Vec::new(env);
        let categories = [
            Category::Spending,
            Category::Savings,
            Category::Bills,
            Category::Insurance,
        ];

        for (i, &category) in categories.iter().enumerate() {
            let amount = split_amounts.get(i as u32).unwrap_or(0);
            let percentage = split_percentages.get(i as u32).unwrap_or(0);
            // Safe conversion guards for any u64 -> u32 casts used elsewhere (none here)
            breakdown.push_back(CategoryBreakdown {
                category,
                amount,
                percentage,
            });
        }

        Ok(RemittanceSummary {
            total_received: total_amount,
            total_allocated: total_amount,
            category_breakdown: breakdown,
            period_start,
            period_end,
            data_availability: availability,
        })
    }

    /// Generate savings progress report.
    ///
    /// Aggregates all goals for a user and calculates overall completion progress.
    pub fn get_savings_report(
        env: Env,
        _caller: Address,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<SavingsReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        Self::get_savings_report_internal(&env, user, period_start, period_end)
    }

    fn get_savings_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<SavingsReport, ReportingError> {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let savings_client = SavingsGoalsClient::new(env, &addresses.savings_goals);
        let goals = savings_client.get_all_goals(&user);

        let mut total_target = 0i128;
        let mut total_saved = 0i128;
        let mut completed_count = 0u32;
        let total_goals = goals.len();

        for goal in goals.iter() {
            total_target = total_target.saturating_add(goal.target_amount);
            total_saved = total_saved.saturating_add(goal.current_amount);
            if goal.current_amount >= goal.target_amount {
                completed_count = completed_count.saturating_add(1);
            }
        }

        let completion_percentage = safe_percent(total_saved, total_target, 100).min(100);
        let completion_percentage =
            u64_to_u32(completion_percentage as u64).map_err(|_| ReportingError::Overflow)?;

        Ok(SavingsReport {
            total_goals,
            completed_goals: completed_count,
            total_target,
            total_saved,
            completion_percentage,
            period_start,
            period_end,
        })
    }

    /// Generate bill payment compliance report.
    ///
    /// Analyzes bill statuses and payment deadlines for a specific period.
    pub fn get_bill_compliance_report(
        env: Env,
        _caller: Address,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<BillComplianceReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        Self::get_bill_compliance_report_internal(&env, user, period_start, period_end)
    }

    fn get_bill_compliance_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<BillComplianceReport, ReportingError> {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let bill_client = BillPaymentsClient::new(env, &addresses.bill_payments);
        let current_time = env.ledger().timestamp();

        let result = paginate_dependency(env, |cursor| {
            let page = bill_client.get_all_bills_for_owner(&user, &cursor, &DEP_PAGE_LIMIT);
            (page.items, page.next_cursor)
        });

        let mut total_bills = 0u32;
        let mut paid_bills = 0u32;
        let mut unpaid_bills = 0u32;
        let mut overdue_bills = 0u32;
        let mut total_amount = 0i128;
        let mut paid_amount = 0i128;
        let mut unpaid_amount = 0i128;

        for bill in result.items.iter() {
            if bill.created_at < period_start || bill.created_at > period_end {
                continue;
            }
            total_bills += 1;
            total_amount += bill.amount;
            if bill.paid {
                paid_bills += 1;
                paid_amount += bill.amount;
            } else {
                unpaid_bills += 1;
                unpaid_amount += bill.amount;
                if bill.due_date < current_time {
                    overdue_bills += 1;
                }
            }
        }

        let compliance_percentage = if total_bills == 0 {
            100u32
        } else {
            let val: i128 =
                safe_percent(paid_bills as i128, total_bills as i128, 100).clamp(0, 100);
            u64_to_u32(val as u64).map_err(|_| ReportingError::Overflow)?
        };

        Ok(BillComplianceReport {
            total_bills,
            paid_bills,
            unpaid_bills,
            overdue_bills,
            total_amount,
            paid_amount,
            unpaid_amount,
            compliance_percentage,
            period_start,
            period_end,
            data_availability: result.data_availability,
        })
    }

    /// Generate insurance coverage report.
    ///
    /// Summarizes active policies, coverage amounts, and premium ratios.
    pub fn get_insurance_report(
        env: Env,
        _caller: Address,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<InsuranceReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        Self::get_insurance_report_internal(&env, user, period_start, period_end)
    }

    fn get_insurance_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<InsuranceReport, ReportingError> {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let insurance_client = InsuranceClient::new(env, &addresses.insurance);
        let monthly_premium = insurance_client.get_total_monthly_premium(&user);

        // The insurance contract's `get_active_policies` returns policy IDs only.
        // Page through the IDs, then resolve each to a full policy via `get_policy`.
        let result = paginate_dependency(env, |cursor| {
            let page = insurance_client.get_active_policies(&user, &cursor, &DEP_PAGE_LIMIT);
            (page.items, page.next_cursor)
        });

        let mut total_coverage = 0i128;
        let mut active_policies = 0u32;

        for policy_id in result.items.iter() {
            if let Some(policy) = insurance_client.get_policy(&policy_id) {
                active_policies += 1;
                total_coverage += policy.coverage_amount;
            }
        }

        let annual_premium = monthly_premium.saturating_mul(12);
        let coverage_to_premium_ratio = {
            let val: i128 =
                safe_percent(total_coverage, annual_premium, 100).clamp(0, u32::MAX as i128);
            u64_to_u32(val as u64).map_err(|_| ReportingError::Overflow)?
        };

        Ok(InsuranceReport {
            active_policies,
            total_coverage,
            monthly_premium,
            annual_premium,
            coverage_to_premium_ratio,
            period_start,
            period_end,
            data_availability: result.data_availability,
        })
    }

    /// Generate a family-wallet spending report.
    ///
    /// Reads the configured `family_wallet` dependency via [`FamilyWalletClient`]
    /// to enumerate members (`get_member_addresses_page`) and fetch each member's
    /// current [`SpendingTracker`] (`get_spending_tracker`), returning a per-member
    /// breakdown plus aggregate totals.
    ///
    /// # Aggregation
    ///
    /// - Members are paged with [`DEP_PAGE_LIMIT`] and deduplicated by address.
    /// - `total_spending` sums `SpendingTracker.current_spent` using checked
    ///   arithmetic; overflow saturates and marks the report `Partial`.
    /// - `average_per_member` is `total_spending / total_members`, or `0` when
    ///   there are no members (divide-by-zero safe).
    ///
    /// # DataAvailability degradation
    ///
    /// | Value | Condition |
    /// |---|---|
    /// | `Complete` | All member pages drained and every spending read succeeded (or returned `None`). |
    /// | `Partial` | Member paging hit [`MAX_DEP_PAGES`], a mid-pagination call failed after at least one page, a spending tracker read failed, or total spending overflowed. |
    /// | `Missing` | The first member page is empty, or the family wallet is unreachable on the first fetch. |
    ///
    /// # Errors
    ///
    /// - `InvalidPeriod` when `period_start > period_end`.
    /// - `AddressesNotConfigured` when dependency addresses have not been set.
    pub fn get_family_spending_report(
        env: Env,
        _caller: Address,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<FamilySpendingReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        Self::get_family_spending_report_internal(&env, period_start, period_end)
    }

    fn get_family_spending_report_internal(
        env: &Env,
        period_start: u64,
        period_end: u64,
    ) -> Result<FamilySpendingReport, ReportingError> {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let family_client = FamilyWalletClient::new(env, &addresses.family_wallet);
        let mut availability = DataAvailability::Complete;
        let mut breakdown: Vec<FamilyMemberSpending> = Vec::new(env);
        let mut seen_members: Map<Address, bool> = Map::new(env);
        let mut total_spending = 0i128;

        let mut cursor = 0u32;
        let mut page_index = 0u32;
        let mut saw_member_page = false;

        loop {
            if page_index >= MAX_DEP_PAGES {
                availability = DataAvailability::Partial;
                break;
            }

            let page = match family_client.try_get_member_addresses_page(&cursor, &DEP_PAGE_LIMIT) {
                Ok(Ok(page)) => page,
                _ if saw_member_page => {
                    availability = DataAvailability::Partial;
                    break;
                }
                _ => {
                    availability = DataAvailability::Missing;
                    break;
                }
            };

            page_index = page_index.saturating_add(1);

            if page.items.is_empty() && cursor == 0 {
                availability = DataAvailability::Missing;
                break;
            }

            saw_member_page = true;

            for member in page.items.iter() {
                if seen_members.get(member.clone()).unwrap_or(false) {
                    continue;
                }
                seen_members.set(member.clone(), true);

                let tracker_result = family_client.try_get_spending_tracker(&member);
                let (member_spending, data_available) = match tracker_result {
                    Ok(Ok(Some(tracker))) => (tracker.current_spent, true),
                    Ok(Ok(None)) => (0, true),
                    _ => {
                        availability = DataAvailability::Partial;
                        (0, false)
                    }
                };

                total_spending = match total_spending.checked_add(member_spending) {
                    Some(sum) => sum,
                    None => {
                        availability = DataAvailability::Partial;
                        total_spending.saturating_add(member_spending)
                    }
                };

                breakdown.push_back(FamilyMemberSpending {
                    member,
                    total_spending: member_spending,
                    data_available,
                });
            }

            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }

        let total_members = breakdown.len();
        let average_per_member = if total_members == 0 {
            0
        } else {
            total_spending / (total_members as i128)
        };

        Ok(FamilySpendingReport {
            member_breakdown: breakdown,
            total_members,
            total_spending,
            average_per_member,
            period_start,
            period_end,
            data_availability: availability,
        })
    }

    /// Calculate financial health score with hardened arithmetic and normalization
    ///
    /// This function computes a comprehensive financial health score (0-100) based on:
    /// - Savings progress (0-40 points): Based on goal completion percentage
    /// - Bill payment compliance (0-40 points): tiered 40 (none unpaid) /
    ///   35 (unpaid, none overdue) / 20 (overdue)
    /// - Insurance coverage (0-20 points): binary, 20 if any active policy else 0
    ///
    /// The final score is `clamp(savings + bills + insurance, 0, 100)`. The
    /// `_total_remittance` argument is currently unused. When a downstream
    /// contract has no data the relevant component falls back to its default
    /// (savings 20, bills 40, insurance 0); if addresses are unconfigured the
    /// call returns `AddressesNotConfigured` rather than a partial score.
    ///
    /// See `docs/HEALTH_SCORE.md` (at the repository root) for the full model,
    /// the exact input of each component, clamping, the `DataAvailability`
    /// (Partial/Missing) relationship, and worked examples.
    ///
    /// # Arithmetic Safety
    /// - Uses safe division to prevent overflow
    /// - Clamps all intermediate and final scores to valid ranges
    /// - Handles edge cases: zero targets, negative amounts, extreme values
    ///
    /// # Normalization Guarantees
    /// - Overall score is always bounded [0, 100]
    /// - Component scores are bounded to their respective maximums
    /// - Progress calculations use saturating arithmetic
    ///
    /// # Arguments
    /// * `env` - Soroban environment
    /// * `user` - Address of the user to calculate score for
    /// * `_total_remittance` - Total remittance amount (currently unused)
    ///
    /// # Returns
    /// `HealthScore` struct with overall and component scores
    ///
    /// # Security Notes
    /// - All cross-contract calls are made to configured addresses
    /// - Arithmetic operations are overflow-safe
    /// - No external dependencies on ledger state beyond cross-contract data
    pub fn calculate_health_score(
        env: Env,
        user: Address,
        _total_remittance: i128,
    ) -> Result<HealthScore, ReportingError> {
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        // Calculate savings score (0-40 points) with safe arithmetic
        let savings_score = Self::calculate_savings_score(&env, &addresses, &user);

        // Calculate bills score (0-40 points) with safe arithmetic
        let bills_score = Self::calculate_bills_score(&env, &addresses, &user);

        // Calculate insurance score (0-20 points)
        let insurance_score = Self::calculate_insurance_score(&env, &addresses, &user);

        // Calculate total score with bounds checking
        let total_score = Self::clamp_score(savings_score + bills_score + insurance_score, 0, 100);

        Ok(HealthScore {
            score: total_score,
            savings_score,
            bills_score,
            insurance_score,
        })
    }

    /// Calculate savings score component (0-40 points)
    ///
    /// Score is based on the percentage of savings goals achieved.
    /// Uses safe division to prevent overflow and clamps result to [0, 40].
    fn calculate_savings_score(env: &Env, addresses: &ContractAddresses, user: &Address) -> u32 {
        let savings_client = SavingsGoalsClient::new(env, &addresses.savings_goals);
        let goals = savings_client.get_all_goals(user);

        let mut total_target = 0i128;
        let mut total_saved = 0i128;

        // Sum all goals with overflow protection
        for goal in goals.iter() {
            // Clamp individual amounts to prevent extreme values from dominating
            let target = Self::clamp_amount(goal.target_amount, 0, i128::MAX / 2);
            let saved = Self::clamp_amount(goal.current_amount, 0, target);

            total_target = total_target.saturating_add(target);
            total_saved = total_saved.saturating_add(saved);
        }

        if total_target == 0 {
            // No goals set - assign default score
            return 20;
        }

        // Safe percentage calculation: (saved * 100) / target
        // To prevent overflow: use (saved * 100) / target, but check for overflow
        let progress_percentage = if total_saved >= total_target {
            100u32
        } else {
            // Safe division: multiply first, then divide to maintain precision
            // (saved * 100) / target, but avoid intermediate overflow
            let saved_scaled = total_saved.saturating_mul(100);
            let progress = saved_scaled.checked_div(total_target).unwrap_or(0);
            u64_to_u32(progress as u64).unwrap_or(0).min(100)
        };

        // Convert percentage to score: (progress * 40) / 100
        let score = (progress_percentage * 40) / 100;
        score.min(40) // Ensure maximum is 40
    }

    /// Calculate bills score component (0-40 points)
    ///
    /// Score is based on bill payment compliance:
    /// - 40 points: No unpaid bills
    /// - 35 points: Has unpaid bills but none overdue
    /// - 20 points: Has overdue bills
    fn calculate_bills_score(env: &Env, addresses: &ContractAddresses, user: &Address) -> u32 {
        let bill_client = BillPaymentsClient::new(env, &addresses.bill_payments);
        let unpaid_bills = bill_client.get_unpaid_bills(user, &0u32, &1000u32).items; // Large limit to get all

        if unpaid_bills.is_empty() {
            return 40; // Perfect compliance
        }

        let current_time = env.ledger().timestamp();
        let overdue_count = unpaid_bills
            .iter()
            .filter(|bill| bill.due_date < current_time)
            .count();

        if overdue_count == 0 {
            35 // Has unpaid but none overdue
        } else {
            20 // Has overdue bills
        }
    }

    /// Calculate insurance score component (0-20 points)
    ///
    /// Score is binary: 20 points if at least one active policy, 0 otherwise.
    fn calculate_insurance_score(env: &Env, addresses: &ContractAddresses, user: &Address) -> u32 {
        let insurance_client = InsuranceClient::new(env, &addresses.insurance);
        let policy_page = insurance_client.get_active_policies(user, &0, &1); // Just check if any exist

        if policy_page.items.is_empty() {
            0
        } else {
            20
        }
    }

    /// Clamp a score value to specified min/max bounds
    fn clamp_score(value: u32, min: u32, max: u32) -> u32 {
        if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        }
    }

    /// Clamp an amount to specified min/max bounds
    fn clamp_amount(value: i128, min: i128, max: i128) -> i128 {
        if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        }
    }

    fn worst_data_availability(
        left: DataAvailability,
        right: DataAvailability,
    ) -> DataAvailability {
        match (left, right) {
            (DataAvailability::Missing, _) | (_, DataAvailability::Missing) => {
                DataAvailability::Missing
            }
            (DataAvailability::Partial, _) | (_, DataAvailability::Partial) => {
                DataAvailability::Partial
            }
            _ => DataAvailability::Complete,
        }
    }

    /// Generate comprehensive financial health report
    pub fn get_financial_health_report(
        env: Env,
        _caller: Address,
        user: Address,
        total_remittance: i128,
        period_start: u64,
        period_end: u64,
    ) -> Result<FinancialHealthReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        user.require_auth();
        let health_score =
            Self::calculate_health_score(env.clone(), user.clone(), total_remittance)?;
        let remittance_summary = Self::get_remittance_summary_internal(
            &env,
            total_remittance,
            period_start,
            period_end,
        )?;
        let savings_report =
            Self::get_savings_report_internal(&env, user.clone(), period_start, period_end);
        let bill_compliance =
            Self::get_bill_compliance_report_internal(&env, user.clone(), period_start, period_end);
        let insurance_report =
            Self::get_insurance_report_internal(&env, user, period_start, period_end);

        let generated_at = env.ledger().timestamp();
        let bill_compliance = bill_compliance?;
        let insurance_report = insurance_report?;
        let data_availability = Self::worst_data_availability(
            remittance_summary.data_availability,
            Self::worst_data_availability(
                bill_compliance.data_availability,
                insurance_report.data_availability,
            ),
        );

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportGenerated),
            generated_at,
        );

        Ok(FinancialHealthReport {
            health_score,
            remittance_summary,
            savings_report: savings_report?,
            bill_compliance,
            insurance_report,
            data_availability,
            generated_at,
        })
    }

    /// Top-N bills sorted by amount (descending) within the period.
    pub fn get_top_bills_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<TopNBillsReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        // Defence-in-depth: the hardcoded MAX_ITEMS_PER_REPORT must not
        // exceed the shared MAX_TOP_N cap.  If a future code change
        // raises MAX_ITEMS_PER_REPORT above MAX_TOP_N, this guard fails
        // closed rather than letting an oversized N reach the sort loop.
        remitwise_common::require_bounded_top_n(MAX_ITEMS_PER_REPORT, MAX_TOP_N)
            .map_err(|_| ReportingError::TopNTooLarge)?;
        user.require_auth();
        Self::get_top_bills_report_internal(&env, user, period_start, period_end)
    }

    /// ✅ FIXED: Now returns Result and uses proper error handling
    fn get_top_bills_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<TopNBillsReport, ReportingError> {
        // ✅ REPLACED panic with proper error handling
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let bill_client = BillPaymentsClient::new(env, &addresses.bill_payments);

        let result = paginate_dependency(env, |cursor| {
            let page = bill_client.get_all_bills_for_owner(&user, &cursor, &DEP_PAGE_LIMIT);
            (page.items, page.next_cursor)
        });

        let mut total_amount = 0i128;
        let mut total_count = 0u32;
        let mut availability = result.data_availability;
        let mut top_bills: Vec<Bill> = Vec::new(env);

        for bill in result.items.iter() {
            if bill.created_at < period_start || bill.created_at > period_end {
                continue;
            }
            total_amount += bill.amount;
            total_count += 1;

            // Sorted insertion for Top-N (bounded)
            remitwise_common::insert_top_n(
                env,
                &mut top_bills,
                MAX_ITEMS_PER_REPORT,
                bill,
                |a, b| match a.amount.cmp(&b.amount) {
                    core::cmp::Ordering::Equal => {
                        // Deterministic tie-break by id ascending
                        // Smaller ID should be Greater (appear earlier)
                        b.id.cmp(&a.id)
                    }
                    other => other,
                },
            );
        }

        if total_count > MAX_ITEMS_PER_REPORT {
            availability = DataAvailability::Partial;
        }

        Ok(TopNBillsReport {
            items: top_bills,
            total_amount,
            total_count,
            period_start,
            period_end,
            data_availability: availability,
        })
    }

    /// Top-N savings goals sorted by target amount (descending).
    pub fn get_top_savings_report(
        env: Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<TopNSavingsReport, ReportingError> {
        remitwise_common::validate_period(period_start, period_end)
            .map_err(|_| ReportingError::InvalidPeriod)?;
        // Defence-in-depth: see [`get_top_bills_report`] for rationale.
        remitwise_common::require_bounded_top_n(MAX_ITEMS_PER_REPORT, MAX_TOP_N)
            .map_err(|_| ReportingError::TopNTooLarge)?;
        user.require_auth();
        Self::get_top_savings_report_internal(&env, user, period_start, period_end)
    }

    /// ✅ FIXED: Now returns Result and uses proper error handling
    fn get_top_savings_report_internal(
        env: &Env,
        user: Address,
        period_start: u64,
        period_end: u64,
    ) -> Result<TopNSavingsReport, ReportingError> {
        // ✅ REPLACED panic with proper error handling
        let addresses: ContractAddresses = env
            .storage()
            .instance()
            .get(&symbol_short!("ADDRS"))
            .ok_or(ReportingError::AddressesNotConfigured)?;

        let savings_client = SavingsGoalsClient::new(env, &addresses.savings_goals);

        let result = paginate_dependency(env, |cursor| {
            let page = savings_client.get_goals(&user, &cursor, &DEP_PAGE_LIMIT);
            (page.items, page.next_cursor)
        });

        let mut total_target = 0i128;
        let mut total_saved = 0i128;
        let mut total_count = 0u32;
        let mut availability = result.data_availability;
        let mut top_goals: Vec<SavingsGoal> = Vec::new(env);

        for goal in result.items.iter() {
            // Goals don't have created_at in the struct, using target_date for period filtering if appropriate
            // But typically goals are active across periods. For now, we take all active goals.
            total_target += goal.target_amount;
            total_saved += goal.current_amount;
            total_count += 1;

            // Sorted insertion for Top-N (bounded)
            remitwise_common::insert_top_n(
                env,
                &mut top_goals,
                MAX_ITEMS_PER_REPORT,
                goal,
                |a, b| match a.target_amount.cmp(&b.target_amount) {
                    core::cmp::Ordering::Equal => {
                        // Deterministic tie-break by id ascending
                        // Smaller ID should be Greater (appear earlier)
                        b.id.cmp(&a.id)
                    }
                    other => other,
                },
            );
        }

        if total_count > MAX_ITEMS_PER_REPORT {
            availability = DataAvailability::Partial;
        }

        Ok(TopNSavingsReport {
            items: top_goals,
            total_target,
            total_saved,
            total_count,
            period_start,
            period_end,
            data_availability: availability,
        })
    }

    /// Generate trend analysis comparing two data points.
    pub fn get_trend_analysis(
        _env: Env,
        _caller: Address,
        _user: Address,
        current_amount: i128,
        previous_amount: i128,
    ) -> TrendData {
        trend_from_amounts(current_amount, previous_amount)
    }

    /// Compute trend analysis over a window of historical data points.
    ///
    /// Walks `history` in the order supplied by the caller and returns one
    /// `TrendData` for each current point. Callers must sort by timestamp
    /// ascending before calling when chronological sequencing is required; this
    /// function does not reorder or validate timestamps.
    ///
    /// The first point is compared against a zero baseline, so a single-point
    /// history returns one entry with `previous_amount == 0`. Each later point is
    /// compared with the immediately preceding input point. `change_amount` is
    /// computed with checked subtraction and saturates to the relevant `i128`
    /// bound on overflow.
    ///
    /// `change_percentage` is `(current - previous) * 100 / previous` when
    /// `previous_amount > 0`, clamped to `i32`. When `previous_amount <= 0`, the
    /// zero/negative-baseline convention is `100` for a positive current amount
    /// and `0` otherwise. Negative changes from a positive baseline produce
    /// negative percentages.
    ///
    /// # Arguments
    /// * `_env`      - Contract environment
    /// * `_user`     - Address of the user (reserved for future auth scoping)
    /// * `history`   - Vec of `(period_key: u64, amount: i128)` tuples
    ///
    /// # Returns
    /// `Vec<TrendData>` with `history.len()` elements. Empty when no data points
    /// are supplied.
    pub fn get_trend_analysis_multi(
        env: Env,
        user: Address,
        history: Vec<(u64, i128)>,
    ) -> Vec<TrendData> {
        user.require_auth();
        let mut result = Vec::new(&env);
        let len = history.len();
        if len == 0 {
            return result;
        }
        for i in 0..len {
            let prev_amount = if i == 0 {
                0
            } else {
                let (_, amount) = history.get(i - 1).unwrap_or((0, 0));
                amount
            };
            let (_, curr_amount) = history.get(i).unwrap_or((0, 0));
            result.push_back(trend_from_amounts(curr_amount, prev_amount));
        }
        result
    }

    /// Store a financial health report for a user (must authorize).
    ///
    /// The report is written to its own persistent entry
    /// `DataKey::Report(user, period_key)` and `period_key` is recorded in the
    /// user's `DataKey::ReportIndex`. Cost is independent of how many other
    /// users have stored reports.
    ///
    /// Overwriting an existing `(user, period_key)` is always allowed. Adding a
    /// new period key fails once the user already holds
    /// [`MAX_ACTIVE_REPORTS_PER_USER`] active reports.
    ///
    /// # Errors
    /// * `TooManyActiveReports` - the per-user active-report cap is reached.
    pub fn store_report(
        env: Env,
        user: Address,
        report: FinancialHealthReport,
        period_key: u64,
    ) -> Result<bool, ReportingError> {
        user.require_auth();

        Self::extend_instance_ttl(&env);

        let report_key = DataKey::Report(user.clone(), period_key);
        let mut index = Self::load_report_index(&env, &user);
        let entry = ReportIndexEntry {
            period_key,
            generated_at: report.generated_at,
        };

        let mut is_new_record = false;
        if env.storage().persistent().has(&report_key) {
            // Overwrite: refresh the denormalised timestamp in the index.
            match Self::index_position(&index, period_key)? {
                Some(pos) => index.set(pos, entry),
                None => index.push_back(entry),
            }
        } else {
            if index.len() >= MAX_ACTIVE_REPORTS_PER_USER {
                return Err(ReportingError::TooManyActiveReports);
            }
            index.push_back(entry);
            // A not-yet-migrated legacy entry for the same key is already
            // counted in `STOR_STAT`; replacing it must not double count.
            is_new_record = !Self::legacy_report_exists(&env, &user, period_key);
        }

        Self::write_persistent(&env, &report_key, &report);
        Self::save_report_index(&env, &user, &index);

        env.events().publish(
            (symbol_short!("report"), ReportEvent::ReportStored),
            (user, period_key),
        );

        Self::adjust_storage_stats(&env, u32::from(is_new_record), 0, 0, 0);

        Ok(true)
    }

    /// Retrieve a previously stored report.
    ///
    /// Reads `DataKey::Report(user, period_key)` and falls back to the legacy
    /// instance map until `migrate_reports_to_persistent` has completed.
    pub fn get_stored_report(
        env: Env,
        _caller: Address,
        user: Address,
        period_key: u64,
    ) -> Option<FinancialHealthReport> {
        user.require_auth();
        Self::load_report(&env, &user, period_key)
    }

    /// Delegated equivalent of [`get_stored_report`]. The viewer must hold an
    /// active `Stored` grant for this exact owner; the owner may always read
    /// their own report. Unauthorized calls fail before storage is
    /// inspected, so report existence is not distinguishable.
    pub fn get_stored_report_for(
        env: Env,
        viewer: Address,
        owner: Address,
        period_key: u64,
    ) -> Result<Option<FinancialHealthReport>, ReportingError> {
        Self::require_viewer(&env, &viewer, &owner, ReportScope::Stored)?;
        Ok(Self::load_report(&env, &owner, period_key))
    }

    /// Get configured contract addresses.
    pub fn get_addresses(env: Env) -> Option<ContractAddresses> {
        env.storage().instance().get(&symbol_short!("ADDRS"))
    }

    /// Get current administrator address.
    pub fn get_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("ADMIN"))
    }

    /// Archive old reports before the specified timestamp (admin only).
    ///
    /// Moves report data from the primary `REPORTS` storage to the `ARCH_RPT`
    /// storage, potentially reducing gas costs for active users.
    ///
    /// Maintains set semantics on `ARCH_IDX`: each `period_key` appears at most
    /// once per user. If a report for a period was previously archived and
    /// re-stored, re-archiving overwrites the archive record in `ARCH_RPT` and
    /// deduplicates insertion into `ARCH_IDX`, leaving the index unchanged.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `before_timestamp` - Archive reports generated before this ledger timestamp
    ///
    /// # Returns
    /// `Ok(u32)` containing the number of reports archived
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    /// Archive old reports before the specified timestamp for a specific user (admin only).
    ///
    /// Moves report data from `DataKey::Report` to `DataKey::Archived`, and
    /// moves period keys from `ReportIndex` to `ArchiveIndex`. Cost is strictly
    /// bounded by `limit`, not by the global data size.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `user` - Target user whose reports are being archived
    /// * `before_timestamp` - Archive reports generated before this ledger timestamp
    /// * `limit` - Maximum number of reports to archive in this call (0 defaults to MAX_STORAGE_BATCH)
    ///
    /// # Returns
    /// `Ok(u32)` containing the number of reports archived
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    /// * `ArchiveIndexFull` - If user's archive index would exceed MAX_ARCHIVE_PER_USER
    pub fn archive_old_reports(
        env: Env,
        caller: Address,
        user: Address,
        before_timestamp: u64,
        limit: u32,
    ) -> Result<u32, ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let batch_limit = if limit == 0 {
            MAX_STORAGE_BATCH
        } else {
            limit.min(MAX_STORAGE_BATCH)
        };

        let current_time = env.ledger().timestamp();
        let mut report_idx = Self::load_report_index(&env, &user);
        let mut arch_idx = Self::load_archive_index(&env, &user);

        let mut archived_count = 0u32;
        let mut new_report_idx = Vec::new(&env);

        for entry in report_idx.iter() {
            if entry.generated_at < before_timestamp && archived_count < batch_limit {
                if arch_idx.len() >= MAX_ARCHIVE_PER_USER {
                    return Err(ReportingError::ArchiveIndexFull);
                }

                if let Some(report) = Self::load_report(&env, &user, entry.period_key) {
                    let archived_report = ArchivedReport {
                        user: user.clone(),
                        period_key: entry.period_key,
                        health_score: report.health_score.score,
                        generated_at: report.generated_at,
                        archived_at: current_time,
                    };

                    let arch_key = DataKey::Archived(user.clone(), entry.period_key);
                    Self::write_archive(&env, &arch_key, &archived_report);

                    let rep_key = DataKey::Report(user.clone(), entry.period_key);
                    env.storage().persistent().remove(&rep_key);

                    arch_idx.push_back(entry.period_key);
                    archived_count += 1;
                } else {
                    new_report_idx.push_back(entry);
                }
            } else {
                new_report_idx.push_back(entry);
            }
        }

        if archived_count > 0 {
            Self::save_report_index(&env, &user, &new_report_idx);
            Self::save_archive_index(&env, &user, &arch_idx);
            Self::adjust_storage_stats(&env, 0, archived_count, archived_count, 0);

            env.events().publish(
                (symbol_short!("report"), ReportEvent::ReportsArchived),
                (archived_count, caller),
            );
        }

        Ok(archived_count)
    }

    /// Get archived reports for a user — **DEPRECATED**.
    ///
    /// EOL: This entrypoint is deprecated as of v0.2.0 and is planned for removal in a
    /// future contract release after the migration window. Downstream callers should
    /// migrate to [`ReportingContract::get_archived_reports_page`] and walk the archive
    /// by advancing the returned `next_cursor` until it reaches `0`.
    #[deprecated(
        since = "0.2.0",
        note = "Returns at most DEFAULT_PAGE_LIMIT entries; migrate to get_archived_reports_page to walk the full archive."
    )]
    #[allow(deprecated)]
    pub fn get_archived_reports(env: Env, user: Address) -> Vec<ArchivedReport> {
        let ArchivedPage { items, .. } =
            Self::get_archived_reports_page(env, user, 0u32, DEFAULT_PAGE_LIMIT);
        items
    }

    /// Get a paginated list of archived reports for a user.
    pub fn get_archived_reports_page(
        env: Env,
        user: Address,
        cursor: u32,
        limit: u32,
    ) -> ArchivedPage {
        user.require_auth();

        let user_idx = Self::load_archive_index(&env, &user);
        let total_count = user_idx.len();

        let mut items = Vec::new(&env);
        if cursor >= total_count {
            return ArchivedPage {
                items,
                next_cursor: 0u32,
                count: total_count,
            };
        }

        let limit = remitwise_common::clamp_limit(limit);
        let end = cursor.saturating_add(limit).min(total_count);
        for i in cursor..end {
            if let Some(period_key) = user_idx.get(i) {
                if let Some(report) = Self::load_archived_report(&env, &user, period_key) {
                    items.push_back(report);
                }
            }
        }

        ArchivedPage {
            items,
            next_cursor: if end < total_count { end } else { 0u32 },
            count: total_count,
        }
    }

    /// Delegated, paginated archive read with the same cursor semantics as the
    /// owner-only endpoint.
    pub fn get_archived_reports_page_for(
        env: Env,
        viewer: Address,
        owner: Address,
        cursor: u32,
        limit: u32,
    ) -> Result<ArchivedPage, ReportingError> {
        Self::require_viewer(&env, &viewer, &owner, ReportScope::Archived)?;
        let user_idx = Self::load_archive_index(&env, &owner);
        let total_count = user_idx.len();

        let mut items = Vec::new(&env);
        if cursor >= total_count {
            return Ok(ArchivedPage {
                items,
                next_cursor: 0,
                count: total_count,
            });
        }
        let normalized_limit = remitwise_common::clamp_limit(limit);
        let end = cursor.saturating_add(normalized_limit).min(total_count);
        for i in cursor..end {
            if let Some(period_key) = user_idx.get(i) {
                if let Some(report) = Self::load_archived_report(&env, &owner, period_key) {
                    items.push_back(report);
                }
            }
        }
        Ok(ArchivedPage {
            items,
            next_cursor: if end < total_count { end } else { 0 },
            count: total_count,
        })
    }

    /// Permanently delete old archives before specified timestamp for a user (admin only).
    ///
    /// Cost is bounded by `limit`, not by the global data size.
    ///
    /// Maintains set semantics on `ARCH_IDX`: removes all occurrences of the
    /// deleted `period_key` from the user's archive index, healing any legacy
    /// duplicate entries. When a user's index becomes empty, the user key is
    /// removed from `ARCH_IDX`.
    ///
    /// # Arguments
    /// * `caller` - Address of the administrator (must authorize)
    /// * `user` - Target user whose archives are being cleaned up
    /// * `before_timestamp` - Delete archives created before this ledger timestamp
    /// * `limit` - Maximum number of archives to delete in this call (0 defaults to MAX_STORAGE_BATCH)
    ///
    /// # Returns
    /// `Ok(u32)` containing the number of archives deleted
    ///
    /// # Errors
    /// * `NotInitialized` - If contract has not been initialized
    /// * `Unauthorized` - If caller is not the admin
    pub fn cleanup_old_reports(
        env: Env,
        caller: Address,
        user: Address,
        before_timestamp: u64,
        limit: u32,
    ) -> Result<u32, ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let batch_limit = if limit == 0 {
            MAX_STORAGE_BATCH
        } else {
            limit.min(MAX_STORAGE_BATCH)
        };

        let arch_idx = Self::load_archive_index(&env, &user);
        let mut new_arch_idx = Vec::new(&env);
        let mut deleted_count = 0u32;

        for period_key in arch_idx.iter() {
            let mut should_delete = false;
            if deleted_count < batch_limit {
                if let Some(report) = Self::load_archived_report(&env, &user, period_key) {
                    if report.archived_at < before_timestamp {
                        should_delete = true;
                    }
                }
            }

            if should_delete {
                let arch_key = DataKey::Archived(user.clone(), period_key);
                env.storage().persistent().remove(&arch_key);
                deleted_count += 1;
            } else {
                new_arch_idx.push_back(period_key);
            }
        }

        if deleted_count > 0 {
            Self::save_archive_index(&env, &user, &new_arch_idx);
            Self::adjust_storage_stats(&env, 0, 0, 0, deleted_count);

            env.events().publish(
                (symbol_short!("report"), ReportEvent::ArchivesCleaned),
                (deleted_count, caller),
            );
        }

        Ok(deleted_count)
    }

    /// One-shot, admin-only, paginated migration entrypoint.
    /// Moves existing instance data (REPORTS, ARCH_RPT, ARCH_IDX, VIEW_ACL) to persistent storage
    /// and deletes the instance keys when done.
    pub fn migrate_reports_to_persistent(
        env: Env,
        caller: Address,
        limit: u32,
    ) -> Result<u32, ReportingError> {
        caller.require_auth();

        let admin: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("ADMIN"))
            .ok_or(ReportingError::NotInitialized)?;

        if caller != admin {
            return Err(ReportingError::Unauthorized);
        }

        Self::extend_instance_ttl(&env);

        let mut remaining_limit = if limit == 0 {
            MAX_STORAGE_BATCH
        } else {
            limit.min(MAX_STORAGE_BATCH)
        };
        let mut total_migrated = 0u32;

        // 1. Migrate REPORTS
        if let Some(mut reports) = env
            .storage()
            .instance()
            .get::<_, Map<(Address, u64), FinancialHealthReport>>(&LEGACY_REPORTS)
        {
            let mut keys_to_remove = Vec::new(&env);
            for ((user, period_key), report) in reports.iter() {
                if remaining_limit == 0 {
                    break;
                }
                let rep_key = DataKey::Report(user.clone(), period_key);
                Self::write_persistent(&env, &rep_key, &report);

                // Add to user's ReportIndex if not already present
                let mut idx = Self::load_report_index(&env, &user);
                let already_indexed = Self::index_position(&idx, period_key)?.is_some();
                if !already_indexed {
                    idx.push_back(ReportIndexEntry {
                        period_key,
                        generated_at: report.generated_at,
                    });
                    Self::save_report_index(&env, &user, &idx);
                }

                keys_to_remove.push_back((user, period_key));
                remaining_limit -= 1;
                total_migrated += 1;
            }

            for k in keys_to_remove.iter() {
                reports.remove(k);
            }

            if reports.is_empty() {
                env.storage().instance().remove(&LEGACY_REPORTS);
            } else {
                Self::store_legacy_map(&env, &LEGACY_REPORTS, &reports);
            }
        }

        // 2. Migrate ARCH_RPT
        if remaining_limit > 0 {
            if let Some(mut archived) = env
                .storage()
                .instance()
                .get::<_, Map<(Address, u64), ArchivedReport>>(&LEGACY_ARCH_RPT)
            {
                let mut keys_to_remove = Vec::new(&env);
                for ((user, period_key), arch_report) in archived.iter() {
                    if remaining_limit == 0 {
                        break;
                    }
                    let arch_key = DataKey::Archived(user.clone(), period_key);
                    Self::write_archive(&env, &arch_key, &arch_report);

                    keys_to_remove.push_back((user, period_key));
                    remaining_limit -= 1;
                    total_migrated += 1;
                }

                for k in keys_to_remove.iter() {
                    archived.remove(k);
                }

                if archived.is_empty() {
                    env.storage().instance().remove(&LEGACY_ARCH_RPT);
                } else {
                    Self::store_legacy_map(&env, &LEGACY_ARCH_RPT, &archived);
                }
            }
        }

        // 3. Migrate ARCH_IDX
        if remaining_limit > 0 {
            if let Some(mut arch_idx_map) = env
                .storage()
                .instance()
                .get::<_, Map<Address, Vec<u64>>>(&LEGACY_ARCH_IDX)
            {
                let mut users_to_remove = Vec::new(&env);
                for (user, legacy_idx) in arch_idx_map.iter() {
                    if remaining_limit == 0 {
                        break;
                    }
                    let mut persistent_idx = Self::load_archive_index(&env, &user);
                    for pk in legacy_idx.iter() {
                        let mut exists = false;
                        for existing_pk in persistent_idx.iter() {
                            if existing_pk == pk {
                                exists = true;
                                break;
                            }
                        }
                        if !exists {
                            persistent_idx.push_back(pk);
                        }
                    }
                    Self::save_archive_index(&env, &user, &persistent_idx);
                    users_to_remove.push_back(user);
                    remaining_limit -= 1;
                    total_migrated += 1;
                }

                for u in users_to_remove.iter() {
                    arch_idx_map.remove(u);
                }

                if arch_idx_map.is_empty() {
                    env.storage().instance().remove(&LEGACY_ARCH_IDX);
                } else {
                    Self::store_legacy_map(&env, &LEGACY_ARCH_IDX, &arch_idx_map);
                }
            }
        }

        // 4. Migrate VIEW_ACL
        if remaining_limit > 0 {
            if let Some(mut acl_map) = env
                .storage()
                .instance()
                .get::<_, Map<(Address, Address, ReportScope), u64>>(&LEGACY_VIEW_ACL)
            {
                let mut grants_to_remove = Vec::new(&env);
                for ((owner, viewer, scope), expires_at) in acl_map.iter() {
                    if remaining_limit == 0 {
                        break;
                    }
                    let grant_key = DataKey::ViewerGrant(owner.clone(), viewer.clone(), scope);
                    Self::write_persistent(&env, &grant_key, &expires_at);

                    grants_to_remove.push_back((owner, viewer, scope));
                    remaining_limit -= 1;
                    total_migrated += 1;
                }

                for g in grants_to_remove.iter() {
                    acl_map.remove(g);
                }

                if acl_map.is_empty() {
                    env.storage().instance().remove(&LEGACY_VIEW_ACL);
                } else {
                    Self::store_legacy_map(&env, &LEGACY_VIEW_ACL, &acl_map);
                }
            }
        }

        Ok(total_migrated)
    }

    /// Returns aggregate counts of active and archived reports for observability.
    pub fn get_storage_stats(env: Env) -> StorageStats {
        env.storage()
            .instance()
            .get(&symbol_short!("STOR_STAT"))
            .unwrap_or(StorageStats {
                active_reports: 0,
                archived_reports: 0,
                last_updated: 0,
            })
    }

    fn extend_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Extend the TTL of archive storage with longer duration
    fn extend_archive_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // Persistent Storage Helper Methods
    // -----------------------------------------------------------------------

    fn read_persistent<T: soroban_sdk::TryFromVal<Env, Val>>(env: &Env, key: &DataKey) -> Option<T> {
        if env.storage().persistent().has(key) {
            env.storage()
                .persistent()
                .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
            env.storage().persistent().get(key)
        } else {
            None
        }
    }

    fn write_persistent<T: soroban_sdk::IntoVal<Env, Val>>(env: &Env, key: &DataKey, val: &T) {
        env.storage().persistent().set(key, val);
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }

    fn write_archive<T: soroban_sdk::IntoVal<Env, Val>>(env: &Env, key: &DataKey, val: &T) {
        env.storage().persistent().set(key, val);
        env.storage()
            .persistent()
            .extend_ttl(key, ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
    }

    fn load_report(env: &Env, user: &Address, period_key: u64) -> Option<FinancialHealthReport> {
        let key = DataKey::Report(user.clone(), period_key);
        if let Some(report) = Self::read_persistent::<FinancialHealthReport>(env, &key) {
            return Some(report);
        }
        // Fallback to legacy instance REPORTS map
        env.storage()
            .instance()
            .get::<_, Map<(Address, u64), FinancialHealthReport>>(&LEGACY_REPORTS)
            .and_then(|reports| reports.get((user.clone(), period_key)))
    }

    fn legacy_report_exists(env: &Env, user: &Address, period_key: u64) -> bool {
        env.storage()
            .instance()
            .get::<_, Map<(Address, u64), FinancialHealthReport>>(&LEGACY_REPORTS)
            .map(|reports| reports.contains_key((user.clone(), period_key)))
            .unwrap_or(false)
    }

    fn load_archived_report(env: &Env, user: &Address, period_key: u64) -> Option<ArchivedReport> {
        let key = DataKey::Archived(user.clone(), period_key);
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, ARCHIVE_LIFETIME_THRESHOLD, ARCHIVE_BUMP_AMOUNT);
            return env.storage().persistent().get(&key);
        }
        // Fallback to legacy instance ARCH_RPT map
        env.storage()
            .instance()
            .get::<_, Map<(Address, u64), ArchivedReport>>(&LEGACY_ARCH_RPT)
            .and_then(|archived| archived.get((user.clone(), period_key)))
    }

    fn load_report_index(env: &Env, user: &Address) -> Vec<ReportIndexEntry> {
        let key = DataKey::ReportIndex(user.clone());
        if let Some(idx) = Self::read_persistent::<Vec<ReportIndexEntry>>(env, &key) {
            return idx;
        }
        // Fallback: build from legacy REPORTS if present for this user
        let mut idx = Vec::new(env);
        if let Some(reports) = env
            .storage()
            .instance()
            .get::<_, Map<(Address, u64), FinancialHealthReport>>(&LEGACY_REPORTS)
        {
            for ((u, period_key), report) in reports.iter() {
                if u == *user {
                    idx.push_back(ReportIndexEntry {
                        period_key,
                        generated_at: report.generated_at,
                    });
                }
            }
        }
        idx
    }

    fn save_report_index(env: &Env, user: &Address, index: &Vec<ReportIndexEntry>) {
        let key = DataKey::ReportIndex(user.clone());
        Self::write_persistent(env, &key, index);
    }

    fn load_archive_index(env: &Env, user: &Address) -> Vec<u64> {
        let key = DataKey::ArchiveIndex(user.clone());
        if let Some(idx) = Self::read_persistent::<Vec<u64>>(env, &key) {
            return idx;
        }
        // Fallback: read from legacy ARCH_IDX
        if let Some(arch_idx) = env
            .storage()
            .instance()
            .get::<_, Map<Address, Vec<u64>>>(&LEGACY_ARCH_IDX)
        {
            if let Some(user_idx) = arch_idx.get(user.clone()) {
                return user_idx;
            }
        }
        Vec::new(env)
    }

    fn save_archive_index(env: &Env, user: &Address, index: &Vec<u64>) {
        let key = DataKey::ArchiveIndex(user.clone());
        Self::write_persistent(env, &key, index);
    }

    fn index_position(
        index: &Vec<ReportIndexEntry>,
        period_key: u64,
    ) -> Result<Option<u32>, ReportingError> {
        for i in 0..index.len() {
            if let Some(entry) = index.get(i) {
                if entry.period_key == period_key {
                    return Ok(Some(i));
                }
            }
        }
        Ok(None)
    }

    fn adjust_storage_stats(
        env: &Env,
        active_add: u32,
        active_sub: u32,
        archived_add: u32,
        archived_sub: u32,
    ) {
        let mut stats = Self::get_storage_stats(env.clone());
        stats.active_reports = stats
            .active_reports
            .saturating_add(active_add)
            .saturating_sub(active_sub);
        stats.archived_reports = stats
            .archived_reports
            .saturating_add(archived_add)
            .saturating_sub(archived_sub);
        stats.last_updated = env.ledger().timestamp();
        env.storage()
            .instance()
            .set(&symbol_short!("STOR_STAT"), &stats);
    }

    fn store_legacy_map<K: IntoVal<Env, Val>, V: IntoVal<Env, Val>>(
        env: &Env,
        key: &Symbol,
        map: &Map<K, V>,
    ) {
        env.storage().instance().set(key, map);
    }
}

#[cfg(test)]
mod events_schema_test;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_auth_acl;

#[cfg(test)]
mod paginate_dependency_tests;

#[cfg(test)]
mod tests_data_availability;

#[cfg(test)]
mod tests_archived_pagination_bound;

#[cfg(test)]
mod tests_safe_math;

#[cfg(test)]
mod tests_viewer_acl;

#[cfg(test)]
mod tests_archive_dedup;
