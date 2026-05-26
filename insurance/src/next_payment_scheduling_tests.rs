#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    Address, Env, String, TryFromVal, Val, Vec as SorobanVec,
};
use std::vec::Vec as StdVec;

const PERIOD: u64 = 30 * 86_400;

fn setup_env() -> (Env, InsuranceClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    (env, client)
}

fn create_policy_at(env: &Env, client: &InsuranceClient, owner: &Address, t: u64) -> u32 {
    env.ledger().with_mut(|li| li.timestamp = t);
    client.create_policy(
        owner,
        &String::from_str(env, "Policy"),
        &CoverageType::Health,
        &1_000,
        &10_000,
        &None,
    )
}

fn paid_events_for(env: &Env) -> SorobanVec<(Address, SorobanVec<Val>, Val)> {
    let mut out = SorobanVec::new(env);
    for event in env.events().all().iter() {
        let topics = &event.1;
        if topics.len() < 4 {
            continue;
        }
        let ns = soroban_sdk::Symbol::try_from_val(env, &topics.get(0).unwrap());
        let action = soroban_sdk::Symbol::try_from_val(env, &topics.get(3).unwrap());
        if ns.is_ok()
            && action.is_ok()
            && ns.unwrap() == symbol_short!("Remitwise")
            && action.unwrap() == EVT_PREMIUM_PAID
        {
            out.push_back(event);
        }
    }
    out
}

#[test]
fn test_pay_premium_on_time_advances_one_period() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let created_at = 1_000_000u64;
    let id = create_policy_at(&env, &client, &owner, created_at);
    let due = client.get_policy(&id).unwrap().next_payment_date;

    env.ledger().with_mut(|li| li.timestamp = due);
    assert!(client.pay_premium(&owner, &id));

    let p = client.get_policy(&id).unwrap();
    assert_eq!(p.next_payment_date, due + PERIOD);
}

#[test]
fn test_pay_premium_early_keeps_cadence_anchored_to_due_date() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id = create_policy_at(&env, &client, &owner, 1_000_000u64);
    let due = client.get_policy(&id).unwrap().next_payment_date;

    env.ledger().with_mut(|li| li.timestamp = due - 10);
    assert!(client.pay_premium(&owner, &id));

    let p = client.get_policy(&id).unwrap();
    assert_eq!(p.next_payment_date, due + PERIOD);
}

#[test]
fn test_pay_premium_late_moves_due_to_future_date() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id = create_policy_at(&env, &client, &owner, 1_000_000u64);
    let due = client.get_policy(&id).unwrap().next_payment_date;

    // 95 days late: should skip enough 30-day periods so new due is in the future.
    let now = due + (95 * 86_400);
    env.ledger().with_mut(|li| li.timestamp = now);
    assert!(client.pay_premium(&owner, &id));

    let p = client.get_policy(&id).unwrap();
    assert!(p.next_payment_date > now);
    assert_eq!(p.next_payment_date, due + (4 * PERIOD));
}

#[test]
fn test_batch_pay_premiums_advances_each_policy_independently_and_counts() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id_a = create_policy_at(&env, &client, &owner, 1_000_000u64);
    let id_b = create_policy_at(&env, &client, &owner, 1_300_000u64);
    let id_c = create_policy_at(&env, &client, &owner, 1_600_000u64);

    let p_a = client.get_policy(&id_a).unwrap();
    let p_b = client.get_policy(&id_b).unwrap();
    let p_c = client.get_policy(&id_c).unwrap();

    let now = p_a.next_payment_date + (65 * 86_400);
    env.ledger().with_mut(|li| li.timestamp = now);

    client.deactivate_policy(&owner, &id_c); // should be skipped

    let ids = soroban_sdk::vec![&env, id_a, id_b, id_c, 999u32];
    let advanced = client.batch_pay_premiums(&owner, &ids);
    assert_eq!(advanced, 2);

    let updated_a = client.get_policy(&id_a).unwrap();
    let updated_b = client.get_policy(&id_b).unwrap();
    let updated_c = client.get_policy(&id_c).unwrap();

    assert!(updated_a.next_payment_date > now);
    assert!(updated_b.next_payment_date > now);
    assert_eq!(
        updated_a.next_payment_date,
        p_a.next_payment_date + (3 * PERIOD)
    );
    assert_eq!(
        updated_b.next_payment_date,
        p_b.next_payment_date + (3 * PERIOD)
    );
    assert_eq!(updated_c.next_payment_date, p_c.next_payment_date);
}

#[test]
fn test_premium_paid_event_next_payment_date_matches_stored_value() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id = create_policy_at(&env, &client, &owner, 2_000_000u64);
    let due = client.get_policy(&id).unwrap().next_payment_date;

    env.ledger().with_mut(|li| li.timestamp = due + 1);
    assert!(client.pay_premium(&owner, &id));

    let stored = client.get_policy(&id).unwrap().next_payment_date;
    let events = paid_events_for(&env);
    assert_eq!(events.len(), 1);

    let event: PremiumPaidEvent =
        PremiumPaidEvent::try_from_val(&env, &events.get(0).unwrap().2).unwrap();
    assert_eq!(event.next_payment_date, stored);
    assert!(event.next_payment_date > due + 1);
}

#[test]
fn test_batch_event_next_payment_dates_match_each_policy_value() {
    let (env, client) = setup_env();
    let owner = Address::generate(&env);

    let id1 = create_policy_at(&env, &client, &owner, 1_000_000u64);
    let id2 = create_policy_at(&env, &client, &owner, 1_250_000u64);

    let due1 = client.get_policy(&id1).unwrap().next_payment_date;
    let due2 = client.get_policy(&id2).unwrap().next_payment_date;

    let now = due1 + 40 * 86_400;
    env.ledger().with_mut(|li| li.timestamp = now);

    let ids = soroban_sdk::vec![&env, id1, id2];
    assert_eq!(client.batch_pay_premiums(&owner, &ids), 2);

    let p1 = client.get_policy(&id1).unwrap();
    let p2 = client.get_policy(&id2).unwrap();

    let mut by_id: StdVec<(u32, u64)> = StdVec::new();
    let events = paid_events_for(&env);
    assert_eq!(events.len(), 2);
    for e in events.iter() {
        let data: PremiumPaidEvent = PremiumPaidEvent::try_from_val(&env, &e.2).unwrap();
        by_id.push((data.policy_id, data.next_payment_date));
    }
    by_id.sort_by_key(|(id, _)| *id);

    assert_eq!(p1.next_payment_date, due1 + (2 * PERIOD));
    assert_eq!(p2.next_payment_date, due2 + (2 * PERIOD));
    assert_eq!(by_id[0], (id1, p1.next_payment_date));
    assert_eq!(by_id[1], (id2, p2.next_payment_date));
}
