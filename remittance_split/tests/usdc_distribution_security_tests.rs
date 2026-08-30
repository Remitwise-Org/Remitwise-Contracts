//! Adversarial regression coverage for owner-bound USDC distribution.
//!
//! The production contract has two distribution surfaces: the legacy
//! u64-hash entrypoint and the request-structured SHA-256 entrypoint. These
//! tests exercise both where the acceptance criteria overlap and focus the new
//! regression on the hashed path's owner authorization boundary.

use remittance_split::{
    AccountGroup, DistributeUsdcRequest, RemittanceSplit, RemittanceSplitClient,
    RemittanceSplitError, MAX_BATCH_SIZE, MAX_DEADLINE_WINDOW_SECS,
};
use soroban_sdk::testutils::{Address as AddressTrait, Ledger};
use soroban_sdk::token::{StellarAssetClient, TokenClient};
use soroban_sdk::{vec, Address, Bytes, Env, Vec};

struct Harness<'a> {
    env: Env,
    client: RemittanceSplitClient<'a>,
    owner: Address,
    token: Address,
    token_admin: StellarAssetClient<'a>,
}

fn setup<'a>(env: &'a Env) -> Harness<'a> {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(env, &contract_id);
    let owner = Address::generate(env);
    let token_admin_address = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin_address);
    let token = token_contract.address();
    let token_admin = StellarAssetClient::new(env, &token);

    client.initialize_split(&owner, &0, &token, &4000, &3000, &2000, &1000);

    Harness {
        env: env.clone(),
        client,
        owner,
        token,
        token_admin,
    }
}

fn accounts(env: &Env) -> AccountGroup {
    AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    }
}

fn request(
    harness: &Harness<'_>,
    from: Address,
    token: Address,
    nonce: u64,
) -> DistributeUsdcRequest {
    DistributeUsdcRequest {
        usdc_contract: token,
        from,
        nonce,
        accounts: accounts(&harness.env),
        total_amount: 1_000,
        deadline: harness.env.ledger().timestamp() + 1_800,
    }
}

fn mint_for(harness: &Harness<'_>, address: &Address, amount: i128) {
    harness.token_admin.mint(address, &amount);
}

fn assert_zero_destinations(harness: &Harness<'_>, group: &AccountGroup) {
    let token = TokenClient::new(&harness.env, &harness.token);
    assert_eq!(token.balance(&group.spending), 0);
    assert_eq!(token.balance(&group.savings), 0);
    assert_eq!(token.balance(&group.bills), 0);
    assert_eq!(token.balance(&group.insurance), 0);
}

#[test]
fn hashed_distribution_rejects_a_valid_signature_from_non_owner() {
    let env = Env::default();
    let harness = setup(&env);
    let caller = Address::generate(&env);
    let mut req = request(&harness, caller.clone(), harness.token.clone(), 1);
    mint_for(&harness, &caller, req.total_amount);
    let hash = harness.client.get_request_hash(&req);
    let owner_nonce_before = harness.client.get_nonce(&harness.owner);
    let caller_nonce_before = harness.client.get_nonce(&caller);

    let result = harness.client.try_distribute_usdc_hashed(&req, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
    assert_eq!(harness.client.get_nonce(&harness.owner), owner_nonce_before);
    assert_eq!(harness.client.get_nonce(&caller), caller_nonce_before);
    assert_eq!(
        TokenClient::new(&env, &harness.token).balance(&caller),
        req.total_amount
    );
    assert_zero_destinations(&harness, &req.accounts);

    // Keep this mutation in the test to make it explicit that the request was
    // otherwise a validly hashed request from the wrong authorized principal.
    req.total_amount = 1_001;
    assert_ne!(harness.client.get_request_hash(&req), hash);
}

#[test]
fn hashed_distribution_rejects_untrusted_asset_before_transfer() {
    let env = Env::default();
    let harness = setup(&env);
    let other_admin = Address::generate(&env);
    let other_token_contract = env.register_stellar_asset_contract_v2(other_admin);
    let other_token = other_token_contract.address();
    let req = request(&harness, harness.owner.clone(), other_token, 1);
    let hash = harness.client.get_request_hash(&req);
    let nonce_before = harness.client.get_nonce(&harness.owner);

    let result = harness.client.try_distribute_usdc_hashed(&req, &hash);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UntrustedTokenContract))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), nonce_before);
    assert_zero_destinations(&harness, &req.accounts);
}

#[test]
fn hashed_distribution_rejects_request_hash_tampering_before_token_calls() {
    let env = Env::default();
    let harness = setup(&env);
    let req = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    let wrong_hash = Bytes::from_slice(&env, &[7; 32]);
    let nonce_before = harness.client.get_nonce(&harness.owner);

    let result = harness.client.try_distribute_usdc_hashed(&req, &wrong_hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
    assert_eq!(harness.client.get_nonce(&harness.owner), nonce_before);
    assert_zero_destinations(&harness, &req.accounts);
}

#[test]
fn hashed_distribution_rejects_recipient_substitution_with_original_hash() {
    let env = Env::default();
    let harness = setup(&env);
    let original = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    let hash = harness.client.get_request_hash(&original);
    let mut tampered = original.clone();
    tampered.accounts.spending = Address::generate(&env);

    let result = harness.client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_zero_destinations(&harness, &original.accounts);
    assert_zero_destinations(&harness, &tampered.accounts);
}

#[test]
fn hashed_distribution_rejects_amount_substitution_with_original_hash() {
    let env = Env::default();
    let harness = setup(&env);
    let original = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    let hash = harness.client.get_request_hash(&original);
    let mut tampered = original.clone();
    tampered.total_amount += 1;

    let result = harness.client.try_distribute_usdc_hashed(&tampered, &hash);
    assert_eq!(result, Err(Ok(RemittanceSplitError::RequestHashMismatch)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn hashed_distribution_does_not_advance_nonce_when_token_transfer_fails() {
    let env = Env::default();
    let harness = setup(&env);
    let req = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    let hash = harness.client.get_request_hash(&req);
    let nonce_before = harness.client.get_nonce(&harness.owner);

    // No USDC is minted to the owner, so the token contract rejects the first
    // transfer. Soroban rolls the entire invocation back, including nonce use.
    let result = harness.client.try_distribute_usdc_hashed(&req, &hash);
    assert!(result.is_err());
    assert_eq!(harness.client.get_nonce(&harness.owner), nonce_before);
    assert_zero_destinations(&harness, &req.accounts);
}

#[test]
fn hashed_distribution_marks_nonce_only_after_all_transfers_succeed() {
    let env = Env::default();
    let harness = setup(&env);
    let req = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    mint_for(&harness, &harness.owner, req.total_amount);
    let hash = harness.client.get_request_hash(&req);

    assert_eq!(
        harness.client.try_distribute_usdc_hashed(&req, &hash),
        Ok(Ok(true))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 2);
    let token = TokenClient::new(&env, &harness.token);
    let received = token.balance(&req.accounts.spending)
        + token.balance(&req.accounts.savings)
        + token.balance(&req.accounts.bills)
        + token.balance(&req.accounts.insurance);
    assert_eq!(received, req.total_amount);
}

#[test]
fn hashed_distribution_replay_is_rejected_without_second_payout() {
    let env = Env::default();
    let harness = setup(&env);
    let req = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    mint_for(&harness, &harness.owner, req.total_amount * 2);
    let hash = harness.client.get_request_hash(&req);
    harness.client.distribute_usdc_hashed(&req, &hash);
    let balances_after_first = [
        TokenClient::new(&env, &harness.token).balance(&req.accounts.spending),
        TokenClient::new(&env, &harness.token).balance(&req.accounts.savings),
        TokenClient::new(&env, &harness.token).balance(&req.accounts.bills),
        TokenClient::new(&env, &harness.token).balance(&req.accounts.insurance),
    ];

    let replay = harness.client.try_distribute_usdc_hashed(&req, &hash);
    assert_eq!(replay, Err(Ok(RemittanceSplitError::NonceAlreadyUsed)));
    let token = TokenClient::new(&env, &harness.token);
    assert_eq!(
        balances_after_first,
        [
            token.balance(&req.accounts.spending),
            token.balance(&req.accounts.savings),
            token.balance(&req.accounts.bills),
            token.balance(&req.accounts.insurance),
        ]
    );
}

#[test]
fn batch_transfer_rejects_mismatched_recipient_and_amount_vectors() {
    let env = Env::default();
    let harness = setup(&env);
    let recipients = vec![&env, Address::generate(&env), Address::generate(&env)];
    let amounts = vec![&env, 10_i128];

    let result = harness.client.try_batch_transfer(
        &harness.owner,
        &1,
        &harness.token,
        &recipients,
        &amounts,
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::BatchLengthMismatch)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn batch_transfer_rejects_more_than_the_configured_recipient_limit() {
    let env = Env::default();
    let harness = setup(&env);
    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..=MAX_BATCH_SIZE {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1);
    }

    let result = harness.client.try_batch_transfer(
        &harness.owner,
        &1,
        &harness.token,
        &recipients,
        &amounts,
    );
    assert_eq!(result, Err(Ok(RemittanceSplitError::BatchSizeExceeded)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn batch_transfer_wrong_asset_cannot_substitute_the_configured_usdc() {
    let env = Env::default();
    let harness = setup(&env);
    let other_admin = Address::generate(&env);
    let other = env
        .register_stellar_asset_contract_v2(other_admin)
        .address();
    let recipients = vec![&env, Address::generate(&env)];
    let amounts = vec![&env, 10_i128];

    let result =
        harness
            .client
            .try_batch_transfer(&harness.owner, &1, &other, &recipients, &amounts);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UntrustedTokenContract))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn deadline_and_nonce_guards_leave_distribution_state_unchanged() {
    let env = Env::default();
    let harness = setup(&env);
    let mut expired = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    expired.deadline = env.ledger().timestamp() - 1;
    let expired_hash = harness.client.get_request_hash(&expired);
    assert_eq!(
        harness
            .client
            .try_distribute_usdc_hashed(&expired, &expired_hash),
        Err(Ok(RemittanceSplitError::DeadlineExpired))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);

    let mut too_far = request(&harness, harness.owner.clone(), harness.token.clone(), 1);
    too_far.deadline = env.ledger().timestamp() + MAX_DEADLINE_WINDOW_SECS + 1;
    let too_far_hash = harness.client.get_request_hash(&too_far);
    assert_eq!(
        harness
            .client
            .try_distribute_usdc_hashed(&too_far, &too_far_hash),
        Err(Ok(RemittanceSplitError::InvalidDeadline))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn legacy_distribution_rejects_non_owner_before_token_transfer() {
    let env = Env::default();
    let harness = setup(&env);
    let caller = Address::generate(&env);
    let destinations = accounts(&env);
    let deadline = env.ledger().timestamp() + 1_800;
    let hash = RemittanceSplit::compute_request_hash(
        soroban_sdk::symbol_short!("distrib"),
        caller.clone(),
        1,
        1_000,
        deadline,
    );
    mint_for(&harness, &caller, 1_000);

    let result = harness.client.try_distribute_usdc(
        &harness.token,
        &caller,
        &1,
        &deadline,
        &hash,
        &destinations,
        &1_000,
    );

    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_eq!(
        TokenClient::new(&env, &harness.token).balance(&caller),
        1_000
    );
    assert_zero_destinations(&harness, &destinations);
}

#[test]
fn legacy_distribution_rejects_asset_substitution_before_nonce_use() {
    let env = Env::default();
    let harness = setup(&env);
    let other_admin = Address::generate(&env);
    let other_token = env
        .register_stellar_asset_contract_v2(other_admin)
        .address();
    let destinations = accounts(&env);
    let deadline = env.ledger().timestamp() + 1_800;
    let hash = RemittanceSplit::compute_request_hash(
        soroban_sdk::symbol_short!("distrib"),
        harness.owner.clone(),
        1,
        1_000,
        deadline,
    );

    let result = harness.client.try_distribute_usdc(
        &other_token,
        &harness.owner,
        &1,
        &deadline,
        &hash,
        &destinations,
        &1_000,
    );

    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UntrustedTokenContract))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_zero_destinations(&harness, &destinations);
}

#[test]
fn hashed_distribution_rejects_self_transfer_without_mutating_nonce() {
    let env = Env::default();
    let harness = setup(&env);
    let mut destinations = accounts(&env);
    destinations.savings = harness.owner.clone();
    let req = DistributeUsdcRequest {
        usdc_contract: harness.token.clone(),
        from: harness.owner.clone(),
        nonce: 1,
        accounts: destinations.clone(),
        total_amount: 1_000,
        deadline: env.ledger().timestamp() + 1_800,
    };
    let hash = harness.client.get_request_hash(&req);

    let result = harness.client.try_distribute_usdc_hashed(&req, &hash);

    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::SelfTransferNotAllowed))
    );
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_zero_destinations(&harness, &destinations);
}

#[test]
fn batch_transfer_rejects_non_positive_amount_without_state_change() {
    let env = Env::default();
    let harness = setup(&env);
    let recipient = Address::generate(&env);
    let recipients = vec![&env, recipient.clone()];
    let amounts = vec![&env, 0_i128];

    let result = harness.client.try_batch_transfer(
        &harness.owner,
        &1,
        &harness.token,
        &recipients,
        &amounts,
    );

    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_eq!(
        TokenClient::new(&env, &harness.token).balance(&recipient),
        0
    );
}

#[test]
fn batch_transfer_failure_rolls_back_nonce_and_earlier_transfers() {
    let env = Env::default();
    let harness = setup(&env);
    let first = Address::generate(&env);
    let second = Address::generate(&env);
    let recipients = vec![&env, first.clone(), second.clone()];
    let amounts = vec![&env, 400_i128, 600_i128];
    // The owner has no balance, so the first token transfer fails. This also
    // verifies the no-partial-payout invariant if the loop is later changed.
    let result = harness.client.try_batch_transfer(
        &harness.owner,
        &1,
        &harness.token,
        &recipients,
        &amounts,
    );

    assert!(result.is_err());
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
    assert_eq!(TokenClient::new(&env, &harness.token).balance(&first), 0);
    assert_eq!(TokenClient::new(&env, &harness.token).balance(&second), 0);
}

#[test]
fn update_split_cannot_replace_the_pinned_asset() {
    let env = Env::default();
    let harness = setup(&env);
    let original = harness.client.get_config().expect("initialized config");
    let result =
        harness
            .client
            .try_update_split(&harness.owner, &1, &2_500, &2_500, &2_500, &2_500);

    assert_eq!(result, Ok(Ok(true)));
    let updated = harness.client.get_config().expect("updated config");
    assert_eq!(updated.owner, original.owner);
    assert_eq!(updated.usdc_contract, original.usdc_contract);
    // `update_split` is authenticated independently and does not consume the
    // distribution nonce; the asset pin must survive that configuration edit.
    assert_eq!(harness.client.get_nonce(&harness.owner), 1);
}

#[test]
fn legacy_distribution_conserves_the_configured_asset() {
    let env = Env::default();
    let harness = setup(&env);
    let destinations = accounts(&env);
    let amount = 1_000_i128;
    let deadline = env.ledger().timestamp() + 1_800;
    let hash = RemittanceSplit::compute_request_hash(
        soroban_sdk::symbol_short!("distrib"),
        harness.owner.clone(),
        1,
        amount,
        deadline,
    );
    mint_for(&harness, &harness.owner, amount);

    assert!(harness.client.distribute_usdc(
        &harness.token,
        &harness.owner,
        &1,
        &deadline,
        &hash,
        &destinations,
        &amount,
    ));

    let token = TokenClient::new(&env, &harness.token);
    let received = token.balance(&destinations.spending)
        + token.balance(&destinations.savings)
        + token.balance(&destinations.bills)
        + token.balance(&destinations.insurance);
    assert_eq!(received, amount);
    assert_eq!(token.balance(&harness.owner), 0);
    assert_eq!(harness.client.get_nonce(&harness.owner), 2);
}

#[test]
fn legacy_distribution_replay_does_not_pay_twice() {
    let env = Env::default();
    let harness = setup(&env);
    let destinations = accounts(&env);
    let amount = 1_000_i128;
    let deadline = env.ledger().timestamp() + 1_800;
    let hash = RemittanceSplit::compute_request_hash(
        soroban_sdk::symbol_short!("distrib"),
        harness.owner.clone(),
        1,
        amount,
        deadline,
    );
    mint_for(&harness, &harness.owner, amount * 2);
    harness.client.distribute_usdc(
        &harness.token,
        &harness.owner,
        &1,
        &deadline,
        &hash,
        &destinations,
        &amount,
    );
    let first_payout = TokenClient::new(&env, &harness.token).balance(&destinations.spending)
        + TokenClient::new(&env, &harness.token).balance(&destinations.savings)
        + TokenClient::new(&env, &harness.token).balance(&destinations.bills)
        + TokenClient::new(&env, &harness.token).balance(&destinations.insurance);

    let replay = harness.client.try_distribute_usdc(
        &harness.token,
        &harness.owner,
        &1,
        &deadline,
        &hash,
        &destinations,
        &amount,
    );

    assert_eq!(replay, Err(Ok(RemittanceSplitError::InvalidNonce)));
    let second_payout = TokenClient::new(&env, &harness.token).balance(&destinations.spending)
        + TokenClient::new(&env, &harness.token).balance(&destinations.savings)
        + TokenClient::new(&env, &harness.token).balance(&destinations.bills)
        + TokenClient::new(&env, &harness.token).balance(&destinations.insurance);
    assert_eq!(first_payout, amount);
    assert_eq!(second_payout, first_payout);
    assert_eq!(harness.client.get_nonce(&harness.owner), 2);
}
