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

/// Storage TTL constants for active data
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
pub const INSTANCE_BUMP_AMOUNT: u32 = 518400; // ~30 days

/// Storage TTL constants for archived data
pub const ARCHIVE_LIFETIME_THRESHOLD: u32 = 17280; // ~1 day
pub const ARCHIVE_BUMP_AMOUNT: u32 = 2592000; // ~180 days (6 months)

/// Signature expiration time (24 hours in seconds)
pub const SIGNATURE_EXPIRATION: u64 = 86400;

/// Contract version
pub const CONTRACT_VERSION: u32 = 1;

/// Maximum batch size for operations
pub const MAX_BATCH_SIZE: u32 = 50;

/// Helper function to clamp limit
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

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Events, Env, FromVal, IntoVal, Symbol, TryFromVal, symbol_short};

    #[test]
    fn test_constants_stability() {
        assert_eq!(DEFAULT_PAGE_LIMIT, 20);
        assert_eq!(MAX_PAGE_LIMIT, 50);
        assert_eq!(INSTANCE_LIFETIME_THRESHOLD, 17280);
        assert_eq!(INSTANCE_BUMP_AMOUNT, 518400);
        assert_eq!(ARCHIVE_LIFETIME_THRESHOLD, 17280);
        assert_eq!(ARCHIVE_BUMP_AMOUNT, 2592000);
        assert_eq!(SIGNATURE_EXPIRATION, 86400);
        assert_eq!(CONTRACT_VERSION, 1);
        assert_eq!(MAX_BATCH_SIZE, 50);
    }

    #[test]
    fn test_clamp_limit_edge_cases() {
        assert_eq!(clamp_limit(0), DEFAULT_PAGE_LIMIT);
        assert_eq!(clamp_limit(1), 1);
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
        assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
    }

    #[test]
    fn test_enum_discriminants_and_ordering() {
        assert_eq!(Category::Spending as u32, 1);
        assert_eq!(Category::Savings as u32, 2);
        assert_eq!(Category::Bills as u32, 3);
        assert_eq!(Category::Insurance as u32, 4);

        assert_eq!(FamilyRole::Owner as u32, 1);
        assert_eq!(FamilyRole::Admin as u32, 2);
        assert_eq!(FamilyRole::Member as u32, 3);
        assert_eq!(FamilyRole::Viewer as u32, 4);
        assert!(FamilyRole::Owner < FamilyRole::Admin);
        assert!(FamilyRole::Admin < FamilyRole::Member);
        assert!(FamilyRole::Member < FamilyRole::Viewer);

        assert_eq!(CoverageType::Health as u32, 1);
        assert_eq!(CoverageType::Life as u32, 2);
        assert_eq!(CoverageType::Property as u32, 3);
        assert_eq!(CoverageType::Auto as u32, 4);
        assert_eq!(CoverageType::Liability as u32, 5);

        assert_eq!(EventCategory::Transaction as u32, 0);
        assert_eq!(EventCategory::State as u32, 1);
        assert_eq!(EventCategory::Alert as u32, 2);
        assert_eq!(EventCategory::System as u32, 3);
        assert_eq!(EventCategory::Access as u32, 4);

        assert_eq!(EventPriority::Low as u32, 0);
        assert_eq!(EventPriority::Medium as u32, 1);
        assert_eq!(EventPriority::High as u32, 2);
    }

    #[test]
    fn test_enum_serialization_round_trip() {
        let env = Env::default();

        for category in [Category::Spending, Category::Savings, Category::Bills, Category::Insurance] {
            let val = category.into_val(&env);
            let restored = Category::try_from_val(&env, &val).unwrap();
            assert_eq!(restored, category);
        }

        for role in [FamilyRole::Owner, FamilyRole::Admin, FamilyRole::Member, FamilyRole::Viewer] {
            let val = role.into_val(&env);
            let restored = FamilyRole::try_from_val(&env, &val).unwrap();
            assert_eq!(restored, role);
        }

        for coverage in [CoverageType::Health, CoverageType::Life, CoverageType::Property, CoverageType::Auto, CoverageType::Liability] {
            let val = coverage.into_val(&env);
            let restored = CoverageType::try_from_val(&env, &val).unwrap();
            assert_eq!(restored, coverage);
        }

        for category in [EventCategory::Transaction, EventCategory::State, EventCategory::Alert, EventCategory::System, EventCategory::Access] {
            let val = category.into_val(&env);
            let restored = EventCategory::try_from_val(&env, &val).unwrap();
            assert_eq!(restored as u32, category as u32);
        }

        for priority in [EventPriority::Low, EventPriority::Medium, EventPriority::High] {
            let val = priority.into_val(&env);
            let restored = EventPriority::try_from_val(&env, &val).unwrap();
            assert_eq!(restored as u32, priority as u32);
        }
    }

    #[test]
    fn test_remitwise_events_topics_and_payload() {
        let env = Env::default();

        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::High,
            symbol_short!("stability-test"),
            123u32,
        );

        let events = env.events().all();
        assert!(!events.is_empty());
        let last_event = events.last().unwrap();
        let topics = &last_event.1;

        let namespace: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
        let category: u32 = u32::try_from_val(&env, &topics.get(1).unwrap()).unwrap();
        let priority: u32 = u32::try_from_val(&env, &topics.get(2).unwrap()).unwrap();
        let action: Symbol = Symbol::try_from_val(&env, &topics.get(3).unwrap()).unwrap();

        assert_eq!(namespace, symbol_short!("Remitwise"));
        assert_eq!(category, EventCategory::System.to_u32());
        assert_eq!(priority, EventPriority::High.to_u32());
        assert_eq!(action, symbol_short!("stability-test"));

        let actual_data: u32 = u32::try_from_val(&env, &last_event.2).unwrap();
        assert_eq!(actual_data, 123);

        RemitwiseEvents::emit_batch(&env, EventCategory::Alert, symbol_short!("batch-test"), 55);
        let batch_event = env.events().all().last().unwrap();

        let batch_topics = &batch_event.1;
        let alert_category: u32 = u32::try_from_val(&env, &batch_topics.get(1).unwrap()).unwrap();
        let alert_priority: u32 = u32::try_from_val(&env, &batch_topics.get(2).unwrap()).unwrap();

        assert_eq!(alert_category, EventCategory::Alert.to_u32());
        assert_eq!(alert_priority, EventPriority::Low.to_u32());

        let payload: (Symbol, u32) = FromVal::from_val(&env, &batch_event.2);
        assert_eq!(payload.0, symbol_short!("batch-test"));
        assert_eq!(payload.1, 55);
    }
}
