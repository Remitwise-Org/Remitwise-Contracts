# Feature Flags Contract

A simple on-chain feature flags system for gradual rollouts and feature gating in Soroban smart contracts.

## Overview

The Feature Flags contract provides a centralized way to manage feature toggles on-chain. This enables:

- **Gradual rollouts**: Enable features for specific users or gradually roll out to all users
- **Kill switches**: Quickly disable problematic features without redeploying contracts
- **A/B testing**: Test different behaviors by toggling features
- **Safe deployments**: Deploy code with features disabled, then enable when ready

## Features

- Simple key-value flag storage
- Admin-controlled flag management
- Public read access (no auth required for queries)
- Event emission for flag changes
- Metadata tracking (description, last updated timestamp and address)

## Usage

### Initialization

```rust
// Initialize with an admin address
feature_flags::initialize(env, admin);
```

### Setting Flags

```rust
// Enable a feature
feature_flags::set_flag(
    env,
    admin,
    String::from_str(&env, "strict_goal_dates"),
    true,
    String::from_str(&env, "Enforce future dates for savings goals")
);

// Disable a feature
feature_flags::set_flag(
    env,
    admin,
    String::from_str(&env, "strict_goal_dates"),
    false,
    String::from_str(&env, "Enforce future dates for savings goals")
);
```

### Querying Flags

```rust
// Check if a feature is enabled (returns false if flag doesn't exist)
let enabled = feature_flags::is_enabled(
    env,
    String::from_str(&env, "strict_goal_dates")
);

// Get full flag details
let flag = feature_flags::get_flag(
    env,
    String::from_str(&env, "strict_goal_dates")
);

// Get all flags
let all_flags = feature_flags::get_all_flags(env);
```

### Admin Management

```rust
// Transfer admin role
feature_flags::transfer_admin(env, current_admin, new_admin);

// Remove a flag
feature_flags::remove_flag(
    env,
    admin,
    String::from_str(&env, "old_feature")
);
```

## Integration Example

Here's how to gate a feature in another contract:

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};

#[contract]
pub struct MyContract;

#[contractimpl]
impl MyContract {
    pub fn my_function(env: Env, user: Address) {
        // Check feature flag
        let flags_client = FeatureFlagsContractClient::new(&env, &flags_contract_id);
        let strict_dates_enabled = flags_client.is_enabled(
            &String::from_str(&env, "strict_goal_dates")
        );

        if strict_dates_enabled {
            // New behavior: enforce strict date validation
            Self::validate_future_date(&env, target_date);
        } else {
            // Old behavior: allow any date
        }
        
        // ... rest of function
    }
}
```

## Example: Gating Savings Goals Date Validation

The `strict_goal_dates` flag can be used to control whether savings goals must have future target dates:

```rust
pub fn create_goal(
    env: Env,
    owner: Address,
    name: String,
    target_amount: i128,
    target_date: u64,
) -> Result<u32, SavingsGoalsError> {
    owner.require_auth();
    
    // Check feature flag
    let flags_client = FeatureFlagsContractClient::new(&env, &flags_contract_id);
    if flags_client.is_enabled(&String::from_str(&env, "strict_goal_dates")) {
        // Enforce future dates when flag is enabled
        let current_time = env.ledger().timestamp();
        if target_date <= current_time {
            return Err(SavingsGoalsError::InvalidTargetDate);
        }
    }
    
    // ... rest of create_goal logic
}
```

## Common Feature Flag Keys

Here are some suggested feature flag keys for the RemitWise contracts:

- `strict_goal_dates` - Enforce future dates for savings goals
- `enhanced_validation` - Enable additional input validation
- `batch_operations` - Enable batch operation endpoints
- `advanced_analytics` - Enable advanced analytics features
- `emergency_mode` - Enable emergency mode restrictions
- `rate_limiting` - Enable rate limiting on operations
- `cross_contract_calls` - Enable cross-contract integrations

## Events

The contract emits the following events:

### Flag Updated
```rust
FlagUpdatedEvent {
    key: String,
    enabled: bool,
    updated_by: Address,
    timestamp: u64,
}
```

Topics: `("flags", "updated")`

### Flag Removed
Data: `(key: String, removed_by: Address)`

Topics: `("flags", "removed")`

### Admin Transferred
Data: `(old_admin: Address, new_admin: Address)`

Topics: `("flags", "admin")`

## Storage

The contract uses instance storage with the following keys:

- `ADMIN` - Current admin address
- `FLAGS` - Map of all feature flags
- `INIT` - Initialization status

## Security Considerations

1. **Admin Control**: Only the admin can modify flags. Ensure the admin key is properly secured.
2. **Public Reads**: Anyone can query flag status. Don't use flags for sensitive information.
3. **No History**: Flag changes don't maintain history. Use events for audit trails.
4. **Default Behavior**: Non-existent flags return `false`. Design features to work when flags are missing.

## Testing

Run the test suite:

```bash
cd feature_flags
cargo test
```

## Deployment

Build the contract:

```bash
cargo build --release --target wasm32-unknown-unknown
```

Deploy to testnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/feature_flags.wasm \
  --source <your-key> \
  --network testnet
```

Initialize the contract:

```bash
soroban contract invoke \
  --id <contract-id> \
  --source <admin-key> \
  --network testnet \
  -- initialize \
  --admin <admin-address>
```

## Best Practices

1. **Descriptive Keys**: Use clear, descriptive flag keys (e.g., `strict_goal_dates` not `sgd`)
2. **Documentation**: Always provide meaningful descriptions when setting flags
3. **Gradual Rollout**: Test flags on testnet before enabling on mainnet
4. **Monitoring**: Monitor events to track flag changes
5. **Cleanup**: Remove obsolete flags to reduce storage costs
6. **Default Safe**: Design features so the default (flag disabled) is the safe/stable behavior

## Future Enhancements

Potential improvements for future versions:

- Per-user flag overrides
- Time-based flag activation
- Percentage-based rollouts
- Flag dependencies (flag A requires flag B)
- Flag history/audit log
- Bulk flag operations
