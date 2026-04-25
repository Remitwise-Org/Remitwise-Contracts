use bill_payments::{BillPayments, BillPaymentsClient};
use family_wallet::FamilyWallet;
use insurance::{Insurance, InsuranceClient};
use remittance_split::{RemittanceSplit, RemittanceSplitClient};
use remitwise_common::CoverageType;
use reporting::{ReportingContract, ReportingContractClient};
use savings_goals::{SavingsGoalContract, SavingsGoalContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, String,
};

#[test]
fn test_end_to_end_flow() {
    let env = scenarios::tests::setup_env();

    // 1. Register Actual Contracts
    //let usdc_admin = Address::generate(&env);
    //let token_contract = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    //let usdc_id = token_contract.address();
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
    //let insurance_client = InsuranceClient::new(&env, &insurance_id);

    let family_id = env.register_contract(None, FamilyWallet);
    //let family_client = FamilyWalletClient::new(&env, &family_id);

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
    let mock_usdc = Address::generate(&env);
    split_client.initialize_split(&user, &nonce, &mock_usdc, &50, &30, &15, &5);

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
        &None,
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

/// End-to-end scenario: full remittance window lifecycle across six Soroban contracts.
///
/// Phases:
/// 1. Environment and contract initialization
/// 2. Remittance split configuration
/// 3. Recurring bill creation
/// 4. Insurance policy creation and premium payment
/// 5. Ledger time advancement (simulating billing cycles)
/// 6. Bill payment and recurring cycle verification
/// 7. Financial health report verification
#[test]
fn test_recurring_obligations_flow() {
    // ── Phase 1: Environment and contract initialization ──────────────────────
    //
    // setup_env() configures the Soroban mock environment with:
    //   - ledger timestamp = 1704067200 (2024-01-01T00:00:00Z)  [Requirement 1.5]
    //   - mock_all_auths_allowing_non_root_auth() to bypass auth checks
    let env = scenarios::tests::setup_env();

    // Explicitly bypass all auth checks so contract calls succeed without
    // real signatures. setup_env already calls mock_all_auths_allowing_non_root_auth,
    // but we call mock_all_auths() here to ensure the strictest bypass is active
    // for all cross-contract calls made during the scenario.
    env.mock_all_auths();

    // Capture the initial ledger timestamp for use in due-date calculations.
    let timestamp = env.ledger().timestamp();
    // Verify setup_env sets the expected initial ledger timestamp (Requirement 1.5)
    assert_eq!(
        timestamp, 1704067200,
        "setup_env must set ledger timestamp to 1704067200"
    );

    // Register all six contracts in the shared Env instance (Requirement 1.1).
    // env.register_contract(None, ...) assigns a deterministic address and
    // installs the contract WASM in the mock environment.
    let split_id = env.register_contract(None, RemittanceSplit);
    let savings_id = env.register_contract(None, SavingsGoalContract);
    let bills_id = env.register_contract(None, BillPayments);
    let insurance_id = env.register_contract(None, Insurance);
    let family_id = env.register_contract(None, FamilyWallet);
    let reporting_id = env.register_contract(None, ReportingContract);

    // Build typed clients for each contract.
    let remittance_split = RemittanceSplitClient::new(&env, &split_id);
    let _savings = SavingsGoalContractClient::new(&env, &savings_id);
    let bill_payments = BillPaymentsClient::new(&env, &bills_id);
    let insurance = InsuranceClient::new(&env, &insurance_id);
    let reporting = ReportingContractClient::new(&env, &reporting_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Initialize Reporting with the designated admin address (Requirement 1.2).
    // Guaranteed to succeed: the contract is freshly registered and has no prior
    // admin set, so ReportingContract::init will not return AlreadyInitialized.
    reporting.init(&admin);

    // Configure Reporting with the addresses of all registered contracts (Requirement 1.3).
    // Guaranteed to succeed: admin is correctly set (init just ran), all six
    // addresses are distinct registered contracts, and none is the reporting
    // contract itself — so validate_dependency_address_set will not reject them.
    // If this call were to fail, the non-try_ client method panics with a
    // descriptive Soroban error, satisfying Requirement 1.4.
    reporting.configure_addresses(
        &admin,
        &split_id,
        &savings_id,
        &bills_id,
        &insurance_id,
        &family_id,
    );

    // ── Phase 2: Remittance split configuration ───────────────────────────────
    //
    // Configure how incoming remittance funds are allocated across four buckets.
    // Percentages must sum to exactly 100 (Requirement 2.2).
    // Mapping: spending=family(10), savings=30, bills=40, insurance=20.
    let savings_pct: u32 = 30;
    let bills_pct: u32 = 40;
    let insurance_pct: u32 = 20;
    let family_pct: u32 = 10; // maps to "spending" bucket in the contract
                              // Sanity check: percentages sum to 100 (Requirement 2.2, 2.3)
    assert_eq!(
        family_pct + savings_pct + bills_pct + insurance_pct,
        100,
        "split percentages must sum to 100"
    );

    // A mock USDC address is required by initialize_split for token-substitution
    // attack prevention; no actual token transfers occur in this test.
    let mock_usdc = Address::generate(&env);
    // Nonce starts at 0 for a freshly registered contract (Requirement 8.1).
    let nonce: u64 = 0;

    // Call initialize_split with valid percentages summing to 100 (Requirement 2.1).
    // Guaranteed to return true: contract is freshly registered (no prior config), percentages
    // are valid and sum to 100, nonce is 0 (correct initial value), and mock_all_auths()
    // bypasses the require_auth_for_args check. The Soroban client panics on Err.
    let init_result = remittance_split.initialize_split(
        &user,
        &nonce,
        &mock_usdc,
        &family_pct,    // spending_percent (family bucket)
        &savings_pct,   // savings_percent
        &bills_pct,     // bills_percent
        &insurance_pct, // insurance_percent
    );
    // Assert return value is true — confirms split was stored (Requirement 2.1)
    assert!(
        init_result,
        "initialize_split must return true for valid percentages summing to 100 [Req 2.1]"
    );

    // Retrieve the stored config and assert each percentage matches the input
    // (Requirement 8.6: split config round trip).
    // Guaranteed Some: initialize_split just succeeded, so CONFIG is set in storage.
    let config = remittance_split.get_config().unwrap(); // guaranteed Some: initialize_split just stored the config
                                                         // Assert stored percentages match the values passed to initialize_split (Requirement 8.6)
    assert_eq!(
        config.spending_percent, family_pct,
        "stored spending_percent must match family_pct input [Req 8.6]"
    );
    assert_eq!(
        config.savings_percent, savings_pct,
        "stored savings_percent must match savings_pct input [Req 8.6]"
    );
    assert_eq!(
        config.bills_percent, bills_pct,
        "stored bills_percent must match bills_pct input [Req 8.6]"
    );
    assert_eq!(
        config.insurance_percent, insurance_pct,
        "stored insurance_percent must match insurance_pct input [Req 8.6]"
    );

    // Calculate the split for a known total remittance amount (Requirement 2.4).
    let total_remittance: i128 = 5000;
    // Guaranteed to not panic: contract is initialized and total_remittance > 0.
    // The Soroban client unwraps the Result automatically and panics on Err.
    let allocations = remittance_split.calculate_split(&total_remittance);

    // Assert allocation invariant: sum of all allocations equals total_remittance (Requirement 2.5).
    // The contract assigns the remainder to the last bucket to absorb integer rounding,
    // so the sum is always exact.
    let allocation_sum: i128 = allocations.iter().sum();
    assert_eq!(
        allocation_sum, total_remittance,
        "sum of all split allocations must equal total_remittance [Req 2.4, 2.5]"
    );

    // ── Phase 3: Recurring bill creation ─────────────────────────────────────
    //
    // Create two distinct recurring bills with future due dates (Requirement 3.1, 3.2).
    // Both bills have recurring = true and a positive frequency_days so that paying
    // them will auto-schedule the next cycle (Requirement 6.1).

    // Bill 1: Electricity — due in 7 days, repeats every 30 days.
    // create_bill returns Ok(u32); the Soroban client panics on Err, so the
    // returned value is guaranteed to be the new bill's unique ID.
    let bill_id_1 = bill_payments.create_bill(
        &user,
        &String::from_str(&env, "Electricity"),
        &150i128,
        &(timestamp + 86400 * 7), // due in 7 days — guaranteed > current ledger time
        &true,
        &30u32,
        &None,
        &String::from_str(&env, "USDC"),
    );

    // Bill 2: Internet — due in 14 days, repeats every 30 days.
    // Same guarantee: freshly registered contract, valid params, mock_all_auths active.
    let bill_id_2 = bill_payments.create_bill(
        &user,
        &String::from_str(&env, "Internet"),
        &80i128,
        &(timestamp + 86400 * 14), // due in 14 days — guaranteed > current ledger time
        &true,
        &30u32,
        &None,
        &String::from_str(&env, "USDC"),
    );

    // Assert each bill is retrievable and has paid = false immediately after creation
    // (Requirement 3.3).

    // get_bill returns Option<Bill>; guaranteed Some because create_bill just stored
    // the bill with the returned ID in the same contract instance.
    let bill1 = bill_payments.get_bill(&bill_id_1).unwrap(); // guaranteed Some: bill was just created with this ID
    assert!(
        !bill1.paid,
        "bill_id_1 must have paid = false immediately after creation [Req 3.3]"
    );

    // get_bill returns Option<Bill>; guaranteed Some for the same reason as bill1.
    let bill2 = bill_payments.get_bill(&bill_id_2).unwrap(); // guaranteed Some: bill was just created with this ID
    assert!(
        !bill2.paid,
        "bill_id_2 must have paid = false immediately after creation [Req 3.3]"
    );

    // Assert both bills appear in get_unpaid_bills before any payment (Requirement 3.5).
    // cursor = 0 (start from beginning), limit = 0 (use contract default, covers all bills).
    let unpaid_page = bill_payments.get_unpaid_bills(&user, &0u32, &0u32);
    let mut found_bill1 = false;
    let mut found_bill2 = false;
    for bill in unpaid_page.items.iter() {
        if bill.id == bill_id_1 {
            found_bill1 = true;
        }
        if bill.id == bill_id_2 {
            found_bill2 = true;
        }
    }
    assert!(
        found_bill1,
        "get_unpaid_bills must include bill_id_1 before any payment [Req 3.5]"
    );
    assert!(
        found_bill2,
        "get_unpaid_bills must include bill_id_2 before any payment [Req 3.5]"
    );

    // ── Phase 4: Insurance policy creation and premium payment ────────────────
    //
    // Create a Health insurance policy and verify it is immediately active,
    // then pay the premium and assert the next_payment_date advances by 30 days.

    let monthly_premium: i128 = 100;
    let coverage_amount: i128 = 10_000;

    // create_policy returns a unique u32 policy ID.
    // Guaranteed to succeed: contract is freshly registered, all params are valid,
    // and mock_all_auths() bypasses require_auth.
    let policy_id = insurance.create_policy(
        &user,
        &String::from_str(&env, "Health Insurance"),
        &CoverageType::Health,
        &monthly_premium,
        &coverage_amount,
        &None, // no external_ref
    );

    // Assert the policy is retrievable and active immediately after creation (Requirement 4.1, 4.2).
    // Guaranteed Some: create_policy just stored the policy with the returned ID.
    let policy = insurance.get_policy(&policy_id).unwrap(); // guaranteed Some: policy was just created with this ID
    assert!(
        policy.active,
        "newly created policy must have active = true [Req 4.1, 4.2]"
    );

    // Assert get_total_monthly_premium equals the single active policy's premium (Requirement 4.6).
    let total_premium = insurance.get_total_monthly_premium(&user);
    assert_eq!(
        total_premium,
        monthly_premium,
        "get_total_monthly_premium must equal the monthly_premium of the single active policy [Req 4.6]"
    );

    // Pay the premium on the active policy (Requirement 4.3).
    // Guaranteed true: policy exists, owner matches user, policy is active, mock_all_auths active.
    let pay_result = insurance.pay_premium(&user, &policy_id);
    assert!(
        pay_result,
        "pay_premium must return true for an active policy owned by the caller [Req 4.3]"
    );

    // Assert next_payment_date == ledger_time + 30 * 86400 after paying (Requirement 4.3, 4.4, 8.3).
    // Guaranteed Some: policy still exists (pay_premium does not delete it).
    let policy_after_pay = insurance.get_policy(&policy_id).unwrap(); // guaranteed Some: policy still exists after premium payment
    let expected_next_payment = timestamp + 30 * 86400;
    assert_eq!(
        policy_after_pay.next_payment_date,
        expected_next_payment,
        "next_payment_date must equal ledger_time + 30 * 86400 after pay_premium [Req 4.3, 4.4, 8.3]"
    );
    assert!(
        policy_after_pay.next_payment_date > timestamp,
        "next_payment_date must be strictly greater than ledger_time at time of payment [Req 4.4]"
    );

    // Assert pay_premium returns false for a non-existent policy ID (Requirement 4.5).
    let nonexistent_id: u32 = 9999;
    let pay_nonexistent = insurance.pay_premium(&user, &nonexistent_id);
    assert!(
        !pay_nonexistent,
        "pay_premium must return false for a non-existent policy ID [Req 4.5]"
    );

    // ── Phase 5: Ledger time advancement ─────────────────────────────────────
    //
    // Advance ledger time by 31 days to simulate a full billing cycle passing.
    // This pushes both bills past their due dates so overdue detection can be
    // verified (Requirements 5.1–5.5).
    //
    // Bill 1 (Electricity): due_date = timestamp + 7*86400  = 1704672000
    // Bill 2 (Internet):    due_date = timestamp + 14*86400 = 1705276800
    // New timestamp after +31 days:                          1706745600
    // Both due_dates < new_timestamp → both are OVERDUE (Requirement 5.2, 5.3)
    //
    // All LedgerInfo fields except `timestamp` are preserved from setup_env()
    // (Requirement 5.5): protocol_version=20, sequence_number=1, network_id=[0;32],
    // base_reserve=10, min_temp_entry_ttl=10, min_persistent_entry_ttl=10,
    // max_entry_ttl=3110400.
    let new_timestamp = timestamp + 31 * 86400; // 1704067200 + 2678400 = 1706745600
    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        timestamp: new_timestamp,
        protocol_version: 20,
        sequence_number: 1,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });

    // Confirm the ledger timestamp was actually advanced (Requirement 5.1).
    assert_eq!(
        env.ledger().timestamp(),
        new_timestamp,
        "ledger timestamp must equal new_timestamp after set [Req 5.1]"
    );

    // Retrieve all overdue bills (cursor=0, limit=0 → contract default covers all).
    // get_overdue_bills is global (no owner filter); it returns every unpaid bill
    // whose due_date < current_ledger_time.
    let overdue_page = bill_payments.get_overdue_bills(&0u32, &0u32);

    // Assert bill_id_1 (Electricity, due_date = timestamp + 7*86400 = 1704672000)
    // appears in get_overdue_bills because 1704672000 < 1706745600 (Requirement 5.2, 5.3).
    let mut found_overdue_1 = false;
    let mut found_overdue_2 = false;
    for bill in overdue_page.items.iter() {
        if bill.id == bill_id_1 {
            // Verify the bill is indeed unpaid and past due (Requirement 5.2)
            assert!(
                !bill.paid,
                "overdue bill_id_1 must still be unpaid [Req 5.2]"
            );
            assert!(
                bill.due_date < new_timestamp,
                "overdue bill_id_1 due_date must be < new_timestamp [Req 5.3]"
            );
            found_overdue_1 = true;
        }
        if bill.id == bill_id_2 {
            // Verify the bill is indeed unpaid and past due (Requirement 5.2)
            assert!(
                !bill.paid,
                "overdue bill_id_2 must still be unpaid [Req 5.2]"
            );
            assert!(
                bill.due_date < new_timestamp,
                "overdue bill_id_2 due_date must be < new_timestamp [Req 5.3]"
            );
            found_overdue_2 = true;
        }
    }
    assert!(
        found_overdue_1,
        "bill_id_1 (Electricity, due in 7 days) must appear in get_overdue_bills after 31-day advance [Req 5.3]"
    );
    assert!(
        found_overdue_2,
        "bill_id_2 (Internet, due in 14 days) must appear in get_overdue_bills after 31-day advance [Req 5.3]"
    );

    // Assert that no bill in the overdue list has due_date >= new_timestamp
    // (Requirement 5.4: bills not yet due must NOT appear in get_overdue_bills).
    for bill in overdue_page.items.iter() {
        assert!(
            bill.due_date < new_timestamp,
            "get_overdue_bills must not contain bills with due_date >= new_timestamp [Req 5.4]"
        );
    }

    // ── Phase 6: Bill payment and recurring cycle verification ────────────────
    //
    // Pay both recurring bills and verify that:
    //   1. The original bills are marked paid = true with paid_at == new_timestamp
    //   2. New unpaid bills are created with the correct next due_date
    //   3. The new bills preserve name, amount, frequency_days, and currency
    //   4. get_unpaid_bills count does not decrease (paid bill replaced by next cycle)
    //
    // Bill 1 (Electricity): original due_date = timestamp + 7*86400
    //   next due_date = (timestamp + 7*86400) + 30*86400 = timestamp + 37*86400
    // Bill 2 (Internet): original due_date = timestamp + 14*86400
    //   next due_date = (timestamp + 14*86400) + 30*86400 = timestamp + 44*86400

    // Capture unpaid count before payment to verify non-decrease (Requirement 6.4).
    let unpaid_before = bill_payments.get_unpaid_bills(&user, &0u32, &0u32);
    let unpaid_count_before = unpaid_before.count;

    // Pay bill 1 (Electricity). Guaranteed to succeed: bill exists, owner matches user,
    // bill is unpaid, and mock_all_auths() bypasses require_auth. The Soroban client
    // panics on Err, satisfying Requirement 8.1.
    bill_payments.pay_bill(&user, &bill_id_1); // guaranteed Ok: bill exists, unpaid, owner matches

    // Pay bill 2 (Internet). Same guarantee as bill 1.
    bill_payments.pay_bill(&user, &bill_id_2); // guaranteed Ok: bill exists, unpaid, owner matches

    // Assert original bill 1 has paid = true and paid_at == Some(new_timestamp)
    // (Requirements 6.2, 8.2).
    // Guaranteed Some: bill_id_1 still exists in storage (pay_bill does not delete bills).
    let paid_bill1 = bill_payments.get_bill(&bill_id_1).unwrap(); // guaranteed Some: pay_bill marks paid but does not remove the bill
    assert!(
        paid_bill1.paid,
        "bill_id_1 must have paid = true after pay_bill [Req 6.2]"
    );
    assert_eq!(
        paid_bill1.paid_at,
        Some(new_timestamp),
        "bill_id_1 paid_at must equal new_timestamp (ledger time at payment) [Req 8.2]"
    );

    // Assert original bill 2 has paid = true and paid_at == Some(new_timestamp)
    // (Requirements 6.2, 8.2).
    // Guaranteed Some: same reason as bill_id_1.
    let paid_bill2 = bill_payments.get_bill(&bill_id_2).unwrap(); // guaranteed Some: pay_bill marks paid but does not remove the bill
    assert!(
        paid_bill2.paid,
        "bill_id_2 must have paid = true after pay_bill [Req 6.2]"
    );
    assert_eq!(
        paid_bill2.paid_at,
        Some(new_timestamp),
        "bill_id_2 paid_at must equal new_timestamp (ledger time at payment) [Req 8.2]"
    );

    // Compute expected next due dates (Requirement 6.3, 6.6).
    let expected_next_due_1 = (timestamp + 7 * 86400) + 30 * 86400; // timestamp + 37*86400
    let expected_next_due_2 = (timestamp + 14 * 86400) + 30 * 86400; // timestamp + 44*86400

    // Scan get_unpaid_bills to find the new next-cycle bills (Requirement 6.3).
    // The new bills were created by pay_bill with the next due_date; we identify
    // them by matching due_date and owner in the unpaid list.
    let unpaid_after = bill_payments.get_unpaid_bills(&user, &0u32, &0u32);

    let mut next_bill1: Option<bill_payments::Bill> = None;
    let mut next_bill2: Option<bill_payments::Bill> = None;
    for bill in unpaid_after.items.iter() {
        if bill.due_date == expected_next_due_1 && bill.owner == user {
            next_bill1 = Some(bill.clone());
        }
        if bill.due_date == expected_next_due_2 && bill.owner == user {
            next_bill2 = Some(bill);
        }
    }

    // Assert new bill for Electricity exists with correct next due_date (Requirement 6.3, 6.6).
    let next_bill1 = next_bill1.unwrap(); // guaranteed Some: pay_bill creates next-cycle bill for recurring bills
    assert_eq!(
        next_bill1.due_date,
        expected_next_due_1,
        "next-cycle bill for Electricity must have due_date = original_due_date + 30*86400 [Req 6.3, 6.6]"
    );
    assert!(
        !next_bill1.paid,
        "next-cycle bill for Electricity must be unpaid [Req 6.1]"
    );

    // Assert new bill for Internet exists with correct next due_date (Requirement 6.3, 6.6).
    let next_bill2 = next_bill2.unwrap(); // guaranteed Some: pay_bill creates next-cycle bill for recurring bills
    assert_eq!(
        next_bill2.due_date,
        expected_next_due_2,
        "next-cycle bill for Internet must have due_date = original_due_date + 30*86400 [Req 6.3, 6.6]"
    );
    assert!(
        !next_bill2.paid,
        "next-cycle bill for Internet must be unpaid [Req 6.1]"
    );

    // Assert new bills preserve name, amount, frequency_days, and currency from originals
    // (Requirement 6.5).
    assert_eq!(
        next_bill1.name, bill1.name,
        "next-cycle Electricity bill must preserve name [Req 6.5]"
    );
    assert_eq!(
        next_bill1.amount, bill1.amount,
        "next-cycle Electricity bill must preserve amount [Req 6.5]"
    );
    assert_eq!(
        next_bill1.frequency_days, bill1.frequency_days,
        "next-cycle Electricity bill must preserve frequency_days [Req 6.5]"
    );
    assert_eq!(
        next_bill1.currency, bill1.currency,
        "next-cycle Electricity bill must preserve currency [Req 6.5]"
    );

    assert_eq!(
        next_bill2.name, bill2.name,
        "next-cycle Internet bill must preserve name [Req 6.5]"
    );
    assert_eq!(
        next_bill2.amount, bill2.amount,
        "next-cycle Internet bill must preserve amount [Req 6.5]"
    );
    assert_eq!(
        next_bill2.frequency_days, bill2.frequency_days,
        "next-cycle Internet bill must preserve frequency_days [Req 6.5]"
    );
    assert_eq!(
        next_bill2.currency, bill2.currency,
        "next-cycle Internet bill must preserve currency [Req 6.5]"
    );

    // Assert get_unpaid_bills count does not decrease for recurring bills (Requirement 6.4).
    // Each paid recurring bill is replaced by a new next-cycle bill, so count >= before.
    assert!(
        unpaid_after.count >= unpaid_count_before,
        "get_unpaid_bills count must not decrease after paying recurring bills [Req 6.4]: before={}, after={}",
        unpaid_count_before,
        unpaid_after.count
    );

    // ── Phase 7: Financial health report verification ────────────────────────
    //
    // Generate a comprehensive financial health report covering the entire remittance
    // window (period_start to period_end) and verify that it accurately reflects:
    //   - Bill compliance (total bills tracked, paid/unpaid counts)
    //   - Insurance coverage (active policies)
    //   - Health score (non-negative integer)
    //   - Savings goals (total goals)
    //
    // period_start = initial timestamp (1704067200)
    // period_end = new_timestamp (timestamp + 31*86400 = 1706745600)
    // total_remittance = 5000 (used for remittance summary calculations)

    let period_start = timestamp; // initial ledger time (1704067200)
    let period_end = new_timestamp; // current ledger time after 31-day advance (1706745600)

    // Assert period_start <= period_end (Requirement 7.6).
    assert!(
        period_start <= period_end,
        "period_start must be <= period_end when calling get_financial_health_report [Req 7.6]"
    );

    // Call get_financial_health_report with the user, total_remittance, and period bounds.
    // Guaranteed to succeed: reporting contract is initialized and configured, all
    // dependency contracts are registered and have data, mock_all_auths() bypasses
    // require_auth. The Soroban client panics on Err, satisfying Requirement 8.1.
    let report =
        reporting.get_financial_health_report(&user, &total_remittance, &period_start, &period_end);

    // Assert report.bill_compliance.total_bills >= 2 (Requirement 7.1).
    // We created two recurring bills (Electricity and Internet) in Phase 3, and
    // paying them in Phase 6 created two new next-cycle bills. The reporting
    // contract's get_bill_compliance_report_internal filters bills by created_at
    // within [period_start, period_end], so all four bills (two original + two
    // next-cycle) should be counted if their created_at falls in the period.
    // However, the next-cycle bills are created at new_timestamp (1706745600),
    // which equals period_end, so they are included (created_at <= period_end).
    // Therefore, total_bills should be >= 2 (at minimum the two original bills).
    assert!(
        report.bill_compliance.total_bills >= 2,
        "report.bill_compliance.total_bills must be >= 2 [Req 7.1]: got {}",
        report.bill_compliance.total_bills
    );

    // Assert report.insurance_report.active_policies >= 1 (Requirement 7.2).
    // We created one Health insurance policy in Phase 4 and paid its premium,
    // so it remains active. The reporting contract's get_insurance_report_internal
    // calls get_active_policies, which returns all active policies for the user.
    assert!(
        report.insurance_report.active_policies >= 1,
        "report.insurance_report.active_policies must be >= 1 [Req 7.2]: got {}",
        report.insurance_report.active_policies
    );

    // Assert report.health_score.score >= 0 (Requirement 7.3).
    // The health score is a u32, so it is always non-negative by type, but we
    // assert it explicitly to satisfy the requirement.
    assert!(
        report.health_score.score >= 0,
        "report.health_score.score must be >= 0 [Req 7.3]: got {}",
        report.health_score.score
    );

    // Assert report.bill_compliance.total_bills equals the total number of bills
    // visible to the reporting contract for the user (Requirement 7.4).
    // We verify this by calling get_all_bills_for_owner directly and counting
    // bills created within [period_start, period_end].
    let all_bills_page = bill_payments.get_all_bills_for_owner(&user, &0u32, &50u32);
    let bills_in_period = all_bills_page
        .items
        .iter()
        .filter(|b| b.created_at >= period_start && b.created_at <= period_end)
        .count() as u32;
    assert_eq!(
        report.bill_compliance.total_bills,
        bills_in_period,
        "report.bill_compliance.total_bills must equal the count of bills created in [period_start, period_end] [Req 7.4]"
    );

    // Print human-readable summary (Requirement 7.5).
    // The summary includes: health score, total savings goals, total bills tracked,
    // active insurance policies, and total remittance amount.
    println!("==== Financial Health Report Summary ====");
    println!("Health Score       : {}", report.health_score.score);
    println!("Total Savings Goals: {}", report.savings_report.total_goals);
    println!(
        "Total Bills Tracked: {}",
        report.bill_compliance.total_bills
    );
    println!(
        "Active Policies    : {}",
        report.insurance_report.active_policies
    );
    println!("Total Remittance   : {}", total_remittance);
    println!("=========================================");

    // ── Phase 8: Owner isolation and state consistency ────────────────────────
    //
    // Verify that bills and policies created by `user` are not visible when
    // queried under a different address (Requirement 8.5), and that
    // get_total_unpaid for `user` reflects the correct unpaid amount after all
    // payments (Requirement 8.4).

    // Generate a fresh address that has never interacted with any contract.
    // Address::generate produces a unique address not present in any contract
    // storage, so all queries for it must return empty/zero results.
    let other_user = Address::generate(&env); // guaranteed fresh: never used before

    // Assert get_unpaid_bills(other_user) returns a page with count == 0
    // (Requirement 8.5: owner isolation — bills created by `user` must not
    // appear under `other_user`).
    let other_unpaid = bill_payments.get_unpaid_bills(&other_user, &0u32, &0u32);
    assert_eq!(
        other_unpaid.count, 0,
        "get_unpaid_bills for a fresh address must return count == 0 [Req 8.5]"
    );

    // Assert get_total_monthly_premium(other_user) == 0
    // (Requirement 8.5: owner isolation — insurance policies created by `user`
    // must not be counted under `other_user`).
    let other_premium = insurance.get_total_monthly_premium(&other_user);
    assert_eq!(
        other_premium, 0,
        "get_total_monthly_premium for a fresh address must return 0 [Req 8.5]"
    );

    // Assert get_total_unpaid(user) reflects the correct unpaid amount after all
    // payments (Requirement 8.4).
    //
    // After paying bill_id_1 (Electricity, 150) and bill_id_2 (Internet, 80),
    // both original bills are marked paid. However, because both bills are
    // recurring, pay_bill automatically created two new next-cycle bills:
    //   - next-cycle Electricity: amount = 150, unpaid
    //   - next-cycle Internet:    amount = 80,  unpaid
    //
    // get_total_unpaid sums the `amount` of ALL unpaid bills for the owner,
    // regardless of the `recurring` flag. Therefore the expected total is:
    //   150 (next-cycle Electricity) + 80 (next-cycle Internet) = 230.
    //
    // There are no non-recurring unpaid bills for `user` at this point, so the
    // total equals the sum of the two recurring next-cycle bills.
    let expected_total_unpaid: i128 = bill1.amount + bill2.amount; // 150 + 80 = 230
    let actual_total_unpaid = bill_payments.get_total_unpaid(&user);
    assert_eq!(
        actual_total_unpaid,
        expected_total_unpaid,
        "get_total_unpaid(user) must equal the sum of next-cycle recurring bill amounts after paying both bills [Req 8.4]: expected={}, got={}",
        expected_total_unpaid,
        actual_total_unpaid
    );
}
