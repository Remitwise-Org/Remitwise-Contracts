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

```bash
# Run all integration tests
cargo test -p integration_tests

# Run a specific test
cargo test -p integration_tests test_multi_contract_user_flow

# Run with output
cargo test -p integration_tests -- --nocapture
```

## CI Integration

These tests are designed to run in CI pipelines. They:

- Use no external dependencies (fully self-contained)
- Mock all authentication (no real signatures needed)
- Complete quickly (no network calls or delays)
- Provide clear assertion messages for debugging failures

## Future Enhancements

Potential additions for comprehensive testing:

- Cross-contract allocation flow (when implemented)
- Event verification across all contracts
- Error handling scenarios (insufficient funds, invalid percentages)
- Time-based scenarios (overdue bills, goal deadlines)
- Multi-user scenarios (family wallet integration)
