# Feature Flags System

## Overview

The RemitWise contracts now include a feature flags system for gradual rollouts and safe feature deployment. This allows you to:

- Deploy new features in a disabled state
- Enable features gradually without redeploying contracts
- Quickly disable problematic features (kill switch)
- Test features with specific users or conditions
- Reduce deployment risk

## Architecture

The feature flags system consists of:

1. **Feature Flags Contract** (`feature_flags/`) - Centralized flag storage and management
2. **Integration Pattern** - How other contracts query and use flags
3. **Admin Controls** - Secure flag management

### Contract Structure

```
┌─────────────────────────────┐
│  Feature Flags Contract     │
│  ┌───────────────────────┐  │
│  │ Admin: Address        │  │
│  │ Flags: Map<String,    │  │
│  │        FeatureFlag>   │  │
│  └───────────────────────┘  │
└──────────────┬──────────────┘
               │
               │ Query (no auth)
               │
    ┏━━━━━━━━━━┻━━━━━━━━━━┓
    ┃                      ┃
┌───▼──────┐      ┌────────▼────┐
│ Savings  │      │    Bill     │
│  Goals   │      │  Payments   │
└──────────┘      └─────────────┘
```

## Quick Start

### 1. Deploy Feature Flags Contract

```bash
# Build
cargo build --release --target wasm32-unknown-unknown

# Deploy
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/feature_flags.wasm \
  --source <admin-key> \
  --network testnet

# Initialize
soroban contract invoke \
  --id <contract-id> \
  --source <admin-key> \
  --network testnet \
  -- initialize \
  --admin <admin-address>
```

### 2. Set Feature Flags

```bash
# Enable a feature
soroban contract invoke \
  --id <contract-id> \
  --source <admin-key> \
  --network testnet \
  -- set_flag \
  --caller <admin-address> \
  --key "strict_goal_dates" \
  --enabled true \
  --description "Enforce future dates for savings goals"
```

### 3. Query Flags

```bash
# Check if enabled
soroban contract invoke \
  --id <contract-id> \
  --network testnet \
  -- is_enabled \
  --key "strict_goal_dates"

# Get all flags
soroban contract invoke \
  --id <contract-id> \
  --network testnet \
  -- get_all_flags
```

## Integration Guide

### Basic Integration

To use feature flags in your contract:

```rust
use soroban_sdk::{contract, contractimpl, Address, Env, String};
use feature_flags::FeatureFlagsContractClient;

#[contract]
pub struct MyContract;

#[contractimpl]
impl MyContract {
    // Store the flags contract address during initialization
    pub fn initialize(env: Env, flags_contract: Address) {
        env.storage()
            .instance()
            .set(&symbol_short!("FLAGS"), &flags_contract);
    }

    pub fn my_function(env: Env, user: Address) {
        // Get flags contract address
        let flags_addr: Address = env
            .storage()
            .instance()
            .get(&symbol_short!("FLAGS"))
            .unwrap();

        // Create client
        let flags_client = FeatureFlagsContractClient::new(&env, &flags_addr);

        // Check flag
        let feature_enabled = flags_client.is_enabled(
            &String::from_str(&env, "my_feature")
        );

        if feature_enabled {
            // New behavior
            Self::new_implementation(&env, user);
        } else {
            // Old behavior
            Self::old_implementation(&env, user);
        }
    }
}
```

### Advanced Pattern: Optional Flags Contract

For backward compatibility, make the flags contract optional:

```rust
pub fn my_function(env: Env, user: Address) {
    // Try to get flags contract (may not exist)
    let flags_addr: Option<Address> = env
        .storage()
        .instance()
        .get(&symbol_short!("FLAGS"));

    let feature_enabled = match flags_addr {
        Some(addr) => {
            let client = FeatureFlagsContractClient::new(&env, &addr);
            client.is_enabled(&String::from_str(&env, "my_feature"))
        }
        None => false, // Default to disabled if no flags contract
    };

    if feature_enabled {
        // New behavior
    } else {
        // Old behavior (default)
    }
}
```

## Recommended Feature Flags

### Savings Goals Contract

| Flag Key | Description | Default | Use Case |
|----------|-------------|---------|----------|
| `strict_goal_dates` | Enforce future target dates | `false` | Prevent backdated goals |
| `goal_time_locks` | Enable time-locked withdrawals | `false` | Add withdrawal restrictions |
| `goal_tags_required` | Require at least one tag | `false` | Improve organization |
| `batch_contributions` | Enable batch add operations | `true` | Performance optimization |

### Bill Payments Contract

| Flag Key | Description | Default | Use Case |
|----------|-------------|---------|----------|
| `auto_recurring` | Auto-create recurring bills | `true` | Automation feature |
| `overdue_penalties` | Calculate late fees | `false` | Financial enforcement |
| `bill_reminders` | Enable reminder events | `true` | User notifications |
| `external_refs_required` | Require external reference | `false` | Integration requirement |

### Insurance Contract

| Flag Key | Description | Default | Use Case |
|----------|-------------|---------|----------|
| `auto_premium_deduct` | Auto-deduct premiums | `false` | Automation feature |
| `grace_period` | Allow payment grace period | `true` | User-friendly policy |
| `policy_bundling` | Enable multi-policy discounts | `false` | Advanced feature |

### Family Wallet Contract

| Flag Key | Description | Default | Use Case |
|----------|-------------|---------|----------|
| `enhanced_multisig` | Advanced multisig features | `false` | Security enhancement |
| `emergency_cooldown` | Enforce emergency cooldowns | `true` | Security feature |
| `spending_analytics` | Track spending patterns | `false` | Analytics feature |

## Example: Implementing strict_goal_dates

Here's a complete example of gating the savings goals date validation:

### Step 1: Update Contract Initialization

```rust
// In savings_goals/src/lib.rs

pub fn initialize(env: Env, flags_contract: Option<Address>) {
    // Store flags contract address if provided
    if let Some(addr) = flags_contract {
        env.storage()
            .instance()
            .set(&symbol_short!("FLAGS"), &addr);
    }
    
    // ... rest of initialization
}
```

### Step 2: Add Flag Check to create_goal

```rust
pub fn create_goal(
    env: Env,
    owner: Address,
    name: String,
    target_amount: i128,
    target_date: u64,
) -> Result<u32, SavingsGoalsError> {
    owner.require_auth();

    // Existing validation
    if target_amount <= 0 {
        return Err(SavingsGoalsError::InvalidAmount);
    }

    // NEW: Check feature flag for strict date validation
    let flags_addr: Option<Address> = env
        .storage()
        .instance()
        .get(&symbol_short!("FLAGS"));

    if let Some(addr) = flags_addr {
        let flags_client = FeatureFlagsContractClient::new(&env, &addr);
        let strict_dates = flags_client.is_enabled(
            &String::from_str(&env, "strict_goal_dates")
        );

        if strict_dates {
            let current_time = env.ledger().timestamp();
            if target_date <= current_time {
                return Err(SavingsGoalsError::InvalidTargetDate);
            }
        }
    }

    // ... rest of create_goal logic
}
```

### Step 3: Deploy and Configure

```bash
# 1. Deploy feature flags contract
FLAGS_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/feature_flags.wasm \
  --source admin \
  --network testnet)

# 2. Initialize flags contract
soroban contract invoke \
  --id $FLAGS_ID \
  --source admin \
  --network testnet \
  -- initialize --admin <admin-address>

# 3. Deploy savings goals with flags contract
SAVINGS_ID=$(soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/savings_goals.wasm \
  --source admin \
  --network testnet)

# 4. Initialize savings goals with flags contract address
soroban contract invoke \
  --id $SAVINGS_ID \
  --source admin \
  --network testnet \
  -- initialize --flags_contract $FLAGS_ID

# 5. Enable the feature flag
soroban contract invoke \
  --id $FLAGS_ID \
  --source admin \
  --network testnet \
  -- set_flag \
  --caller <admin-address> \
  --key "strict_goal_dates" \
  --enabled true \
  --description "Enforce future dates for savings goals"
```

## Best Practices

### 1. Naming Conventions

- Use lowercase with underscores: `strict_goal_dates`
- Be descriptive: `enhanced_validation` not `ev`
- Use consistent prefixes for related flags: `goal_*`, `bill_*`

### 2. Default Behavior

- Design features so `false` (disabled) is the safe default
- Existing behavior should work when flag doesn't exist
- New/experimental features should default to disabled

### 3. Documentation

- Always provide clear descriptions when setting flags
- Document flag purpose in contract comments
- Maintain a flags registry (see Recommended Feature Flags above)

### 4. Testing

```rust
#[test]
fn test_feature_with_flag_enabled() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Set up flags contract
    let flags_id = env.register_contract(None, FeatureFlagsContract);
    let flags_client = FeatureFlagsContractClient::new(&env, &flags_id);
    
    let admin = Address::generate(&env);
    flags_client.initialize(&admin);
    
    // Enable feature
    flags_client.set_flag(
        &admin,
        &String::from_str(&env, "my_feature"),
        &true,
        &String::from_str(&env, "Test feature")
    );
    
    // Test with feature enabled
    // ... your test logic
}

#[test]
fn test_feature_with_flag_disabled() {
    // Test with feature disabled
    // ... your test logic
}
```

### 5. Gradual Rollout Strategy

1. **Deploy with flag disabled** - Deploy new code with feature gated
2. **Test on testnet** - Enable flag on testnet, verify behavior
3. **Monitor** - Watch events and metrics
4. **Enable on mainnet** - Enable for production users
5. **Monitor again** - Watch for issues
6. **Remove gate** - After stable period, remove flag check and make feature permanent

### 6. Security

- Protect admin keys - only admin can modify flags
- Use multi-sig for admin in production
- Monitor flag change events
- Have rollback plan (disable flag quickly)

## Events

The feature flags contract emits events for all changes:

### Flag Updated
```rust
FlagUpdatedEvent {
    key: String,
    enabled: bool,
    updated_by: Address,
    timestamp: u64,
}
```

### Flag Removed
```rust
(key: String, removed_by: Address)
```

### Admin Transferred
```rust
(old_admin: Address, new_admin: Address)
```

## API Reference

### Admin Functions

- `initialize(env, admin)` - Initialize contract with admin
- `set_flag(env, caller, key, enabled, description)` - Set or update a flag
- `remove_flag(env, caller, key)` - Remove a flag
- `transfer_admin(env, caller, new_admin)` - Transfer admin role

### Query Functions (No Auth Required)

- `is_enabled(env, key)` - Check if flag is enabled (returns false if not found)
- `get_flag(env, key)` - Get full flag details
- `get_all_flags(env)` - Get all flags
- `get_admin(env)` - Get current admin address
- `is_initialized(env)` - Check if contract is initialized

## Troubleshooting

### Flag not taking effect

1. Verify flag is set: `get_flag(key)`
2. Check contract has correct flags contract address
3. Verify flag key spelling matches exactly
4. Check if contract is caching flag values

### Unauthorized errors

1. Verify caller is the admin: `get_admin()`
2. Check authorization is being passed correctly
3. Ensure admin hasn't been transferred

### Performance concerns

- Flag queries are cheap (single storage read)
- Consider caching flag values if queried frequently
- Batch flag checks if checking multiple flags

## Migration Guide

### Adding Flags to Existing Contract

1. Add flags contract address to storage
2. Add flag checks to relevant functions
3. Deploy updated contract
4. Set flags contract address via admin function
5. Configure desired flags

### Removing Flag Gates

Once a feature is stable:

1. Remove flag check from code
2. Make feature always enabled
3. Deploy updated contract
4. Optionally remove flag from flags contract

## Future Enhancements

Potential improvements for future versions:

- **Per-user overrides** - Enable features for specific users
- **Percentage rollouts** - Enable for X% of users
- **Time-based activation** - Auto-enable at specific time
- **Flag dependencies** - Flag A requires Flag B
- **Audit history** - Track all flag changes
- **Bulk operations** - Set multiple flags at once
- **Flag groups** - Manage related flags together

## Support

For questions or issues:

- Check the [Feature Flags README](feature_flags/README.md)
- Review the [example code](examples/feature_flags_example.rs)
- Run tests: `cargo test -p feature_flags`
