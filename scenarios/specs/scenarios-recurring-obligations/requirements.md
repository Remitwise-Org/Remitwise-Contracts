# Requirements Document

## Introduction

This feature adds a realistic end-to-end scenario test in `scenarios/tests/flow.rs` that exercises the full remittance window lifecycle: configuring a remittance split, creating multiple recurring bills, creating an insurance policy and paying its premium, advancing ledger time across billing cycles, and verifying that the reporting contract reflects correct compliance and coverage signals throughout. The scenario must be deterministic, isolated, well-documented, and achieve at least 95% test coverage of the scenario code paths.

## Glossary

- **System/Scenario**: An integration test exercising multiple contracts together in a realistic user flow.
- **RemittanceSplit**: Contract that configures how incoming remittance funds are allocated across categories.
- **BillPayments**: Contract that manages recurring and one-off bill obligations.
- **Insurance**: Contract that manages insurance policies and premium payments.
- **Reporting**: Contract that aggregates data from all contracts and produces a FinancialHealthReport.
- **Remittance_Window**: A defined time period (period_start to period_end) over which a remittance is processed.
- **Recurring_Bill**: A Bill with recurring = true and a positive frequency_days, which auto-generates the next bill upon payment.
- **Insurance_Policy**: An InsurancePolicy record with a monthly_premium, coverage_amount, and next_payment_date.
- **Premium_Payment**: The act of calling pay_premium on an active Insurance_Policy, advancing its next_payment_date by 30 days.
- **Financial_Health_Report**: The FinancialHealthReport struct returned by Reporting.get_financial_health_report.
- **Ledger_Time**: The env.ledger().timestamp() value used by all contracts as the authoritative clock.
- **Env**: The Soroban Env test environment, configured via scenarios::tests::setup_env().

---

## Requirements

### Requirement 1: Environment and Contract Initialization

**User Story:** As a scenario test author, I want all contracts initialized in a shared test environment, so that cross-contract interactions reflect realistic on-chain conditions.

#### Acceptance Criteria

1. THE Scenario SHALL register RemittanceSplit, BillPayments, Insurance, Reporting, SavingsGoalContract, and FamilyWallet contracts in the same Env instance.
2. THE Scenario SHALL call Reporting.init with a designated admin address before any other reporting calls.
3. THE Scenario SHALL call Reporting.configure_addresses with the addresses of all registered contracts before generating any report.
4. IF Reporting.init or Reporting.configure_addresses returns an error, THEN THE Scenario SHALL panic with a descriptive message.
5. THE Env SHALL be initialized via scenarios::tests::setup_env(), which sets Ledger_Time to 1704067200 (2024-01-01T00:00:00Z).

---

### Requirement 2: Remittance Split Configuration

**User Story:** As a remittance sender, I want to configure how my funds are split across obligations, so that bills and insurance premiums are funded from the correct allocation buckets.

#### Acceptance Criteria

1. WHEN RemittanceSplit.initialize_split is called with valid percentages summing to 100, THE RemittanceSplit SHALL store the split configuration and emit a SplitInitializedEvent.
2. THE Scenario SHALL configure the split with savings, bills, insurance, and family percentages that sum to exactly 100.
3. IF the percentages do not sum to 100, THEN THE RemittanceSplit SHALL return a RemittanceSplitError and THE Scenario SHALL not proceed.
4. WHEN RemittanceSplit.calculate_split is called with a total remittance amount, THE RemittanceSplit SHALL return allocation amounts proportional to the configured percentages.
5. THE Scenario SHALL assert that the sum of all calculated allocation amounts equals the total remittance amount (allocation invariant).

---

### Requirement 3: Recurring Bill Creation

**User Story:** As a user, I want to create recurring bills for regular obligations, so that each payment cycle automatically schedules the next due date.

#### Acceptance Criteria

1. WHEN BillPayments.create_bill is called with recurring = true and a positive frequency_days, THE BillPayments SHALL create a bill with paid = false and return a unique bill ID.
2. THE Scenario SHALL create at least two distinct recurring bills (e.g., electricity and internet) with different amounts, due dates, and frequency_days values.
3. THE Scenario SHALL assert that each created bill is retrievable via BillPayments.get_bill and has paid = false immediately after creation.
4. IF BillPayments.create_bill is called with a due_date less than the current Ledger_Time, THEN THE BillPayments SHALL return BillPaymentsError::InvalidDueDate and THE Scenario SHALL not proceed.
5. THE Scenario SHALL assert that BillPayments.get_unpaid_bills returns all created recurring bills before any payment is made.

---

### Requirement 4: Insurance Policy Creation and Premium Payment

**User Story:** As a user, I want to create an insurance policy and pay its premium within the same remittance window, so that my coverage remains active and is reflected in the financial health report.

#### Acceptance Criteria

1. WHEN Insurance.create_policy is called with a valid coverage_type, monthly_premium, and coverage_amount, THE Insurance SHALL create an active policy and return a unique policy ID.
2. THE Scenario SHALL assert that the created policy is retrievable via Insurance.get_policy with active = true immediately after creation.
3. WHEN Insurance.pay_premium is called by the policy owner on an active policy, THE Insurance SHALL update next_payment_date to current_Ledger_Time + 30 \* 86400 and return true.
4. THE Scenario SHALL assert that after pay_premium, the policy's next_payment_date is strictly greater than the Ledger_Time at the time of payment.
5. IF Insurance.pay_premium is called on a non-existent policy ID, THEN THE Insurance SHALL return false and THE Scenario SHALL assert this outcome.
6. THE Scenario SHALL assert that Insurance.get_total_monthly_premium for the user equals the sum of all active policy premiums after creation.

---

### Requirement 5: Ledger Time Advancement

**User Story:** As a scenario test author, I want to advance ledger time to simulate billing cycles, so that overdue detection and next-cycle scheduling can be verified.

#### Acceptance Criteria

1. THE Scenario SHALL advance Ledger_Time by at least one full billing cycle (minimum 30 days = 30 \* 86400 seconds) after creating bills and policies.
2. WHEN Ledger_Time is advanced past a bill's due_date, THE BillPayments SHALL include that bill in the result of get_overdue_bills.
3. THE Scenario SHALL assert that bills with due_date < current_Ledger_Time appear in get_overdue_bills after the time advance.
4. THE Scenario SHALL assert that bills with due_date >= current_Ledger_Time do not appear in get_overdue_bills.
5. WHILE Ledger_Time is advanced, THE Scenario SHALL preserve the protocol_version, network_id, base_reserve, min_temp_entry_ttl, min_persistent_entry_ttl, and max_entry_ttl values from the initial setup_env configuration.

---

### Requirement 6: Bill Payment and Recurring Cycle Verification

**User Story:** As a user, I want to pay recurring bills and verify that the next cycle is automatically created, so that my obligation schedule remains continuous.

#### Acceptance Criteria

1. WHEN BillPayments.pay_bill is called on a recurring bill, THE BillPayments SHALL mark the bill as paid = true and create a new bill with due_date = old_due_date + (frequency_days \* 86400).
2. THE Scenario SHALL assert that after paying a recurring bill, the original bill has paid = true.
3. THE Scenario SHALL assert that after paying a recurring bill, a new unpaid bill exists with the correct next due_date (original due_date + frequency_days \* 86400).
4. THE Scenario SHALL assert that BillPayments.get_unpaid_bills count does not decrease for recurring bills (paid bill is replaced by next cycle bill).
5. THE Scenario SHALL assert that the new recurring bill preserves the original bill's name, amount, frequency_days, and currency.
6. FOR ALL recurring bills paid in the scenario, THE Scenario SHALL verify that the next-cycle bill's due_date equals the previous bill's due_date plus frequency_days \* 86400 (round-trip scheduling property).

---

### Requirement 7: Financial Health Report Verification

**User Story:** As a user, I want the financial health report to accurately reflect my bill compliance and insurance coverage after processing payments, so that I can trust the reporting signals.

#### Acceptance Criteria

1. WHEN Reporting.get_financial_health_report is called after all payments, THE Reporting SHALL return a Financial_Health_Report with bill_compliance.total_bills >= 2.
2. THE Scenario SHALL assert that report.insurance_report.active_policies >= 1 after creating and paying the insurance premium.
3. THE Scenario SHALL assert that report.health_score.score is a non-negative integer.
4. THE Scenario SHALL assert that report.bill_compliance.total_bills equals the total number of bills visible to the reporting contract for the user.
5. WHEN the scenario is run with --nocapture, THE Scenario SHALL print a human-readable summary including: health score, total savings goals, total bills tracked, active insurance policies, and total remittance amount.
6. THE Scenario SHALL assert that the period_start passed to get_financial_health_report is less than or equal to period_end.

---

### Requirement 8: State Consistency and Event Integrity

**User Story:** As a developer, I want all contract state changes and emitted events to remain consistent across the full scenario, so that the system behaves correctly under realistic multi-contract interactions.

#### Acceptance Criteria

1. THE Scenario SHALL assert that no contract call returns an unexpected error at any step of the flow.
2. WHEN a bill is paid, THE BillPayments SHALL set the bill's paid_at field, and THE Scenario SHALL verify paid_at is Some and equals the Ledger_Time at the time of payment.
3. WHEN a premium is paid, THE Insurance SHALL update the policy's next_payment_date, and THE Scenario SHALL verify the updated value equals Ledger_Time + 30 \* 86400.
4. THE Scenario SHALL assert that BillPayments.get_total_unpaid for the user reflects only unpaid non-recurring bills after all payments.
5. THE Scenario SHALL assert that owner isolation is maintained: bills and policies created by the test user are not visible when queried under a different address.
6. THE Scenario SHALL assert that the split configuration stored in RemittanceSplit is retrievable via get_config and matches the values passed to initialize_split.

---

### Requirement 9: Test Coverage and Documentation

**User Story:** As a developer, I want the scenario to be well-documented and achieve at least 95% test coverage, so that the code is maintainable and reviewable.

#### Acceptance Criteria

1. THE Scenario SHALL be implemented in scenarios/tests/flow.rs as a #[test] function named test_recurring_obligations_flow.
2. THE Scenario SHALL include inline comments explaining each major phase: initialization, split configuration, bill creation, policy creation, time advancement, payment processing, and report verification.
3. THE Scenario SHALL be runnable via cargo test -p scenarios -- --nocapture without requiring external network access or token funding.
4. WHERE the scenario exercises a code path that could panic, THE Scenario SHALL use unwrap() only with an accompanying comment explaining why the value is guaranteed to be Some.
5. THE Scenario SHALL achieve at least 95% line coverage of the scenarios crate as measured by cargo tarpaulin or equivalent.
6. THE Scenario SHALL not depend on any external state, randomness, or timing outside of the controlled Env ledger.
