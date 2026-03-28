#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as AddressTrait, Events, Ledger},
    testutils::storage::Instance as StorageInstance,
    token::{StellarAssetClient, TokenClient},
    Address, Env, Symbol, TryFromVal,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Register a native Stellar asset (SAC) and return (contract_id, admin).
/// The admin is the issuer; we mint `amount` to `recipient`.
fn setup_token(env: &Env, admin: &Address, recipient: &Address, amount: i128) -> Address {
    let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let sac = StellarAssetClient::new(env, &token_id);
    sac.mint(recipient, &amount);
    token_id
}

/// Build a fresh AccountGroup with four distinct addresses.
fn make_accounts(env: &Env) -> AccountGroup {
    AccountGroup {
        spending: Address::generate(env),
        savings: Address::generate(env),
        bills: Address::generate(env),
        insurance: Address::generate(env),
    }
}

// ---------------------------------------------------------------------------
// initialize_split
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_split_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    let success = client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    assert_eq!(success, true);

    let config = client.get_config().unwrap();
    assert_eq!(config.owner, owner);
    assert_eq!(config.spending_percent, 50);
    assert_eq!(config.savings_percent, 30);
    assert_eq!(config.bills_percent, 15);
    assert_eq!(config.insurance_percent, 5);
    assert_eq!(config.usdc_contract, token_id);
}

#[test]
fn test_initialize_split_invalid_sum() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    let result = client.try_initialize_split(&owner, &0, &token_id, &50, &50, &10, &0);
    assert_eq!(result, Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100)));
}

#[test]
fn test_initialize_split_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let result = client.try_initialize_split(&owner, &1, &token_id, &50, &30, &15, &5);
    assert_eq!(result, Err(Ok(RemittanceSplitError::AlreadyInitialized)));
}

#[test]
#[should_panic]
fn test_initialize_split_requires_auth() {
    let env = Env::default();
    // No mock_all_auths — owner has not authorized
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
}

// ---------------------------------------------------------------------------
// update_split
// ---------------------------------------------------------------------------

#[test]
fn test_update_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let success = client.update_split(&owner, &1, &40, &40, &10, &10);
    assert_eq!(success, true);

    let config = client.get_config().unwrap();
    assert_eq!(config.spending_percent, 40);
    assert_eq!(config.savings_percent, 40);
    assert_eq!(config.bills_percent, 10);
    assert_eq!(config.insurance_percent, 10);
}

#[test]
fn test_update_split_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let result = client.try_update_split(&other, &0, &40, &40, &10, &10);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
}

#[test]
fn test_update_split_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    let result = client.try_update_split(&caller, &0, &25, &25, &25, &25);
    assert_eq!(result, Err(Ok(RemittanceSplitError::NotInitialized)));
}

#[test]
fn test_update_split_percentages_must_sum_to_100() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let result = client.try_update_split(&owner, &1, &60, &30, &15, &5);
    assert_eq!(result, Err(Ok(RemittanceSplitError::PercentagesDoNotSumTo100)));
}

// ---------------------------------------------------------------------------
// calculate_split
// ---------------------------------------------------------------------------

#[test]
fn test_calculate_split() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 500);
    assert_eq!(amounts.get(1).unwrap(), 300);
    assert_eq!(amounts.get(2).unwrap(), 150);
    assert_eq!(amounts.get(3).unwrap(), 50);
}

#[test]
fn test_calculate_split_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let result = client.try_calculate_split(&0);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
}

#[test]
fn test_calculate_split_rounding() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &33, &33, &33, &1);
    let amounts = client.calculate_split(&100);
    let sum: i128 = amounts.iter().sum();
    assert_eq!(sum, 100);
}

#[test]
fn test_calculate_complex_rounding() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &17, &19, &23, &41);
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 170);
    assert_eq!(amounts.get(1).unwrap(), 190);
    assert_eq!(amounts.get(2).unwrap(), 230);
    assert_eq!(amounts.get(3).unwrap(), 410);
}

// ---------------------------------------------------------------------------
// distribute_usdc — happy path
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_success() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let total = 1_000i128;
    let token_id = setup_token(&env, &token_admin, &owner, total);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let accounts = make_accounts(&env);
    let result = client.distribute_usdc(&token_id, &owner, &1, &accounts, &total);
    assert_eq!(result, true);

    let token = TokenClient::new(&env, &token_id);
    assert_eq!(token.balance(&accounts.spending), 500);
    assert_eq!(token.balance(&accounts.savings), 300);
    assert_eq!(token.balance(&accounts.bills), 150);
    assert_eq!(token.balance(&accounts.insurance), 50);
    assert_eq!(token.balance(&owner), 0);
}

#[test]
fn test_distribute_usdc_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let accounts = make_accounts(&env);
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = Symbol::try_from_val(&env, &last.1.get(0).unwrap()).unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &last.1.get(1).unwrap()).unwrap();
    assert_eq!(topic0, symbol_short!("split"));
    assert_eq!(topic1, SplitEvent::DistributionCompleted);
}

#[test]
fn test_distribute_usdc_nonce_increments() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 2_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    // nonce after init = 1
    let accounts = make_accounts(&env);
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    // nonce after first distribute = 2
    assert_eq!(client.get_nonce(&owner), 2);
}

// ---------------------------------------------------------------------------
// distribute_usdc — auth must be first (before amount check)
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_distribute_usdc_requires_auth() {
    // Set up contract state with a mocked env first
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    // Now call distribute_usdc without mocking auth for `owner` — should panic
    // We create a fresh env that does NOT mock auths
    let env2 = Env::default();
    // Re-register the same contract in env2 (no mock_all_auths)
    let contract_id2 = env2.register_contract(None, RemittanceSplit);
    let client2 = RemittanceSplitClient::new(&env2, &contract_id2);
    let accounts = make_accounts(&env2);
    // This should panic because owner has not authorized in env2
    client2.distribute_usdc(&token_id, &owner, &0, &accounts, &1_000);
}

// ---------------------------------------------------------------------------
// distribute_usdc — owner-only enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_non_owner_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    // Attacker self-authorizes but is not the config owner
    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&token_id, &attacker, &0, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — untrusted token contract
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_untrusted_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    // Supply a different (malicious) token contract address
    let evil_token = Address::generate(&env);
    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&evil_token, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::UntrustedTokenContract)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — self-transfer guard
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_self_transfer_spending_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    // spending destination == owner
    let accounts = AccountGroup {
        spending: owner.clone(),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::SelfTransferNotAllowed)));
}

#[test]
fn test_distribute_usdc_self_transfer_savings_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: owner.clone(),
        bills: Address::generate(&env),
        insurance: Address::generate(&env),
    };
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::SelfTransferNotAllowed)));
}

#[test]
fn test_distribute_usdc_self_transfer_bills_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: owner.clone(),
        insurance: Address::generate(&env),
    };
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::SelfTransferNotAllowed)));
}

#[test]
fn test_distribute_usdc_self_transfer_insurance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let accounts = AccountGroup {
        spending: Address::generate(&env),
        savings: Address::generate(&env),
        bills: Address::generate(&env),
        insurance: owner.clone(),
    };
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::SelfTransferNotAllowed)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — invalid amount
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &0);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
}

#[test]
fn test_distribute_usdc_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &-1);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidAmount)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — not initialized
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_not_initialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);

    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&token_id, &owner, &0, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::NotInitialized)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — replay protection
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_replay_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 2_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let accounts = make_accounts(&env);
    // First call with nonce=1 succeeds
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    // Replaying nonce=1 must fail
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &500);
    assert_eq!(result, Err(Ok(RemittanceSplitError::InvalidNonce)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — paused contract
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_paused_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    client.pause(&owner);

    let accounts = make_accounts(&env);
    let result = client.try_distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    assert_eq!(result, Err(Ok(RemittanceSplitError::Unauthorized)));
}

// ---------------------------------------------------------------------------
// distribute_usdc — correct split math verified end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_split_math_25_25_25_25() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &25, &25, &25, &25);
    let accounts = make_accounts(&env);
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);

    let token = TokenClient::new(&env, &token_id);
    assert_eq!(token.balance(&accounts.spending), 250);
    assert_eq!(token.balance(&accounts.savings), 250);
    assert_eq!(token.balance(&accounts.bills), 250);
    assert_eq!(token.balance(&accounts.insurance), 250);
}

#[test]
fn test_distribute_usdc_split_math_100_0_0_0() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 1_000);

    client.initialize_split(&owner, &0, &token_id, &100, &0, &0, &0);
    let accounts = make_accounts(&env);
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);

    let token = TokenClient::new(&env, &token_id);
    assert_eq!(token.balance(&accounts.spending), 1_000);
    assert_eq!(token.balance(&accounts.savings), 0);
    assert_eq!(token.balance(&accounts.bills), 0);
    assert_eq!(token.balance(&accounts.insurance), 0);
}

#[test]
fn test_distribute_usdc_rounding_remainder_goes_to_insurance() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    // 33/33/33/1 with amount=100: 33+33+33=99, insurance gets remainder=1
    let token_id = setup_token(&env, &token_admin, &owner, 100);

    client.initialize_split(&owner, &0, &token_id, &33, &33, &33, &1);
    let accounts = make_accounts(&env);
    client.distribute_usdc(&token_id, &owner, &1, &accounts, &100);

    let token = TokenClient::new(&env, &token_id);
    let total = token.balance(&accounts.spending)
        + token.balance(&accounts.savings)
        + token.balance(&accounts.bills)
        + token.balance(&accounts.insurance);
    assert_eq!(total, 100, "all funds must be distributed");
    assert_eq!(token.balance(&accounts.insurance), 1);
}

// ---------------------------------------------------------------------------
// distribute_usdc — multiple sequential distributions
// ---------------------------------------------------------------------------

#[test]
fn test_distribute_usdc_multiple_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 3_000);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let accounts = make_accounts(&env);

    client.distribute_usdc(&token_id, &owner, &1, &accounts, &1_000);
    client.distribute_usdc(&token_id, &owner, &2, &accounts, &1_000);
    client.distribute_usdc(&token_id, &owner, &3, &accounts, &1_000);

    let token = TokenClient::new(&env, &token_id);
    assert_eq!(token.balance(&accounts.spending), 1_500); // 3 * 500
    assert_eq!(token.balance(&accounts.savings), 900);    // 3 * 300
    assert_eq!(token.balance(&accounts.bills), 450);      // 3 * 150
    assert_eq!(token.balance(&accounts.insurance), 150);  // 3 * 50
    assert_eq!(token.balance(&owner), 0);
}

// ---------------------------------------------------------------------------
// Boundary tests for split percentages
// ---------------------------------------------------------------------------

#[test]
fn test_split_boundary_100_0_0_0() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    let ok = client.initialize_split(&owner, &0, &token_id, &100, &0, &0, &0);
    assert!(ok);
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 1000);
    assert_eq!(amounts.get(3).unwrap(), 0);
}

#[test]
fn test_split_boundary_0_0_0_100() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    let ok = client.initialize_split(&owner, &0, &token_id, &0, &0, &0, &100);
    assert!(ok);
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 0);
    assert_eq!(amounts.get(3).unwrap(), 1000);
}

#[test]
fn test_split_boundary_25_25_25_25() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &25, &25, &25, &25);
    let amounts = client.calculate_split(&1000);
    assert_eq!(amounts.get(0).unwrap(), 250);
    assert_eq!(amounts.get(1).unwrap(), 250);
    assert_eq!(amounts.get(2).unwrap(), 250);
    assert_eq!(amounts.get(3).unwrap(), 250);
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[test]
fn test_initialize_split_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let topic0: Symbol = Symbol::try_from_val(&env, &last_event.1.get(0).unwrap()).unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &last_event.1.get(1).unwrap()).unwrap();
    assert_eq!(topic0, symbol_short!("split"));
    assert_eq!(topic1, SplitEvent::Initialized);
}

#[test]
fn test_update_split_events() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    client.update_split(&owner, &1, &40, &40, &10, &10);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let topic1: SplitEvent = SplitEvent::try_from_val(&env, &last_event.1.get(1).unwrap()).unwrap();
    assert_eq!(topic1, SplitEvent::Updated);
}

// ---------------------------------------------------------------------------
// Remittance schedules
// ---------------------------------------------------------------------------

#[test]
fn test_create_remittance_schedule_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let schedule_id = client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    assert_eq!(schedule_id, 1);

    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert_eq!(schedule.amount, 10000);
    assert_eq!(schedule.next_due, 3000);
    assert!(schedule.active);
}

#[test]
fn test_cancel_remittance_schedule() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let schedule_id = client.create_remittance_schedule(&owner, &10000, &3000, &86400);
    client.cancel_remittance_schedule(&owner, &schedule_id);

    let schedule = client.get_remittance_schedule(&schedule_id).unwrap();
    assert!(!schedule.active);
}

// ---------------------------------------------------------------------------
// TTL extension
// ---------------------------------------------------------------------------

#[test]
fn test_instance_ttl_extended_on_initialize_split() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        protocol_version: 20,
        sequence_number: 100,
        timestamp: 1000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 100,
        max_entry_ttl: 700_000,
    });

    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = setup_token(&env, &token_admin, &owner, 0);

    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(ttl >= 518_400, "TTL must be >= INSTANCE_BUMP_AMOUNT after init");
}

// ============================================================================
// Snapshot schema version tests
//
// These tests verify that:
//  1. export_snapshot embeds the correct schema_version tag.
//  2. import_snapshot accepts any version in MIN_SUPPORTED_SCHEMA_VERSION..=SCHEMA_VERSION.
//  3. import_snapshot rejects a future (too-new) schema version.
//  4. import_snapshot rejects a past (too-old, below min) schema version.
//  5. import_snapshot rejects a tampered checksum regardless of version.
// ============================================================================

#[test]
fn test_export_snapshot_contains_correct_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let snapshot = client.export_snapshot(&owner).unwrap();
    assert_eq!(
        snapshot.schema_version, 1,
        "schema_version must equal SCHEMA_VERSION (1)"
    );
}

#[test]
fn test_import_snapshot_current_schema_version_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let snapshot = client.export_snapshot(&owner).unwrap();
    assert_eq!(snapshot.schema_version, 1);

    let ok = client.import_snapshot(&owner, &1, &snapshot);
    assert!(ok, "import with current schema version must succeed");
}

#[test]
fn test_import_snapshot_future_schema_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let mut snapshot = client.export_snapshot(&owner).unwrap();
    // Simulate a snapshot produced by a newer contract version.
    snapshot.schema_version = 999;

    let result = client.try_import_snapshot(&owner, &1, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UnsupportedVersion)),
        "future schema_version must be rejected"
    );
}

#[test]
fn test_import_snapshot_too_old_schema_version_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let mut snapshot = client.export_snapshot(&owner).unwrap();
    // Simulate a snapshot too old to import.
    snapshot.schema_version = 0;

    let result = client.try_import_snapshot(&owner, &1, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::UnsupportedVersion)),
        "schema_version below minimum must be rejected"
    );
}

#[test]
fn test_import_snapshot_tampered_checksum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let mut snapshot = client.export_snapshot(&owner).unwrap();
    snapshot.checksum = snapshot.checksum.wrapping_add(1);

    let result = client.try_import_snapshot(&owner, &1, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::ChecksumMismatch)),
        "tampered checksum must be rejected"
    );
}

#[test]
fn test_snapshot_export_import_roundtrip_restores_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    // Update so there is something interesting to round-trip.
    // Note: update_split checks the nonce but does NOT increment it.
    client.update_split(&owner, &1, &40, &40, &10, &10);

    let snapshot = client.export_snapshot(&owner).unwrap();
    assert_eq!(snapshot.schema_version, 1);

    // Nonce is 1 after initialize_split (update_split does not increment nonce).
    let ok = client.import_snapshot(&owner, &1, &snapshot);
    assert!(ok);

    let config = client.get_config().unwrap();
    assert_eq!(config.spending_percent, 40);
    assert_eq!(config.savings_percent, 40);
    assert_eq!(config.bills_percent, 10);
    assert_eq!(config.insurance_percent, 10);
}

#[test]
fn test_import_snapshot_unauthorized_caller_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, RemittanceSplit);
    let client = RemittanceSplitClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);
    let token_id = Address::generate(&env);
    client.initialize_split(&owner, &0, &token_id, &50, &30, &15, &5);

    let snapshot = client.export_snapshot(&owner).unwrap();

    let result = client.try_import_snapshot(&other, &0, &snapshot);
    assert_eq!(
        result,
        Err(Ok(RemittanceSplitError::Unauthorized)),
        "non-owner must not import snapshot"
    );
}
