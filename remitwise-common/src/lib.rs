#![no_std]

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

/// Event categories for logging
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

/// Event priorities for logging
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

/// Signature expiration time (24 hours in seconds)
pub const SIGNATURE_EXPIRATION: u64 = 86400;

/// Contract version
pub const CONTRACT_VERSION: u32 = 1;

/// Maximum batch size for operations
pub const MAX_BATCH_SIZE: u32 = 50;

/// Helper function to clamp limit
///
/// # Behavior Contract
///
/// `clamp_limit` normalises a caller-supplied page-size value so that every
/// pagination call in the workspace uses a consistent, bounded limit.
///
/// ## Rules (in evaluation order)
///
/// | Input condition          | Returned value        | Rationale                                      |
/// |--------------------------|----------------------|------------------------------------------------|
/// | `limit == 0`             | `DEFAULT_PAGE_LIMIT` | Zero is treated as "use the default".          |
/// | `limit > MAX_PAGE_LIMIT` | `MAX_PAGE_LIMIT`     | Cap to prevent unbounded storage reads.        |
/// | otherwise                | `limit`              | Caller value is within the valid range.        |
///
/// ## Invariants
///
/// - The return value is always in the range `[1, MAX_PAGE_LIMIT]`.
/// - `clamp_limit(0) == DEFAULT_PAGE_LIMIT` (default substitution).
/// - `clamp_limit(MAX_PAGE_LIMIT) == MAX_PAGE_LIMIT` (boundary is inclusive).
/// - `clamp_limit(MAX_PAGE_LIMIT + 1) == MAX_PAGE_LIMIT` (cap is enforced).
/// - The function is pure and has no side effects.
///
/// ## Security Assumptions
///
/// - Callers must not rely on receiving a value larger than `MAX_PAGE_LIMIT`.
/// - A zero input is **not** an error; it is silently replaced with the default.
///   Contracts that need to distinguish "no limit requested" from "default limit"
///   should inspect the raw input before calling this function.
///
/// ## Usage
///
/// ```rust
/// use remitwise_common::{clamp_limit, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
///
/// assert_eq!(clamp_limit(0),                  DEFAULT_PAGE_LIMIT);
/// assert_eq!(clamp_limit(10),                 10);
/// assert_eq!(clamp_limit(MAX_PAGE_LIMIT),     MAX_PAGE_LIMIT);
/// assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
/// ```
pub fn clamp_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_PAGE_LIMIT
    } else if limit > MAX_PAGE_LIMIT {
        MAX_PAGE_LIMIT
    } else {
        limit
    }
}

/// Event emission helper
///
/// # Deterministic topic naming
///
/// All events emitted via `RemitwiseEvents` follow a deterministic topic schema:
///
/// 1. A fixed namespace symbol: `"Remitwise"`.
/// 2. An event category as `u32` (see `EventCategory`).
/// 3. An event priority as `u32` (see `EventPriority`).
/// 4. An action `Symbol` describing the specific event or a subtype (e.g. `"created"`).
///
/// This ordering allows consumers to index and filter events reliably across contracts.
pub struct RemitwiseEvents;

impl RemitwiseEvents {
    /// Emit a single event with deterministic topics.
    ///
    /// # Parameters
    /// - `env`: Soroban environment used to publish the event.
    /// - `category`: Logical event category (`EventCategory`).
    /// - `priority`: Event priority (`EventPriority`).
    /// - `action`: A `Symbol` identifying the action or event name.
    /// - `data`: The serializable payload for the event.
    ///
    /// # Security
    /// Do not include sensitive personal data in `data` because events are publicly visible on-chain.
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

    /// Emit a small batch-style event indicating bulk operations.
    ///
    /// The `action` parameter is included in the payload rather than as the final topic
    /// to make the topic schema consistent for batch analytics.
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

// Standardized TTL Constants (Ledger Counts)
pub const DAY_IN_LEDGERS: u32 = 17280; // ~5 seconds per ledger

pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS; // 30 days
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 1 * DAY_IN_LEDGERS; // 1 day

pub const ARCHIVE_BUMP_AMOUNT: u32 = 150 * DAY_IN_LEDGERS; // ~150 days
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = 1 * DAY_IN_LEDGERS; // 1 day

pub mod nonce {
    use soroban_sdk::{symbol_short, Address, Env, Map, Symbol, Vec};

    /// @notice Errors returned by canonical nonce operations.
    #[derive(Copy, Clone, Debug, Eq, PartialEq)]
    #[repr(u32)]
    pub enum NonceError {
        /// @notice The supplied nonce does not equal the current nonce.
        InvalidNonce = 1,
        /// @notice The nonce has already been consumed for this address.
        NonceAlreadyUsed = 2,
        /// @notice Nonce increment overflowed.
        Overflow = 3,
    }

    const NONCES_KEY: Symbol = symbol_short!("NONCES");
    const USED_NONCES_KEY: Symbol = symbol_short!("USED_N");
    const MAX_USED_NONCES_PER_ADDR: u32 = 256;

    /// @notice Returns the current sequential nonce for `address`.
    pub fn get(env: &Env, address: &Address) -> u64 {
        let nonces: Option<Map<Address, u64>> = env.storage().instance().get(&NONCES_KEY);
        nonces
            .as_ref()
            .and_then(|m| m.get(address.clone()))
            .unwrap_or(0)
    }

    /// @notice Returns true if `nonce` is recorded as consumed for `address`.
    pub fn is_used(env: &Env, address: &Address, nonce: u64) -> bool {
        let map: Option<Map<Address, Vec<u64>>> = env.storage().instance().get(&USED_NONCES_KEY);
        match map {
            None => false,
            Some(m) => match m.get(address.clone()) {
                None => false,
                Some(used) => used.contains(nonce),
            },
        }
    }

    /// @notice Validates that `expected` equals the current nonce for `address`.
    pub fn require_current(env: &Env, address: &Address, expected: u64) -> Result<(), NonceError> {
        let current = get(env, address);
        if expected != current {
            return Err(NonceError::InvalidNonce);
        }
        Ok(())
    }

    /// @notice Marks the current nonce as consumed and increments the stored counter.
    ///
    /// @dev Call only after all state changes for the signed/replayable action have succeeded.
    pub fn increment(env: &Env, address: &Address) -> Result<u64, NonceError> {
        let current = get(env, address);
        if is_used(env, address, current) {
            return Err(NonceError::NonceAlreadyUsed);
        }
        mark_used(env, address, current);
        let next = current.checked_add(1).ok_or(NonceError::Overflow)?;

        let mut nonces: Map<Address, u64> = env
            .storage()
            .instance()
            .get(&NONCES_KEY)
            .unwrap_or_else(|| Map::new(env));
        nonces.set(address.clone(), next);
        env.storage().instance().set(&NONCES_KEY, &nonces);

        Ok(next)
    }

    /// @notice Validates the nonce and, on success, records it as consumed and increments.
    ///
    /// @dev Prefer `require_current` + `increment` so nonce updates only happen after success.
    pub fn consume(env: &Env, address: &Address, expected: u64) -> Result<u64, NonceError> {
        require_current(env, address, expected)?;
        increment(env, address)
    }

    fn mark_used(env: &Env, address: &Address, nonce: u64) {
        let mut map: Map<Address, Vec<u64>> = env
            .storage()
            .instance()
            .get(&USED_NONCES_KEY)
            .unwrap_or_else(|| Map::new(env));

        let mut used: Vec<u64> = map.get(address.clone()).unwrap_or_else(|| Vec::new(env));

        if used.len() >= MAX_USED_NONCES_PER_ADDR {
            let mut trimmed = Vec::new(env);
            for i in 1..used.len() {
                if let Some(v) = used.get(i) {
                    trimmed.push_back(v);
                }
            }
            used = trimmed;
        }

        used.push_back(nonce);
        map.set(address.clone(), used);
        env.storage().instance().set(&USED_NONCES_KEY, &map);
    }


/// Tag validation and canonicalization constants
pub const TAG_MIN_LENGTH: usize = 1;
pub const TAG_MAX_LENGTH: usize = 32;

/// Tag validation and canonicalization utilities
///
/// Provides consistent tag handling across all contracts to ensure
/// safe indexing and predictable search behavior.
pub mod tags {
    use super::{TAG_MAX_LENGTH, TAG_MIN_LENGTH};
    use soroban_sdk::{Env, String, Vec};

    /// Validates a tag for length and character constraints.
    ///
    /// # Requirements
    /// - Tag length must be between 1 and 32 characters (inclusive)
    /// - Tag must not be empty
    ///
    /// # Character Set
    /// Tags should consist of printable ASCII characters. The validation
    /// enforces length bounds but allows the contract layer to define
    /// additional charset restrictions if needed.
    ///
    /// # Panics
    /// Panics if the tag violates any validation rule.
    pub fn validate_tag(tag: &String) {
        let len = tag.len() as usize;
        if len < TAG_MIN_LENGTH {
            panic!("Tag must be at least 1 character");
        }
        if len > TAG_MAX_LENGTH {
            panic!("Tag must be at most 32 characters");
        }
    }

    /// Validates a batch of tags.
    ///
    /// # Requirements
    /// - At least one tag must be provided
    /// - Each tag must pass individual validation
    ///
    /// # Panics
    /// Panics if the tag list is empty or any tag is invalid.
    pub fn validate_tags(tags: &Vec<String>) {
        if tags.is_empty() {
            panic!("Tags list cannot be empty");
        }
        for tag in tags.iter() {
            validate_tag(&tag);
        }
    }

    /// Canonicalizes a tag by applying normalization rules.
    ///
    /// # Normalization Rules
    /// 1. Trim leading and trailing whitespace
    /// 2. Convert to lowercase for consistency
    /// 3. Collapse multiple consecutive spaces to single space
    ///
    /// # Security
    /// This function ensures tags are in a canonical form for reliable
    /// indexing and comparison. All tags should be canonicalized before
    /// storage or comparison.
    ///
    /// # Returns
    /// A new canonicalized String.
    pub fn canonicalize_tag(env: &Env, tag: &String) -> String {
        // Convert to bytes for manipulation
        let bytes = tag.to_bytes();
        let len = bytes.len();

        if len == 0 {
            return String::from_str(env, "");
        }

        // Find start (skip leading whitespace)
        let mut start = 0;
        while start < len && is_whitespace(bytes.get(start).unwrap_or(0)) {
            start += 1;
        }

        // Find end (skip trailing whitespace)
        let mut end = len;
        while end > start && is_whitespace(bytes.get(end - 1).unwrap_or(0)) {
            end -= 1;
        }

        // Build canonicalized string
        let mut result = Vec::new(env);
        let mut prev_was_space = false;

        for i in start..end {
            let byte = bytes.get(i).unwrap_or(0);

            if is_whitespace(byte) {
                // Collapse multiple spaces
                if !prev_was_space {
                    result.push_back(b' ');
                    prev_was_space = true;
                }
            } else {
                // Convert to lowercase (ASCII only)
                let lower = if byte >= b'A' && byte <= b'Z' {
                    byte + 32
                } else {
                    byte
                };
                result.push_back(lower);
                prev_was_space = false;
            }
        }

        // Convert bytes back to string
        String::from_bytes(env, &result)
    }

    /// Validates and canonicalizes a single tag.
    ///
    /// Combines validation and canonicalization in one step.
    /// The tag is validated first, then canonicalized.
    ///
    /// # Returns
    /// A validated and canonicalized tag string.
    ///
    /// # Panics
    /// Panics if the tag fails validation.
    pub fn validate_and_canonicalize(env: &Env, tag: &String) -> String {
        let canonical = canonicalize_tag(env, tag);
        validate_tag(&canonical);
        canonical
    }

    /// Validates and canonicalizes a batch of tags.
    ///
    /// # Returns
    /// A vector of validated and canonicalized tag strings.
    ///
    /// # Panics
    /// Panics if any tag fails validation.
    pub fn validate_and_canonicalize_tags(env: &Env, tags: &Vec<String>) -> Vec<String> {
        let mut result = Vec::new(env);
        for tag in tags.iter() {
            let canonical = validate_and_canonicalize(env, &tag);
            result.push_back(canonical);
        }
        // Validate the final list is not empty
        if result.is_empty() {
            panic!("Tags list cannot be empty");
        }
        result
    }

    /// Check if a byte is a whitespace character
    fn is_whitespace(byte: u8) -> bool {
        byte == b' ' || byte == b'	' || byte == b'
' || byte == b''
    }
}

