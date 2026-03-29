# Integration Tests — Remitwise Contracts

## Overview

This crate contains integration tests for the Remitwise smart contract ecosystem.
Tests cover multi-contract interactions, rollback semantics, accounting consistency,
and recovery behavior across all remittance flow legs.

## Running the Tests

### Run orchestrator unit tests only
```bash
cargo test -p orchestrator
```

### Run integration tests only
```bash
cargo test -p integration_tests
```

### Run all tests
```bash
cargo test -p orchestrator && cargo test -p integration_tests
```

test/orchestrator-insurance-payment-failure-paths
Contact
- For modifications to the canonical schema, update `remitwise-common/src/lib.rs`
  and coordinate with indexers consuming events.

## Orchestrator Insurance Failure Safety

### What was added
- `test_orchestrator_flow_inactive_policy_reverts_downstream_state`
- `test_orchestrator_flow_missing_policy_reverts_downstream_state`

### Security assumptions validated
- Orchestrated remittance flow is fail-closed for insurance failure paths.
- If insurance payment fails (inactive or missing policy), prior downstream calls
  (savings deposit and bill payment) do not leave persistent state mutations.
- No hidden state mutation survives transaction failure in tested paths.

### Notes for reviewers
- Tests intentionally use deterministic split and permission mocks so assertions
  focus only on orchestrator failure semantics and atomicity.
- Existing multi-contract tests remain unchanged and continue to verify baseline
  cross-contract behavior.
# Integration Tests

This module contains integration tests that verify the interaction between multiple RemitWise contracts.

## Overview

The integration tests simulate real-world user flows by deploying multiple contracts in a test environment and executing operations across them in sequence.

## Test Coverage

### `test_multi_contract_user_flow`

Simulates a complete user journey:

1. **Deploy Contracts**: Deploys all four core contracts (remittance_split, savings_goals, bill_payments, insurance)
2. **Initialize Split**: Configures remittance allocation (40% spending, 30% savings, 20% bills, 10% insurance)
3. **Create Entities**:
   - Creates a savings goal (Education Fund: 10,000 target)
   - Creates a recurring bill (Electricity: 500/month)
   - Creates an insurance policy (Health: 200/month premium, 50,000 coverage)
4. **Calculate Split**: Processes a 10,000 remittance and verifies allocation
5. **Verify Amounts**: Ensures calculated amounts match expected percentages and sum to total

### `test_split_with_rounding`

Tests edge cases with rounding:

- Uses percentages that don't divide evenly (33%, 33%, 17%, 17%)
- Verifies that the insurance category receives the remainder to ensure total equals original amount
- Confirms no funds are lost or created due to rounding

### `test_multiple_entities_creation`

Tests creating multiple entities across contracts:

- Creates 2 savings goals (Emergency Fund, Vacation)
- Creates 2 recurring bills (Rent, Internet)
- Creates 2 insurance policies (Life, Emergency Coverage)
- Verifies all entities are created successfully with unique IDs

## Running the Tests

From the workspace root:


### Run a specific test by name
main
```bash
cargo test -p orchestrator test_rollback_bills_leg_bill_not_found
cargo test -p integration_tests test_integration_rollback_insurance_leg
```

### Run with output printed (recommended for first run)
```bash
cargo test -p orchestrator -- --nocapture
cargo test -p integration_tests -- --nocapture
```

---

## Test Structure

### `orchestrator/src/test.rs` — Unit Tests

Unit tests for the orchestrator contract using mock implementations of all
downstream contracts. Tests are self-contained and do not require real contract
deployments.

#### Existing Tests (preserved)
| Test | Description |
|------|-------------|
| `test_execute_savings_deposit_succeeds` | Happy path: valid savings deposit |
| `test_execute_savings_deposit_invalid_goal_fails` | Invalid goal ID panics |
| `test_execute_savings_deposit_spending_limit_exceeded_fails` | Amount over limit denied |
| `test_execute_bill_payment_succeeds` | Happy path: valid bill payment |
| `test_execute_bill_payment_invalid_bill_fails` | Invalid bill ID panics |
| `test_execute_insurance_payment_succeeds` | Happy path: valid insurance payment |
| `test_execute_remittance_flow_succeeds` | Happy path: full flow with correct split |
| `test_execute_remittance_flow_spending_limit_exceeded_fails` | Full flow denied over limit |
| `test_execute_remittance_flow_invalid_amount_fails` | Zero amount rejected |
| `test_get_execution_stats_succeeds` | Stats initialized to zero |
| `test_get_audit_log_succeeds` | Audit log initially empty |

#### Rollback Semantics Tests (new — issue #300)

**Savings Leg Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_savings_leg_goal_not_found` | ROLLBACK-01 | Savings panics → full revert |
| `test_rollback_savings_leg_goal_already_completed` | ROLLBACK-02 | Completed goal → revert |
| `test_rollback_savings_deposit_goal_not_found` | ROLLBACK-03 | Individual deposit: goal not found |
| `test_rollback_savings_deposit_goal_already_completed` | ROLLBACK-04 | Individual deposit: completed goal |

**Bills Leg Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_bills_leg_bill_not_found` | ROLLBACK-05 | Bills panics after savings → full revert |
| `test_rollback_bills_leg_already_paid` | ROLLBACK-06 | Double payment protection → revert |
| `test_rollback_bill_payment_bill_not_found` | ROLLBACK-07 | Individual payment: bill not found |
| `test_rollback_bill_payment_already_paid` | ROLLBACK-08 | Individual payment: already paid |

**Insurance Leg Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_insurance_leg_policy_not_found` | ROLLBACK-09 | Insurance panics after savings+bills → full revert |
| `test_rollback_insurance_leg_inactive_policy` | ROLLBACK-10 | Inactive policy soft/hard failure |
| `test_rollback_insurance_payment_policy_not_found` | ROLLBACK-11 | Individual payment: policy not found |

**Permission & Validation Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_permission_denied_before_any_leg` | ROLLBACK-12 | Permission gate stops flow |
| `test_rollback_negative_amount_rejected` | ROLLBACK-13 | Negative amount → InvalidAmount |
| `test_rollback_zero_amount_rejected` | ROLLBACK-14 | Zero amount → InvalidAmount |

**All Legs Fail**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_all_legs_fail` | ROLLBACK-15 | All three legs invalid → fail fast on savings |

**Accounting Consistency**
| Test | ID | Description |
|------|----|-------------|
| `test_accounting_consistency_on_success` | ROLLBACK-16 | Allocations sum to total |
| `test_accounting_split_percentages_correct` | ROLLBACK-17 | 40/30/20/10 split verified |
| `test_accounting_minimum_valid_amount` | ROLLBACK-18 | Boundary: amount = 1 |
| `test_accounting_maximum_valid_amount_at_spending_limit` | ROLLBACK-19 | Boundary: amount = 10000 |
| `test_accounting_one_above_spending_limit_rejected` | ROLLBACK-20 | Boundary: amount = 10001 rejected |

**Independent Operation Rollbacks**
| Test | ID | Description |
|------|----|-------------|
| `test_rollback_failed_savings_does_not_poison_subsequent_call` | ROLLBACK-21 | Rollback state isolation: savings |
| `test_rollback_failed_bill_does_not_poison_subsequent_call` | ROLLBACK-22 | Rollback state isolation: bills |
| `test_rollback_failed_insurance_does_not_poison_subsequent_call` | ROLLBACK-23 | Rollback state isolation: insurance |
| `test_rollback_failed_full_flow_does_not_poison_subsequent_full_flow` | ROLLBACK-24 | Full flow isolation across transactions |

---

### `integration_tests/tests/multi_contract_integration.rs` — Integration Tests

Integration tests deploy real contract implementations alongside mock dependency
contracts to test cross-contract behavior end-to-end.

#### Existing Tests (preserved)
| Test | Description |
|------|-------------|
| `test_multi_contract_user_flow` | Full user flow across all contracts |
| `test_split_with_rounding` | Rounding behavior in split calculation |
| `test_multiple_entities_creation` | Create multiple goals, bills, policies |

#### Rollback Integration Tests (new — issue #300)

**Savings Leg Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_integration_rollback_savings_leg_goal_not_found` | INT-ROLLBACK-01 | Savings panic → atomic revert |
| `test_integration_rollback_bills_leg_after_savings_succeeds` | INT-ROLLBACK-02 | Bills panic after savings → revert all |
| `test_integration_rollback_insurance_leg_after_savings_and_bills_succeed` | INT-ROLLBACK-03 | Insurance panic after savings+bills → revert all |

**Duplicate / Already-Processed Protection**
| Test | ID | Description |
|------|----|-------------|
| `test_integration_rollback_duplicate_bill_payment` | INT-ROLLBACK-04 | Already-paid bill → rollback |
| `test_integration_rollback_completed_savings_goal` | INT-ROLLBACK-05 | Completed goal → rollback |

**Accounting Consistency**
| Test | ID | Description |
|------|----|-------------|
| `test_integration_accounting_split_sums_to_total` | INT-ACCOUNTING-01 | Split sums verified across multiple amounts |
| `test_integration_accounting_flow_result_consistency` | INT-ACCOUNTING-02 | Flow result metadata consistency |

**Recovery After Failure**
| Test | ID | Description |
|------|----|-------------|
| `test_integration_recovery_after_savings_failure` | INT-RECOVERY-01 | Contract recovers after savings rollback |
| `test_integration_recovery_after_bills_failure` | INT-RECOVERY-02 | Contract recovers after bills rollback |
| `test_integration_recovery_after_insurance_failure` | INT-RECOVERY-03 | Contract recovers after insurance rollback |

**Permission Failures**
| Test | ID | Description |
|------|----|-------------|
| `test_integration_permission_denied_stops_flow` | INT-PERMISSION-01 | Spending limit exceeded stops flow |
| `test_integration_invalid_amounts_rejected_early` | INT-PERMISSION-02 | Zero/negative amounts rejected early |

---

## Mock Contract Behavior Reference

### `MockFamilyWallet`
| Input | Behavior |
|-------|----------|
| `amount <= 10_000` (unit tests) | Returns `true` (approved) |
| `amount <= 100_000` (integration tests) | Returns `true` (approved) |
| `amount > limit` | Returns `false` → `PermissionDenied` |

### `MockSavingsGoals`
| `goal_id` | Behavior |
|-----------|----------|
| `1` (or any valid) | Returns `amount` (success) |
| `998` | Panics: "Goal already completed" |
| `999` | Panics: "Goal not found" |

### `MockBillPayments`
| `bill_id` | Behavior |
|-----------|----------|
| `1` (or any valid) | No-op (success) |
| `998` | Panics: "Bill already paid" |
| `999` | Panics: "Bill not found" |

### `MockInsurance`
| `policy_id` | Behavior |
|-------------|----------|
| `1` (or any valid) | Returns `true` (success) |
| `998` | Returns `false` (inactive policy — soft failure) |
| `999` | Panics: "Policy not found" |

### `MockRemittanceSplit`
Always returns a `[40%, 30%, 20%, 10%]` split:
- Index 0: spending = `total * 40 / 100`
- Index 1: savings  = `total * 30 / 100`
- Index 2: bills    = `total * 20 / 100`
- Index 3: insurance = remainder (ensures sum == total)

---

## Rollback Semantics

The orchestrator relies on **Soroban's panic/revert atomicity guarantee**:

> If any cross-contract call panics during a transaction, **all state changes
> made within that transaction are automatically reverted** by the Soroban runtime.

This means:
- If the savings leg panics → bills and insurance were never called; savings state reverts
- If the bills leg panics → savings state also reverts (even though it completed)
- If the insurance leg panics → both savings and bills state revert

There is **no manual rollback logic** in the orchestrator itself. The contract
delegates rollback responsibility entirely to the Soroban execution environment.

### Soft vs Hard Failures

Some failure conditions produce a **soft failure** (the orchestrator records
`*_success = false` but does not panic) vs a **hard failure** (the downstream
contract panics, causing a full transaction revert):

| Scenario | Type | Result |
|----------|------|--------|
| Downstream contract panics | Hard | Full transaction revert |
| `check_spending_limit` returns `false` | Hard | `PermissionDenied` error returned |
| Insurance `pay_premium` returns `false` | Soft | `insurance_success = false` in result |
| `total_amount <= 0` | Hard | `InvalidAmount` error returned |

---

## Security Assumptions

1. **Atomicity**: Soroban guarantees all-or-nothing execution. No partial state
   can persist from a failed transaction.

2. **Authorization**: Every public function calls `caller.require_auth()` before
   any state-modifying operation. Unauthorized callers cannot execute flows.

3. **Input validation**: `total_amount` is validated as positive before any
   cross-contract calls are made.

4. **Spending limits**: The Family Wallet contract enforces per-caller spending
   limits. The orchestrator checks this before executing any downstream legs.

5. **No re-entrancy**: Soroban's execution model prevents re-entrant calls.
   Each cross-contract call completes fully before the next instruction executes.

6. **No overflow**: The `soroban-sdk` uses `i128` for amounts. Arithmetic in mock
   contracts uses standard Rust multiplication/division which will panic on overflow
   in debug mode, triggering a rollback.

---

## Coverage Notes

- All three downstream legs (savings, bills, insurance) have failure scenarios
- Each failure position in the flow is tested (first, middle, last leg)
- Boundary conditions on amounts are covered (0, 1, limit, limit+1, negative)
- State isolation between transactions is verified (rollback does not poison future calls)
- Accounting invariants (sum of allocations == total) are verified on success paths
- Recovery behavior after each leg failure type is verified
