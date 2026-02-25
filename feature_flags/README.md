# Feature Flags Contract

A simple on-chain feature flags system for gradual rollouts and feature toggles in the RemitWise smart contract ecosystem.

## Overview

The Feature Flags contract provides a centralized mechanism to control feature availability across all RemitWise contracts. This enables:

- **Gradual Rollouts**: Enable features for specific users or gradually roll out to all users
- **Emergency Toggles**: Quickly disable problematic features without redeployment
- **A/B Testing**: Test different feature configurations
- **Safe Deployments**: Deploy code with features disabled, then enable when ready

## Key Features

### Admin-Controlled
Only the designated admin can create, update, or delete feature flags. The admin can be updated by the current admin.

### Public Read Access
Any contract or user can check if a feature is enabled without authentication, making it easy to integrate into existing contracts.

### Event Emission
All flag changes emit events for off-chain monitoring and audit trails.

### Default Behavior
Non-existent flags default to `false` (disabled), ensuring safe-by-default behavior.

## API Reference

### `initialize(admin: Address)`
Initialize the contract with an admin address.

### `set_flag(key: String, enabled: bool, description: String)`
Create or update a feature flag (admin only).

### `is_enabled(key: String) -> bool`
Check if a feature flag is enabled (public read).

### `get_flag(key: String) -> Option<FeatureFlag>`
Get feature flag details including metadata.

### `get_all_flags() -> Map<String, FeatureFlag>`
List all feature flags.

### `delete_flag(key: String)`
Remove a feature flag (admin only).

### `update_admin(new_admin: Address)`
Change the admin address (current admin only).

### `get_admin() -> Address`
Get the current admin address.

## Usage Example

```rust
// Check if a feature is enabled
let flags_client = FeatureFlagsContractClient::new(&env, &flags_contract_id);

if flags_client.is_enabled(&String::from_str(&env, "strict_goal_dates")) {
    // Execute feature-gated code
    validate_strict_dates(target_date);
} else {
    // Execute default behavior
    validate_basic_dates(target_date);
}
```

## Predefined Feature Flags

### `strict_goal_dates`
- **Contract**: savings_goals
- **Description**: Enforces strict validation that target dates must be in the future
- **Default**: Disabled (allows past dates for flexibility)
- **Use Case**: Enable when you want to prevent users from creating goals with past target dates

## Testing

Run the test suite:

```bash
cd feature_flags
cargo test
```

All 11 tests cover:
- Initialization and re-initialization prevention
- Setting and getting flags
- Updating flags
- Deleting flags
- Default behavior for non-existent flags
- Admin updates
- Multiple independent flags
- Event emission

## Deployment

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

## Best Practices

1. **Descriptive Keys**: Use clear, descriptive flag names (e.g., "strict_goal_dates" not "flag1")
2. **Documentation**: Always provide meaningful descriptions when setting flags
3. **Gradual Rollout**: Test flags on testnet before enabling on mainnet
4. **Monitoring**: Monitor events to track flag changes
5. **Cleanup**: Delete unused flags to reduce storage costs
6. **Non-Critical Features**: Only gate non-critical features; critical functionality should not depend on flags
7. **Default Disabled**: Design features to work safely when flags are disabled

## License

MIT
