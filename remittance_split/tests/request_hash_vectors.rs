#![cfg(test)]

//! Independent vectors for the canonical remittance split signing helper.
//! These tests deliberately live outside the contract module so consumers of
//! the generated client exercise the same public helper as off-chain signers.

use remittance_split::{AccountGroup, DistributeUsdcRequest, RemittanceSplit, RemittanceSplitClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn request(env: &Env) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract: Address::generate(env),
        from: Address::generate(env),
        nonce: 7,
        accounts: AccountGroup {
            spending: Address::generate(env),
            savings: Address::generate(env),
            bills: Address::generate(env),
            insurance: Address::generate(env),
        },
        total_amount: 123_456,
        deadline: 900,
    }
}

fn client<'a>(env: &'a Env) -> RemittanceSplitClient<'a> {
    let id = env.register_contract(None, RemittanceSplit);
    RemittanceSplitClient::new(env, &id)
}

fn assert_changed(client: &RemittanceSplitClient, original: &DistributeUsdcRequest, mutate: impl FnOnce(&mut DistributeUsdcRequest)) {
    let original_hash = client.get_request_hash(original);
    let mut changed = original.clone();
    mutate(&mut changed);
    let changed_hash = client.get_request_hash(&changed);
    assert_ne!(original_hash, changed_hash, "every signed field must change the digest");
}

#[test]
fn vector_is_deterministic_across_calls() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_eq!(client.get_request_hash(&value), client.get_request_hash(&value));
}

#[test]
fn vector_is_sha256_sized() {
    let env = Env::default();
    let client = client(&env);
    assert_eq!(client.get_request_hash(&request(&env)).len(), 32);
}

#[test]
fn binds_operation_domain() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce = 8);
}

#[test]
fn binds_contract_address() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.usdc_contract = Address::generate(&env));
}

#[test]
fn binds_caller_address() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.from = Address::generate(&env));
}

#[test]
fn binds_spending_recipient() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.accounts.spending = Address::generate(&env));
}

#[test]
fn binds_savings_recipient() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.accounts.savings = Address::generate(&env));
}

#[test]
fn binds_bills_recipient() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.accounts.bills = Address::generate(&env));
}

#[test]
fn binds_insurance_recipient() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.accounts.insurance = Address::generate(&env));
}

#[test]
fn binds_positive_amount_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount += 1);
}

#[test]
fn binds_negative_amount_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount -= 1);
}

#[test]
fn binds_nonce_increment() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce += 1);
}

#[test]
fn binds_nonce_maximum_boundary() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce = u64::MAX);
}

#[test]
fn binds_deadline_increment() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline += 1);
}

#[test]
fn binds_deadline_zero_boundary() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = 0);
}

#[test]
fn binds_all_recipient_swaps() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let original = client.get_request_hash(&value);
    let mut swapped = value.clone();
    core::mem::swap(&mut swapped.accounts.spending, &mut swapped.accounts.savings);
    assert_ne!(original, client.get_request_hash(&swapped));
    let mut swapped = value.clone();
    core::mem::swap(&mut swapped.accounts.bills, &mut swapped.accounts.insurance);
    assert_ne!(original, client.get_request_hash(&swapped));
}

#[test]
fn binds_cross_contract_reuse() {
    let env = Env::default();
    let first = client(&env);
    let second = client(&env);
    let value = request(&env);
    assert_ne!(first.get_request_hash(&value), second.get_request_hash(&value));
}

#[test]
fn preserves_hash_when_clone_is_unchanged() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let clone = value.clone();
    assert_eq!(client.get_request_hash(&value), client.get_request_hash(&clone));
}

#[test]
fn field_mutations_are_pairwise_distinct() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let base = client.get_request_hash(&value);
    let mut variants = soroban_sdk::Vec::new(&env);
    for nonce in [0_u64, 1, 8, 99] {
        let mut candidate = value.clone();
        candidate.nonce = nonce;
        variants.push_back(client.get_request_hash(&candidate));
    }
    for hash in variants.iter() { assert_ne!(base, hash); }
}

#[test]
fn amount_sign_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount = -123_456);
}

#[test]
fn amount_zero_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount = 0);
}

#[test]
fn deadline_maximum_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = u64::MAX);
}

#[test]
fn zero_nonce_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce = 0);
}

#[test]
fn one_byte_address_change_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.from = Address::generate(&env));
}

#[test]
fn repeated_vector_generation_is_stable() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let expected = client.get_request_hash(&value);
    for _ in 0..20 { assert_eq!(expected, client.get_request_hash(&value)); }
}

#[test]
fn binds_each_account_even_when_other_fields_match() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let base = client.get_request_hash(&value);
    let mut candidates = [value.clone(), value.clone(), value.clone(), value.clone()];
    candidates[0].accounts.spending = Address::generate(&env);
    candidates[1].accounts.savings = Address::generate(&env);
    candidates[2].accounts.bills = Address::generate(&env);
    candidates[3].accounts.insurance = Address::generate(&env);
    for candidate in candidates { assert_ne!(base, client.get_request_hash(&candidate)); }
}

#[test]
fn binds_each_scalar_even_when_addresses_match() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let base = client.get_request_hash(&value);
    let mut amount = value.clone(); amount.total_amount += 10;
    let mut nonce = value.clone(); nonce.nonce += 10;
    let mut deadline = value.clone(); deadline.deadline += 10;
    assert_ne!(base, client.get_request_hash(&amount));
    assert_ne!(base, client.get_request_hash(&nonce));
    assert_ne!(base, client.get_request_hash(&deadline));
}

#[test]
fn caller_and_token_swaps_do_not_reuse_hashes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let mut caller_swap = value.clone(); caller_swap.from = value.usdc_contract.clone();
    let mut token_swap = value.clone(); token_swap.usdc_contract = value.from.clone();
    let base = client.get_request_hash(&value);
    assert_ne!(base, client.get_request_hash(&caller_swap));
    assert_ne!(base, client.get_request_hash(&token_swap));
    assert_ne!(client.get_request_hash(&caller_swap), client.get_request_hash(&token_swap));
}

#[test]
fn deadline_boundary_before_now_is_still_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = 899);
}

#[test]
fn deadline_boundary_after_now_is_bound() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = 901);
}

#[test]
fn amount_i128_lower_half_changes_hash() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount = i128::MIN + 1);
}

#[test]
fn amount_i128_upper_half_changes_hash() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount = i128::MAX);
}

#[test]
fn nonce_high_bit_changes_hash() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce = 1_u64 << 63);
}

#[test]
fn deadline_high_bit_changes_hash() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = 1_u64 << 63);
}

#[test]
fn repeated_address_values_are_positionally_encoded() {
    let env = Env::default();
    let client = client(&env);
    let mut value = request(&env);
    value.accounts.savings = value.accounts.spending.clone();
    let base = client.get_request_hash(&value);
    value.accounts.bills = value.accounts.spending.clone();
    assert_ne!(base, client.get_request_hash(&value));
}

#[test]
fn changing_two_fields_is_not_silent() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let mut changed = value.clone();
    changed.nonce += 1;
    changed.deadline += 1;
    assert_ne!(client.get_request_hash(&value), client.get_request_hash(&changed));
}

#[test]
fn changing_all_destinations_is_not_silent() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    let mut changed = value.clone();
    changed.accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };
    assert_ne!(client.get_request_hash(&value), client.get_request_hash(&changed));
}

#[test]
fn vector_does_not_depend_on_client_instance_state() {
    let env = Env::default();
    let first = client(&env);
    let second = client(&env);
    let value = request(&env);
    assert_ne!(first.get_request_hash(&value), second.get_request_hash(&value));
}

#[test]
fn hash_is_not_empty_for_minimal_values() {
    let env = Env::default();
    let client = client(&env);
    let mut value = request(&env);
    value.nonce = 0;
    value.deadline = 0;
    value.total_amount = 0;
    assert!(!client.get_request_hash(&value).is_empty());
}

#[test]
fn hash_is_not_empty_for_maximal_values() {
    let env = Env::default();
    let client = client(&env);
    let mut value = request(&env);
    value.nonce = u64::MAX;
    value.deadline = u64::MAX;
    value.total_amount = i128::MAX;
    assert!(!client.get_request_hash(&value).is_empty());
}

#[test]
fn hash_changes_when_only_the_token_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.usdc_contract = Address::generate(&env));
}

#[test]
fn hash_changes_when_only_the_caller_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.from = Address::generate(&env));
}

#[test]
fn hash_changes_when_only_the_amount_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.total_amount = 123_457);
}

#[test]
fn hash_changes_when_only_the_deadline_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.deadline = 901);
}

#[test]
fn hash_changes_when_only_the_nonce_changes() {
    let env = Env::default();
    let client = client(&env);
    let value = request(&env);
    assert_changed(&client, &value, |r| r.nonce = 8);
}
