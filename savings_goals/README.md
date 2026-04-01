# Savings Goals Contract

## Lock/Unlock Behavior

### Idempotent Transitions
`lock_goal` and `unlock_goal` are idempotent:
- Calling `lock_goal` on an already-locked goal returns `true` with no state change and no duplicate event.
- Calling `unlock_goal` on an already-unlocked goal returns `true` with no state change and no duplicate event.
- `GoalLocked` and `GoalUnlocked` events fire **only** on real state transitions.

### Security
- Only the goal owner can lock or unlock a goal.
- Idempotent calls are recorded in the audit log as successful.
- Time-locks are not bypassed by repeated unlock calls.
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
- Owner-controlled goal metadata tags
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
    pub tags: Vec<String>,
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

#### `add_tags_to_goal(env, caller, goal_id, tags)`

Adds metadata tags to a goal.

**Parameters:**

- `caller`: Address of the caller (must authorize and be owner)
- `goal_id`: ID of the goal
- `tags`: Tag list to append

**Validation and behavior:**

- Tag list must not be empty
- Each tag must have length 1..=32
- Duplicate tags are allowed

**Panics:** If caller is unauthorized, goal not found, or tags are invalid

#### `remove_tags_from_goal(env, caller, goal_id, tags)`

Removes metadata tags from a goal.

**Parameters:**

- `caller`: Address of the caller (must authorize and be owner)
- `goal_id`: ID of the goal
- `tags`: Tag list to remove

**Validation and behavior:**

- Tag list must not be empty
- Each tag must have length 1..=32
- Removing non-existent tags is a no-op

**Panics:** If caller is unauthorized, goal not found, or tags are invalid

## Time-lock & Schedules

### Time-lock Boundary Behavior

The contract enforces strict timestamp-based access control for withdrawals:
- **Before `unlock_date`**: Withdrawal attempts return `GoalLocked` error.
- **At/After `unlock_date`**: Withdrawal is permitted (assuming the goal is also manually unlocked).

### Schedule Drift Handling

Recurring savings schedules are designed to maintain their cadence even if execution is delayed:
- **Catching Up**: If a schedule is executed after its `next_due`, the contract calculates how many whole `interval` periods have passed since `next_due`. 
- **Missed Count**: Each passed interval that wasn't executed is recorded in `missed_count`.
- **Deterministic Next Due**: The `next_due` for the next execution is set to the next future interval anchor, ensuring no drift accumulates over time.

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

## Savings Schedules

Savings schedules automate recurring or one-shot deposits into a goal.

### Data Structures

#### SavingsSchedule

```rust
pub struct SavingsSchedule {
    pub id: u32,
    pub owner: Address,
    pub goal_id: u32,
    pub amount: i128,
    /// Unix timestamp when the next deposit is due.
    pub next_due: u64,
    /// Seconds between recurring executions; 0 = one-shot.
    pub interval: u64,
    pub recurring: bool,
    pub active: bool,
    pub created_at: u64,
    /// Ledger timestamp of the last successful execution, or None.
    pub last_executed: Option<u64>,
    /// Number of intervals skipped (late executions).
    pub missed_count: u32,
}
```

### Schedule Functions

#### `create_savings_schedule(env, owner, goal_id, amount, next_due, interval) -> u32`

Creates a new savings schedule.

- `owner` must authorize and must be the goal owner.
- `next_due` must be strictly in the future at creation time.
- Set `interval = 0` for a one-shot schedule.

**Returns:** Schedule ID

#### `modify_savings_schedule(env, caller, schedule_id, amount, next_due, interval) -> bool`

Updates the amount, next due date, and interval of an existing schedule.
`next_due` must be in the future at call time.

#### `cancel_savings_schedule(env, caller, schedule_id) -> bool`

Deactivates a schedule; it will not execute after this call.

#### `execute_due_savings_schedules(env) -> Vec<u32>`

Executes all active schedules whose `next_due` is at or before the current
ledger timestamp. Typically called by an off-chain keeper or cron job.

**Returns:** IDs of schedules that were executed.

**Idempotency guarantee:** A schedule is credited to its goal at most once per
due window. If this function is called multiple times at the same ledger
timestamp – for example, when two transactions land in the same Stellar ledger –
only the first call credits the goal. Subsequent calls skip any schedule whose
`last_executed >= next_due` (idempotency guard) or whose `active = false`
(one-shot deactivation).

**Next-due advancement (recurring schedules):**
After execution, `next_due` is advanced by `interval` until it strictly exceeds
`current_time`. Any skipped intervals are counted in `missed_count` and a
`ScheduleMissed` event is emitted.

#### `get_savings_schedule(env, schedule_id) -> Option<SavingsSchedule>`

Retrieves a single schedule by ID.

#### `get_savings_schedules(env, owner) -> Vec<SavingsSchedule>`

Returns all schedules owned by `owner`.

### Schedule Usage Example

```rust
// Create a monthly deposit of 100 XLM into goal_id starting one month from now.
let one_month = 30 * 24 * 3600u64;
let schedule_id = client.create_savings_schedule(
    &owner,
    &goal_id,
    &100_0000000,                              // 100 XLM in stroops
    &(env.ledger().timestamp() + one_month),   // first due date
    &one_month,                                // recurring interval
);

// Off-chain keeper calls this each day; already-executed schedules are skipped.
let executed_ids = client.execute_due_savings_schedules();
```

## Events

- `SavingsEvent::GoalCreated`: When a goal is created
- `SavingsEvent::FundsAdded`: When funds are added
- `SavingsEvent::FundsWithdrawn`: When funds are withdrawn
- `SavingsEvent::GoalCompleted`: When goal reaches target
- `SavingsEvent::GoalLocked`: When goal is locked
- `SavingsEvent::GoalUnlocked`: When goal is unlocked
- `SavingsEvent::ScheduleCreated`: When a schedule is created
- `SavingsEvent::ScheduleExecuted`: When a schedule is executed
- `SavingsEvent::ScheduleMissed`: When one or more intervals are skipped
- `SavingsEvent::ScheduleModified`: When a schedule is modified
- `SavingsEvent::ScheduleCancelled`: When a schedule is cancelled
- `tags_add`: Emitted when tags are added to a goal (`goal_id`, `owner`, `tags`)
- `tags_rem`: Emitted when tags are removed from a goal (`goal_id`, `owner`, `tags`)

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

- Owner authorization required for all mutating operations
- Goal locking and **time-lock boundaries** prevent unauthorized or premature withdrawals
- Support for **deterministic schedule execution** with drift compensation
- Input validation for amounts and ownership
- Balance checks prevent overdrafts
- Access control ensures user data isolation

---

## Migration Compatibility

The Savings Goals contract provides first-class support for off-chain data export
and migration through the `data_migration` crate. This covers four serialisation
formats and includes cryptographic integrity checking.

### On-chain API

| Function | Description |
|---|---|
| `export_snapshot(caller)` | Exports all goals as a `GoalsExportSnapshot` (version + checksum + goal list). Caller must authorize. |
| `import_snapshot(caller, nonce, snapshot)` | Imports a validated snapshot, replacing contract state. Caller must authorize. Nonce prevents replay attacks. |

### Off-chain formats (via `data_migration`)

| Format | Helper (export) | Helper (import) | Notes |
|---|---|---|---|
| JSON | `export_to_json` | `import_from_json` | Human-readable; includes checksum validation |
| Binary | `export_to_binary` | `import_from_binary` | Compact bincode; includes checksum validation |
| CSV | `export_to_csv` | `import_goals_from_csv` | Flat tabular; for spreadsheet tooling |
| Encrypted | `export_to_encrypted_payload` | `import_from_encrypted_payload` | Base64 wrapper; caller handles encryption layer |

The `build_savings_snapshot` helper (in `data_migration`) wraps a
`SavingsGoalsExport` payload into a fully-checksummed `ExportSnapshot` for any
target format.

### Security assumptions

- **Checksum integrity**: Every snapshot carries a SHA-256 checksum over the
  canonical JSON of the payload. Any mutation after export is detected by
  `validate_for_import` → `Err(ChecksumMismatch)`.
- **Version gating**: Snapshots with an unsupported schema version are rejected
  by `validate_for_import` → `Err(IncompatibleVersion)`.
- **Nonce replay protection**: `import_snapshot` requires a monotonically
  increasing nonce per caller; reusing a nonce is rejected on-chain.
- **Authorization**: Both `export_snapshot` and `import_snapshot` require
  `caller.require_auth()`.
- **Encrypted path**: The `Encrypted` format uses base64 as a transport
  envelope. Callers are responsible for applying actual encryption (e.g. AES-GCM)
  to the serialised bytes before passing them to `export_to_encrypted_payload`.

### Example: full JSON roundtrip

```rust
// 1. Export on-chain state
let snapshot: GoalsExportSnapshot = client.export_snapshot(&admin);

// 2. Convert to data_migration format
let export = SavingsGoalsExport {
    next_id: snapshot.next_id,
    goals: snapshot.goals.iter().map(|g| SavingsGoalExport {
        id: g.id,
        owner: format!("{:?}", g.owner),
        name: g.name.to_string(),
        target_amount: g.target_amount as i64,
        current_amount: g.current_amount as i64,
        target_date: g.target_date,
        locked: g.locked,
    }).collect(),
};

// 3. Build migration snapshot (computes checksum)
let mig_snapshot = build_savings_snapshot(export, ExportFormat::Json);

// 4. Serialize to JSON bytes
let bytes = export_to_json(&mig_snapshot).unwrap();

// 5. (transmit bytes off-chain ...)

// 6. Import and validate
let loaded = import_from_json(&bytes).unwrap(); // validates checksum + version
```

### Running migration tests

```bash
# data_migration package (format-level e2e tests)
cargo test -p data_migration

# savings_goals package (contract + cross-package e2e tests)
cargo test -p savings_goals
```
### Savings Schedule Security

| Threat | Mitigation |
|--------|-----------|
| Double-execution (same ledger timestamp) | Idempotency guard: `last_executed >= next_due` skips already-executed schedules |
| Re-execution via `modify_savings_schedule` resetting `next_due` | `modify_savings_schedule` enforces `next_due > current_time`, so a new future date is required; `last_executed` is not reset |
| One-shot replay | Schedule is deactivated (`active = false`) after first execution; subsequent calls skip inactive schedules |
| Overflow on credit | `checked_add` panics on overflow rather than silently wrapping |
| Unauthorized schedule creation | `create_savings_schedule` requires owner authorization and verifies caller is the goal owner |
