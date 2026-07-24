use crate::{EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{symbol_short, Env, Vec};

#[test]
fn test_compact_event_passes() {
    let env = Env::default();
    let data = 42u32;
    RemitwiseEvents::emit(
        &env,
        EventCategory::Transaction,
        EventPriority::High,
        symbol_short!("test"),
        data,
    );
}

#[test]
#[should_panic(expected = "exceeds 256-byte budget")]
fn test_oversized_event_flagged() {
    let env = Env::default();
    let mut large_data = Vec::<u32>::new(&env);
    for i in 0..100 {
        large_data.push_back(i);
    }
    RemitwiseEvents::emit(
        &env,
        EventCategory::Transaction,
        EventPriority::High,
        symbol_short!("test"),
        large_data,
    );
}

#[test]
fn test_emit_all_categories_are_distinct() {
    assert_ne!(EventCategory::Transaction.to_u32(), EventCategory::Alert.to_u32());
    assert_ne!(EventCategory::Alert.to_u32(), EventCategory::System.to_u32());
    assert_ne!(EventCategory::Transaction.to_u32(), EventCategory::System.to_u32());
    assert_ne!(EventCategory::System.to_u32(), EventCategory::Access.to_u32());
    assert_ne!(EventCategory::Transaction.to_u32(), EventCategory::Access.to_u32());
}

#[test]
fn test_emit_all_priorities_are_distinct() {
    assert_ne!(EventPriority::Low.to_u32(), EventPriority::High.to_u32());
    assert_ne!(EventPriority::High.to_u32(), EventPriority::Medium.to_u32());
    assert_ne!(EventPriority::Low.to_u32(), EventPriority::Medium.to_u32());
}
