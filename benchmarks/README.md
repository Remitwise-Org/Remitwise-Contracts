# Gas Benchmarking System

This directory contains the gas benchmarking infrastructure for Remitwise smart contracts. The system tracks CPU and memory costs for critical operations to detect performance regressions early in development.

## Overview

Gas benchmarking helps ensure that contract operations remain efficient and predictable. Each benchmark measures:
- **CPU Instructions**: Computational cost of operations
- **Memory Usage**: Storage and temporary memory allocation costs

## Structure

```
benchmarks/
├── README.md           # This documentation
├── baseline.json       # Baseline measurements for all operations
├── thresholds.json     # Regression detection thresholds
└── history/           # Historical benchmark data
```

## Configuration Files

### baseline.json
Contains baseline CPU and memory costs for each benchmarked operation. These values are updated when legitimate performance improvements are made.

### thresholds.json
Defines regression detection thresholds as percentage increases from baseline:
- **default**: 10% increase triggers warning for most operations
- **contract_specific**: Custom thresholds per contract
- **method_specific**: Custom thresholds per method

## Running Benchmarks

### Individual Contract Benchmarks
```bash
# Run remittance_split schedule operation benchmarks
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture

# Run bill_payments benchmarks
RUST_TEST_THREADS=1 cargo test -p bill_payments --test gas_bench -- --nocapture
```

### All Benchmarks
```bash
# Run all gas benchmarks across contracts
./scripts/run_all_benchmarks.sh
```

## Benchmark Output Format

Each benchmark outputs JSON with the following structure:
```json
{
  "contract": "remittance_split",
  "method": "create_remittance_schedule", 
  "scenario": "single_recurring_schedule",
  "cpu": 12345,
  "mem": 6789
}
```

## Remittance Split Schedule Operations

The remittance split contract includes comprehensive benchmarks for schedule lifecycle operations:

### Create Operations
- `create_remittance_schedule/single_recurring_schedule`: Basic schedule creation
- `create_remittance_schedule/11th_schedule_with_existing`: Scaling with existing schedules

### Modify Operations  
- `modify_remittance_schedule/single_schedule_modification`: Update existing schedule

### Cancel Operations
- `cancel_remittance_schedule/single_schedule_cancellation`: Cancel active schedule

### Query Operations
- `get_remittance_schedules/empty_schedules`: Query with no schedules
- `get_remittance_schedules/5_schedules_with_isolation`: Query with data isolation
- `get_remittance_schedules/50_schedules_worst_case`: Worst-case query performance
- `get_remittance_schedule/single_schedule_lookup`: Single schedule retrieval

## Insurance Schedule Operations

The insurance contract includes comprehensive benchmarks for premium schedule lifecycle operations under heavy workloads.

### Running Insurance Benchmarks
```bash
RUST_TEST_THREADS=1 cargo test -p insurance --test gas_bench -- --nocapture
```

### Create Operations
- `create_premium_schedule/single_recurring_schedule`: Basic schedule creation
- `create_premium_schedule/51st_schedule_with_existing`: Scaling with 50 existing schedules

### Modify Operations
- `modify_premium_schedule/single_schedule_modification`: Update existing schedule
- `modify_premium_schedule/modify_middle_of_100_schedules`: Modify schedule in middle of 100

### Cancel Operations
- `cancel_premium_schedule/single_schedule_cancellation`: Cancel active schedule
- `cancel_premium_schedule/cancel_middle_of_50_schedules`: Cancel schedule in middle of 50

### Execute Operations
- `execute_due_premium_schedules/single_due_schedule`: Execute one due schedule
- `execute_due_premium_schedules/10_due_of_50_schedules`: Execute 10 of 50 schedules
- `execute_due_premium_schedules/all_50_schedules_due`: Execute all 50 due schedules
- `execute_due_premium_schedules/schedule_with_5_missed_periods`: Execute with missed periods

### Query Operations
- `get_premium_schedule/single_schedule_lookup`: Single schedule retrieval
- `get_active_schedules/empty_schedules`: Query with no schedules
- `get_active_schedules/50_active_schedules`: Query with 50 schedules
- `get_active_schedules/100_schedules_worst_case`: Worst-case query with 100 schedules
- `get_active_schedules/50_schedules_2_owners_isolation`: Owner isolation validation
- `get_total_monthly_premium/100_active_policies`: Aggregate query over 100 policies

## Security Considerations

All benchmarks include security validations:

1. **Authorization**: Tests verify proper authentication and authorization
2. **Data Isolation**: Ensures users can only access their own data
3. **Input Validation**: Tests with valid parameters to ensure proper validation
4. **Edge Cases**: Covers boundary conditions and error scenarios

## Regression Detection

The system automatically detects regressions by comparing current measurements against baselines:

- **Green**: Within threshold (no action needed)
- **Yellow**: Exceeds threshold but < 25% increase (review recommended)  
- **Red**: > 25% increase (investigation required)

## Adding New Benchmarks

When adding new contract methods:

1. **Create benchmark test** in `contracts/{contract}/tests/gas_bench.rs`
2. **Add baseline entry** in `baseline.json` 
3. **Set thresholds** in `thresholds.json` if non-standard
4. **Document security assumptions** in test comments

### Benchmark Test Template

```rust
/// Benchmark: {Operation description}
/// Security: {Security validations performed}
#[test]
fn bench_{operation_name}() {
    let env = bench_env();
    let contract_id = env.register_contract(None, YourContract);
    let client = YourContractClient::new(&env, &contract_id);

    // Setup test data
    let owner = <Address as AddressTrait>::generate(&env);
    
    let (cpu, mem, result) = measure(&env, || {
        client.your_method(&owner, &param1, &param2)
    });
    
    // Validate result
    assert!(result.is_ok());

    println!(
        r#"{{"contract":"your_contract","method":"your_method","scenario":"test_scenario","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}
```

## Best Practices

1. **Consistent Environment**: Use `bench_env()` for reproducible conditions
2. **Realistic Data**: Test with representative data sizes and patterns
3. **Worst-Case Scenarios**: Include stress tests with maximum realistic loads
4. **Security Validation**: Always verify security assumptions in benchmarks
5. **Clear Naming**: Use descriptive scenario names that indicate test conditions

## Monitoring and Alerts

- Benchmark results are tracked in CI/CD pipelines
- Significant regressions trigger build failures
- Historical data enables trend analysis
- Performance improvements can be validated before deployment

## Troubleshooting

### High Variance in Results
- Ensure `RUST_TEST_THREADS=1` for consistent execution
- Check for external factors affecting test environment
- Verify test data setup is deterministic

### Unexpected Regressions
- Review recent code changes for performance impacts
- Check if test scenarios still match actual usage patterns
- Validate that baseline measurements are still accurate

### Adding Contract-Specific Thresholds
Some operations may have inherently higher variance:
- Iteration-heavy operations (higher CPU threshold)
- Dynamic memory allocation (higher memory threshold)
- Complex calculations (higher CPU threshold)

Update `thresholds.json` with appropriate values based on operation characteristics.
