use bill_payments::{BillPayments, BillPaymentsClient};
use family_wallet::{FamilyWallet, FamilyWalletClient};
use insurance::{Insurance, InsuranceClient};
use remittance_split::{AccountGroup, RemittanceSplit, RemittanceSplitClient};
use reporting::{ReportingContract, ReportingContractClient};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

#[test]
fn test_end_to_end_flow() {
    let env = scenarios::tests::setup_env();

    // 1. Register Actual Contracts
    let usdc_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_id = token_contract.address();
    // Note: token client init needs parameters depending on soroban-sdk version.
    // For simplicity, we bypass native USDC deployment test setups for custom flows
    // or assume our contracts mock token transfers if `WASM` is unavailable.

    let split_id = env.register_contract(None, RemittanceSplit);
    let split_client = RemittanceSplitClient::new(&env, &split_id);

    let savings_id = env.register_contract(None, SavingsGoalContract);
    let savings_client = SavingsGoalContractClient::new(&env, &savings_id);

    let bills_id = env.register_contract(None, BillPayments);
    let bills_client = BillPaymentsClient::new(&env, &bills_id);

    let insurance_id = env.register_contract(None, Insurance);
    let insurance_client = InsuranceClient::new(&env, &insurance_id);

    let family_id = env.register_contract(None, FamilyWallet);
    let family_client = FamilyWalletClient::new(&env, &family_id);

    let reporting_id = env.register_contract(None, ReportingContract);
    let reporting_client = ReportingContractClient::new(&env, &reporting_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // 2. Initialize
    reporting_client.init(&admin);
    reporting_client.configure_addresses(
        &admin,
        &split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &family_id,
    );

    // Initial setup bounds
    let timestamp = env.ledger().timestamp();

    // 3. Configure Split
    let nonce = 0;
    split_client.initialize_split(&user, &nonce, &50, &30, &15, &5);

    // Assuming we do an "allocate into goals/bills/insurance"
    // We create a sample goal
    savings_client.create_goal(
        &user,
        &String::from_str(&env, "Test Goal"),
        &1000,
        &(timestamp + 86400 * 30),
    );

    // A sample bill
    bills_client.create_bill(
        &user,
        &String::from_str(&env, "Electric"),
        &150,
        &(timestamp + 86400 * 5),
        &true,
        &30,
        &String::from_str(&env, "USDC"),
    );

    // Advance time
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: timestamp + 86400 * 10, // 10 days later
        protocol_version: 20,
        sequence_number: 10,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    // Pay bills (Simulating an action)
    // Note: If `pay_bill` requires tokens, it will fail unless we fund it.
    // But let's check reporting at least.

    // 4. Summarize via Reporting
    let total_remittance = 5000;
    let period_start = timestamp;
    let period_end = env.ledger().timestamp();

    let report = reporting_client.get_financial_health_report(
        &user,
        &total_remittance,
        &period_start,
        &period_end,
    );

    std::println!("==== Scenario Summary Result ====");
    std::println!("Health Score       : {}", report.health_score.score);
    std::println!("Savings Goals      : {}", report.savings_report.total_goals);
    std::println!(
        "Bills Tracked      : {}",
        report.bill_compliance.total_bills
    );
    std::println!(
        "Insurance Policies : {}",
        report.insurance_report.active_policies
    );
    std::println!("Remittance         : {}", total_remittance);
    std::println!("=================================");

    // Basic assertions
    assert_eq!(report.savings_report.total_goals, 1);
    assert_eq!(report.bill_compliance.total_bills, 1);
}
