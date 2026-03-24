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
