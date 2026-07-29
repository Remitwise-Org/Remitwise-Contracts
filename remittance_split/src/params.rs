//! Corridor-specific limits and constants.
//!
//! All corridor validation thresholds live here rather than being
//! hard-coded at call sites, keeping the contract auditable and
//! safe to change.

/// Maximum number of corridors allowed per contract instance.
pub const MAX_CORRIDORS: u32 = 100;

/// Maximum fee in basis points (1 000 bps = 10 %).
pub const MAX_FEE_BPS: u32 = 1_000;

/// Minimum allowed corridor amount (1 unit of the base asset).
pub const MIN_CORRIDOR_AMOUNT: i128 = 1;
