# Integration Testing Guide

## Overview

This document describes the integration testing strategy for RemitWise smart contracts. Integration tests verify that multiple contracts work together correctly by simulating real-world user flows.

## Architecture

### Test Environment

Integration tests use Soroban SDK's test utilities to create an isolated test environment:

```rust
let env = Env::default();
env.mock_all_auths(); // Mock authentication for testing
```

### Contract Deployment

All contracts are deployed in the test environment:

```rust
// Deploy remittance_split
let remittance_contract_id = env.register_contract(None, RemittanceSplit);
let remittance_client = RemittanceSplitClient::new(&env, &remittance_contract_id);

// Deploy savings_goals
let savings_contract_id = env.register_contract(None, SavingsGoals);
let savings_client = SavingsGoalsClient::new(&env, &savings_contract_id);

// Deploy bill_payments
let bills_contract_id = env.register_contract(None, BillPayments);
let bills_client = BillPaymentsClient::new(&env, &bills_contract_id);

// Deploy insurance
let insurance_contract_id = env.register_contract(None, Insurance);
let insurance_client = InsuranceClient::new(&env, &insurance_contract_id);
```

## Test Scenarios

### 1. Complete User Flow (`test_multi_contract_user_flow`)

**Purpose**: Verify end-to-end functionality across all contracts

**Steps**:

1. Deploy all four contracts
2. Initialize remittance split with allocation percentages
3. Create a savings goal
4. Create a recurring bill
5. Create an insurance policy
6. Calculate split for a remittance amount
7. Verify amounts match expected percentages
8. Verify total equals original amount

**Assertions**:

- Split initialization succeeds
- All entities (goal, bill, policy) are created with ID = 1
- Calculated amounts match percentages exactly
- Sum of all allocations equals total remittance

**Example Output**:

```
✅ Multi-contract integration test passed!
   Total Remittance: 10000
   Spending: 4000 (40%)
   Savings: 3000 (30%)
   Bills: 2000 (20%)
   Insurance: 1000 (10%)
```

### 2. Rounding Behavior (`test_split_with_rounding`)

**Purpose**: Verify correct handling of rounding in percentage calculations

**Steps**:

1. Initialize split with percentages that don't divide evenly (33%, 33%, 17%, 17%)
2. Calculate split for an amount that will have rounding issues
3. Verify total still equals original amount

**Key Insight**: The insurance category receives the remainder to ensure no funds are lost or created due to rounding.

**Assertions**:

- Total allocated equals original amount despite rounding
- No funds are lost or created

### 3. Multiple Entities (`test_multiple_entities_creation`)

**Purpose**: Verify multiple entities can be created across contracts

**Steps**:

1. Create 2 savings goals
2. Create 2 bills
3. Create 2 insurance policies
4. Verify all have unique sequential IDs

**Assertions**:

- All entities are created successfully
- IDs are sequential and unique per contract

## Running Tests

### Local Development

```bash
# Run all integration tests
cargo test -p integration_tests

# Run specific test
cargo test -p integration_tests test_multi_contract_user_flow

# Run with output
cargo test -p integration_tests -- --nocapture

# Run with backtrace on failure
RUST_BACKTRACE=1 cargo test -p integration_tests
```

### CI/CD Pipeline

Integration tests run automatically in the CI pipeline:

```yaml
- name: Run Integration tests
  run: |
    cargo test -p integration_tests --verbose
  continue-on-error: false
```

The tests are part of the main CI workflow and must pass for PRs to be merged.

## Test Data

### Default Test Values

**User**: Generated test address
**Split Percentages**: 40% spending, 30% savings, 20% bills, 10% insurance
**Remittance Amount**: 10,000 (for easy percentage calculation)

**Savings Goal**:

- Name: "Education Fund"
- Target: 10,000
- Duration: 1 year

**Bill**:

- Name: "Electricity Bill"
- Amount: 500
- Recurring: Monthly (30 days)

**Insurance Policy**:

- Name: "Health Insurance"
- Type: "health"
- Premium: 200/month
- Coverage: 50,000

## Extending Tests

### Adding New Test Cases

1. Create a new test function in `tests/multi_contract_integration.rs`
2. Follow the pattern: deploy → initialize → execute → verify
3. Use descriptive assertion messages
4. Add documentation comments

Example:

```rust
/// Test description
#[test]
fn test_new_scenario() {
    let env = Env::default();
    env.mock_all_auths();

    // Setup
    let user = Address::generate(&env);

    // Deploy contracts
    // ...

    // Execute operations
    // ...

    // Verify results
    assert!(result.is_ok(), "Operation should succeed");
}
```

### Testing Cross-Contract Calls

When cross-contract allocation is implemented:

```rust
#[test]
fn test_cross_contract_allocation() {
    // Deploy all contracts
    // Initialize split
    // Call allocate function that distributes to other contracts
    // Verify balances updated in savings, bills, insurance contracts
}
```

## Best Practices

1. **Isolation**: Each test should be independent and not rely on state from other tests
2. **Clarity**: Use descriptive variable names and assertion messages
3. **Coverage**: Test both happy paths and edge cases
4. **Performance**: Keep tests fast (no unnecessary delays or operations)
5. **Documentation**: Add comments explaining complex test logic

## Troubleshooting

### Common Issues

**Issue**: Test fails with "AlreadyInitialized" error
**Solution**: Each test creates a new environment, so this shouldn't happen. Check if you're calling initialize_split twice.

**Issue**: Amounts don't sum to total
**Solution**: Check the rounding logic. Insurance should receive the remainder.

**Issue**: Authentication errors
**Solution**: Ensure `env.mock_all_auths()` is called before any contract operations.

### Debug Output

Add debug output to tests:

```rust
println!("Debug: amount = {}", amount);
println!("Debug: split result = {:?}", split_result);
```

Run with `--nocapture` to see output:

```bash
cargo test -p integration_tests -- --nocapture
```

## Future Enhancements

- [ ] Add event verification tests
- [ ] Test error scenarios (invalid inputs, unauthorized access)
- [ ] Add time-based tests (overdue bills, goal deadlines)
- [ ] Test multi-user scenarios
- [ ] Add performance benchmarks
- [ ] Test storage limits and cleanup
- [ ] Add fuzz testing for edge cases
