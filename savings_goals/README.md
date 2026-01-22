# Savings Goals Contract

[![Contract](https://img.shields.io/badge/Contract-Savings_Goals-blue)](https://github.com/your-org/remitwise-contracts/tree/main/savings_goals)

A Soroban smart contract for managing goal-based savings with target dates, progress tracking, and automated fund allocation.

## Overview

The Savings Goals contract enables users to create and track specific savings objectives, such as education funds, medical expenses, or emergency savings. It supports multiple concurrent goals with progress monitoring and completion tracking.

## Features

- ✅ **Goal Creation**: Create savings goals with target amounts and dates
- ✅ **Fund Allocation**: Add funds to specific goals from remittances
- ✅ **Progress Tracking**: Monitor savings progress toward targets
- ✅ **Completion Detection**: Automatically detect when goals are achieved
- ✅ **Multiple Goals**: Support for multiple concurrent savings objectives

## Data Structures

### SavingsGoal

```rust
pub struct SavingsGoal {
    pub id: u32,            // Unique goal identifier
    pub name: String,       // Goal description (e.g., "Education", "Medical")
    pub target_amount: i128, // Target savings amount
    pub current_amount: i128, // Current saved amount
    pub target_date: u64,   // Target completion date (Unix timestamp)
    pub locked: bool,       // Whether funds are locked until target date
}
```

## Functions

### Core Functions

| Function | Description |
|----------|-------------|
| `create_goal` | Create a new savings goal |
| `add_to_goal` | Allocate funds to a specific goal |
| `get_goal` | Retrieve goal details by ID |
| `get_all_goals` | Get all savings goals |
| `is_goal_completed` | Check if a goal has reached its target |

### Usage Examples

#### Creating Savings Goals

```rust
// Create education savings goal
let goal_id = contract.create_goal(
    "Children's Education".to_string(),
    5000,       // target amount
    1735689600  // target date (Jan 1, 2025)
);

// Create emergency fund
let emergency_id = contract.create_goal(
    "Emergency Fund".to_string(),
    2000,       // target amount
    1704067200  // target date (Jan 1, 2024)
);
```

#### Adding Funds to Goals

```rust
// Add $300 to education goal
let new_amount = contract.add_to_goal(goal_id, 300);
println!("Education goal now has: {}", new_amount);
```

#### Monitoring Goal Progress

```rust
// Check all goals
let goals = contract.get_all_goals();
for goal in goals {
    let progress = (goal.current_amount as f64 / goal.target_amount as f64) * 100.0;
    println!("{}: {:.1}% complete", goal.name, progress);

    if contract.is_goal_completed(goal.id) {
        println!("🎉 {} goal completed!", goal.name);
    }
}
```

## Integration Patterns

### With Remittance Split

Automatically allocate savings portion to goals:

```rust
// 1. Calculate split from remittance
let split_amounts = remittance_split.calculate_split(total_remittance);

// 2. Allocate savings portion to goals (split_amounts[1])
let goals = savings_goals.get_all_goals();
let mut remaining_savings = split_amounts[1];

// 3. Distribute to goals (prioritize by urgency)
goals.sort_by(|a, b| a.target_date.cmp(&b.target_date)); // Soonest first

for goal in goals {
    if remaining_savings <= 0 { break; }

    let remaining_for_goal = goal.target_amount - goal.current_amount;
    if remaining_for_goal > 0 {
        let allocate = min(remaining_for_goal, remaining_savings);
        savings_goals.add_to_goal(goal.id, allocate);
        remaining_savings -= allocate;
    }
}
```

### Goal-Based Financial Planning

```rust
// Set up comprehensive savings plan
let goals_data = vec![
    ("Emergency Fund", 3000, 6),     // 6 months
    ("School Fees", 2000, 12),       // 1 year
    ("Medical Fund", 1500, 6),       // 6 months
    ("Home Improvement", 5000, 24),  // 2 years
];

let current_time = /* current timestamp */;
for (name, amount, months) in goals_data {
    let target_date = current_time + (months as u64 * 30 * 24 * 60 * 60);
    contract.create_goal(name.to_string(), amount, target_date);
}

// Monthly contribution planning
let monthly_savings = 500; // From remittance split
let goals = contract.get_all_goals();

// Calculate required monthly contributions
for goal in goals {
    let remaining = goal.target_amount - goal.current_amount;
    let months_left = ((goal.target_date - current_time) / (30 * 24 * 60 * 60)) as i128;
    if months_left > 0 {
        let monthly_needed = remaining / months_left;
        println!("{} needs {} monthly", goal.name, monthly_needed);
    }
}
```

## Testing

Run the contract tests:

```bash
cd savings_goals
cargo test
```

Run with verbose output:

```bash
cargo test -- --nocapture
```

## Deployment

Build for deployment:

```bash
cargo build --release --target wasm32-unknown-unknown
```

The compiled WASM file will be available at:
`target/wasm32-unknown-unknown/release/savings_goals.wasm`

## API Reference

For complete API documentation, see [Savings Goals API](../../docs/api/savings_goals.md).

## Error Handling

The contract uses Option types and special return values for error handling:

- `create_goal`: Always succeeds (returns new goal ID)
- `add_to_goal`: Returns -1 if goal not found, otherwise returns updated amount
- `get_goal`: Returns `None` if goal doesn't exist
- `get_all_goals`: Always succeeds (may return empty vector)
- `is_goal_completed`: Returns `false` if goal not found

## Security Considerations

- Goal IDs are auto-incremented to prevent conflicts
- Target amounts are validated as positive values
- Target dates must be in the future
- Funds can only be added (no withdrawal function)
- Goal data is immutable except for current_amount

## Goal Management Best Practices

### Goal Prioritization

1. **Emergency Fund First**: Always prioritize emergency savings
2. **Time-Sensitive Goals**: Focus on goals with near target dates
3. **High-Impact Goals**: Prioritize goals that prevent financial hardship
4. **Progressive Building**: Start with smaller, achievable goals

### Contribution Strategies

1. **Consistent Allocation**: Regular contributions from each remittance
2. **Accelerated Saving**: Extra contributions when possible
3. **Goal-Specific Timing**: Align contributions with goal timelines
4. **Flexible Redistribution**: Move funds between goals as priorities change

### Monitoring and Adjustment

1. **Regular Progress Checks**: Monitor goal progress monthly
2. **Timeline Adjustments**: Extend deadlines if needed
3. **Amount Modifications**: Adjust targets based on changing needs
4. **Celebrate Milestones**: Recognize progress and completion

## Future Enhancements

- [ ] Goal categories and prioritization
- [ ] Automatic fund redistribution
- [ ] Goal progress notifications
- [ ] Savings milestones and rewards
- [ ] Integration with investment options
- [ ] Goal sharing and family coordination