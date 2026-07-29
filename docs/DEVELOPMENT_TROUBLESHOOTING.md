# Development Troubleshooting Guide

A collection of common development issues, gotchas, and tribal knowledge for RemitWise contract contributors. This guide captures the hard-won knowledge that experienced developers rely on but isn't documented elsewhere.

## Quick Fixes for Common Issues

### Build Failures After Soroban Upgrades

**Problem**: After upgrading Soroban SDK versions, builds fail with cryptic errors.

**Solution**: Clean all artifacts and rebuild:
```bash
cargo clean
rm -rf target/
rm Cargo.lock
cargo build --release --target wasm32-unknown-unknown
```

**Why**: Cached artifacts from different SDK versions can cause incompatibility issues.

### Test Failures with TTL Errors

**Problem**: Tests fail with "TTL expired" or similar storage errors.

**Solution**: Ensure your test environment has proper TTL settings:
```rust
use soroban_sdk::testutils::{Ledger, LedgerInfo};

fn setup_test_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    
    // Critical: max_entry_ttl must exceed contract TTL usage
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_000_000,  // Fixed timestamp for consistency
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,    // Must be > 518,400 (INSTANCE_BUMP_AMOUNT)
    });
    
    env
}
```

**Why**: Contracts extend TTL by 518,400 ledgers. If `max_entry_ttl` is lower, tests fail.

### Gas Benchmark Inconsistencies

**Problem**: Gas benchmark results vary between runs, causing CI failures.

**Solution**: Always run with single thread:
```bash
RUST_TEST_THREADS=1 cargo test -p <contract> --test gas_bench -- --nocapture
```

**Why**: Parallel execution affects budget measurements and memory allocation patterns.

### ed25519-dalek Version Conflicts

**Problem**: Fresh lockfile generates causes build failures with incompatible ed25519-dalek versions.

**Solution**: Pin to compatible version:
```bash
cargo update -p "ed25519-dalek@3.0.0" --precise 2.2.0
```

**Why**: soroban-env-host v21.2.1 is incompatible with ed25519-dalek v3.0.0.

## Testing Patterns

### Standard Test Environment Setup

**Always use this pattern** for consistent test behavior:

```rust
use soroban_sdk::testutils::{Address as _, Ledger as _};

fn setup() -> (Env, Address, ContractClient) {
    let env = Env::default();
    env.mock_all_auths();
    
    // Set fixed ledger state for reproducibility
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    
    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    
    (env, admin, client)
}
```

### Rate Limiting in Tests

**Problem**: Tests creating many entities hit rate limits.

**Solution**: Advance ledger time when needed:
```rust
fn maybe_advance_for_rate_limit(env: &Env, create_index: u32) {
    if create_index > 0 && create_index % CREATE_RATE_LIMIT == 0 {
        let current_time = env.ledger().timestamp();
        env.ledger().set(LedgerInfo {
            timestamp: current_time + RATE_LIMIT_WINDOW_SECONDS + 1,
            ..env.ledger().get()
        });
    }
}
```

### Authentication Mocking Patterns

**Global auth mocking** (most common):
```rust
env.mock_all_auths(); // Mocks ALL auth checks for entire environment
```

**Specific auth mocking** (when you need to test auth failures):
```rust
// Don't use mock_all_auths()
env.mock_auths(&[MockAuth {
    address: &user,
    invoke: &MockAuthInvoke {
        contract: &contract_id,
        fn_name: "function_name",
        args: (arg1, arg2).into_val(&env),
        sub_invokes: &[],
    },
}]);
```

### Gas Benchmarking Requirements

**Critical**: Gas benchmarks must output specific JSON format:
```rust
println!(
    r#"{{"contract":"{}","method":"{}","scenario":"{}","cpu":{},"mem":{}}}"#,
    contract_name, method_name, scenario_name, cpu_cost, memory_cost
);
```

**Pattern for measuring**:
```rust
fn measure_gas<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    env.budget().reset_unlimited();  // Prevent artificial limits
    env.budget().reset_tracker();
    let result = f();
    let cpu = env.budget().cpu_instruction_cost();
    let mem = env.budget().memory_bytes_cost();
    (cpu, mem, result)
}
```

## Performance & Storage Gotchas

### TTL Management for Gas Efficiency

**Read-only functions** should NOT extend TTL unnecessarily:
```rust
// ❌ Bad: Wastes gas on read-only operations
pub fn get_policy(env: Env, id: u32) -> Option<Policy> {
    extend_instance_ttl(&env);  // Don't do this for reads!
    env.storage().persistent().get(&DataKey::Policy(id))
}

// ✅ Good: Only extend TTL on writes
pub fn pay_premium(env: Env, owner: Address, id: u32) {
    owner.require_auth();
    extend_instance_ttl(&env);  // Extend only on state changes
    // ... update logic
}
```

### Storage Type Selection

**Instance storage**: Contract-wide config, frequently accessed data
```rust
env.storage().instance().set(&Symbol::new(&env, "config"), &config);
```

**Persistent storage**: User data that must survive contract expiry
```rust
env.storage().persistent().set(&DataKey::Policy(id), &policy);
```

**Temporary storage**: Short-lived data (avoid in production)
```rust
env.storage().temporary().set(&key, &value);
```

### Avoiding O(n) Performance Traps

**Problem**: Functions that scan ID ranges become slow with gaps:
```rust
// ❌ Bad: O(n) scan with potential gaps
pub fn get_all_bills(env: Env, owner: Address) -> Vec<Bill> {
    let next_id: u32 = env.storage().instance().get(&NEXT_BILL_ID).unwrap_or(1);
    let mut bills = Vec::new(&env);
    
    for id in 1..next_id {  // Scans even deleted/inactive IDs
        if let Some(bill) = env.storage().persistent().get(&DataKey::Bill(id)) {
            if bill.owner == owner {
                bills.push_back(bill);
            }
        }
    }
    bills
}
```

**Solution**: Use pagination and maintain active ID lists:
```rust
// ✅ Better: Paginated access with cursor
pub fn get_active_bills(env: Env, owner: Address, cursor: u32, limit: u32) -> BillPage {
    // Implementation with pagination...
}
```

## Error Handling Patterns

### Contract Error Organization

**Standard error code allocation**:
```rust
#[contracterror]
#[repr(u32)]
pub enum ContractError {
    // Initialization: 1-10
    AlreadyInitialized = 1,
    NotInitialized = 2,
    
    // Authentication: 10-50
    Unauthorized = 10,
    InvalidCaller = 11,
    
    // Input validation: 50-100
    InvalidAmount = 50,
    InvalidDate = 51,
    
    // Business logic: 100+
    RecordNotFound = 100,
    InsufficientFunds = 101,
}
```

### Panic vs Result Guidelines

**Use Result** for expected failures:
```rust
pub fn create_policy(...) -> Result<u32, ContractError> {
    if premium == 0 {
        return Err(ContractError::InvalidPremium);
    }
    // ...
}
```

**Use panic_with_error!** for invariant violations:
```rust
pub fn internal_helper() {
    let config = env.storage().instance().get(&CONFIG_KEY);
    if config.is_none() {
        panic_with_error!(&env, ContractError::NotInitialized);
    }
}
```

### Testing Error Cases

**Pattern for testing expected errors**:
```rust
assert_eq!(
    client.try_create_policy(...).unwrap_err().unwrap(),
    ContractError::InvalidPremium
);
```

## Multi-Contract Integration

### Deployment Order Dependencies

**Critical**: Contracts must be deployed in dependency order:

1. **Core contracts**: remittance_split, savings_goals, bill_payments, insurance, family_wallet
2. **Orchestrator**: Needs addresses of core contracts
3. **Reporting**: Must be deployed LAST (needs all other addresses)

### Cross-Contract Error Handling

**Graceful degradation pattern**:
```rust
// Used in reporting contract - allows partial data if dependencies fail
match client.try_get_savings_summary(&user) {
    Ok(data) => data_availability = DataAvailability::Complete,
    Err(_) => {
        // Log but don't fail - partial data is better than no data
        data_availability = DataAvailability::Partial;
    }
}
```

## Development Environment Variables

### Required for Scripts

```bash
# Gas benchmarking
export RUST_TEST_THREADS=1

# Bootstrap deployment (skip build if already done)
export SKIP_BUILD=1

# Custom output location
export OUTPUT_FILE=./my-contracts.json
```

## Debugging Tools

### Contract Event Inspection

```rust
// View all events in tests
let events = env.events().all();
for event in events {
    println!("Event: {:?}", event);
}
```

### Storage Inspection

```rust
// Check what's in storage during debugging
use soroban_sdk::testutils::Storage;
println!("Instance storage: {:?}", env.storage().instance().all());
```

### Auth Debugging

```rust
// Check auth calls during test execution
use soroban_sdk::testutils::AuthSnapshot;
let auths = env.auths();
for auth in auths {
    println!("Auth required: {:?}", auth);
}
```

## CI/Build System Quirks

### Feature Flag Consistency

The CI checks that every `#[cfg(feature = "...")]` has a corresponding `[features]` entry:
```bash
python3 scripts/check_features.py
```

### Unsafe Code Detection

Contracts are scanned for unsafe code outside soroban-sdk:
```bash
python3 scripts/check_unsafe.py
```

### Dependency Pinning Logic

The `check_ci.sh` script includes dependency fixes that prevent random failures:
```bash
# This prevents version resolution issues
if cargo tree -i "ed25519-dalek@3.0.0" &>/dev/null; then
    cargo update -p "ed25519-dalek@3.0.0" --precise 2.2.0
fi
```

## Getting Help

When you encounter issues not covered here:

1. **Check existing tests** for similar patterns
2. **Look at `check_ci.sh`** for environment setup clues
3. **Review contract error enums** for expected failure modes
4. **Check the gas benchmark tests** for performance patterns
5. **Ask in team channels** and consider adding the solution to this guide

## Contributing to This Guide

Found a new gotcha or solution? Please add it! This guide should capture all the tribal knowledge that makes development smoother.

**Format for new entries**:
```markdown
### Problem Title

**Problem**: Brief description of the issue

**Solution**: Step-by-step fix

**Why**: Explanation of root cause
```