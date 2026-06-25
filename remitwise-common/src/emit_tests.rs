use soroban_sdk::{Env, Val, IntoVal, symbol_short, Vec};
use crate::{RemitwiseEvents, EventCategory, EventPriority};

#[test]
fn test_compact_event_passes() {
    let env = Env::default();
    // A small payload
    let data = 42u32;
    RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::High, symbol_short!("test"), data);
}

#[test]
#[should_panic(expected = "exceeds the 256-byte budget")]
fn test_oversized_event_flagged() {
    let env = Env::default();
    // A very large payload
    let mut large_data = Vec::<u32>::new(&env);
    for i in 0..100 {
        large_data.push_back(i);
    }
    RemitwiseEvents::emit(&env, EventCategory::Transaction, EventPriority::High, symbol_short!("test"), large_data);
}
