# Soroban Version Upgrade Guide

This guide provides detailed instructions for upgrading RemitWise contracts to new Soroban SDK and protocol versions.

## Table of Contents

- [Pre-Upgrade Checklist](#pre-upgrade-checklist)
- [Upgrade Process](#upgrade-process)
- [Version-Specific Migration Guides](#version-specific-migration-guides)
- [Testing Strategy](#testing-strategy)
- [Rollback Procedures](#rollback-procedures)
- [Post-Upgrade Validation](#post-upgrade-validation)

## Pre-Upgrade Checklist

Before upgrading to a new Soroban version:

- [ ] Review [Soroban SDK release notes](https://github.com/stellar/rs-soroban-sdk/releases)
- [ ] Check [Stellar Protocol upgrade announcements](https://stellar.org/developers)
- [ ] Backup current contract deployments and state
- [ ] Document current gas benchmark baseline
- [ ] Ensure all tests pass on current version
- [ ] Review breaking changes and deprecations
- [ ] Plan testnet validation window
- [ ] Notify stakeholders of upgrade timeline

## Upgrade Process

### Step 1: Environment Preparation

```bash
# Create a new branch for the upgrade
git checkout -b upgrade/soroban-vX.Y.Z

# Backup current gas benchmarks
cp gas_results.json gas_results_baseline_vOLD.json

# Document current versions
soroban version > versions_before_upgrade.txt
rustc --version >> versions_before_upgrade.txt
```

### Step 2: Update Dependencies

Update all contract `Cargo.toml` files:

```bash
# Use find and sed to update all contracts at once (Unix/Linux/macOS)
find . -name "Cargo.toml" -type f -exec sed -i 's/soroban-sdk = "OLD_VERSION"/soroban-sdk = "NEW_VERSION"/g' {} +

# Or manually update each contract:
# - remittance_split/Cargo.toml
# - savings_goals/Cargo.toml
# - bill_payments/Cargo.toml
# - insurance/Cargo.toml
# - family_wallet/Cargo.toml
# - data_migration/Cargo.toml
# - reporting/Cargo.toml
# - orchestrator/Cargo.toml
```

### Step 3: Update Soroban CLI

```bash
# Install new CLI version
cargo install --locked --version X.Y.Z soroban-cli

# Verify installation
soroban version

# Update README installation instructions
# Edit README.md to reflect new version
```

### Step 4: Clean Build

```bash
# Remove all build artifacts
cargo clean
rm -rf target/
rm Cargo.lock

# Update dependencies
cargo update

# Build all contracts
cargo build --release --target wasm32-unknown-unknown
```

### Step 5: Run Tests

```bash
# Run all unit tests
cargo test

# Run tests with verbose output to catch warnings
cargo test -- --nocapture

# Run specific contract tests
cargo test -p remittance_split
cargo test -p savings_goals
cargo test -p bill_payments
cargo test -p insurance
cargo test -p family_wallet
cargo test -p reporting
cargo test -p orchestrator
```

### Step 6: Gas Benchmark Comparison

```bash
# Run new benchmarks
./scripts/run_gas_benchmarks.sh

# Compare against baseline (10% threshold)
./scripts/compare_gas_results.sh gas_results_baseline_vOLD.json gas_results.json 10

# Review any significant changes
# Document performance improvements or regressions
```

### Step 7: Testnet Deployment

```bash
# Build optimized contracts
cargo build --release --target wasm32-unknown-unknown

# Deploy each contract to testnet
for contract in remittance_split savings_goals bill_payments insurance family_wallet reporting orchestrator; do
  echo "Deploying $contract..."
  soroban contract deploy \
    --wasm target/wasm32-unknown-unknown/release/${contract}.wasm \
    --source <your-testnet-key> \
    --network testnet
done

# Save contract IDs for testing
```

### Step 8: Integration Testing

Run integration tests on testnet:

```bash
# Test remittance split
soroban contract invoke \
  --id <remittance-split-id> \
  --source <your-key> \
  --network testnet \
  -- initialize_split \
  --owner <address> \
  --spending_percent 40 \
  --savings_percent 30 \
  --bills_percent 20 \
  --insurance_percent 10

# Test savings goals
soroban contract invoke \
  --id <savings-goals-id> \
  --source <your-key> \
  --network testnet \
  -- create_goal \
  --owner <address> \
  --name "Education" \
  --target_amount 100000 \
  --target_date <timestamp>

# Test bill payments
soroban contract invoke \
  --id <bill-payments-id> \
  --source <your-key> \
  --network testnet \
  -- create_bill \
  --owner <address> \
  --name "Electricity" \
  --amount 5000 \
  --due_date <timestamp> \
  --recurring false

# Test insurance
soroban contract invoke \
  --id <insurance-id> \
  --source <your-key> \
  --network testnet \
  -- create_policy \
  --owner <address> \
  --name "Health Insurance" \
  --coverage_type "Health" \
  --monthly_premium 2000 \
  --coverage_amount 50000

# Verify events are emitted correctly
soroban events --id <contract-id> --network testnet
```

### Step 9: Documentation Updates

```bash
# Update README.md compatibility section
# Update DEPLOYMENT.md with any new deployment steps
# Update this UPGRADE_GUIDE.md with version-specific notes
# Update inline code comments if APIs changed
```

### Step 10: Commit and Review

```bash
# Stage all changes
git add .

# Commit with descriptive message
git commit -m "Upgrade to Soroban SDK vX.Y.Z

- Updated all contracts to SDK vX.Y.Z
- Updated soroban-cli to vX.Y.Z
- All tests passing
- Gas benchmarks within acceptable range
- Testnet deployment validated
- Documentation updated"

# Push and create PR
git push origin upgrade/soroban-vX.Y.Z
```

## Version-Specific Migration Guides

### Upgrading to SDK 21.0.0 (Current)

**Status**: Current stable version

**Changes**:
- Stable release with improved storage APIs
- Enhanced event emission capabilities
- Better TTL management

**Migration Steps**:
- No breaking changes from 20.x series
- Review storage patterns for optimization opportunities
- Consider using new event features

### Upgrading to SDK 22.0.0+ (Future)

**Status**: Not yet released

**Expected Changes** (monitor release notes):
- Potential storage API improvements
- New authorization patterns
- Enhanced cross-contract call capabilities

**Preparation**:
- Monitor [SDK repository](https://github.com/stellar/rs-soroban-sdk) for announcements
- Review beta releases when available
- Test early with release candidates

## Testing Strategy

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html --output-dir coverage
```

### Integration Tests

Create a test script for comprehensive validation:

```bash
#!/bin/bash
# test_upgrade.sh

set -e

echo "Running upgrade validation tests..."

# Test each contract's core functionality
echo "Testing remittance_split..."
cargo test -p remittance_split

echo "Testing savings_goals..."
cargo test -p savings_goals

echo "Testing bill_payments..."
cargo test -p bill_payments

echo "Testing insurance..."
cargo test -p insurance

echo "Testing family_wallet..."
cargo test -p family_wallet

echo "Testing reporting..."
cargo test -p reporting

echo "Testing orchestrator..."
cargo test -p orchestrator

echo "All tests passed!"
```

### Gas Regression Tests

```bash
# Run benchmarks
./scripts/run_gas_benchmarks.sh

# Compare with baseline
./scripts/compare_gas_results.sh baseline.json gas_results.json 10

# Review results
cat gas_comparison_report.txt
```

### Testnet Validation Checklist

- [ ] All contracts deploy successfully
- [ ] Contract initialization works
- [ ] Core functions execute without errors
- [ ] Events are emitted correctly
- [ ] Storage operations work as expected
- [ ] Cross-contract calls function properly (orchestrator)
- [ ] Gas costs are within acceptable range
- [ ] TTL management works correctly

## Rollback Procedures

If issues are discovered after upgrade:

### Immediate Rollback

```bash
# Revert to previous branch
git checkout main

# Reinstall previous CLI version
cargo install --locked --version OLD_VERSION soroban-cli

# Rebuild with old version
cargo clean
cargo build --release --target wasm32-unknown-unknown

# Redeploy previous contracts if needed
```

### Partial Rollback

If only specific contracts have issues:

```bash
# Revert specific contract's Cargo.toml
git checkout HEAD~1 -- <contract>/Cargo.toml

# Rebuild that contract
cargo build -p <contract> --release --target wasm32-unknown-unknown

# Redeploy
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/<contract>.wasm \
  --source <your-key> \
  --network testnet
```

### Document Issues

```bash
# Create issue report
cat > upgrade_issues.md << EOF
# Upgrade Issues - Soroban vX.Y.Z

## Environment
- SDK Version: X.Y.Z
- CLI Version: X.Y.Z
- Rust Version: $(rustc --version)

## Issues Encountered
1. [Describe issue]
   - Error message: [paste error]
   - Steps to reproduce: [list steps]
   - Affected contracts: [list contracts]

## Resolution
- [Describe resolution or rollback action]

## Next Steps
- [List action items]
EOF
```

## Post-Upgrade Validation

### Mainnet Deployment Checklist

Before deploying to mainnet after an upgrade:

- [ ] All testnet tests passed for at least 7 days
- [ ] No critical issues reported
- [ ] Gas costs reviewed and acceptable
- [ ] Documentation fully updated
- [ ] Team trained on any new features or changes
- [ ] Monitoring and alerting configured
- [ ] Rollback plan documented and tested
- [ ] Stakeholders notified of deployment window

### Monitoring

After mainnet deployment:

```bash
# Monitor contract events
soroban events --id <contract-id> --network mainnet --start-ledger <ledger>

# Check contract state
soroban contract invoke \
  --id <contract-id> \
  --network mainnet \
  -- <getter-function>

# Monitor gas usage
# Review transaction costs in Stellar Expert or similar tools
```

### Success Criteria

Upgrade is considered successful when:

- [ ] All contracts deployed to mainnet
- [ ] All core functions working as expected
- [ ] No increase in error rates
- [ ] Gas costs within 10% of baseline
- [ ] Events emitting correctly
- [ ] No user-reported issues for 48 hours
- [ ] Performance metrics stable

## Additional Resources

- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Soroban SDK Repository](https://github.com/stellar/rs-soroban-sdk)
- [Stellar Protocol Upgrades](https://stellar.org/developers/docs/fundamentals-and-concepts/stellar-consensus-protocol#protocol-upgrades)
- [Soroban Discord](https://discord.gg/stellar)
- [Stellar Stack Exchange](https://stellar.stackexchange.com/)

## Support

If you encounter issues during upgrade:

1. Check [Soroban SDK Issues](https://github.com/stellar/rs-soroban-sdk/issues)
2. Search [Stellar Stack Exchange](https://stellar.stackexchange.com/)
3. Ask in [Soroban Discord](https://discord.gg/stellar)
4. Review this guide's troubleshooting section
5. Create a detailed issue report with reproduction steps
