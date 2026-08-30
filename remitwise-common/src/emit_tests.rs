use crate::{EventCategory, EventPriority, RemitwiseEvents};
use soroban_sdk::{symbol_short, Env, IntoVal, Val, Vec};

#[test]
fn test_compact_event_passes() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EventHarness);
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
    let contract_id = env.register_contract(None, EventHarness);
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
