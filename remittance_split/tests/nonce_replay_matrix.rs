#![cfg(test)]

//! Runtime nonce-policy matrix for signed remittance operations.
//!
//! The matrix is intentionally separate from the broad contract unit suite so
//! reviewers can see the security boundary in one place: authentication and
//! validation happen before nonce advancement, successful execution advances
//! exactly once, and a rejected request leaves the transfer state unchanged.

use remittance_split::{
    AccountGroup, DistributeUsdcRequest, RemittanceSplit, RemittanceSplitClient,
    RemittanceSplitError,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

struct Fixture<'a> {
    env: &'a Env,
    client: RemittanceSplitClient<'a>,
    owner: Address,
    token: Address,
    accounts: AccountGroup,
}

fn fixture<'a>(env: &'a Env) -> Fixture<'a> {
    env.mock_all_auths();
    let contract = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract);
    let owner = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let accounts = AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    };
    StellarAssetClient::new(env, &token).mint(&owner, &1_000_000);
    client.initialize_split(&owner, &0, &token, &5000, &3000, &1500, &500);
    Fixture {
        env,
        client,
        owner,
        token,
        accounts,
    }
}

fn signed_request(
    f: &Fixture<'_>,
    nonce: u64,
    deadline: u64,
    amount: i128,
) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract: f.token.clone(),
        from: f.owner.clone(),
        nonce,
        accounts: f.accounts.clone(),
        total_amount: amount,
        deadline,
    }
}

fn signed_hash(f: &Fixture<'_>, request: &DistributeUsdcRequest) -> soroban_sdk::Bytes {
    f.client.get_request_hash(request)
}

fn assert_rejected<T, E>(result: Result<T, E>) {
    assert!(
        result.is_err(),
        "invalid nonce policy input must be rejected"
    );
}

#[test]
fn initialization_consumes_nonce_zero_once() {
    let env = Env::default();
    let f = fixture(&env);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn first_signed_distribution_uses_current_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let result = f
        .client
        .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request));
    assert!(result.is_ok());
    assert_eq!(f.client.get_nonce(&f.owner), 2);
}

#[test]
fn duplicate_signed_distribution_is_rejected() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    assert!(f.client.try_distribute_usdc_hashed(&request, &hash).is_ok());
    assert_rejected(f.client.try_distribute_usdc_hashed(&request, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 2);
}

#[test]
fn stale_nonce_cannot_transfer_funds() {
    let env = Env::default();
    let f = fixture(&env);
    let before = TokenClient::new(&env, &f.token).balance(&f.owner);
    let request = signed_request(&f, 0, 100, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(TokenClient::new(&env, &f.token).balance(&f.owner), before);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn future_nonce_cannot_transfer_funds() {
    let env = Env::default();
    let f = fixture(&env);
    let before = TokenClient::new(&env, &f.token).balance(&f.owner);
    let request = signed_request(&f, 2, 100, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(TokenClient::new(&env, &f.token).balance(&f.owner), before);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn stale_deadline_is_rejected_before_nonce_advance() {
    let env = Env::default();
    let f = fixture(&env);
    env.ledger().set_timestamp(100);
    let request = signed_request(&f, 1, 100, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn past_deadline_is_rejected_before_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    env.ledger().set_timestamp(100);
    let request = signed_request(&f, 1, 99, 1000);
    let before = TokenClient::new(&env, &f.token).balance(&f.owner);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
    assert_eq!(TokenClient::new(&env, &f.token).balance(&f.owner), before);
}

#[test]
fn zero_deadline_is_rejected() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 0, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn excessive_future_deadline_is_rejected() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100_000_000, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn exact_valid_future_deadline_is_accepted() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    assert!(f
        .client
        .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request))
        .is_ok());
    assert_eq!(f.client.get_nonce(&f.owner), 2);
}

#[test]
fn wrong_hash_does_not_consume_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let mut wrong = signed_hash(&f, &request);
    wrong.set(0, wrong.get(0).unwrap().wrapping_add(1));
    assert_rejected(f.client.try_distribute_usdc_hashed(&request, &wrong));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn wrong_caller_hash_does_not_authorize_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    let mut tampered = request.clone();
    tampered.from = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&tampered, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn wrong_token_hash_does_not_authorize_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    let mut tampered = request.clone();
    tampered.usdc_contract = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&tampered, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn wrong_recipient_hash_does_not_authorize_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    let mut tampered = request.clone();
    tampered.accounts.spending = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&tampered, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn wrong_amount_hash_does_not_authorize_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    let mut tampered = request.clone();
    tampered.total_amount = 1001;
    assert_rejected(f.client.try_distribute_usdc_hashed(&tampered, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn wrong_deadline_hash_does_not_authorize_transfer() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &request);
    let mut tampered = request.clone();
    tampered.deadline = 101;
    assert_rejected(f.client.try_distribute_usdc_hashed(&tampered, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn successful_nonce_sequence_is_monotonic() {
    let env = Env::default();
    let f = fixture(&env);
    for nonce in 1..=3 {
        let request = signed_request(&f, nonce, 1000 + nonce, 1000);
        assert!(f
            .client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request))
            .is_ok());
        assert_eq!(f.client.get_nonce(&f.owner), nonce + 1);
    }
}

#[test]
fn invalid_attempt_does_not_skip_next_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let invalid = signed_request(&f, 2, 100, 1000);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&invalid, &signed_hash(&f, &invalid)),
    );
    let valid = signed_request(&f, 1, 100, 1000);
    assert!(f
        .client
        .try_distribute_usdc_hashed(&valid, &signed_hash(&f, &valid))
        .is_ok());
    assert_eq!(f.client.get_nonce(&f.owner), 2);
}

#[test]
fn duplicate_after_another_valid_nonce_is_rejected() {
    let env = Env::default();
    let f = fixture(&env);
    let first = signed_request(&f, 1, 100, 1000);
    let first_hash = signed_hash(&f, &first);
    assert!(f
        .client
        .try_distribute_usdc_hashed(&first, &first_hash)
        .is_ok());
    let second = signed_request(&f, 2, 101, 1000);
    assert!(f
        .client
        .try_distribute_usdc_hashed(&second, &signed_hash(&f, &second))
        .is_ok());
    assert_rejected(f.client.try_distribute_usdc_hashed(&first, &first_hash));
    assert_eq!(f.client.get_nonce(&f.owner), 3);
}

#[test]
fn rejected_zero_amount_does_not_consume_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 0);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn rejected_negative_amount_does_not_consume_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, -1);
    assert_rejected(
        f.client
            .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
    );
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn nonce_read_is_stable_without_mutation() {
    let env = Env::default();
    let f = fixture(&env);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn separate_signers_have_independent_nonce_domains() {
    let env = Env::default();
    let f = fixture(&env);
    let other = Address::generate(&env);
    assert_eq!(f.client.get_nonce(&other), 0);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn different_contract_instances_have_independent_nonce_storage() {
    let env = Env::default();
    let first = fixture(&env);
    let second_contract = env.register_contract(None, RemittanceSplit);
    let second = RemittanceSplitClient::new(&env, &second_contract);
    assert_eq!(first.client.get_nonce(&first.owner), 1);
    assert_eq!(second.get_nonce(&first.owner), 0);
}

#[test]
fn replay_hash_does_not_change_after_reading_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let before = signed_hash(&f, &request);
    let _ = f.client.get_nonce(&f.owner);
    let after = signed_hash(&f, &request);
    assert_eq!(before, after);
}

#[test]
fn domain_symbol_is_explicit_for_audit_review() {
    let env = Env::default();
    let value = symbol_short!("distrib");
    assert_eq!(value.to_string(), "distrib");
}

#[test]
fn no_recipient_is_credited_on_hash_failure() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let mut wrong = signed_hash(&f, &request);
    wrong.set(0, wrong.get(0).unwrap().wrapping_add(1));
    assert_rejected(f.client.try_distribute_usdc_hashed(&request, &wrong));
    let token = TokenClient::new(&env, &f.token);
    assert_eq!(token.balance(&f.accounts.spending), 0);
    assert_eq!(token.balance(&f.accounts.savings), 0);
    assert_eq!(token.balance(&f.accounts.bills), 0);
    assert_eq!(token.balance(&f.accounts.insurance), 0);
}

#[test]
fn invalid_nonce_is_stable_across_retries() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 9, 100, 1000);
    let hash = signed_hash(&f, &request);
    let first = f.client.try_distribute_usdc_hashed(&request, &hash);
    let second = f.client.try_distribute_usdc_hashed(&request, &hash);
    assert_rejected(first);
    assert_rejected(second);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn malformed_hash_length_is_rejected() {
    let env = Env::default();
    let f = fixture(&env);
    let request = signed_request(&f, 1, 100, 1000);
    let short = soroban_sdk::Bytes::from_slice(&env, &[1, 2, 3]);
    assert_rejected(f.client.try_distribute_usdc_hashed(&request, &short));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_nonce() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.nonce = 2;
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_deadline() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.deadline = 101;
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_amount() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.total_amount = 999;
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_savings_account() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.accounts.savings = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_bills_account() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.accounts.bills = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn valid_hash_cannot_be_reused_with_new_insurance_account() {
    let env = Env::default();
    let f = fixture(&env);
    let original = signed_request(&f, 1, 100, 1000);
    let hash = signed_hash(&f, &original);
    let mut changed = original.clone();
    changed.accounts.insurance = Address::generate(&env);
    assert_rejected(f.client.try_distribute_usdc_hashed(&changed, &hash));
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn invalid_requests_do_not_change_owner_balance_repeatedly() {
    let env = Env::default();
    let f = fixture(&env);
    let before = TokenClient::new(&env, &f.token).balance(&f.owner);
    for amount in [0, -1, -100, i128::MIN + 1] {
        let request = signed_request(&f, 1, 100, amount);
        assert_rejected(
            f.client
                .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request)),
        );
    }
    assert_eq!(TokenClient::new(&env, &f.token).balance(&f.owner), before);
    assert_eq!(f.client.get_nonce(&f.owner), 1);
}

#[test]
fn nonce_domain_survives_unrelated_read_calls() {
    let env = Env::default();
    let f = fixture(&env);
    let _ = f.client.get_config();
    let _ = f.client.get_nonce(&f.owner);
    let request = signed_request(&f, 1, 100, 1000);
    assert!(f
        .client
        .try_distribute_usdc_hashed(&request, &signed_hash(&f, &request))
        .is_ok());
    assert_eq!(f.client.get_nonce(&f.owner), 2);
}
