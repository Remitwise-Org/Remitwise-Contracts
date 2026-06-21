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

/// Normalizes caller-supplied pagination limits for all shared paginated reads.
///
/// # Contract
/// - `0` is treated as a request for the default limit and returns `DEFAULT_PAGE_LIMIT`.
/// - Values between `1` and `MAX_PAGE_LIMIT` (inclusive) are passed through unchanged.
/// - Values greater than `MAX_PAGE_LIMIT` are capped at `MAX_PAGE_LIMIT`.
/// - The returned value is always in `1..=MAX_PAGE_LIMIT`.
/// - The function is idempotent: applying it to an already-normalized value returns
///   the same value.
/// - Extremely large inputs, including `u32::MAX`, clamp without arithmetic and
///   cannot overflow.
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

// ---------------------------------------------------------------------------
// Encoding stability tests (cross-contract ABI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod encoding_stability_tests {
    use super::{Category, CoverageType, FamilyRole};
    use soroban_sdk::{Env, Map, Vec};

    fn round_trip<T>(env: &Env, v: T) -> T
    where
        T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
    {
        let val = v.into_val(env);
        T::try_from_val(env, &val).unwrap()
    }

    fn assert_encoding_matches_discriminant<T>(env: &Env, v: T, expected: u32)
    where
        T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>
            + soroban_sdk::TryFromVal<Env, soroban_sdk::Val>
            + core::fmt::Debug
            + PartialEq,
    {
        let val = v.into_val(env);

        // `#[repr(u32)]` + `#[contracttype]` should encode via a stable u32 discriminant.
        // We pin the expected discriminant by decoding the value as `u32`.
        let actual_u32: u32 = soroban_sdk::TryFromVal::try_from_val(env, &val)
            .unwrap_or_else(|_| panic!("unexpected Val for encoding: {val:?}"));
        assert_eq!(actual_u32, expected, "encoding mismatch");

        // And ensure round-trip identity.
        let decoded = T::try_from_val(env, &val).unwrap();
        assert_eq!(decoded, v, "round-trip mismatch");
    }

    #[test]
    fn category_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, Category::Spending, 1);
        assert_encoding_matches_discriminant(&env, Category::Savings, 2);
        assert_encoding_matches_discriminant(&env, Category::Bills, 3);
        assert_encoding_matches_discriminant(&env, Category::Insurance, 4);

        // Exhaustiveness enforcement: every variant must be explicitly handled.
        fn cover_all_variants(v: Category) {
            match v {
                Category::Spending => {}
                Category::Savings => {}
                Category::Bills => {}
                Category::Insurance => {}
            }
        }

        for v in [
            Category::Spending,
            Category::Savings,
            Category::Bills,
            Category::Insurance,
        ] {
            cover_all_variants(v);
        }

        // Container round-trips
        let vec = Vec::from_array(&env, [Category::Spending, Category::Savings, Category::Bills]);
        let mut out = Vec::<Category>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, Category>::new(&env);
        map.set(1u32, Category::Spending);
        map.set(2u32, Category::Savings);
        map.set(3u32, Category::Bills);

        let mut out_map = Map::<u32, Category>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }

    #[test]
    fn family_role_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, FamilyRole::Owner, 1);
        assert_encoding_matches_discriminant(&env, FamilyRole::Admin, 2);
        assert_encoding_matches_discriminant(&env, FamilyRole::Member, 3);
        assert_encoding_matches_discriminant(&env, FamilyRole::Viewer, 4);

        fn cover_all_variants(v: FamilyRole) {
            match v {
                FamilyRole::Owner => {}
                FamilyRole::Admin => {}
                FamilyRole::Member => {}
                FamilyRole::Viewer => {}
            }
        }

        for v in [
            FamilyRole::Owner,
            FamilyRole::Admin,
            FamilyRole::Member,
            FamilyRole::Viewer,
        ] {
            cover_all_variants(v);
        }

        let vec = Vec::from_array(&env, [FamilyRole::Owner, FamilyRole::Admin, FamilyRole::Viewer]);
        let mut out = Vec::<FamilyRole>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, FamilyRole>::new(&env);
        map.set(1u32, FamilyRole::Owner);
        map.set(2u32, FamilyRole::Admin);
        map.set(3u32, FamilyRole::Viewer);

        let mut out_map = Map::<u32, FamilyRole>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }

    #[test]
    fn coverage_type_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, CoverageType::Health, 1);
        assert_encoding_matches_discriminant(&env, CoverageType::Life, 2);
        assert_encoding_matches_discriminant(&env, CoverageType::Property, 3);
        assert_encoding_matches_discriminant(&env, CoverageType::Auto, 4);
        assert_encoding_matches_discriminant(&env, CoverageType::Liability, 5);

        fn cover_all_variants(v: CoverageType) {
            match v {
                CoverageType::Health => {}
                CoverageType::Life => {}
                CoverageType::Property => {}
                CoverageType::Auto => {}
                CoverageType::Liability => {}
            }
        }

        for v in [
            CoverageType::Health,
            CoverageType::Life,
            CoverageType::Property,
            CoverageType::Auto,
            CoverageType::Liability,
        ] {
            cover_all_variants(v);
        }

        let vec = Vec::from_array(
            &env,
            [
                CoverageType::Health,
                CoverageType::Life,
                CoverageType::Property,
                CoverageType::Auto,
            ],
        );
        let mut out = Vec::<CoverageType>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, CoverageType>::new(&env);
        map.set(1u32, CoverageType::Health);
        map.set(2u32, CoverageType::Life);
        map.set(3u32, CoverageType::Liability);

        let mut out_map = Map::<u32, CoverageType>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }
}

