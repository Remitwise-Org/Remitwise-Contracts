#![no_std]

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use soroban_sdk::{contracterror, contracttype, symbol_short, Address, Bytes, Env, Map, Symbol};

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

/// Policy mode for access control
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PolicyMode {
    Strict = 1,
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

/// Pre-upgrade snapshot version
pub const SNAPSHOT_VERSION: u32 = 1;

/// Storage key for pre-upgrade snapshots
pub const SNAPSHOT_KEY: Symbol = symbol_short!("SNAPSHOT");

/// Rate limiting constants
pub const RATE_LIMIT_WINDOW_SECONDS: u64 = 86400; // 24 hours
const STORAGE_RATE_LIMIT: Symbol = symbol_short!("RATE_LIM");

/// Rate limit record: stores count per address + operation + window
#[contracttype]
#[derive(Clone)]
pub struct RateLimitRecord {
    pub count: u32,
    pub window_id: u64,
}

/// Rate limit error
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitError {
    RateLimitExceeded,
}

/// Helper to check and increment rate limit
/// 
/// # Arguments
/// * `env` - Soroban environment
/// * `caller` - Address of the caller
/// * `operation` - Symbol identifying the operation to rate limit
/// * `limit` - Maximum allowed operations per window
/// 
/// # Returns
/// * `Ok(())` if within limit
/// * `Err(RateLimitError::RateLimitExceeded)` if limit exceeded
pub fn check_and_increment_rate_limit(
    env: &Env,
    caller: &Address,
    operation: Symbol,
    limit: u32,
) -> Result<(), RateLimitError> {
    let now = env.ledger().timestamp();
    let window_id = (now / RATE_LIMIT_WINDOW_SECONDS) * RATE_LIMIT_WINDOW_SECONDS;
    
    let key = (caller.clone(), operation, window_id);
    
    let mut rate_limits: Map<(Address, Symbol, u64), RateLimitRecord> = env
        .storage()
        .instance()
        .get(&STORAGE_RATE_LIMIT)
        .unwrap_or_else(|| Map::new(env));
    
    let record = rate_limits.get(key.clone()).unwrap_or(RateLimitRecord {
        count: 0,
        window_id,
    });
    
    if record.count >= limit {
        return Err(RateLimitError::RateLimitExceeded);
    }
    
    let new_record = RateLimitRecord {
        count: record.count + 1,
        window_id,
    };
    
    rate_limits.set(key, new_record);
    env.storage().instance().set(&STORAGE_RATE_LIMIT, &rate_limits);
    
    Ok(())
}

/// Helper to get current rate limit status for an operation
pub fn get_rate_limit_status(
    env: &Env,
    caller: &Address,
    operation: Symbol,
) -> (u32, u64) {
    let now = env.ledger().timestamp();
    let window_id = (now / RATE_LIMIT_WINDOW_SECONDS) * RATE_LIMIT_WINDOW_SECONDS;
    
    let key = (caller.clone(), operation, window_id);
    
    let rate_limits: Map<(Address, Symbol, u64), RateLimitRecord> = env
        .storage()
        .instance()
        .get(&STORAGE_RATE_LIMIT)
        .unwrap_or_else(|| Map::new(env));
    
    let record = rate_limits.get(key).unwrap_or(RateLimitRecord {
        count: 0,
        window_id,
    });
    
    (record.count, window_id + RATE_LIMIT_WINDOW_SECONDS)
}

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

/// Error converting an integer to `i128` when the value is out of range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntConversionError {
    Overflow,
}

/// Fallible conversion to `i128` for safe cross-type arithmetic.
pub trait ToI128Checked {
    fn to_i128_checked(self) -> Result<i128, IntConversionError>;
}

impl ToI128Checked for u32 {
    fn to_i128_checked(self) -> Result<i128, IntConversionError> {
        Ok(self as i128)
    }
}

impl ToI128Checked for i32 {
    fn to_i128_checked(self) -> Result<i128, IntConversionError> {
        Ok(self as i128)
    }
}

/// Error related to time and periods.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TimeError {
    InvalidPeriod = 7,
}

/// Validates that a requested period is logically ordered.
///
/// # Errors
/// Returns `TimeError::InvalidPeriod` if `start > end`.
pub fn validate_period(start: u64, end: u64) -> Result<(), TimeError> {
    if start > end {
        Err(TimeError::InvalidPeriod)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tag canonicalization
// ---------------------------------------------------------------------------

/// Maximum allowed byte length for a single tag.
pub const TAG_MAX_LEN: u32 = 32;

/// Validation failure returned by [`canonicalize_tags_checked`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagError {
    /// The tag batch is empty, or an individual tag is zero bytes long.
    Empty,
    /// A tag exceeds [`TAG_MAX_LEN`] bytes.
    TooLong,
    /// A byte at `position` is not in the allowed charset after upper-case folding.
    InvalidChar { position: u32 },
}

/// Signature verification failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignatureError {
    /// Invalid signature length (must be 64 bytes for Ed25519).
    InvalidSignatureLength,
    /// Invalid public key length (must be 32 bytes for Ed25519).
    InvalidPublicKeyLength,
    /// Signature verification failed.
    VerificationFailed,
}

/// Verify an Ed25519 signature with domain separation.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `domain_separator` - Domain separator to prevent cross-domain replay attacks
/// * `message` - The message to verify
/// * `signature` - The Ed25519 signature (64 bytes)
/// * `public_key` - The Ed25519 public key (32 bytes)
///
/// # Returns
/// * `Ok(())` if the signature is valid
/// * `Err(SignatureError)` if verification fails
pub fn verify_signature(
    env: &soroban_sdk::Env,
    domain_separator: &[u8],
    message: &[u8],
    signature: &[u8],
    public_key: &[u8],
) -> Result<(), SignatureError> {
    let pk_arr: [u8; 32] = public_key
        .try_into()
        .map_err(|_| SignatureError::InvalidPublicKeyLength)?;
    let sig_arr: [u8; 64] = signature
        .try_into()
        .map_err(|_| SignatureError::InvalidSignatureLength)?;

    let mut msg_bytes = Bytes::from_slice(env, domain_separator);
    msg_bytes.extend_from_slice(message);

    let sig_bytes = soroban_sdk::BytesN::from_array(env, &sig_arr);
    let pk_bytes = soroban_sdk::BytesN::from_array(env, &pk_arr);

    env.crypto()
        .ed25519_verify(&pk_bytes, &msg_bytes, &sig_bytes);
    Ok(())
}

/// Typed error for slash signature verification.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SlashError {
    InvalidSignature = 8,
}

/// Verify an optional second-party slash signature.
///
/// This provides a defence-in-depth gate before executing destructive slash operations.
/// 
/// # Arguments
/// * `env` - Soroban environment
/// * `message` - The payload being authorized (e.g. amount or slash payload)
/// * `signature` - Optional 64-byte Ed25519 signature
/// * `public_key` - 32-byte Ed25519 public key of the second party
///
/// # Returns
/// * `Ok(())` if signature is valid or not provided (optional gate)
/// * `Err(SlashError)` if the provided signature is invalid
pub fn verify_slash_signature(
    env: &soroban_sdk::Env,
    message: &[u8],
    signature: Option<&[u8]>,
    public_key: &[u8],
) -> Result<(), SlashError> {
    if let Some(sig) = signature {
        if verify_signature(env, b"slash-auth", message, sig, public_key).is_err() {
            return Err(SlashError::InvalidSignature);
        }
    }
    Ok(())
}

/// Validates and canonicalizes a batch of tags without panicking.
///
/// # Rules
/// - The batch must contain at least one tag ([`TagError::Empty`]).
/// - Each tag must be between 1 and [`TAG_MAX_LEN`] bytes inclusive
///   ([`TagError::Empty`] for zero length, [`TagError::TooLong`] otherwise).
/// - Allowed charset: `[a-z0-9\-_]`. ASCII uppercase letters are silently
///   folded to lowercase; any other byte yields [`TagError::InvalidChar`].
///
/// Validation short-circuits on the first violation (empty batch, length, or
/// invalid byte) for gas efficiency.
///
/// # Returns
/// On success, a new `Vec<String>` containing the normalized (lowercased) tags
/// in the same order as the input. The function does **not** deduplicate.
///
/// # Usage
/// ```ignore
/// use remitwise_common::{canonicalize_tags_checked, TagError};
/// match canonicalize_tags_checked(&env, &tags) {
///     Ok(normalized) => { /* store normalized */ }
///     Err(TagError::InvalidChar { .. }) => {
///         soroban_sdk::panic_with_error!(&env, MyError::InvalidTagContent)
///     }
///     Err(TagError::Empty) | Err(TagError::TooLong) => { /* map to caller error */ }
/// }
/// ```
pub fn canonicalize_tags_checked(
    env: &soroban_sdk::Env,
    tags: &soroban_sdk::Vec<soroban_sdk::String>,
) -> Result<soroban_sdk::Vec<soroban_sdk::String>, TagError> {
    if tags.is_empty() {
        return Err(TagError::Empty);
    }
    let mut out = soroban_sdk::Vec::new(env);
    for tag in tags.iter() {
        let len = tag.len();
        if len == 0 {
            return Err(TagError::Empty);
        }
        if len > TAG_MAX_LEN {
            return Err(TagError::TooLong);
        }
        let mut buf = [0u8; 32];
        tag.copy_into_slice(&mut buf[..len as usize]);
        for (position, byte) in buf.iter_mut().take(len as usize).enumerate() {
            if byte.is_ascii_uppercase() {
                *byte += b'a' - b'A';
            }
            let b = *byte;
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_') {
                return Err(TagError::InvalidChar {
                    position: position as u32,
                });
            }
        }
        let s = match core::str::from_utf8(&buf[..len as usize]) {
            Ok(v) => v,
            Err(_) => {
                return Err(TagError::InvalidChar { position: 0 });
            }
        };
        out.push_back(soroban_sdk::String::from_str(env, s));
    }
    Ok(out)
}

/// Validates and canonicalizes a batch of tags, panicking on failure.
///
/// This is a thin wrapper around [`canonicalize_tags_checked`] that preserves
/// the legacy panic-based contract for existing callers. Prefer
/// [`canonicalize_tags_checked`] when handling untrusted or indexer-supplied
/// tag strings so errors can be mapped to typed contract errors.
///
/// # Rules
/// - The batch must contain at least one tag (`panic!("Tags cannot be empty")`).
/// - Each tag must be between 1 and [`TAG_MAX_LEN`] bytes inclusive
///   (`panic!("Tag must be between 1 and 32 characters")`).
/// - Allowed charset: `[a-z0-9\-_]`. ASCII uppercase letters are silently
///   folded to lowercase; any other byte causes the supplied `on_invalid_char`
///   closure to be called once (typically `panic_with_error!` or `panic!`).
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
    match canonicalize_tags_checked(env, tags) {
        Ok(out) => out,
        Err(TagError::Empty) => {
            if tags.is_empty() {
                panic!("Tags cannot be empty");
            }
            panic!("Tag must be between 1 and 32 characters");
        }
        Err(TagError::TooLong) => panic!("Tag must be between 1 and 32 characters"),
        Err(TagError::InvalidChar { .. }) => {
            on_invalid_char();
            // on_invalid_char must diverge (panic); this is unreachable.
            soroban_sdk::Vec::new(env)
        }
    }
}

/// Event emission helper
pub struct RemitwiseEvents;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod emit_tests;

impl RemitwiseEvents {
    /// Emits a single event with the given category, priority, and action.
    ///
    /// * `category` – The `EventCategory` describing the type of event.
    /// * `priority` – The `EventPriority` indicating the importance level.
    /// * `action` – A short `Symbol` identifying the specific action.
    /// * `data` – The event payload implementing `IntoVal`.
    ///
    /// The emitted event follows the topic schema defined in `docs/EVENT_TAXONOMY.md`.
    ///
    /// **Size Budget**: Event data must be compact (topics + small payload, not bulk records).
    /// The recommended maximum serialized size for the `data` payload is 256 bytes.
    /// Oversized payloads will trigger a debug/test assertion.
    #[allow(unexpected_cfgs)]
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

        #[cfg(test)]
        {
            use soroban_sdk::TryFromVal;
            let val = data.into_val(env);
            if let Ok(sc_val) = soroban_sdk::xdr::ScVal::try_from_val(env, &val) {
                let xdr_bytes = sc_val.to_xdr(env);
                let size = xdr_bytes.len();
                if size > 256 {
                    panic!("Event data size {} exceeds 256-byte budget. Emits must be compact.", size);
                }
            }
            env.events().publish(topics, val);
        }

        #[cfg(not(test))]
        env.events().publish(topics, data);
    }

    /// Emits a batch event for the given category and action with a count.
    ///
    /// * `category` – The `EventCategory` of the batched events.
    /// * `action` – Symbol representing the batch action.
    /// * `count` – Number of events in the batch.
    ///
    /// This always uses `EventPriority::Low` for batch events.
    ///
    /// **Size Budget**: Batch payloads (action + count) are inherently compact and conform
    /// to the recommended event data budget.
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

    /// Test helper: asserts that the most recently emitted Remitwise event has
    /// the expected category, priority, action, and that `data_pred` accepts
    /// the decoded payload. Uses `env.events().all()` so the assertion covers
    /// the real published event stream instead of a mock.
    ///
    /// Panics when no event has been emitted, when the topic tuple does not
    /// match the `(Remitwise, category, priority, action)` schema emitted by
    /// `EventEmitter::emit`, or when the data predicate returns false.
    #[cfg(test)]
    pub fn assert_last_event<T, F>(
        env: &soroban_sdk::Env,
        expected_category: EventCategory,
        expected_priority: EventPriority,
        expected_action: Symbol,
        data_pred: F,
    ) where
        T: soroban_sdk::TryFromVal<soroban_sdk::Env, soroban_sdk::Val>,
        F: FnOnce(&T) -> bool,
    {
        use soroban_sdk::TryFromVal;

        let all = env.events().all();
        let (_cid, topics, data) = all
            .last()
            .expect("expected at least one emitted event");

        // Topic schema emitted by `EventEmitter::emit`:
        // (symbol_short!("Remitwise"), category_u32, priority_u32, action)
        assert_eq!(
            topics.len(),
            4,
            "expected a 4-element Remitwise event topic tuple"
        );
        assert_eq!(
            topics.get(0).unwrap(),
            symbol_short!("Remitwise").into_val(env),
            "first topic must be the Remitwise marker"
        );
        assert_eq!(
            topics.get(1).unwrap(),
            expected_category.to_u32().into_val(env),
            "event category mismatch"
        );
        assert_eq!(
            topics.get(2).unwrap(),
            expected_priority.to_u32().into_val(env),
            "event priority mismatch"
        );
        assert_eq!(
            topics.get(3).unwrap(),
            expected_action.into_val(env),
            "event action mismatch"
        );

        let payload: T = T::try_from_val(env, &data).expect("failed to decode event data");
        assert!(
            data_pred(&payload),
            "event data predicate failed for action {:?}",
            expected_action
        );
    }
}

// ---------------------------------------------------------------------------
// Encoding stability tests (cross-contract ABI)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod assert_event_tests {
    use super::{EventCategory, EventPriority, RemitwiseEvents};

    #[test]
    fn assert_last_event_matches_emitted_topic_and_data() {
        let env = soroban_sdk::Env::default();
        let action = soroban_sdk::Symbol::new(&env, "test_act");

        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::High,
            action,
            (1u32, 2u32),
        );

        RemitwiseEvents::assert_last_event::<(u32, u32), _>(
            &env,
            EventCategory::Access,
            EventPriority::High,
            action,
            |(a, b)| *a == 1 && *b == 2,
        );
    }

    #[test]
    #[should_panic(expected = "event action mismatch")]
    fn assert_last_event_panics_on_action_mismatch() {
        let env = soroban_sdk::Env::default();
        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::High,
            soroban_sdk::Symbol::new(&env, "one"),
            1u32,
        );
        RemitwiseEvents::assert_last_event::<u32, _>(
            &env,
            EventCategory::Access,
            EventPriority::High,
            soroban_sdk::Symbol::new(&env, "two"),
            |_| true,
        );
    }
}

#[cfg(test)]
mod encoding_stability_tests {
    use super::{Category, CoverageType, FamilyRole, PolicyMode};
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
        let vec = Vec::from_array(
            &env,
            [Category::Spending, Category::Savings, Category::Bills],
        );
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

        let vec = Vec::from_array(
            &env,
            [FamilyRole::Owner, FamilyRole::Admin, FamilyRole::Viewer],
        );
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

    #[test]
    fn policy_mode_round_trip_and_encoding_stability() {
        let env = Env::default();

        assert_encoding_matches_discriminant(&env, PolicyMode::Strict, 1);

        fn cover_all_variants(v: PolicyMode) {
            match v {
                PolicyMode::Strict => {}
            }
        }

        for v in [PolicyMode::Strict] {
            cover_all_variants(v);
        }

        let vec = Vec::from_array(&env, [PolicyMode::Strict]);
        let mut out = Vec::<PolicyMode>::new(&env);
        for item in vec.iter() {
            out.push_back(round_trip(&env, item));
        }
        assert_eq!(out, vec);

        let mut map = Map::<u32, PolicyMode>::new(&env);
        map.set(1u32, PolicyMode::Strict);

        let mut out_map = Map::<u32, PolicyMode>::new(&env);
        for (k, v) in map.iter() {
            out_map.set(k, round_trip(&env, v));
        }
        assert_eq!(out_map, map);
    }
}
