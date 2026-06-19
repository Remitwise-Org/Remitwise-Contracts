#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use soroban_sdk::{contracttype, symbol_short, Symbol};

/// Financial categories for remittance allocation
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Category {
    Spending = 1,
    Savings = 2,
    Bills = 3,
    Insurance = 4,
}

/// Family roles for access control
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FamilyRole {
    Owner = 1,
    Admin = 2,
    Member = 3,
    Viewer = 4,
}

/// Insurance coverage types
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CoverageType {
    Health = 1,
    Life = 2,
    Property = 3,
    Auto = 4,
    Liability = 5,
}

/// Event categories used for logging across all contracts.
///
/// Determines the high-level classification of an event. The taxonomy is documented in
/// `docs/EVENT_TAXONOMY.md`.
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum EventCategory {
    Transaction = 0,
    State = 1,
    Alert = 2,
    System = 3,
    Access = 4,
}

/// Priority levels for events emitted by contracts.
/// Determines the importance of the event. Lower numbers represent lower priority.
/// See `docs/EVENT_TAXONOMY.md` for full taxonomy details.
#[allow(dead_code)]
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum EventPriority {
    Low = 0,
    Medium = 1,
    High = 2,
}

impl EventCategory {
    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

impl EventPriority {
    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

/// Pagination limits
pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 50;

/// Standardized TTL Constants (Ledger Counts)
pub const DAY_IN_LEDGERS: u32 = 17280; // ~5 seconds per ledger

/// Storage TTL constants for active data
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS; // 7 days
pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS; // 30 days

/// Storage TTL constants for persistent data
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS; // 15 days
pub const PERSISTENT_BUMP_AMOUNT: u32 = 60 * DAY_IN_LEDGERS; // 60 days

/// Storage TTL constants for archived data
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS; // 7 days
pub const ARCHIVE_BUMP_AMOUNT: u32 = 180 * DAY_IN_LEDGERS; // 180 days (6 months)

/// Signature expiration time (24 hours in seconds)
pub const SIGNATURE_EXPIRATION: u64 = 86400;

/// Contract version
pub const CONTRACT_VERSION: u32 = 1;

/// Maximum batch size for operations
pub const MAX_BATCH_SIZE: u32 = 50;

/// Clamps a pagination limit to ensure it falls within the allowed boundaries.
///
/// # Behavior
/// - `0` is treated as a request for the default limit and returns `DEFAULT_PAGE_LIMIT`.
/// - Values between `1` and `MAX_PAGE_LIMIT` (inclusive) are passed through unchanged.
/// - Values greater than `MAX_PAGE_LIMIT` are capped at `MAX_PAGE_LIMIT`.
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_PAGE_LIMIT
    } else if limit > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        limit
    }
}

// ---------------------------------------------------------------------------
// Tag canonicalization
// ---------------------------------------------------------------------------

/// Maximum allowed byte length for a single tag.
pub const TAG_MAX_LEN: u32 = 32;

/// Validates and canonicalizes a batch of tags.
///
/// # Rules
/// - The batch must contain at least one tag (`panic!("Tags cannot be empty")`).
/// - Each tag must be between 1 and `TAG_MAX_LEN` bytes inclusive
///   (`panic!("Tag must be between 1 and 32 characters")`).
/// - Allowed charset: `[a-z0-9\-_]`.  ASCII uppercase letters are silently
///   folded to lowercase; any other byte causes the supplied `on_invalid_char`
///   closure to be called (typically `panic_with_error!` or `panic!`).
///
/// # Returns
/// A new `Vec<String>` containing the normalized (lowercased) tags in the
/// same order as the input.
///
/// # Usage
/// ```ignore
/// use remitwise_common::canonicalize_tags;
/// let normalized = canonicalize_tags(&env, &tags, || {
///     soroban_sdk::panic_with_error!(&env, MyError::InvalidTagContent)
/// });
/// ```
pub fn canonicalize_tags<F>(
    env: &soroban_sdk::Env,
    tags: &soroban_sdk::Vec<soroban_sdk::String>,
    on_invalid_char: F,
) -> soroban_sdk::Vec<soroban_sdk::String>
where
    F: Fn(),
{
    if tags.is_empty() {
        panic!("Tags cannot be empty");
    }
    let mut out = soroban_sdk::Vec::new(env);
    for tag in tags.iter() {
        let len = tag.len();
        if len == 0 || len > TAG_MAX_LEN {
            panic!("Tag must be between 1 and 32 characters");
        }
        let mut buf = [0u8; 32];
        tag.copy_into_slice(&mut buf[..len as usize]);
        for byte in buf.iter_mut().take(len as usize) {
            if byte.is_ascii_uppercase() {
                *byte += b'a' - b'A';
            }
            let b = *byte;
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
                on_invalid_char();
            }
        }
        let s = match core::str::from_utf8(&buf[..len as usize]) {
            Ok(v) => v,
            Err(_) => {
                on_invalid_char();
                // on_invalid_char must diverge (panic); this is unreachable.
                ""
            }
        };
        out.push_back(soroban_sdk::String::from_str(env, s));
    }
    out
}

/// Event emission helper
pub struct RemitwiseEvents;

#[cfg(test)]
mod tests;

impl RemitwiseEvents {
    /// Emits a single event with the given category, priority, and action.
///
/// * `category` – The `EventCategory` describing the type of event.
/// * `priority` – The `EventPriority` indicating the importance level.
/// * `action` – A short `Symbol` identifying the specific action.
/// * `data` – The event payload implementing `IntoVal`.
///
/// The emitted event follows the topic schema defined in `docs/EVENT_TAXONOMY.md`.
pub fn emit<T>(
    env: &soroban_sdk::Env,
    category: EventCategory,
    priority: EventPriority,
    action: Symbol,
    data: T,
) where
    T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
        let topics = (
            symbol_short!("Remitwise"),
            category.to_u32(),
            priority.to_u32(),
            action,
        );
        env.events().publish(topics, data);
    }

    /// Emits a batch event for the given category and action with a count.
///
/// * `category` – The `EventCategory` of the batched events.
/// * `action` – Symbol representing the batch action.
/// * `count` – Number of events in the batch.
///
/// This always uses `EventPriority::Low` for batch events.
pub fn emit_batch(env: &soroban_sdk::Env, category: EventCategory, action: Symbol, count: u32) {
        let topics = (
            symbol_short!("Remitwise"),
            category.to_u32(),
            EventPriority::Low.to_u32(),
            symbol_short!("batch"),
        );
        let data = (action, count);
        env.events().publish(topics, data);
    }
}
