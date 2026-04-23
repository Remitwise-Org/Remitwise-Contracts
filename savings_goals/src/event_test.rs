#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, String, Symbol, TryFromVal, Val, Vec as SorobanVec,
};

fn setup_test(env: &Env) -> (SavingsGoalContractClient, Address) {
    let contract_id = env.register_contract(None, SavingsGoalContract);
    let client = SavingsGoalContractClient::new(env, &contract_id);
    let user = Address::generate(env);
    env.mock_all_auths();
    client.init();
    (client, user)
}

fn last_event(env: &Env) -> (Address, SorobanVec<Val>, Val) {
    env.events().all().last().expect("No events emitted")
}

fn get_remitwise_events(env: &Env, action: Symbol) -> SorobanVec<(Address, SorobanVec<Val>, Val)> {
    let mut result = SorobanVec::new(env);
    let events = env.events().all();
    for event in events.iter() {
        if event.1.len() >= 4 {
            let ns: Symbol = Symbol::try_from_val(env, &event.1.get(0).unwrap()).unwrap();
            let act: Symbol = Symbol::try_from_val(env, &event.1.get(3).unwrap()).unwrap();
            if ns == symbol_short!("Remitwise") && act == action {
                result.push_back(event);
            }
        }
    }
    result
}

#[test]
fn test_goal_created_event_schema() {
    let env = Env::default();
    let (client, user) = setup_test(&env);

    let name = String::from_str(&env, "Test Goal");
    let target_amount = 1000i128;
    let target_date = 10000u64;
    let ts = 100u64;

    env.ledger().with_mut(|li| li.timestamp = ts);

    let id = client.create_goal(&user, &name, &target_amount, &target_date);

    let remitwise_events = get_remitwise_events(&env, GOAL_CREATED);
    assert_eq!(
        remitwise_events.len(),
        1,
        "Should emit exactly one Remitwise GOAL_CREATED event"
    );

    let event = remitwise_events.get(0).unwrap();

    // Topic Schema: [Remitwise, State, Medium, created]
    assert_eq!(
        event.1.get(0).unwrap(),
        symbol_short!("Remitwise").into_val(&env)
    );
    assert_eq!(
        event.1.get(1).unwrap(),
        (EventCategory::State as u32).into_val(&env)
    );
    assert_eq!(
        event.1.get(2).unwrap(),
        (EventPriority::Medium as u32).into_val(&env)
    );
    assert_eq!(event.1.get(3).unwrap(), GOAL_CREATED.into_val(&env));

    // Payload Schema: GoalCreatedEvent
    let data: GoalCreatedEvent = GoalCreatedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.goal_id, id);
    assert_eq!(data.owner, user);
    assert_eq!(data.name, name);
    assert_eq!(data.target_amount, target_amount);
    assert_eq!(data.target_date, target_date);
    assert_eq!(data.timestamp, ts);
}

#[test]
fn test_funds_added_event_schema() {
    let env = Env::default();
    let (client, user) = setup_test(&env);

    let id = client.create_goal(&user, &String::from_str(&env, "Add Test"), &1000, &10000);

    let amount = 500i128;
    let ts = 200u64;
    env.ledger().with_mut(|li| li.timestamp = ts);

    client.add_to_goal(&user, &id, &amount);

    let remitwise_events = get_remitwise_events(&env, symbol_short!("funds_add"));
    assert_eq!(remitwise_events.len(), 1);

    let event = remitwise_events.get(0).unwrap();

    // Topic Schema: [Remitwise, Transaction, Medium, funds_add]
    assert_eq!(
        event.1.get(1).unwrap(),
        (EventCategory::Transaction as u32).into_val(&env)
    );
    assert_eq!(
        event.1.get(3).unwrap(),
        symbol_short!("funds_add").into_val(&env)
    );

    // Payload Schema: FundsAddedEvent
    let data: FundsAddedEvent = FundsAddedEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.goal_id, id);
    assert_eq!(data.owner, user);
    assert_eq!(data.amount, amount);
    assert_eq!(data.new_total, amount);
    assert_eq!(data.timestamp, ts);
}

#[test]
fn test_funds_withdrawn_event_schema() {
    let env = Env::default();
    let (client, user) = setup_test(&env);

    let id = client.create_goal(
        &user,
        &String::from_str(&env, "Withdraw Test"),
        &1000,
        &10000,
    );
    client.unlock_goal(&user, &id);
    client.add_to_goal(&user, &id, &800);

    let withdraw_amount = 300i128;
    let ts = 300u64;
    env.ledger().with_mut(|li| li.timestamp = ts);

    client.withdraw_from_goal(&user, &id, &withdraw_amount);

    let remitwise_events = get_remitwise_events(&env, symbol_short!("funds_rem"));
    assert_eq!(remitwise_events.len(), 1);

    let event = remitwise_events.get(0).unwrap();

    // Topic Schema: [Remitwise, Transaction, Medium, funds_rem]
    assert_eq!(
        event.1.get(3).unwrap(),
        symbol_short!("funds_rem").into_val(&env)
    );

    // Payload Schema: FundsWithdrawnEvent
    let data: FundsWithdrawnEvent = FundsWithdrawnEvent::try_from_val(&env, &event.2).unwrap();
    assert_eq!(data.goal_id, id);
    assert_eq!(data.owner, user);
    assert_eq!(data.amount, withdraw_amount);
    assert_eq!(data.new_total, 500); // 800 - 300
    assert_eq!(data.timestamp, ts);
}

#[test]
fn test_goal_completed_event_schema() {
    let env = Env::default();
    let (client, user) = setup_test(&env);

    let name = String::from_str(&env, "Complete Test");
    let id = client.create_goal(&user, &name, &1000, &10000);

    let ts = 400u64;
    env.ledger().with_mut(|li| li.timestamp = ts);

    // Complete the goal
    client.add_to_goal(&user, &id, &1000);

    // GoalCompletedEvent is published with a single topic (GOAL_COMPLETED,)
    // It's not using RemitwiseEvents::emit for this specific one in lib.rs currently
    // Let's verify what it emits
    let events = env.events().all();
    let completed_event = events
        .iter()
        .find(|e| e.1.len() == 1 && e.1.get(0).unwrap() == GOAL_COMPLETED.into_val(&env))
        .expect("GoalCompletedEvent not found");

    let data: GoalCompletedEvent =
        GoalCompletedEvent::try_from_val(&env, &completed_event.2).unwrap();
    assert_eq!(data.goal_id, id);
    assert_eq!(data.owner, user);
    assert_eq!(data.name, name);
    assert_eq!(data.final_amount, 1000);
    assert_eq!(data.timestamp, ts);
}

#[test]
fn test_batch_add_to_goals_events() {
    let env = Env::default();
    let (client, user) = setup_test(&env);

    let id1 = client.create_goal(&user, &String::from_str(&env, "G1"), &1000, &10000);
    let id2 = client.create_goal(&user, &String::from_str(&env, "G2"), &1000, &10000);

    let contributions = SorobanVec::from_array(
        &env,
        [
            ContributionItem {
                goal_id: id1,
                amount: 200,
            },
            ContributionItem {
                goal_id: id2,
                amount: 300,
            },
        ],
    );

    let ts = 500u64;
    env.ledger().with_mut(|li| li.timestamp = ts);

    client.batch_add_to_goals(&user, &contributions);

    let add_events = get_remitwise_events(&env, symbol_short!("funds_add"));
    assert_eq!(add_events.len(), 2);

    let event1_data: FundsAddedEvent =
        FundsAddedEvent::try_from_val(&env, &add_events.get(0).unwrap().2).unwrap();
    assert_eq!(event1_data.goal_id, id1);
    assert_eq!(event1_data.amount, 200);
    assert_eq!(event1_data.owner, user);
    assert_eq!(event1_data.timestamp, ts);

    let event2_data: FundsAddedEvent =
        FundsAddedEvent::try_from_val(&env, &add_events.get(1).unwrap().2).unwrap();
    assert_eq!(event2_data.goal_id, id2);
    assert_eq!(event2_data.amount, 300);
    assert_eq!(event2_data.owner, user);
    assert_eq!(event2_data.timestamp, ts);
}
