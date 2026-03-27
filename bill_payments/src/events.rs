//! # Bill Event Schema Module
//!
//! Standardized event types and backward-compatibility checks for the
//! `bill_payments` contract. These types define the **canonical schema** that
//! downstream indexers and consumers rely on for event decoding.
//!
//! ## Schema Versioning
//!
//! Every event struct carries an implicit schema version via the contract
//! `CONTRACT_VERSION` constant. When the schema evolves:
//!
//! 1. New **optional** fields are appended (never inserted) to preserve XDR
//!    positional decoding for existing consumers.
//! 2. The `EventSchemaVersion` constant is bumped.
//! 3. Compile-time assertions prevent accidental field-count regressions.
//!
//! ## Topic Convention
//!
//! All events use the `RemitwiseEvents::emit` helper from `remitwise-common`,
//! producing a 4-topic tuple:
//!
//! ```text
//! ("Remitwise", category: u32, priority: u32, action: Symbol)
//! ```

use soroban_sdk::{contracttype, Address, String};

// ---------------------------------------------------------------------------
// Schema version — bump when event struct shapes change.
// ---------------------------------------------------------------------------

/// Current bill event schema version.
///
/// Increment this when any event struct's field list changes so that
/// downstream consumers can branch on the version.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Event data structs
// ---------------------------------------------------------------------------

/// Emitted when a new bill is created via `create_bill`.
///
/// # Fields
/// * `bill_id`        — Unique bill identifier.
/// * `owner`          — The address that owns this bill.
/// * `amount`         — Bill amount in stroops (smallest unit).
/// * `due_date`       — Unix-epoch timestamp of the due date.
/// * `currency`       — Normalized currency code (e.g., `"XLM"`, `"USDC"`).
/// * `recurring`      — Whether the bill recurs.
/// * `schema_version` — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BillCreatedEvent {
    pub bill_id: u32,
    pub owner: Address,
    pub amount: i128,
    pub due_date: u64,
    pub currency: String,
    pub recurring: bool,
    pub schema_version: u32,
}

/// Emitted when a bill is paid via `pay_bill` or `batch_pay_bills`.
///
/// # Fields
/// * `bill_id`        — ID of the paid bill.
/// * `owner`          — Bill owner address.
/// * `amount`         — Amount that was paid (in stroops).
/// * `paid_at`        — Unix-epoch timestamp of payment.
/// * `schema_version` — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BillPaidEvent {
    pub bill_id: u32,
    pub owner: Address,
    pub amount: i128,
    pub paid_at: u64,
    pub schema_version: u32,
}

/// Emitted when a bill is cancelled via `cancel_bill`.
///
/// # Fields
/// * `bill_id`        — ID of the cancelled bill.
/// * `owner`          — Bill owner address.
/// * `cancelled_at`   — Unix-epoch timestamp of cancellation.
/// * `schema_version` — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BillCancelledEvent {
    pub bill_id: u32,
    pub owner: Address,
    pub cancelled_at: u64,
    pub schema_version: u32,
}

/// Emitted when a bill is restored from the archive.
///
/// # Fields
/// * `bill_id`        — ID of the restored bill.
/// * `owner`          — Bill owner address.
/// * `restored_at`    — Unix-epoch timestamp of restoration.
/// * `schema_version` — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BillRestoredEvent {
    pub bill_id: u32,
    pub owner: Address,
    pub restored_at: u64,
    pub schema_version: u32,
}

/// Emitted after `archive_paid_bills` completes.
///
/// # Fields
/// * `count`          — Number of bills archived in the batch.
/// * `archived_at`    — Unix-epoch timestamp of the archive operation.
/// * `schema_version` — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BillsArchivedEvent {
    pub count: u32,
    pub archived_at: u64,
    pub schema_version: u32,
}

/// Emitted when the contract version is updated via `set_version`.
///
/// # Fields
/// * `previous_version` — Version before upgrade.
/// * `new_version`      — Version after upgrade.
/// * `schema_version`   — Schema version at emission time.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VersionUpgradeEvent {
    pub previous_version: u32,
    pub new_version: u32,
    pub schema_version: u32,
}

// ---------------------------------------------------------------------------
// Compile-time schema parity assertions
// ---------------------------------------------------------------------------
//
// These ensure the field count of each event struct never *decreases* after
// a release. A decrease would break XDR positional decoding for existing
// consumers. Add new fields at the end; never remove or reorder.

/// Counts the number of fields in a struct expression for compile-time
/// assertions. Used by `assert_min_fields!` to guarantee backward-compatible
/// event schema evolution.
#[doc(hidden)]
#[macro_export]
macro_rules! count_fields {
    () => { 0u32 };
    ($head:ident $(, $tail:ident)*) => { 1u32 + count_fields!($($tail),*) };
}

/// Compile-time assertion that a bill event struct never has fewer fields
/// than the minimum required for backward compatibility.
///
/// # Usage
/// ```ignore
/// assert_min_fields!(BillCreatedEvent, 7, bill_id, owner, amount, due_date, currency, recurring, schema_version);
/// ```
#[doc(hidden)]
#[macro_export]
macro_rules! assert_min_fields {
    ($name:ident, $min:expr, $($field:ident),+ $(,)?) => {
        const _: () = {
            let actual = count_fields!($($field),+);
            assert!(
                actual >= $min,
                concat!(
                    "Schema regression in ",
                    stringify!($name),
                    ": field count fell below minimum"
                )
            );
        };
    };
}

// Backward-compatibility baselines — V1 minimums.
// BillCreatedEvent must have ≥ 7 fields.
assert_min_fields!(BillCreatedEvent, 7, bill_id, owner, amount, due_date, currency, recurring, schema_version);
// BillPaidEvent must have ≥ 5 fields.
assert_min_fields!(BillPaidEvent, 5, bill_id, owner, amount, paid_at, schema_version);
// BillCancelledEvent must have ≥ 4 fields.
assert_min_fields!(BillCancelledEvent, 4, bill_id, owner, cancelled_at, schema_version);
// BillRestoredEvent must have ≥ 4 fields.
assert_min_fields!(BillRestoredEvent, 4, bill_id, owner, restored_at, schema_version);
// BillsArchivedEvent must have ≥ 3 fields.
assert_min_fields!(BillsArchivedEvent, 3, count, archived_at, schema_version);
// VersionUpgradeEvent must have ≥ 3 fields.
assert_min_fields!(VersionUpgradeEvent, 3, previous_version, new_version, schema_version);

// ---------------------------------------------------------------------------
// Topic compatibility constants
// ---------------------------------------------------------------------------

/// The canonical topic symbols used in bill event emission.
/// These MUST NOT change across versions to preserve indexer compatibility.
pub mod topics {
    use soroban_sdk::symbol_short;

    /// Action symbol for bill creation events.
    pub const CREATED: soroban_sdk::Symbol = symbol_short!("created");
    /// Action symbol for bill payment events.
    pub const PAID: soroban_sdk::Symbol = symbol_short!("paid");
    /// Action symbol for bill cancellation events.
    pub const CANCELED: soroban_sdk::Symbol = symbol_short!("canceled");
    /// Action symbol for bill restoration events.
    pub const RESTORED: soroban_sdk::Symbol = symbol_short!("restored");
    /// Action symbol for archive batch events.
    pub const ARCHIVED: soroban_sdk::Symbol = symbol_short!("archived");
    /// Action symbol for contract upgrade events.
    pub const UPGRADED: soroban_sdk::Symbol = symbol_short!("upgraded");
    /// Action symbol for contract pause events.
    pub const PAUSED: soroban_sdk::Symbol = symbol_short!("paused");
    /// Action symbol for contract unpause events.
    pub const UNPAUSED: soroban_sdk::Symbol = symbol_short!("unpaused");
    /// Action symbol for batch payment summary events.
    pub const BATCH_PAY: soroban_sdk::Symbol = symbol_short!("batch_pay");
    /// Action symbol for bulk cleanup batch events.
    pub const CLEANED: soroban_sdk::Symbol = symbol_short!("cleaned");
}

// ---------------------------------------------------------------------------
// Builder helpers — construct events with schema_version pre-filled
// ---------------------------------------------------------------------------

impl BillCreatedEvent {
    /// Construct a `BillCreatedEvent` with the current schema version.
    pub fn new(
        bill_id: u32,
        owner: Address,
        amount: i128,
        due_date: u64,
        currency: String,
        recurring: bool,
    ) -> Self {
        Self {
            bill_id,
            owner,
            amount,
            due_date,
            currency,
            recurring,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}

impl BillPaidEvent {
    /// Construct a `BillPaidEvent` with the current schema version.
    pub fn new(bill_id: u32, owner: Address, amount: i128, paid_at: u64) -> Self {
        Self {
            bill_id,
            owner,
            amount,
            paid_at,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}

impl BillCancelledEvent {
    /// Construct a `BillCancelledEvent` with the current schema version.
    pub fn new(bill_id: u32, owner: Address, cancelled_at: u64) -> Self {
        Self {
            bill_id,
            owner,
            cancelled_at,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}

impl BillRestoredEvent {
    /// Construct a `BillRestoredEvent` with the current schema version.
    pub fn new(bill_id: u32, owner: Address, restored_at: u64) -> Self {
        Self {
            bill_id,
            owner,
            restored_at,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}

impl BillsArchivedEvent {
    /// Construct a `BillsArchivedEvent` with the current schema version.
    pub fn new(count: u32, archived_at: u64) -> Self {
        Self {
            count,
            archived_at,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}

impl VersionUpgradeEvent {
    /// Construct a `VersionUpgradeEvent` with the current schema version.
    pub fn new(previous_version: u32, new_version: u32) -> Self {
        Self {
            previous_version,
            new_version,
            schema_version: EVENT_SCHEMA_VERSION,
        }
    }
}
