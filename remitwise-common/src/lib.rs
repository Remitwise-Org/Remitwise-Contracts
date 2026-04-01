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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{symbol_short, testutils::Events, Env, IntoVal, Symbol, TryFromVal, Val, Vec};

    // -----------------------------------------------------------------------
    // clamp_limit – boundary and property tests
    // -----------------------------------------------------------------------

    #[test]
    fn clamp_limit_zero_returns_default() {
        assert_eq!(clamp_limit(0), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn clamp_limit_one_returns_one() {
        assert_eq!(clamp_limit(1), 1);
    }

    #[test]
    fn clamp_limit_default_value_passes_through() {
        assert_eq!(clamp_limit(DEFAULT_PAGE_LIMIT), DEFAULT_PAGE_LIMIT);
    }

    #[test]
    fn clamp_limit_max_is_inclusive() {
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
    }

    #[test]
    fn clamp_limit_above_max_is_capped() {
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
    }

    #[test]
    fn clamp_limit_far_above_max_is_capped() {
        assert_eq!(clamp_limit(u32::MAX), MAX_PAGE_LIMIT);
    }

    #[test]
    fn clamp_limit_mid_range_passes_through() {
        for v in [2, 10, 25, MAX_PAGE_LIMIT - 1] {
            assert_eq!(clamp_limit(v), v, "clamp_limit({v}) should pass through");
        }
    }

    #[test]
    fn clamp_limit_return_always_in_valid_range() {
        // Spot-check a range of inputs to ensure invariant: result in [1, MAX_PAGE_LIMIT]
        let inputs = [0, 1, 10, 20, 49, 50, 51, 100, 1000, u32::MAX];
        for input in inputs {
            let result = clamp_limit(input);
            assert!(
                result >= 1 && result <= MAX_PAGE_LIMIT,
                "clamp_limit({input}) = {result} is out of range [1, {MAX_PAGE_LIMIT}]"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Enum discriminant values – prevent accidental renumbering
    // -----------------------------------------------------------------------

    #[test]
    fn category_discriminants() {
        assert_eq!(Category::Spending as u32, 1);
        assert_eq!(Category::Savings as u32, 2);
        assert_eq!(Category::Bills as u32, 3);
        assert_eq!(Category::Insurance as u32, 4);
    }

    #[test]
    fn family_role_discriminants() {
        assert_eq!(FamilyRole::Owner as u32, 1);
        assert_eq!(FamilyRole::Admin as u32, 2);
        assert_eq!(FamilyRole::Member as u32, 3);
        assert_eq!(FamilyRole::Viewer as u32, 4);
    }

    #[test]
    fn family_role_ordering() {
        // Owner < Admin < Member < Viewer (ascending privilege number = decreasing privilege)
        assert!(FamilyRole::Owner < FamilyRole::Admin);
        assert!(FamilyRole::Admin < FamilyRole::Member);
        assert!(FamilyRole::Member < FamilyRole::Viewer);
    }

    #[test]
    fn coverage_type_discriminants() {
        assert_eq!(CoverageType::Health as u32, 1);
        assert_eq!(CoverageType::Life as u32, 2);
        assert_eq!(CoverageType::Property as u32, 3);
        assert_eq!(CoverageType::Auto as u32, 4);
        assert_eq!(CoverageType::Liability as u32, 5);
    }

    #[test]
    fn event_category_discriminants() {
        assert_eq!(EventCategory::Transaction as u32, 0);
        assert_eq!(EventCategory::State as u32, 1);
        assert_eq!(EventCategory::Alert as u32, 2);
        assert_eq!(EventCategory::System as u32, 3);
        assert_eq!(EventCategory::Access as u32, 4);
    }

    #[test]
    fn event_priority_discriminants() {
        assert_eq!(EventPriority::Low as u32, 0);
        assert_eq!(EventPriority::Medium as u32, 1);
        assert_eq!(EventPriority::High as u32, 2);
    }

    // -----------------------------------------------------------------------
    // EventCategory / EventPriority to_u32 conversion
    // -----------------------------------------------------------------------

    #[test]
    fn event_category_to_u32_matches_discriminant() {
        assert_eq!(EventCategory::Transaction.to_u32(), 0);
        assert_eq!(EventCategory::State.to_u32(), 1);
        assert_eq!(EventCategory::Alert.to_u32(), 2);
        assert_eq!(EventCategory::System.to_u32(), 3);
        assert_eq!(EventCategory::Access.to_u32(), 4);
    }

    #[test]
    fn event_priority_to_u32_matches_discriminant() {
        assert_eq!(EventPriority::Low.to_u32(), 0);
        assert_eq!(EventPriority::Medium.to_u32(), 1);
        assert_eq!(EventPriority::High.to_u32(), 2);
    }

    // -----------------------------------------------------------------------
    // Constants – TTL relationships and value sanity
    // -----------------------------------------------------------------------

    #[test]
    fn day_in_ledgers_value() {
        // ~5 seconds per ledger → 86400 / 5 = 17280 ledgers per day
        assert_eq!(DAY_IN_LEDGERS, 17_280);
    }

    #[test]
    fn persistent_ttl_threshold_less_than_bump() {
        assert!(
            PERSISTENT_LIFETIME_THRESHOLD < PERSISTENT_BUMP_AMOUNT,
            "Threshold ({PERSISTENT_LIFETIME_THRESHOLD}) must be less than bump ({PERSISTENT_BUMP_AMOUNT})"
        );
    }

    #[test]
    fn archive_ttl_threshold_less_than_bump() {
        assert!(
            ARCHIVE_LIFETIME_THRESHOLD < ARCHIVE_BUMP_AMOUNT,
            "Threshold ({ARCHIVE_LIFETIME_THRESHOLD}) must be less than bump ({ARCHIVE_BUMP_AMOUNT})"
        );
    }

    #[test]
    fn persistent_bump_is_60_days() {
        assert_eq!(PERSISTENT_BUMP_AMOUNT, 60 * DAY_IN_LEDGERS);
    }

    #[test]
    fn persistent_threshold_is_15_days() {
        assert_eq!(PERSISTENT_LIFETIME_THRESHOLD, 15 * DAY_IN_LEDGERS);
    }

    #[test]
    fn archive_bump_is_150_days() {
        assert_eq!(ARCHIVE_BUMP_AMOUNT, 150 * DAY_IN_LEDGERS);
    }

    #[test]
    fn archive_threshold_is_1_day() {
        assert_eq!(ARCHIVE_LIFETIME_THRESHOLD, 1 * DAY_IN_LEDGERS);
    }

    #[test]
    fn signature_expiration_is_24_hours() {
        assert_eq!(SIGNATURE_EXPIRATION, 86_400);
    }

    #[test]
    fn max_batch_size_value() {
        assert_eq!(MAX_BATCH_SIZE, 50);
    }

    #[test]
    fn contract_version_value() {
        assert_eq!(CONTRACT_VERSION, 1);
    }

    #[test]
    fn pagination_defaults_are_sane() {
        assert!(DEFAULT_PAGE_LIMIT >= 1, "Default page limit must be at least 1");
        assert!(DEFAULT_PAGE_LIMIT <= MAX_PAGE_LIMIT, "Default must not exceed max");
        assert_eq!(DEFAULT_PAGE_LIMIT, 20);
        assert_eq!(MAX_PAGE_LIMIT, 50);
    }

    // -----------------------------------------------------------------------
    // RemitwiseEvents::emit – topic schema consistency
    // -----------------------------------------------------------------------

    /// Helper: extract the last event's topics and data from the environment.
    fn last_event(env: &Env) -> (soroban_sdk::Address, Vec<Val>, Val) {
        let events = env.events().all();
        events.last().unwrap()
    }

    #[test]
    fn emit_produces_four_topic_tuple() {
        let env = Env::default();
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Low,
            symbol_short!("test"),
            42u32,
        );

        let (_contract, topics, _data) = last_event(&env);
        assert_eq!(topics.len(), 4, "Event must have exactly 4 topics");
    }

    #[test]
    fn emit_topic_0_is_namespace() {
        let env = Env::default();
        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("init"),
            true,
        );

        let (_contract, topics, _data) = last_event(&env);
        let ns: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(ns, symbol_short!("Remitwise"), "Topic[0] must be the Remitwise namespace");
    }

    #[test]
    fn emit_topic_1_is_category() {
        let env = Env::default();

        let categories = [
            (EventCategory::Transaction, 0u32),
            (EventCategory::State, 1),
            (EventCategory::Alert, 2),
            (EventCategory::System, 3),
            (EventCategory::Access, 4),
        ];

        for (cat, expected) in categories {
            RemitwiseEvents::emit(
                &env,
                cat,
                EventPriority::Low,
                symbol_short!("t"),
                0u32,
            );

            let (_contract, topics, _data) = last_event(&env);
            let cat_val: u32 = u32::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
            assert_eq!(cat_val, expected, "Topic[1] category mismatch for discriminant {expected}");
        }
    }

    #[test]
    fn emit_topic_2_is_priority() {
        let env = Env::default();

        let priorities = [
            (EventPriority::Low, 0u32),
            (EventPriority::Medium, 1),
            (EventPriority::High, 2),
        ];

        for (pri, expected) in priorities {
            RemitwiseEvents::emit(
                &env,
                EventCategory::Transaction,
                pri,
                symbol_short!("t"),
                0u32,
            );

            let (_contract, topics, _data) = last_event(&env);
            let pri_val: u32 = u32::try_from_val(&env, &topics.get(2).unwrap()).unwrap();
            assert_eq!(pri_val, expected, "Topic[2] priority mismatch for discriminant {expected}");
        }
    }

    #[test]
    fn emit_topic_3_is_action() {
        let env = Env::default();
        let action = symbol_short!("created");

        RemitwiseEvents::emit(
            &env,
            EventCategory::State,
            EventPriority::Medium,
            action.clone(),
            0u32,
        );

        let (_contract, topics, _data) = last_event(&env);
        let act: Symbol = Symbol::try_from_val(&env, &topics.get(3).unwrap()).unwrap();
        assert_eq!(act, action, "Topic[3] must match the action symbol");
    }

    #[test]
    fn emit_data_payload_is_preserved() {
        let env = Env::default();
        let payload = 12345u32;

        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::Low,
            symbol_short!("calc"),
            payload,
        );

        let (_contract, _topics, data) = last_event(&env);
        let received: u32 = u32::try_from_val(&env, &data).unwrap();
        assert_eq!(received, payload, "Event data payload must match emitted value");
    }

    #[test]
    fn emit_bool_payload() {
        let env = Env::default();
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("paused"),
            true,
        );

        let (_contract, _topics, data) = last_event(&env);
        let received: bool = bool::try_from_val(&env, &data).unwrap();
        assert!(received);
    }

    #[test]
    fn emit_tuple_payload() {
        let env = Env::default();
        let payload: (u32, u32) = (1, 2);

        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("upgraded"),
            payload.clone(),
        );

        let (_contract, _topics, data) = last_event(&env);
        let received: (u32, u32) = <(u32, u32)>::try_from_val(&env, &data).unwrap();
        assert_eq!(received, payload);
    }

    #[test]
    fn emit_with_all_category_priority_combinations() {
        let env = Env::default();

        let categories = [
            EventCategory::Transaction,
            EventCategory::State,
            EventCategory::Alert,
            EventCategory::System,
            EventCategory::Access,
        ];
        let priorities = [
            EventPriority::Low,
            EventPriority::Medium,
            EventPriority::High,
        ];

        let mut count = 0u32;
        for cat in &categories {
            for pri in &priorities {
                RemitwiseEvents::emit(
                    &env,
                    *cat,
                    *pri,
                    symbol_short!("test"),
                    count,
                );

                let (_contract, topics, _data) = last_event(&env);
                // Verify namespace is always "Remitwise"
                let ns: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
                assert_eq!(ns, symbol_short!("Remitwise"));
                // Always 4 topics
                assert_eq!(topics.len(), 4);

                count += 1;
            }
        }

        // All 15 combinations emitted (5 categories × 3 priorities)
        assert_eq!(count, 15);
    }

    // -----------------------------------------------------------------------
    // RemitwiseEvents::emit_batch – topic and payload schema
    // -----------------------------------------------------------------------

    #[test]
    fn emit_batch_produces_four_topics() {
        let env = Env::default();
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Access,
            symbol_short!("member"),
            5,
        );

        let (_contract, topics, _data) = last_event(&env);
        assert_eq!(topics.len(), 4, "Batch event must have exactly 4 topics");
    }

    #[test]
    fn emit_batch_topic_0_is_namespace() {
        let env = Env::default();
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Access,
            symbol_short!("member"),
            5,
        );

        let (_contract, topics, _data) = last_event(&env);
        let ns: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        assert_eq!(ns, symbol_short!("Remitwise"));
    }

    #[test]
    fn emit_batch_topic_2_is_always_low_priority() {
        let env = Env::default();

        // Batch events always use Low priority regardless of category
        let categories = [
            EventCategory::Transaction,
            EventCategory::State,
            EventCategory::Alert,
            EventCategory::System,
            EventCategory::Access,
        ];

        for cat in categories {
            RemitwiseEvents::emit_batch(
                &env,
                cat,
                symbol_short!("op"),
                1,
            );

            let (_contract, topics, _data) = last_event(&env);
            let pri: u32 = u32::try_from_val(&env, &topics.get(2).unwrap()).unwrap();
            assert_eq!(pri, EventPriority::Low.to_u32(), "Batch events must always use Low priority");
        }
    }

    #[test]
    fn emit_batch_topic_3_is_always_batch() {
        let env = Env::default();
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Access,
            symbol_short!("member"),
            10,
        );

        let (_contract, topics, _data) = last_event(&env);
        let act: Symbol = Symbol::try_from_val(&env, &topics.get(3).unwrap()).unwrap();
        assert_eq!(act, symbol_short!("batch"), "Topic[3] must always be 'batch' for batch events");
    }

    #[test]
    fn emit_batch_payload_contains_action_and_count() {
        let env = Env::default();
        let action = symbol_short!("member");
        let count = 42u32;

        RemitwiseEvents::emit_batch(&env, EventCategory::Access, action.clone(), count);

        let (_contract, _topics, data) = last_event(&env);
        let (received_action, received_count): (Symbol, u32) =
            <(Symbol, u32)>::try_from_val(&env, &data).unwrap();
        assert_eq!(received_action, action);
        assert_eq!(received_count, count);
    }

    #[test]
    fn emit_batch_zero_count() {
        let env = Env::default();
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Transaction,
            symbol_short!("noop"),
            0,
        );

        let (_contract, _topics, data) = last_event(&env);
        let (_action, count): (Symbol, u32) =
            <(Symbol, u32)>::try_from_val(&env, &data).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn emit_batch_large_count() {
        let env = Env::default();
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Transaction,
            symbol_short!("bulk"),
            MAX_BATCH_SIZE,
        );

        let (_contract, _topics, data) = last_event(&env);
        let (_action, count): (Symbol, u32) =
            <(Symbol, u32)>::try_from_val(&env, &data).unwrap();
        assert_eq!(count, MAX_BATCH_SIZE);
    }

    // -----------------------------------------------------------------------
    // Schema consistency – emit vs emit_batch share the same topic schema
    // -----------------------------------------------------------------------

    #[test]
    fn emit_and_emit_batch_share_namespace_and_category_positions() {
        let env = Env::default();

        // Emit a normal event
        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::High,
            symbol_short!("member"),
            0u32,
        );
        let (_c1, topics_emit, _d1) = last_event(&env);

        // Emit a batch event with the same category
        RemitwiseEvents::emit_batch(
            &env,
            EventCategory::Access,
            symbol_short!("member"),
            1,
        );
        let (_c2, topics_batch, _d2) = last_event(&env);

        // Topic[0] (namespace) must be identical
        let ns_emit: Symbol = Symbol::try_from_val(&env, &topics_emit.get(0).unwrap()).unwrap();
        let ns_batch: Symbol = Symbol::try_from_val(&env, &topics_batch.get(0).unwrap()).unwrap();
        assert_eq!(ns_emit, ns_batch, "Namespace must be identical across emit and emit_batch");

        // Topic[1] (category) must be identical for same category
        let cat_emit: u32 = u32::try_from_val(&env, &topics_emit.get(1).unwrap()).unwrap();
        let cat_batch: u32 = u32::try_from_val(&env, &topics_batch.get(1).unwrap()).unwrap();
        assert_eq!(cat_emit, cat_batch, "Category must be identical for same EventCategory");
    }

    #[test]
    fn emit_batch_action_in_payload_not_topics() {
        let env = Env::default();
        let action = symbol_short!("member");

        RemitwiseEvents::emit_batch(&env, EventCategory::Access, action.clone(), 5);

        let (_contract, topics, data) = last_event(&env);

        // Topic[3] should be "batch", not the action
        let topic_action: Symbol = Symbol::try_from_val(&env, &topics.get(3).unwrap()).unwrap();
        assert_eq!(topic_action, symbol_short!("batch"));
        assert_ne!(topic_action, action, "Action must not appear in batch topic[3]");

        // Action should be in the payload
        let (payload_action, _count): (Symbol, u32) =
            <(Symbol, u32)>::try_from_val(&env, &data).unwrap();
        assert_eq!(payload_action, action, "Action must appear in batch payload");
    }

    // -----------------------------------------------------------------------
    // Enum trait consistency
    // -----------------------------------------------------------------------

    #[test]
    fn category_clone_eq() {
        let a = Category::Spending;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(a, Category::Savings);
    }

    #[test]
    fn family_role_clone_eq() {
        let a = FamilyRole::Owner;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(a, FamilyRole::Viewer);
    }

    #[test]
    fn coverage_type_clone_eq() {
        let a = CoverageType::Health;
        let b = a.clone();
        assert_eq!(a, b);
        assert_ne!(a, CoverageType::Life);
    }

    #[test]
    fn event_category_is_copy() {
        let a = EventCategory::System;
        let b = a; // Copy
        let _ = a; // Still usable — proves Copy
        assert_eq!(b.to_u32(), 3);
    }

    #[test]
    fn event_priority_is_copy() {
        let a = EventPriority::High;
        let b = a; // Copy
        let _ = a; // Still usable — proves Copy
        assert_eq!(b.to_u32(), 2);
    }
}

// Standardized TTL Constants (Ledger Counts)
pub const DAY_IN_LEDGERS: u32 = 17280; // ~5 seconds per ledger

pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS; // 30 days
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 1 * DAY_IN_LEDGERS; // 1 day

pub const PERSISTENT_BUMP_AMOUNT: u32 = 60 * DAY_IN_LEDGERS; // 60 days
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS; // 15 days
pub const INSTANCE_BUMP_AMOUNT: u32 = PERSISTENT_BUMP_AMOUNT;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = PERSISTENT_LIFETIME_THRESHOLD;


pub const INSTANCE_BUMP_AMOUNT: u32 = 30 * DAY_IN_LEDGERS; // 30 days
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS; // 7 days

/// Storage TTL for archived contract data (instance/archive bumps).
pub const ARCHIVE_BUMP_AMOUNT: u32 = 150 * DAY_IN_LEDGERS; // ~150 days
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = DAY_IN_LEDGERS; // 1 day
