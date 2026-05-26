# Savings Goals — GoalCompleted Event

## Overview

When `add_to_goal` or `batch_add_to_goals` brings a goal's `current_amount`
to or above its `target_amount`, the contract emits a single
`SavingsEvent::GoalCompleted` event (carrying a `GoalCompletedEvent` payload).

## Single-emission guarantee

The event fires **exactly once per goal**. Once `is_goal_completed` returns
`true`, no additional `GoalCompleted` events will be emitted for that goal,
even if further contributions are made. This prevents double-triggering in:

- Off-chain indexers polling ledger events
- Push notification pipelines
- Frontend completion animations

## Boundary cases

| Scenario                              | Event emitted? |
|---------------------------------------|---------------|
| Contribution lands exactly on target  | ✅ Yes, once   |
| Contribution overshoots target        | ✅ Yes, once   |
| Partial contribution (below target)   | ❌ No          |
| Contribution after goal is completed  | ❌ No          |

## Event fields

```rust
pub struct GoalCompletedEvent {
    pub goal_id: u32,
    pub name: String,
    pub final_amount: i128,
    pub timestamp: u64,
}
```

## Related symbols

- `add_to_goal(owner, goal_id, amount)` — single-goal contribution
- `batch_add_to_goals(owner, contributions)` — multi-goal batch contribution
- `is_goal_completed(goal_id) -> bool` — query completion status
- `SavingsGoal { current_amount, target_amount }` — underlying storage type
- `SavingsEvent::GoalCompleted` — event variant

## Security note

The single-emission guarantee is enforced by checking `is_goal_completed`
(which reads the persisted `SavingsGoal.current_amount >= target_amount`)
before emitting. Since completion is derived from on-chain state, it cannot
be spoofed or duplicated by replaying transactions.# Savings Goals — GoalCompleted Event

## Overview

When `add_to_goal` or `batch_add_to_goals` brings a goal's `current_amount`
to or above its `target_amount`, the contract emits a single
`SavingsEvent::GoalCompleted` event (carrying a `GoalCompletedEvent` payload).

## Single-emission guarantee

The event fires **exactly once per goal**. Once `is_goal_completed` returns
`true`, no additional `GoalCompleted` events will be emitted for that goal,
even if further contributions are made. This prevents double-triggering in:

- Off-chain indexers polling ledger events
- Push notification pipelines
- Frontend completion animations

## Boundary cases

| Scenario                              | Event emitted? |
|---------------------------------------|---------------|
| Contribution lands exactly on target  | ✅ Yes, once   |
| Contribution overshoots target        | ✅ Yes, once   |
| Partial contribution (below target)   | ❌ No          |
| Contribution after goal is completed  | ❌ No          |

## Event fields

```rust
pub struct GoalCompletedEvent {
    pub goal_id: u32,
    pub name: String,
    pub final_amount: i128,
    pub timestamp: u64,
}
```

## Related symbols

- `add_to_goal(owner, goal_id, amount)` — single-goal contribution
- `batch_add_to_goals(owner, contributions)` — multi-goal batch contribution
- `is_goal_completed(goal_id) -> bool` — query completion status
- `SavingsGoal { current_amount, target_amount }` — underlying storage type
- `SavingsEvent::GoalCompleted` — event variant

## Security note

The single-emission guarantee is enforced by checking `is_goal_completed`
(which reads the persisted `SavingsGoal.current_amount >= target_amount`)
before emitting. Since completion is derived from on-chain state, it cannot
be spoofed or duplicated by replaying transactions.