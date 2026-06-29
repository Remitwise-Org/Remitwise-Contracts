use crate::{EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{symbol_short, Env, IntoVal, Symbol, TryFromVal, Val, Vec};

/// Pins the Remitwise event topic ABI:
/// `(symbol_short!("Remitwise"), category_u32, priority_u32, action)`.
fn assert_topic_tuple(
    env: &Env,
    topics: &Vec<Val>,
    category: EventCategory,
    priority: EventPriority,
    action: Symbol,
) {
    assert_eq!(topics.len(), 4);
    assert_eq!(
        topic_at(topics, 0),
        symbol_short!("Remitwise").into_val(env)
    );
    assert_eq!(topic_at(topics, 1), category.to_u32().into_val(env));
    assert_eq!(topic_at(topics, 2), priority.to_u32().into_val(env));
    assert_eq!(topic_at(topics, 3), action.into_val(env));
}

fn topic_at(topics: &Vec<Val>, index: u32) -> Val {
    match topics.get(index) {
        Some(value) => value,
        None => panic!("expected topic at index {}", index),
    }
}

fn event_topics_and_data_at(env: &Env, index: u32) -> (Vec<Val>, Val) {
    match env.events().all().get(index) {
        Some((_contract_id, topics, data)) => (topics, data),
        None => panic!("expected event at index {}", index),
    }
}

fn decode_u32(env: &Env, value: &Val) -> u32 {
    match u32::try_from_val(env, value) {
        Ok(decoded) => decoded,
        Err(error) => panic!("expected u32 payload: {:?}", error),
    }
}

fn decode_batch_payload(env: &Env, value: &Val) -> (Symbol, u32) {
    match <(Symbol, u32)>::try_from_val(env, value) {
        Ok(decoded) => decoded,
        Err(error) => panic!("expected batch payload: {:?}", error),
    }
}

#[test]
fn test_compact_event_passes() {
    let env = Env::default();
    // A small payload
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
    // A very large payload
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
fn event_category_discriminants_are_stable() {
    assert_eq!(EventCategory::Transaction.to_u32(), 0);
    assert_eq!(EventCategory::State.to_u32(), 1);
    assert_eq!(EventCategory::Alert.to_u32(), 2);
    assert_eq!(EventCategory::System.to_u32(), 3);
    assert_eq!(EventCategory::Access.to_u32(), 4);
}

#[test]
fn event_priority_discriminants_are_stable() {
    assert_eq!(EventPriority::Low.to_u32(), 0);
    assert_eq!(EventPriority::Medium.to_u32(), 1);
    assert_eq!(EventPriority::High.to_u32(), 2);
}

#[test]
fn emit_publishes_standard_topic_tuple_and_payload() {
    let env = Env::default();
    let action = symbol_short!("paid");

    RemitwiseEvents::emit(
        &env,
        EventCategory::Transaction,
        EventPriority::High,
        action,
        42u32,
    );

    let all = env.events().all();
    assert_eq!(all.len(), 1);

    let (topics, data) = event_topics_and_data_at(&env, 0);
    assert_topic_tuple(
        &env,
        &topics,
        EventCategory::Transaction,
        EventPriority::High,
        action,
    );

    let payload = decode_u32(&env, &data);
    assert_eq!(payload, 42);
}

#[test]
fn emit_topics_cover_representative_category_priority_pairs() {
    let env = Env::default();
    let cases = [
        (
            EventCategory::Transaction,
            EventPriority::Low,
            symbol_short!("txn"),
        ),
        (
            EventCategory::State,
            EventPriority::Medium,
            symbol_short!("state"),
        ),
        (
            EventCategory::Alert,
            EventPriority::High,
            symbol_short!("alert"),
        ),
        (
            EventCategory::System,
            EventPriority::High,
            symbol_short!("system"),
        ),
        (
            EventCategory::Access,
            EventPriority::Low,
            symbol_short!("access"),
        ),
    ];

    for (idx, (category, priority, action)) in cases.iter().copied().enumerate() {
        RemitwiseEvents::emit(&env, category, priority, action, idx as u32);
    }

    let all = env.events().all();
    assert_eq!(all.len(), cases.len() as u32);

    for (idx, (category, priority, action)) in cases.iter().copied().enumerate() {
        let (topics, data) = event_topics_and_data_at(&env, idx as u32);
        assert_topic_tuple(&env, &topics, category, priority, action);
        let payload = decode_u32(&env, &data);
        assert_eq!(payload, idx as u32);
    }
}

#[test]
fn emit_batch_publishes_batch_topic_and_count_payload() {
    let env = Env::default();
    let action = symbol_short!("sync");

    RemitwiseEvents::emit_batch(&env, EventCategory::System, action, 3);

    let all = env.events().all();
    assert_eq!(all.len(), 1);

    let (topics, data) = event_topics_and_data_at(&env, 0);
    assert_topic_tuple(
        &env,
        &topics,
        EventCategory::System,
        EventPriority::Low,
        symbol_short!("batch"),
    );

    let payload = decode_batch_payload(&env, &data);
    assert_eq!(payload, (action, 3));
}

#[test]
fn emit_batch_keeps_empty_batch_count_visible() {
    let env = Env::default();
    let action = symbol_short!("noop");

    RemitwiseEvents::emit_batch(&env, EventCategory::Alert, action, 0);

    let all = env.events().all();
    assert_eq!(all.len(), 1);

    let (topics, data) = event_topics_and_data_at(&env, 0);
    assert_topic_tuple(
        &env,
        &topics,
        EventCategory::Alert,
        EventPriority::Low,
        symbol_short!("batch"),
    );

    let payload = decode_batch_payload(&env, &data);
    assert_eq!(payload, (action, 0));
}
