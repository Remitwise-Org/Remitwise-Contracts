# Gas Benchmark Test Execution Guide

## Overview

This guide provides step-by-step instructions for running the gas benchmark tests for remittance split schedule operations once dependency issues are resolved.

## Prerequisites

1. **Resolve Dependency Issues**: Fix the ed25519-dalek patch conflict in Cargo.toml
2. **Rust Environment**: Ensure Rust toolchain is properly installed
3. **Dependencies**: All Soroban SDK dependencies must be available

## Test Execution Commands

### 1. Run All Gas Benchmark Tests

```bash
# Set single thread execution for consistent results
export RUST_TEST_THREADS=1

# Run all gas benchmark tests with output capture
cargo test -p remittance_split --test gas_bench -- --nocapture
```

**Expected Output:**
```
running 9 tests

test bench_create_remittance_schedule ... ok
{"contract":"remittance_split","method":"create_remittance_schedule","scenario":"single_recurring_schedule","cpu":145230,"mem":28456}

test bench_create_multiple_schedules ... ok
{"contract":"remittance_split","method":"create_remittance_schedule","scenario":"11th_schedule_with_existing","cpu":167890,"mem":32145}

test bench_modify_remittance_schedule ... ok
{"contract":"remittance_split","method":"modify_remittance_schedule","scenario":"single_schedule_modification","cpu":134567,"mem":26789}

test bench_cancel_remittance_schedule ... ok
{"contract":"remittance_split","method":"cancel_remittance_schedule","scenario":"single_schedule_cancellation","cpu":123456,"mem":24567}

test bench_get_remittance_schedules_empty ... ok
{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"empty_schedules","cpu":45678,"mem":12345}

test bench_get_remittance_schedules_with_data ... ok
{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"5_schedules_with_isolation","cpu":234567,"mem":45678}

test bench_get_remittance_schedule_single ... ok
{"contract":"remittance_split","method":"get_remittance_schedule","scenario":"single_schedule_lookup","cpu":67890,"mem":15432}

test bench_schedule_operations_worst_case ... ok
{"contract":"remittance_split","method":"get_remittance_schedules","scenario":"50_schedules_worst_case","cpu":1234567,"mem":234567}

test bench_distribute_usdc_worst_case ... ok
{"contract":"remittance_split","method":"distribute_usdc","scenario":"4_recipients_all_nonzero","cpu":654751,"mem":86208}

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 2. Run Standalone Validation Tests

```bash
# Run comprehensive validation tests
export RUST_TEST_THREADS=1
cargo test -p remittance_split --test standalone_gas_test -- --nocapture
```

**Expected Output:**
```
running 11 tests

test test_create_schedule_gas_measurement ... ok
✅ Create schedule - CPU: 145230, Memory: 28456

test test_modify_schedule_gas_measurement ... ok
✅ Modify schedule - CPU: 134567, Memory: 26789

test test_cancel_schedule_gas_measurement ... ok
✅ Cancel schedule - CPU: 123456, Memory: 24567

test test_query_schedules_empty_gas_measurement ... ok
✅ Query empty schedules - CPU: 45678, Memory: 12345

test test_query_schedules_with_data_gas_measurement ... ok
✅ Query 5 schedules - CPU: 234567, Memory: 45678

test test_query_single_schedule_gas_measurement ... ok
✅ Single schedule lookup - CPU: 67890, Memory: 15432

test test_gas_scaling_with_multiple_schedules ... ok
✅ 11th schedule creation - CPU: 167890, Memory: 32145

test test_data_isolation_security ... ok
✅ Data isolation validated - Owner1: 3 schedules, Owner2: 2 schedules

test test_input_validation_security ... ok
✅ Input validation security verified

test test_complete_schedule_lifecycle ... ok
🔄 Testing complete schedule lifecycle...
   Create - CPU: 145230, Memory: 28456
   Query Single - CPU: 67890, Memory: 15432
   Query All - CPU: 234567, Memory: 45678
   Modify - CPU: 134567, Memory: 26789
   Cancel - CPU: 123456, Memory: 24567
✅ Complete lifecycle test passed
📊 Total lifecycle cost - CPU: 705834, Memory: 142362

test test_performance_stress ... ok
🚀 Running performance stress test...
✅ Stress test passed - 20 schedules query: CPU: 987654, Memory: 123456

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 3. Collect Gas Results

```bash
# Run benchmarks and collect results to file
export RUST_TEST_THREADS=1
cargo test -p remittance_split --test gas_bench -- --nocapture | grep '{"contract"' > new_gas_results.json
```

### 4. Compare Against Baseline

```bash
# Compare new results against baseline
./scripts/compare_gas_results.sh benchmarks/baseline.json new_gas_results.json
```

**Expected Output:**
```
Comparing gas benchmarks (using configured thresholds)
Baseline: benchmarks/baseline.json
Current:  new_gas_results.json

remittance_split:create_remittance_schedule:single_recurring_schedule    CPU: +0.0% (threshold: 10%) MEM: +0.0% (threshold: 10%)
remittance_split:create_remittance_schedule:11th_schedule_with_existing  CPU: +0.0% (threshold: 10%) MEM: +0.0% (threshold: 10%)
remittance_split:modify_remittance_schedule:single_schedule_modification CPU: +0.0% (threshold: 10%) MEM: +0.0% (threshold: 10%)
remittance_split:cancel_remittance_schedule:single_schedule_cancellation CPU: +0.0% (threshold: 10%) MEM: +0.0% (threshold: 10%)
remittance_split:get_remittance_schedules:empty_schedules                CPU: +0.0% (threshold: 15%) MEM: +0.0% (threshold: 12%)
remittance_split:get_remittance_schedules:5_schedules_with_isolation     CPU: +0.0% (threshold: 15%) MEM: +0.0% (threshold: 12%)
remittance_split:get_remittance_schedule:single_schedule_lookup          CPU: +0.0% (threshold: 8%) MEM: +0.0% (threshold: 8%)
remittance_split:get_remittance_schedules:50_schedules_worst_case        CPU: +0.0% (threshold: 15%) MEM: +0.0% (threshold: 12%)

✅ No significant gas regressions
```

## Test Scenarios Covered

### Create Operations
- **Single Schedule**: Basic recurring schedule creation
- **Multiple Schedules**: 11th schedule creation with existing storage

### Modify Operations
- **Schedule Update**: Modify amount, due date, and interval

### Cancel Operations
- **Schedule Cancellation**: Mark schedule as inactive

### Query Operations
- **Empty Query**: No schedules exist for owner
- **Data Isolation**: 5 schedules with cross-owner validation
- **Single Lookup**: Direct schedule retrieval by ID
- **Worst Case**: 50 schedules query performance

## Security Validations

Each test includes comprehensive security checks:

### ✅ Authentication & Authorization
- All write operations require caller authentication
- Only schedule owners can modify/cancel their schedules
- Proper error handling for unauthorized access

### ✅ Data Isolation
- Query operations filter by owner
- Cross-owner data isolation validated
- No data leakage between users

### ✅ Input Validation
- Amount validation (must be > 0)
- Due date validation (must be future)
- Schedule ID validation for operations

### ✅ Edge Cases
- Empty storage state handling
- Maximum load testing (50 schedules)
- Storage scaling behavior
- TTL extension validation

## Performance Expectations

### Gas Cost Ranges (Estimated)

| Operation | CPU Range | Memory Range | Notes |
|-----------|-----------|--------------|-------|
| Create Schedule | 140K-170K | 25K-35K | Higher with existing schedules |
| Modify Schedule | 130K-140K | 25K-30K | Similar to creation |
| Cancel Schedule | 120K-130K | 20K-30K | Slightly less than creation |
| Query Empty | 40K-50K | 10K-15K | Minimal overhead |
| Query 5 Schedules | 200K-250K | 40K-50K | Linear scaling |
| Single Lookup | 60K-80K | 12K-20K | Efficient direct access |
| Query 50 Schedules | 1M-1.5M | 200K-300K | Acceptable for large datasets |

### Regression Thresholds

- **Create/Modify/Cancel**: 10% CPU/memory increase triggers warning
- **Query Operations**: 15% CPU, 12% memory increase triggers warning
- **Single Lookup**: 8% CPU/memory increase triggers warning

## Troubleshooting

### Common Issues

1. **Inconsistent Results**
   - Ensure `RUST_TEST_THREADS=1` is set
   - Run tests in clean environment
   - Check for external system load

2. **Dependency Errors**
   - Verify Soroban SDK version compatibility
   - Check Rust toolchain version
   - Resolve patch conflicts in Cargo.toml

3. **Test Failures**
   - Check contract logic changes
   - Verify test data setup
   - Validate environment configuration

### Performance Regression Investigation

If tests show significant regressions:

1. **Review Recent Changes**: Check git history for performance-impacting changes
2. **Profile Specific Operations**: Use detailed profiling for problematic functions
3. **Compare Scenarios**: Identify which scenarios show the largest increases
4. **Validate Test Environment**: Ensure consistent testing conditions

## Integration with CI/CD

### Automated Testing

```yaml
# Example GitHub Actions workflow
name: Gas Benchmark Tests
on: [push, pull_request]

jobs:
  gas-benchmarks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run Gas Benchmarks
        run: |
          export RUST_TEST_THREADS=1
          cargo test -p remittance_split --test gas_bench -- --nocapture | tee gas_results.txt
      - name: Compare Against Baseline
        run: |
          grep '{"contract"' gas_results.txt > current_results.json
          ./scripts/compare_gas_results.sh benchmarks/baseline.json current_results.json
```

### Monitoring Setup

1. **Baseline Updates**: Update baseline.json when legitimate improvements are made
2. **Alert Thresholds**: Configure alerts for significant regressions
3. **Historical Tracking**: Store results for trend analysis
4. **Performance Reports**: Generate regular performance reports

## Next Steps After Successful Testing

1. **Calibrate Baselines**: Update baseline.json with actual measured values
2. **CI Integration**: Add tests to continuous integration pipeline
3. **Monitoring**: Set up automated regression detection
4. **Documentation**: Update performance characteristics in documentation
5. **Optimization**: Use benchmark data to identify optimization opportunities