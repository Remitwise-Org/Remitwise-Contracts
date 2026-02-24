use bill_payments::{BillPayments, BillPaymentsClient, MAX_PAGE_LIMIT};
use global_config::{ConfigValue, GlobalConfig, GlobalConfigClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String, Symbol};

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    env
}

/// Deploy both contracts and wire them together.
/// Returns (bills_client, config_client, upgrade_admin).
fn setup(
    env: &Env,
) -> (
    BillPaymentsClient<'_>,
    GlobalConfigClient<'_>,
    Address,
) {
    let bills_id = env.register_contract(None, BillPayments);
    let bills = BillPaymentsClient::new(env, &bills_id);

    let cfg_id = env.register_contract(None, GlobalConfig);
    let cfg = GlobalConfigClient::new(env, &cfg_id);

    // Bootstrap the upgrade admin for bill_payments
    let upg_admin = Address::generate(env);
    bills.set_upgrade_admin(&upg_admin, &upg_admin);

    // Initialize global config
    let cfg_admin = Address::generate(env);
    cfg.initialize(&cfg_admin);

    // Wire config into bill_payments
    bills.set_config_contract(&upg_admin, &cfg_id);

    (bills, cfg, upg_admin)
}

// -----------------------------------------------------------------------
// set_config_contract
// -----------------------------------------------------------------------

#[test]
fn test_set_config_contract_requires_upgrade_admin() {
    let env = make_env();
    let bills_id = env.register_contract(None, BillPayments);
    let bills = BillPaymentsClient::new(&env, &bills_id);
    let upg_admin = Address::generate(&env);
    bills.set_upgrade_admin(&upg_admin, &upg_admin);

    let cfg_id = env.register_contract(None, GlobalConfig);
    let stranger = Address::generate(&env);

    let result = bills.try_set_config_contract(&stranger, &cfg_id);
    assert!(result.is_err());
}

// -----------------------------------------------------------------------
// sync_limits_from_config
// -----------------------------------------------------------------------

#[test]
fn test_sync_without_config_contract_errors() {
    let env = make_env();
    let bills_id = env.register_contract(None, BillPayments);
    let bills = BillPaymentsClient::new(&env, &bills_id);
    let upg_admin = Address::generate(&env);
    bills.set_upgrade_admin(&upg_admin, &upg_admin);

    // No set_config_contract called — sync must fail
    let result = bills.try_sync_limits_from_config(&upg_admin);
    assert!(result.is_err());
}

#[test]
fn test_sync_reads_max_page_limit_from_config() {
    let env = make_env();
    let (bills, cfg, upg_admin) = setup(&env);

    // Config admin is whoever initialized the config contract
    let cfg_admin = cfg.get_admin().unwrap();

    // Push a custom limit into global config
    cfg.set_config(
        &cfg_admin,
        &Symbol::new(&env, "max_page_lmt"),
        &ConfigValue::U32(30),
    );

    // Sync to bill_payments
    bills.sync_limits_from_config(&upg_admin);

    // Create 40 bills then page with limit=50 — should receive at most 30
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Test");
    for _ in 0..40 {
        bills.create_bill(&owner, &name, &100i128, &1_800_000_000u64, &false, &0u32);
    }

    let page = bills.get_unpaid_bills(&owner, &0, &50);
    assert_eq!(page.count, 30);
    assert!(page.next_cursor > 0);
}

#[test]
fn test_sync_missing_key_leaves_default_unchanged() {
    let env = make_env();
    let (bills, _cfg, upg_admin) = setup(&env);

    // Config has no "max_page_lmt" key — sync should not error
    bills.sync_limits_from_config(&upg_admin);

    // Create 60 bills and page: should still get MAX_PAGE_LIMIT (50)
    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Bill");
    for _ in 0..60 {
        bills.create_bill(&owner, &name, &1i128, &1_800_000_000u64, &false, &0u32);
    }

    let page = bills.get_unpaid_bills(&owner, &0, &99);
    assert_eq!(page.count, MAX_PAGE_LIMIT);
}

#[test]
fn test_sync_can_be_updated() {
    let env = make_env();
    let (bills, cfg, upg_admin) = setup(&env);

    let cfg_admin = cfg.get_admin().unwrap();

    // First sync: limit = 10
    cfg.set_config(
        &cfg_admin,
        &Symbol::new(&env, "max_page_lmt"),
        &ConfigValue::U32(10),
    );
    bills.sync_limits_from_config(&upg_admin);

    let owner = Address::generate(&env);
    let name = String::from_str(&env, "Bill");
    for _ in 0..20 {
        bills.create_bill(&owner, &name, &1i128, &1_800_000_000u64, &false, &0u32);
    }

    let page = bills.get_unpaid_bills(&owner, &0, &50);
    assert_eq!(page.count, 10);

    // Second sync: limit bumped to 15
    cfg.set_config(
        &cfg_admin,
        &Symbol::new(&env, "max_page_lmt"),
        &ConfigValue::U32(15),
    );
    bills.sync_limits_from_config(&upg_admin);

    let page2 = bills.get_unpaid_bills(&owner, &0, &50);
    assert_eq!(page2.count, 15);
}

#[test]
fn test_sync_requires_upgrade_admin() {
    let env = make_env();
    let (bills, _cfg, _upg_admin) = setup(&env);

    let stranger = Address::generate(&env);
    let result = bills.try_sync_limits_from_config(&stranger);
    assert!(result.is_err());
}
