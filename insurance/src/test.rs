extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    vec, Env,
};

fn setup_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    env
}

fn create_test_policy(client: &InsuranceClient, env: &Env, owner: &Address) -> u32 {
    client.create_policy(
        owner,
        &String::from_str(env, "TestPolicy"),
        &CoverageType::Health,
        &100i128,
        &10_000i128,
        &None,
    )
}

// ============================================================================
// get_active_policies — Pagination Semantics
// ============================================================================

#[test]
fn test_get_active_policies_empty() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let page = client.get_active_policies(&owner, &0, &20);
    assert_eq!(page.items.len(), 0);
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.count, 0);
}

#[test]
fn test_get_active_policies_single_policy() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);

    let page = client.get_active_policies(&owner, &0, &20);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().id, id);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_get_active_policies_excludes_inactive() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    let id3 = create_test_policy(&client, &env, &owner);

    client.deactivate_policy(&owner, &id2);

    let page = client.get_active_policies(&owner, &0, &50);
    assert_eq!(page.count, 2);
    assert_eq!(page.items.get(0).unwrap().id, id1);
    assert_eq!(page.items.get(1).unwrap().id, id3);
}

#[test]
fn test_get_active_policies_all_inactive_returns_empty() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    client.deactivate_policy(&owner, &id1);
    client.deactivate_policy(&owner, &id2);

    let page = client.get_active_policies(&owner, &0, &50);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_get_active_policies_id_ascending_order() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    let id3 = create_test_policy(&client, &env, &owner);

    let page = client.get_active_policies(&owner, &0, &50);
    let ids: std::vec::Vec<u32> = page.items.iter().map(|p| p.id).collect();
    assert_eq!(ids, std::vec![id1, id2, id3]);
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn test_get_active_policies_cursor_progression() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let mut expected_ids = std::vec::Vec::new();
    for _ in 0..5 {
        expected_ids.push(create_test_policy(&client, &env, &owner));
    }

    let mut collected = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &2);
        assert!(page.count <= 2);
        for p in page.items.iter() {
            collected.push(p.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        assert!(page.next_cursor > cursor);
        cursor = page.next_cursor;
    }

    assert_eq!(collected, expected_ids);
}

#[test]
fn test_get_active_policies_cursor_skips_inactive_gap() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    let id3 = create_test_policy(&client, &env, &owner);
    let id4 = create_test_policy(&client, &env, &owner);

    client.deactivate_policy(&owner, &id2);
    client.deactivate_policy(&owner, &id3);

    // Start after id1 — must skip inactive id2/id3 and return id4
    let page = client.get_active_policies(&owner, &id1, &1);
    assert_eq!(page.count, 1);
    assert_eq!(page.items.get(0).unwrap().id, id4);
}

#[test]
fn test_get_active_policies_cursor_past_last_returns_empty() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);

    let page = client.get_active_policies(&owner, &(id + 100), &20);
    assert_eq!(page.count, 0);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_next_cursor_equals_last_returned_id_on_full_page() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..5 {
        create_test_policy(&client, &env, &owner);
    }

    let page = client.get_active_policies(&owner, &0, &3);
    assert_eq!(page.count, 3);
    let last_id = page.items.get(page.count - 1).unwrap().id;
    assert_eq!(page.next_cursor, last_id);
}

#[test]
fn test_next_cursor_zero_on_final_page() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..3 {
        create_test_policy(&client, &env, &owner);
    }

    let page = client.get_active_policies(&owner, &0, &50);
    assert_eq!(page.count, 3);
    assert_eq!(page.next_cursor, 0);
}

#[test]
fn test_count_matches_items_len() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..7 {
        create_test_policy(&client, &env, &owner);
    }

    let mut cursor = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &3);
        assert_eq!(page.count, page.items.len());
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
}

#[test]
fn test_pagination_no_duplicates() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..10 {
        create_test_policy(&client, &env, &owner);
    }

    let mut all_ids = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_active_policies(&owner, &cursor, &3);
        for p in page.items.iter() {
            all_ids.push(p.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(all_ids.len(), 10);
    let mut deduped = all_ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(deduped.len(), all_ids.len());
}

// ============================================================================
// Limit Clamping
// ============================================================================

#[test]
fn test_limit_zero_uses_default() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..(DEFAULT_PAGE_LIMIT + 5) {
        create_test_policy(&client, &env, &owner);
    }

    let page = client.get_active_policies(&owner, &0, &0);
    assert_eq!(page.count, DEFAULT_PAGE_LIMIT);
    assert!(page.next_cursor > 0);
}

#[test]
fn test_limit_above_max_clamped() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..(MAX_PAGE_LIMIT + 5) {
        create_test_policy(&client, &env, &owner);
    }

    let page = client.get_active_policies(&owner, &0, &1000);
    assert_eq!(page.count, MAX_PAGE_LIMIT);
    assert!(page.next_cursor > 0);
}

#[test]
fn test_limit_within_bounds_respected() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    for _ in 0..10 {
        create_test_policy(&client, &env, &owner);
    }

    let page = client.get_active_policies(&owner, &0, &7);
    assert_eq!(page.count, 7);
}

// ============================================================================
// Owner Isolation
// ============================================================================

#[test]
fn test_owner_isolation() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    for _ in 0..3 {
        create_test_policy(&client, &env, &alice);
    }
    for _ in 0..2 {
        create_test_policy(&client, &env, &bob);
    }

    let alice_page = client.get_active_policies(&alice, &0, &50);
    let bob_page = client.get_active_policies(&bob, &0, &50);

    assert_eq!(alice_page.count, 3);
    assert_eq!(bob_page.count, 2);

    for p in alice_page.items.iter() {
        assert_eq!(p.owner, alice);
    }
    for p in bob_page.items.iter() {
        assert_eq!(p.owner, bob);
    }
}

// ============================================================================
// Core Contract Functions
// ============================================================================

#[test]
fn test_create_policy_returns_incrementing_ids() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    let id3 = create_test_policy(&client, &env, &owner);

    assert_eq!(id1, 1);
    assert_eq!(id2, 2);
    assert_eq!(id3, 3);
}

#[test]
fn test_get_policy_returns_correct_data() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "HealthPlan"),
        &CoverageType::Life,
        &250i128,
        &50_000i128,
        &None,
    );

    let policy = client.get_policy(&id).unwrap();
    assert_eq!(policy.id, id);
    assert_eq!(policy.owner, owner);
    assert_eq!(policy.coverage_type, CoverageType::Life);
    assert_eq!(policy.monthly_premium, 250);
    assert_eq!(policy.coverage_amount, 50_000);
    assert!(policy.active);
    assert_eq!(policy.next_payment_date, 1_700_000_000 + 30 * 86_400);
}

#[test]
fn test_get_policy_nonexistent_returns_none() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);

    assert!(client.get_policy(&999).is_none());
}

#[test]
fn test_deactivate_policy() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);
    assert!(client.deactivate_policy(&owner, &id));
    assert!(!client.get_policy(&id).unwrap().active);
}

#[test]
fn test_deactivate_wrong_owner_fails() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);
    assert!(!client.deactivate_policy(&other, &id));
    assert!(client.get_policy(&id).unwrap().active);
}

#[test]
fn test_deactivate_nonexistent_returns_false() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    assert!(!client.deactivate_policy(&owner, &999));
}

#[test]
fn test_pay_premium_updates_next_payment_date() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);
    assert!(client.pay_premium(&owner, &id));

    let policy = client.get_policy(&id).unwrap();
    assert_eq!(policy.next_payment_date, 1_700_000_000 + 30 * 86_400);
}

#[test]
fn test_pay_premium_inactive_fails() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);
    client.deactivate_policy(&owner, &id);
    assert!(!client.pay_premium(&owner, &id));
}

#[test]
fn test_pay_premium_wrong_owner_fails() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let id = create_test_policy(&client, &env, &owner);
    assert!(!client.pay_premium(&other, &id));
}

#[test]
fn test_batch_pay_premiums_skips_inactive() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let id1 = create_test_policy(&client, &env, &owner);
    let id2 = create_test_policy(&client, &env, &owner);
    let id3 = create_test_policy(&client, &env, &owner);
    client.deactivate_policy(&owner, &id2);

    let ids = vec![&env, id1, id2, id3];
    assert_eq!(client.batch_pay_premiums(&owner, &ids), 2);
}

#[test]
fn test_get_total_monthly_premium_active_only() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    client.create_policy(
        &owner,
        &String::from_str(&env, "P1"),
        &CoverageType::Health,
        &100i128,
        &10_000i128,
        &None,
    );
    let id2 = client.create_policy(
        &owner,
        &String::from_str(&env, "P2"),
        &CoverageType::Life,
        &200i128,
        &20_000i128,
        &None,
    );
    client.create_policy(
        &owner,
        &String::from_str(&env, "P3"),
        &CoverageType::Property,
        &300i128,
        &30_000i128,
        &None,
    );

    client.deactivate_policy(&owner, &id2);
    assert_eq!(client.get_total_monthly_premium(&owner), 400);
}

#[test]
fn test_get_total_monthly_premium_no_policies() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    assert_eq!(client.get_total_monthly_premium(&owner), 0);
}

#[test]
fn test_set_pause_admin() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let caller = Address::generate(&env);
    let new_admin = Address::generate(&env);

    assert!(client.set_pause_admin(&caller, &new_admin));
}

#[test]
fn test_create_policy_with_external_ref() {
    let env = setup_env();
    let contract_id = env.register_contract(None, Insurance);
    let client = InsuranceClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let ext_ref = Some(String::from_str(&env, "EXT-001"));
    let id = client.create_policy(
        &owner,
        &String::from_str(&env, "RefPolicy"),
        &CoverageType::Auto,
        &150i128,
        &25_000i128,
        &ext_ref,
    );

    let policy = client.get_policy(&id).unwrap();
    assert!(policy.external_ref.is_some());
}
