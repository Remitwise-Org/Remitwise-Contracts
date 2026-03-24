# Remittance Split Contract

A Soroban smart contract for configuring and calculating remittance fund allocations across spending, savings, bills, and insurance categories.

## Overview

The Remittance Split contract manages percentage-based allocations for incoming remittances, automatically distributing funds according to user-defined ratios for different financial categories.

## Features

- Configure allocation percentages (spending, savings, bills, insurance)
- Calculate split amounts from total remittance
- Update split configurations
- Access control for configuration management
- Event emission for audit trails
- Backward compatibility with vector-based storage

## Quickstart

This section provides a minimal example of how to interact with the Remittance Split contract.

**Gotchas:**
- The configured percentages MUST sum up exactly to 100.
- `initialize_split` must be called with a valid `nonce` for replay protection.
- To execute actual underlying asset transfers, use `distribute_usdc` rather than just calculating numbers.

### Write Example: Initializing the Split
*Note: This is pseudo-code demonstrating the Soroban Rust SDK CLI or client approach.*
```rust

let success = client.initialize_split(
    &owner_address,
    &0,  
    &50, 
    &30, 
    &15, 
    &5   
);

```

### Read Example: Fetching the Configuration
```rust

let config = client.get_config();

```

## API Reference

### Data Structures

#### SplitConfig

```rust
pub struct SplitConfig {
    pub owner: Address,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
    pub initialized: bool,
}
```

### Functions

#### `initialize_split(env, owner, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Initializes a remittance split configuration.

**Parameters:**

- `owner`: Address of the split owner (must authorize)
- `spending_percent`: Percentage for spending (0-100)
- `savings_percent`: Percentage for savings (0-100)
- `bills_percent`: Percentage for bills (0-100)
- `insurance_percent`: Percentage for insurance (0-100)

**Returns:** True on success

**Panics:** If percentages don't sum to 100 or already initialized

#### `update_split(env, caller, spending_percent, savings_percent, bills_percent, insurance_percent) -> bool`

Updates an existing split configuration.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `spending_percent`: New spending percentage
- `savings_percent`: New savings percentage
- `bills_percent`: New bills percentage
- `insurance_percent`: New insurance percentage

**Returns:** True on success

**Panics:** If caller not owner, percentages invalid, or not initialized

#### `get_split(env) -> Vec<u32>`

Gets the current split percentages.

**Returns:** Vector [spending, savings, bills, insurance] percentages

#### `get_config(env) -> Option<SplitConfig>`

Gets the full split configuration.

**Returns:** SplitConfig struct or None if not initialized

#### `calculate_split(env, total_amount) -> Vec<i128>`

Calculates split amounts from a total remittance amount.

**Parameters:**

- `total_amount`: Total amount to split (must be positive)

**Returns:** Vector [spending, savings, bills, insurance] amounts

**Panics:** If total_amount not positive

## Usage Examples

### Initializing Split Configuration

```rust
// Initialize with 50% spending, 30% savings, 15% bills, 5% insurance
let success = remittance_split::initialize_split(
    env,
    user_address,
    50, // spending
    30, // savings
    15, // bills
    5,  // insurance
);
```

### Calculating Split Amounts

```rust
// Calculate allocation for 1000 XLM remittance
let amounts = remittance_split::calculate_split(env, 1000_0000000);

// amounts = [500_0000000, 300_0000000, 150_0000000, 50_0000000]
let spending_amount = amounts.get(0).unwrap();
let savings_amount = amounts.get(1).unwrap();
let bills_amount = amounts.get(2).unwrap();
let insurance_amount = amounts.get(3).unwrap();
```

### Updating Configuration

```rust
// Update to 40% spending, 40% savings, 10% bills, 10% insurance
let success = remittance_split::update_split(
    env,
    user_address,
    40, 40, 10, 10
);
```

## Events

- `SplitEvent::Initialized`: When split is initialized
- `SplitEvent::Updated`: When split is updated
- `SplitEvent::Calculated`: When split calculation is performed

## Integration Patterns

### With Other Contracts

The split contract serves as a central allocation engine:

```rust
// Get split amounts
let split = remittance_split::calculate_split(env, remittance_amount);

// Allocate to savings goals
savings_goals::add_to_goal(env, user, goal_id, split.get(1).unwrap())?;

// Create bill payments
bill_payments::create_bill(env, user, "Monthly Bills".into(), split.get(2).unwrap(), due_date, false, 0)?;

// Pay insurance premiums
insurance::pay_premium(env, user, policy_id);
```

### Automated Remittance Processing

```rust
// Process incoming remittance
fn process_remittance(env: Env, user: Address, amount: i128) {
    let split = remittance_split::calculate_split(env, amount);

    // Auto-allocate funds
    allocate_to_savings(env, user, split.get(1).unwrap());
    allocate_to_bills(env, user, split.get(2).unwrap());
    allocate_to_insurance(env, user, split.get(3).unwrap());
}
```

## Security Considerations

- Owner authorization required for configuration changes
- Percentage validation ensures allocations sum to 100%
- Initialization check prevents duplicate setup
- Access control prevents unauthorized modifications

## Snapshot Import Validation

`validate_snapshot_import` is the hardened entry point for importing multi-party split snapshots (Issue #252). Every rule is checked before any state is written — the function is all-or-nothing.

### Validation rules

| # | Rule | Why it exists |
|---|------|---------------|
| 1 | `version` must equal `SNAPSHOT_VERSION` (1) | Prevents importing data from an incompatible schema |
| 2 | `entries` must not be empty | An empty snapshot has no meaningful state to import |
| 3 | `declared_len` must equal `entries.len()` | Detects truncation or tampering in transit |
| 4 | Each `percentage` must be > 0 | A zero-share entry is semantically invalid |
| 5 | Each `percentage` must be ≤ 100 | Values above 100 are out of range and indicate corruption |
| 6 | Sum of all percentages must equal exactly 100 | Ensures the full remittance is allocated with no gap or overlap |
| 7 | No two entries may share the same `owner` | Duplicate owners would allow double-counting |
| 8 | Caller must be the stored contract owner | Prevents unauthorised parties from overwriting split state |
| 9 | Nonce must equal `get_nonce(caller)` | Prevents replay of a previously valid snapshot |

### Error Reference

| Variant | Code | Description |
|---------|------|-------------|
| `EmptySnapshot` | 100 | The entries list is empty |
| `ZeroPercentage` | 101 | An entry has percentage == 0 |
| `PercentageOutOfRange` | 102 | An entry has percentage > 100 |
| `PercentageSumInvalid` | 103 | Sum of percentages ≠ 100 |
| `DuplicateOwner` | 104 | Two entries share the same owner address |
| `OwnerMismatch` | 105 | Entry owner does not match the verified caller |
| `ZeroAddress` | 106 | An entry contains a zero/null address |
| `FieldOutOfRange` | 107 | A numeric field is outside its valid range |
| `LengthMismatch` | 108 | `declared_len` ≠ actual `entries.len()` |
| `UnsupportedVersion` | 109 | Snapshot version is not supported |
| `ChecksumMismatch` | 110 | Checksum does not match recomputed value |
| `Unauthorized` | 111 | Caller is not the contract owner, or nonce is wrong |
| `AlreadyInitialized` | 112 | Contract already initialised (use `update_split`) |

### Security Assumptions

What the module **trusts**:
- The Soroban host correctly enforces `caller.require_auth()`.
- The host's `Address` type guarantees uniqueness — two distinct `Address` values are never equal unless they represent the same account.

What the module **verifies on-chain**:
- Caller identity against the stored `SplitConfig.owner`.
- Nonce freshness to prevent snapshot replay attacks.
- All percentage arithmetic using checked operations to prevent integer overflow.
- Structural integrity (`declared_len` vs actual length) to detect truncated or padded payloads.

### Re-entrancy / double-import

Soroban contracts execute atomically within a single transaction. There is no async callback mechanism, so re-entrancy is not possible. Double-import is prevented by the nonce: each successful call increments the nonce, making the same call invalid on the next invocation.

### Usage Example

```rust
// 1. Fetch the current nonce for the owner
let nonce = client.get_nonce(&owner);

// 2. Build the snapshot
let mut entries = Vec::new(&env);
entries.push_back(SplitEntry { owner: alice.clone(), percentage: 60 });
entries.push_back(SplitEntry { owner: bob.clone(),   percentage: 40 });

let snapshot = MultiSplitSnapshot {
    version: 1,
    declared_len: 2,
    entries,
};

// 3. Import — all validations run before any state is written
client.validate_snapshot_import(&owner, &nonce, &snapshot);
```

## Running Tests

```bash
cargo test -p remittance_split
```

Expected: all tests pass with zero warnings.
