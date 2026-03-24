# Savings Goals Contract

A Soroban smart contract for managing savings goals with fund tracking, locking mechanisms, and goal completion monitoring.

## Overview

The Savings Goals contract allows users to create savings goals, add/withdraw funds, and lock goals to prevent premature withdrawals. It supports multiple goals per user with progress tracking.

## Features

- Create savings goals with target amounts and dates
- Add funds to goals with progress tracking
- Withdraw funds (when goal is unlocked)
- Lock/unlock goals for withdrawal control
- Query goals and completion status
- Access control for goal management
- Event emission for audit trails
- Storage TTL management
- **Batch atomic operations**: All-or-nothing fund additions to multiple goals with comprehensive validation

## Batch Atomicity

The `batch_add_to_goals` function provides atomic batch funding operations with the following guarantees:

### Atomicity Semantics
- **All-or-nothing execution**: Either all contributions succeed or none do
- **Upfront validation**: All inputs are validated before any storage modifications
- **Rollback on failure**: If any contribution fails, no changes are persisted

### Security Features
- **Overflow protection**: Prevents integer overflow in goal balances
- **Authorization checks**: Verifies caller owns all target goals
- **Amount validation**: Ensures positive contribution amounts
- **Size limits**: Maximum 50 goals per batch to prevent gas exhaustion

### Event Emission
- `BatchStartedEvent`: Emitted when batch processing begins
- `FundsAdded`: Emitted for each successful contribution
- `GoalCompleted`: Emitted when goals reach their targets
- `BatchCompletedEvent`: Emitted when all contributions succeed
- `BatchFailedEvent`: Emitted if batch processing fails

### Usage Example
```rust
// Prepare batch contributions
let contributions = Vec::from_array(&env, [
    ContributionItem { goal_id: goal1_id, amount: 500 },
    ContributionItem { goal_id: goal2_id, amount: 1000 },
    ContributionItem { goal_id: goal3_id, amount: 250 },
]);

// Execute atomic batch
let processed_count = client.batch_add_to_goals(&user, &contributions)?;
assert_eq!(processed_count, 3); // All contributions succeeded
```

### Error Handling
- `BatchTooLarge`: Exceeds maximum batch size (50 goals)
- `BatchValidationFailed`: Invalid contribution data or authorization failure
- `InsufficientBalance`: (Future use - not currently triggered in batch context)
- Deterministic cursor pagination with owner-bound consistency checks

## Pagination Stability

`get_goals(owner, cursor, limit)` now uses the owner goal-ID index as the canonical ordering source.

- Ordering is deterministic: ascending goal creation ID for that owner.
- Cursor is exclusive: page N+1 starts strictly after the cursor ID.
- Cursor is owner-bound: a non-zero cursor must exist in that owner's index.
- Invalid/stale non-zero cursors are rejected to prevent silent duplicate/skip behavior.

### Cursor Semantics

- `cursor = 0` starts from the first goal.
- `next_cursor = 0` means there are no more pages.
- If writes happen between reads, new goals are appended and will appear in later pages without duplicating already-read items.

### Security Notes

- Pagination validates index-to-storage consistency and owner binding.
- Any detected index/storage mismatch fails fast instead of returning ambiguous data.
- This reduces the risk of inconsistent client state caused by malformed or stale cursors.

## Quickstart

This section provides a minimal example of how to interact with the Savings Goals contract.

**Gotchas:**
- Amounts are specified in the lowest denomination (e.g., stroops for XLM).
- If a goal is `locked = true`, you cannot withdraw from it until it is unlocked.
- By default, the contract uses paginated reads for scalability, so ensure you handle cursors when querying user goals.

### Write Example: Creating a Goal
*Note: This is pseudo-code demonstrating the Soroban Rust SDK CLI or client approach.*
```rust

let goal_id = client.create_goal(
    &owner_address,
    &String::from_str(&env, "University Fund"),
    &5000_0000000,                          
    &(env.ledger().timestamp() + 31536000)  
);

```

### Read Example: Checking Goal Status
```rust

let goal_opt = client.get_goal(&goal_id);

if let Some(goal) = goal_opt {

}

```

## API Reference

### Data Structures

#### SavingsGoal

```rust
pub struct SavingsGoal {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub target_amount: i128,
    pub current_amount: i128,
    pub target_date: u64,
    pub locked: bool,
}
```

#### ContributionItem

```rust
pub struct ContributionItem {
    pub goal_id: u32,
    pub amount: i128,
}
```

Used in batch operations to specify goal contributions.

### Functions

#### `init(env)`

Initializes contract storage.

**Parameters:**

- `env`: Contract environment

#### `create_goal(env, owner, name, target_amount, target_date) -> u32`

Creates a new savings goal.

**Parameters:**

- `owner`: Address of the goal owner (must authorize)
- `name`: Goal name (e.g., "Education", "Medical")
- `target_amount`: Target amount (must be positive)
- `target_date`: Target date as Unix timestamp

**Returns:** Goal ID

**Panics:** If inputs invalid or owner doesn't authorize

#### `add_to_goal(env, caller, goal_id, amount) -> i128`

Adds funds to a savings goal.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `goal_id`: ID of the goal
- `amount`: Amount to add (must be positive)

**Returns:** Updated current amount

**Panics:** If caller not owner, goal not found, or amount invalid

#### `withdraw_from_goal(env, caller, goal_id, amount) -> i128`

Withdraws funds from a savings goal.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `goal_id`: ID of the goal
- `amount`: Amount to withdraw (must be positive, <= current_amount)

**Returns:** Updated current amount

**Panics:** If caller not owner, goal locked, insufficient balance, etc.

#### `batch_add_to_goals(env, caller, contributions) -> Result<u32, SavingsGoalsError>`

Atomically adds funds to multiple savings goals with all-or-nothing semantics.

**Parameters:**

- `caller`: Address of the caller (must own all goals)
- `contributions`: Vector of ContributionItem structs (max 50 items)

**Returns:** Number of successful contributions (same as input length on success)

**Errors:**
- `BatchTooLarge`: More than 50 contributions
- `BatchValidationFailed`: Invalid data or authorization failure

**Atomicity:** All contributions succeed or none do. Comprehensive validation occurs before any storage changes.

#### `lock_goal(env, caller, goal_id) -> bool`

Locks a goal to prevent withdrawals.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `goal_id`: ID of the goal

**Returns:** True on success

**Panics:** If caller not owner or goal not found

#### `unlock_goal(env, caller, goal_id) -> bool`

Unlocks a goal to allow withdrawals.

**Parameters:**

- `caller`: Address of the caller (must be owner)
- `goal_id`: ID of the goal

**Returns:** True on success

**Panics:** If caller not owner or goal not found

#### `get_goal(env, goal_id) -> Option<SavingsGoal>`

Retrieves a goal by ID.

**Parameters:**

- `goal_id`: ID of the goal

**Returns:** SavingsGoal struct or None

#### `get_all_goals(env, owner) -> Vec<SavingsGoal>`

Gets all goals for an owner.

**Parameters:**

- `owner`: Address of the goal owner

**Returns:** Vector of SavingsGoal structs

#### `get_goals(env, owner, cursor, limit) -> GoalPage`

Returns a deterministic page of goals for an owner.

**Parameters:**

- `owner`: Address of the goal owner
- `cursor`: Exclusive cursor (`0` for first page)
- `limit`: Max records to return (`0` uses default, capped by max)

**Returns:** `GoalPage { items, next_cursor, count }`

**Cursor guarantees:**

- `next_cursor` is the last returned goal ID when more pages exist
- `next_cursor = 0` means end of list
- Non-zero invalid cursors are rejected

#### `is_goal_completed(env, goal_id) -> bool`

Checks if a goal is completed.

**Parameters:**

- `goal_id`: ID of the goal

**Returns:** True if current_amount >= target_amount

## Usage Examples

### Creating a Goal

```rust
// Create an education savings goal
let goal_id = savings_goals::create_goal(
    env,
    user_address,
    "College Fund".into(),
    5000_0000000, // 5000 XLM
    env.ledger().timestamp() + (365 * 86400), // 1 year from now
);
```

### Adding Funds

```rust
// Add 100 XLM to the goal
let new_amount = savings_goals::add_to_goal(
    env,
    user_address,
    goal_id,
    100_0000000
);
```

### Managing Goal State

```rust
// Lock the goal to prevent withdrawals
savings_goals::lock_goal(env, user_address, goal_id);

// Unlock for withdrawals
savings_goals::unlock_goal(env, user_address, goal_id);

// Withdraw funds
let remaining = savings_goals::withdraw_from_goal(
    env,
    user_address,
    goal_id,
    50_0000000
);
```

### Batch Funding

```rust
// Create multiple goals
let emergency_id = savings_goals::create_goal(env, user, "Emergency", 1000_0000000, future_date);
let vacation_id = savings_goals::create_goal(env, user, "Vacation", 2000_0000000, future_date);
let education_id = savings_goals::create_goal(env, user, "Education", 5000_0000000, future_date);

// Prepare batch contributions
let contributions = Vec::from_array(&env, [
    ContributionItem { goal_id: emergency_id, amount: 500_0000000 },
    ContributionItem { goal_id: vacation_id, amount: 1000_0000000 },
    ContributionItem { goal_id: education_id, amount: 2000_0000000 },
]);

// Execute atomic batch - all succeed or none do
let processed = savings_goals::batch_add_to_goals(env, user, contributions)?;
assert_eq!(processed, 3);
```

### Querying Goals

```rust
// Get all goals for a user
let goals = savings_goals::get_all_goals(env, user_address);

// Check completion status
let completed = savings_goals::is_goal_completed(env, goal_id);
```

## Events

- `SavingsEvent::GoalCreated`: When a goal is created
- `SavingsEvent::FundsAdded`: When funds are added
- `SavingsEvent::FundsWithdrawn`: When funds are withdrawn
- `SavingsEvent::GoalCompleted`: When goal reaches target
- `SavingsEvent::GoalLocked`: When goal is locked
- `SavingsEvent::GoalUnlocked`: When goal is unlocked
- `BatchStartedEvent`: When batch processing begins
- `BatchCompletedEvent`: When batch processing succeeds
- `BatchFailedEvent`: When batch processing fails

## Integration Patterns

### With Remittance Split

Automatic allocation to savings goals:

```rust
let split_amounts = remittance_split::calculate_split(env, remittance);
let savings_allocation = split_amounts.get(1).unwrap();

// Add to primary savings goal
savings_goals::add_to_goal(env, user, primary_goal_id, savings_allocation)?;
```

### Goal-Based Financial Planning

```rust
// Create multiple goals
let emergency_id = savings_goals::create_goal(env, user, "Emergency Fund", 1000_0000000, future_date);
let vacation_id = savings_goals::create_goal(env, user, "Vacation", 2000_0000000, future_date);

// Allocate funds based on priorities
```

## Security Considerations

- Owner authorization required for all operations
- Goal locking prevents unauthorized withdrawals
- Input validation for amounts and ownership
- Balance checks prevent overdrafts
- Access control ensures user data isolation
- **Batch atomicity**: All-or-nothing execution prevents partial state corruption
- **Batch size limits**: Prevents gas exhaustion and DoS attacks
- **Overflow protection**: Prevents integer overflow in batch operations
