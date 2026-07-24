use crate::{EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{
    contract, contractimpl, symbol_short, testutils::Events as _, Env, FromVal, Vec,
};

// Events published outside a contract context are dropped by the host
// (soroban-sdk 21), so tests publish through a minimal registered contract.
#[contract]
struct DummyContract;

#[contractimpl]
impl DummyContract {
    pub fn noop(_env: Env) {}
}

fn in_contract_context(env: &Env, f: impl FnOnce()) {
    let id = env.register_contract(None, DummyContract);
    env.as_contract(&id, f);
}

#[test]
fn test_compact_event_passes() {
    let env = Env::default();
    // A small payload
    let data = 42u32;
    in_contract_context(&env, || {
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("test"),
            data,
        );
    });
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
    in_contract_context(&env, || {
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("test"),
            large_data,
        );
    });
}

// ============================================================================
// Taxonomy tests — topic-and-priority for each EventCategory / EventPriority
// combo that RemitwiseEvents::emit should stamp (#1035)
// ============================================================================

#[test]
fn test_emit_topics_include_remitwise_sentinel() {
    let env = Env::default();
    in_contract_context(&env, || {
        RemitwiseEvents::emit(
            &env,
            EventCategory::Transaction,
            EventPriority::High,
            symbol_short!("tx"),
            1u32,
        );
    });
    let events = env.events().all();
    assert!(!events.is_empty());
    // The first topic element must be the Remitwise sentinel symbol.
    let (_, topics, _) = events.last().unwrap();
    let sentinel = soroban_sdk::Val::from_val(&env, &topics.get(0).unwrap());
    let expected = soroban_sdk::Symbol::new(&env, "Remitwise").to_val();
    assert_eq!(sentinel.get_payload(), expected.get_payload());
}

#[test]
fn test_emit_encodes_category_as_second_topic() {
    let env = Env::default();
    in_contract_context(&env, || {
        RemitwiseEvents::emit(
            &env,
            EventCategory::Access,
            EventPriority::Low,
            symbol_short!("kyc"),
            0u32,
        );
    });
    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let cat_raw: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(1).unwrap());
    assert_eq!(cat_raw, EventCategory::Access.to_u32());
}

#[test]
fn test_emit_encodes_priority_as_third_topic() {
    let env = Env::default();
    in_contract_context(&env, || {
        RemitwiseEvents::emit(
            &env,
            EventCategory::System,
            EventPriority::Medium,
            symbol_short!("alert"),
            99u32,
        );
    });
    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let prio_raw: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(2).unwrap());
    assert_eq!(prio_raw, EventPriority::Medium.to_u32());
}

#[test]
fn test_emit_batch_uses_low_priority_topic() {
    let env = Env::default();
    in_contract_context(&env, || {
        RemitwiseEvents::emit_batch(&env, EventCategory::Transaction, symbol_short!("batch"), 5);
    });
    let events = env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let prio_raw: u32 = soroban_sdk::FromVal::from_val(&env, &topics.get(2).unwrap());
    assert_eq!(prio_raw, EventPriority::Low.to_u32());
}

#[test]
fn test_emit_all_categories_are_distinct() {
    assert_ne!(
        EventCategory::Transaction.to_u32(),
        EventCategory::Access.to_u32()
    );
    assert_ne!(
        EventCategory::Access.to_u32(),
        EventCategory::System.to_u32()
    );
    assert_ne!(
        EventCategory::Transaction.to_u32(),
        EventCategory::System.to_u32()
    );
}

#[test]
fn test_emit_all_priorities_are_distinct() {
    assert_ne!(EventPriority::Low.to_u32(), EventPriority::High.to_u32());
    assert_ne!(EventPriority::High.to_u32(), EventPriority::Medium.to_u32());
    assert_ne!(EventPriority::Low.to_u32(), EventPriority::Medium.to_u32());
}
