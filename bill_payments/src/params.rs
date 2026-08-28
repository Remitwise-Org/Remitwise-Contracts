//! Parameter constants and operational limits for bill payments.

pub const MAX_FREQUENCY_DAYS: u32 = 36_500; // 100 years
pub const SECONDS_PER_DAY: u64 = 86_400;
pub const MAX_BILLS_PER_OWNER: u32 = 1_000;
/// Maximum length for bill names in bytes (defence-in-depth: prevents
/// unbounded storage bloat via excessively long names).
pub const MAX_NAME_LEN: u32 = 64;

/// Rate limits for bill payments operations
pub const CREATE_BILL_RATE_LIMIT: u32 = 100; // per address per 24h
pub const PAY_BILL_RATE_LIMIT: u32 = 200; // per address per 24h
pub const CANCEL_BILL_RATE_LIMIT: u32 = 50; // per address per 24h
pub const MIN_EXTERNAL_REF_LEN: u32 = 1;
pub const MAX_EXTERNAL_REF_LEN: u32 = 64;
pub const MIN_SCHEDULE_INTERVAL: u64 = 3_600;
pub const MAX_SCHEDULE_LEAD_TIME: u64 = 365 * 24 * 3_600;
pub const MAX_BILL_SCHEDULES_PER_OWNER: u32 = 50;
/// Admin grant time-to-live in seconds (30 days). After this period the pause admin
/// must call set_pause_admin or refresh_admin_grant to extend the grant.
pub const ADMIN_GRANT_TTL: u64 = 30 * 24 * 60 * 60;

/// Window for proposed admin rotation before it can be finalized (2 days).
pub const ADMIN_ROTATION_TIMELOCK_SECONDS: u64 = 2 * 86400;
