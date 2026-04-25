# Implementation Plan: Scenarios – Recurring Obligations

## Overview

Implement `test_recurring_obligations_flow` in `scenarios/tests/flow.rs` as a deterministic end-to-end integration test covering the full remittance window lifecycle across six Soroban contracts. Property-based tests using `proptest` validate correctness properties from the design.

## Tasks

- [x] 1. Set up test infrastructure and dependencies
  - Add `proptest` to `[dev-dependencies]` in `scenarios/Cargo.toml`
  - Create or update `scenarios/tests/flow.rs` with required imports: contract client types for RemittanceSplit, BillPayments, Insurance, SavingsGoalContract, FamilyWallet, and ReportingContract
  - Verify `scenarios::tests::setup_env()` is accessible and sets ledger timestamp to 1704067200
  - _Requirements: 1.5, 9.1, 9.3_

- [x] 2. Implement environment and contract initialization phase
  - [x] 2.1 Register all six contracts and initialize Reporting
    - In `test_recurring_obligations_flow`, call `env.register_contract(None, ...)` for all six contracts
    - Call `env.mock_all_auths()` to bypass auth checks
    - Call `reporting.init(admin)` and `reporting.configure_addresses(...)` with all contract addresses
    - Use `unwrap()` with comments explaining guaranteed success for init calls
    - _Requirements: 1.1, 1.2, 1.3, 1.4, 1.5_

  - [ ]\* 2.2 Write property test for split allocation invariant
    - **Property 1: Split allocation invariant**
    - **Validates: Requirements 2.4, 2.5**
    - `// Feature: scenarios-recurring-obligations, Property 1: split allocation invariant`

  - [ ]\* 2.3 Write property test for invalid percentages rejection
    - **Property 2: Invalid percentages rejected**
    - **Validates: Requirements 2.3**
    - `// Feature: scenarios-recurring-obligations, Property 2: invalid percentages rejected`

- [x] 3. Implement remittance split configuration phase
  - [x] 3.1 Configure split and assert invariants
    - Call `remittance_split.initialize_split(user, savings_pct, bills_pct, insurance_pct, family_pct)` with percentages summing to 100
    - Assert return value is `Ok(true)` (or equivalent success)
    - Call `remittance_split.get_config(user)` and assert stored percentages match inputs
    - Call `remittance_split.calculate_split(user, total_remittance)` and assert sum of allocations equals `total_remittance`
    - _Requirements: 2.1, 2.2, 2.4, 2.5, 8.6_

  - [ ]\* 3.2 Write property test for split config round trip
    - **Property 13: Split config round trip**
    - **Validates: Requirements 8.6**
    - `// Feature: scenarios-recurring-obligations, Property 13: split config round trip`

- [x] 4. Implement recurring bill creation phase
  - [x] 4.1 Create two recurring bills and assert initial state
    - Call `bill_payments.create_bill(...)` twice with distinct names, amounts, due dates (> current ledger time), and `frequency_days` values; store returned IDs as `bill_id_1` and `bill_id_2`
    - Assert each bill is retrievable via `get_bill(id)` with `paid = false`
    - Assert both bills appear in `get_unpaid_bills(user)`
    - Add inline comment for each `unwrap()` explaining guaranteed `Some`
    - _Requirements: 3.1, 3.2, 3.3, 3.5_

  - [ ]\* 4.2 Write property test for bill creation produces retrievable unpaid bill
    - **Property 3: Bill creation produces retrievable unpaid bill**
    - **Validates: Requirements 3.1, 3.3, 3.5**
    - `// Feature: scenarios-recurring-obligations, Property 3: bill creation produces retrievable unpaid bill`

  - [ ]\* 4.3 Write property test for invalid due date rejection
    - **Property 4: Invalid due date rejected**
    - **Validates: Requirements 3.4**
    - `// Feature: scenarios-recurring-obligations, Property 4: invalid due date rejected`

- [x] 5. Implement insurance policy creation and premium payment phase
  - [x] 5.1 Create policy and pay premium, assert state
    - Call `insurance.create_policy(user, coverage_type, monthly_premium, coverage_amount)` and store `policy_id`
    - Assert `get_policy(policy_id).active == true`
    - Assert `get_total_monthly_premium(user) == monthly_premium`
    - Call `insurance.pay_premium(user, policy_id)` and assert it returns `true`
    - Assert `get_policy(policy_id).next_payment_date == ledger_time + 30 * 86400`
    - Call `insurance.pay_premium(user, nonexistent_id)` and assert it returns `false`
    - _Requirements: 4.1, 4.2, 4.3, 4.4, 4.5, 4.6_

  - [ ]\* 5.2 Write property test for policy creation produces active policy
    - **Property 9: Policy creation produces active policy**
    - **Validates: Requirements 4.1, 4.2**
    - `// Feature: scenarios-recurring-obligations, Property 9: policy creation produces active policy`

  - [ ]\* 5.3 Write property test for insurance premium payment sets next_payment_date
    - **Property 8: Insurance premium payment sets next_payment_date**
    - **Validates: Requirements 4.3, 4.4, 8.3**
    - `// Feature: scenarios-recurring-obligations, Property 8: insurance premium payment sets next_payment_date`

  - [ ]\* 5.4 Write property test for total monthly premium equals sum of active premiums
    - **Property 10: Total monthly premium equals sum of active premiums**
    - **Validates: Requirements 4.6**
    - `// Feature: scenarios-recurring-obligations, Property 10: total monthly premium equals sum of active premiums`

- [x] 6. Checkpoint – Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.

- [x] 7. Implement ledger time advancement phase
  - [x] 7.1 Advance ledger time and assert overdue detection
    - Capture current `LedgerInfo` fields from `setup_env()` configuration
    - Advance `env.ledger().set(LedgerInfo { timestamp: timestamp + 31 * 86400, ... })` preserving all other fields
    - Assert bills with `due_date < new_timestamp` appear in `get_overdue_bills(user)`
    - Assert bills with `due_date >= new_timestamp` do not appear in `get_overdue_bills(user)`
    - _Requirements: 5.1, 5.2, 5.3, 5.4, 5.5_

  - [ ]\* 7.2 Write property test for overdue detection correctness
    - **Property 5: Overdue detection correctness**
    - **Validates: Requirements 5.2, 5.3, 5.4**
    - `// Feature: scenarios-recurring-obligations, Property 5: overdue detection correctness`

- [x] 8. Implement bill payment and recurring cycle verification phase
  - [x] 8.1 Pay recurring bills and verify next-cycle scheduling
    - Call `bill_payments.pay_bill(user, bill_id_1)` and `pay_bill(user, bill_id_2)`
    - Assert original bills have `paid = true` and `paid_at == Some(current_ledger_time)`
    - Assert new unpaid bills exist with `due_date == original_due_date + frequency_days * 86400`
    - Assert new bills preserve `name`, `amount`, `frequency_days`, and `currency` from originals
    - Assert `get_unpaid_bills(user).len() >= 2` (count does not decrease)
    - _Requirements: 6.1, 6.2, 6.3, 6.4, 6.5, 6.6, 8.2_

  - [ ]\* 8.2 Write property test for recurring bill round-trip scheduling
    - **Property 6: Recurring bill round-trip scheduling**
    - **Validates: Requirements 6.1, 6.2, 6.3, 6.5, 6.6**
    - `// Feature: scenarios-recurring-obligations, Property 6: recurring bill round-trip scheduling`

  - [ ]\* 8.3 Write property test for unpaid count non-decrease for recurring bills
    - **Property 7: Unpaid count non-decrease for recurring bills**
    - **Validates: Requirements 6.4**
    - `// Feature: scenarios-recurring-obligations, Property 7: unpaid count non-decrease for recurring bills`

  - [ ]\* 8.4 Write property test for bill paid_at equals ledger time at payment
    - **Property 11: Bill paid_at equals ledger time at payment**
    - **Validates: Requirements 8.2**
    - `// Feature: scenarios-recurring-obligations, Property 11: bill paid_at equals ledger time at payment`

- [x] 9. Implement financial health report verification phase
  - [x] 9.1 Generate and assert financial health report
    - Call `reporting.get_financial_health_report(user, period_start, period_end)` where `period_start <= period_end`
    - Assert `report.bill_compliance.total_bills >= 2`
    - Assert `report.insurance_report.active_policies >= 1`
    - Assert `report.health_score.score >= 0`
    - Assert `report.bill_compliance.total_bills` equals total bills visible to reporting for the user
    - Print human-readable summary via `println!` including health score, total savings goals, total bills tracked, active insurance policies, and total remittance amount
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

  - [ ]\* 9.2 Write property test for health score is non-negative
    - **Property 14: Health score is non-negative**
    - **Validates: Requirements 7.3**
    - `// Feature: scenarios-recurring-obligations, Property 14: health score is non-negative`

- [x] 10. Implement state consistency and owner isolation assertions
  - [x] 10.1 Assert owner isolation and state consistency
    - Create a second address `other_user` and assert `get_unpaid_bills(other_user)` returns empty
    - Assert `get_total_monthly_premium(other_user) == 0`
    - Assert `get_total_unpaid(user)` reflects only unpaid non-recurring bills after all payments
    - _Requirements: 8.4, 8.5_

  - [ ]\* 10.2 Write property test for owner isolation
    - **Property 12: Owner isolation**
    - **Validates: Requirements 8.5**
    - `// Feature: scenarios-recurring-obligations, Property 12: owner isolation`

- [x] 11. Final checkpoint – Ensure all tests pass
  - Ensure all tests pass, ask the user if questions arise.
  - Verify scenario runs via `cargo test -p scenarios -- --nocapture` without external network access
  - _Requirements: 9.3, 9.6_

## Notes

- Tasks marked with `*` are optional and can be skipped for faster MVP
- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests use `proptest` with minimum 100 iterations per property
- All `unwrap()` calls must include a comment explaining why the value is guaranteed
