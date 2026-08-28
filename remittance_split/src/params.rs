//! Contract parameters and limits.
//!
//! All validation thresholds, schedule limits, version markers, and domain
//! separators live here rather than being hard-coded at call sites, keeping
//! the contract auditable and safe to change.

use soroban_sdk::symbol_short;

// ─── Corridor limits ─────────────────────────────────────────────

/// Maximum number of corridors allowed per contract instance.
pub const MAX_CORRIDORS: u32 = 100;

/// Maximum fee in basis points (1 000 bps = 10 %).
pub const MAX_FEE_BPS: u32 = 1_000;

/// Minimum allowed corridor amount (1 unit of the base asset).
pub const MIN_CORRIDOR_AMOUNT: i128 = 1;

// ─── Nonce limits ────────────────────────────────────────────────

/// Maximum number of used nonces tracked per address before the oldest are pruned.
pub const MAX_USED_NONCES_PER_ADDR: u32 = 256;

// ─── Schedule limits ─────────────────────────────────────────────

/// Maximum number of remittance schedules allowed per owner to prevent storage bloat.
pub const MAX_SCHEDULES_PER_OWNER: u32 = 50;

/// Minimum allowed recurrence interval for repeating schedules (1 hour in seconds).
/// One-off schedules (interval == 0) are exempt from this check.
pub const MIN_SCHEDULE_INTERVAL: u64 = 3_600;

/// Maximum allowed lead time for schedule due dates (1 year in seconds).
/// Prevents unrealistic far-future scheduling that creates operational risk.
pub const MAX_SCHEDULE_LEAD_TIME: u64 = 365 * 24 * 3_600;

// ─── Deadline limits ─────────────────────────────────────────────

/// Maximum allowed window for transaction deadlines (1 hour).
pub const MAX_DEADLINE_WINDOW_SECS: u64 = 3_600;

// ─── Audit limits ────────────────────────────────────────────────

/// Maximum number of audit log entries kept in the rotating ring buffer.
pub const MAX_AUDIT_ENTRIES: u32 = 100;

// ─── Schema / version markers ────────────────────────────────────

/// Current snapshot schema version. Bumped to 2 for FNV-1a checksum + exported_at field.
pub const SCHEMA_VERSION: u32 = 2;

/// Oldest snapshot schema version this contract can import. Enables backward compat.
pub const MIN_SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// Contract version marker used for migrations and upgrade coordination.
pub const CONTRACT_VERSION: u32 = 1;

// ─── Domain separators ───────────────────────────────────────────

/// Request hash domain separator for signing (prevents cross-domain attacks).
pub const DISTRIBUTE_USDC_DOMAIN: &[u8] = b"distribute_usdc_v1";

/// Event topic emitted when the split is initialized or updated.
pub const SPLIT_INITIALIZED: soroban_sdk::Symbol = symbol_short!("init");

/// Event topic emitted when a split calculation is performed.
pub const SPLIT_CALCULATED: soroban_sdk::Symbol = symbol_short!("calc");

// ─── Storage constants (re-exported from remitwise-common) ───────

/// Instance storage bump amount (30 days).
pub use remitwise_common::INSTANCE_BUMP_AMOUNT;
/// Instance storage lifetime threshold (7 days).
pub use remitwise_common::INSTANCE_LIFETIME_THRESHOLD;
/// Maximum batch size for batch operations.
pub use remitwise_common::MAX_BATCH_SIZE;
/// Persistent storage bump amount (60 days).
pub use remitwise_common::PERSISTENT_BUMP_AMOUNT;
/// Persistent storage lifetime threshold (15 days).
pub use remitwise_common::PERSISTENT_LIFETIME_THRESHOLD;
/// Storage key for pre-upgrade snapshots.
pub use remitwise_common::SNAPSHOT_KEY;
/// Current snapshot schema version.
pub use remitwise_common::SNAPSHOT_VERSION;
