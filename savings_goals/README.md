# Savings Goals Contract

Soroban smart contract for creating and tracking personal savings goals with target amounts and completion dates.

## Overview

The Savings Goals contract enables users to define and monitor financial objectives:

- Create goals with target amounts and dates
- Track contributions incrementally
- Monitor progress toward targets
- Validate goal completion

## Key Features

### ✓ Goal Management

- Create goals with flexible targets
- Set target dates and amounts
- Track current savings amount
- Automatic completion detection

### ✓ Progress Tracking

- Add funds incrementally
- Monitor percentage progress
- Calculate remaining amount needed
- Detect milestone achievements

### ✓ Goal Categories

- Education funds
- Emergency funds
- Medical expenses
- Home purchases
- Custom goals

## Data Structure

### SavingsGoal

```rust
pub struct SavingsGoal {
    pub id: u32,              // Unique identifier
    pub name: String,         // Goal name
    pub target_amount: i128,  // Target in stroops
    pub current_amount: i128, // Saved so far
    pub target_date: u64,     // Completion date (Unix timestamp)
    pub locked: bool,         // Funds locked until target date?
}
```

## API Reference

### create_goal

Create a new savings goal.

```rust
pub fn create_goal(
    env: Env,
    name: String,
    target_amount: i128,
    target_date: u64,
) -> u32
```

**Parameters:**

- `name`: Goal name (e.g., "Education", "Emergency")
- `target_amount`: Target amount in stroops
- `target_date`: Target completion date (Unix timestamp)

**Returns:** Goal ID

**Notes:** Starts with current_amount = 0, locked = true

**Example:**

```rust
let goal_id = SavingsGoals::create_goal(
    env,
    String::from_small_str("University Fund"),
    500_000_000_000,   // 5,000 USDC target
    1830787200,        // 2028-01-01
);
```

### add_to_goal

Deposit funds into a savings goal.

```rust
pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128
```

**Parameters:**

- `goal_id`: ID of goal
- `amount`: Amount to add in stroops

**Returns:** Updated current_amount, or -1 if goal not found

**Example:**

```rust
let contribution = 50_000_000;  // 5 USDC
let new_total = SavingsGoals::add_to_goal(env, goal_id, contribution);

if new_total == -1 {
    println!("Goal not found");
} else {
    println!("New balance: {}", new_total / 100_000_000);
}
```

### get_goal

Retrieve a specific savings goal.

```rust
pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal>
```

**Returns:** `Some(SavingsGoal)` if found, `None` otherwise

### get_all_goals

Get all savings goals.

```rust
pub fn get_all_goals(env: Env) -> Vec<SavingsGoal>
```

**Returns:** Vector of all goals

### is_goal_completed

Check if goal target reached.

```rust
pub fn is_goal_completed(env: Env, goal_id: u32) -> bool
```

**Parameters:**

- `goal_id`: ID of goal to check

**Returns:** `true` if current_amount >= target_amount

**Example:**

```rust
if SavingsGoals::is_goal_completed(env, goal_id) {
    println!("Goal completed!");
}
```

## Usage Examples

### Create Education Goal

```rust
// Create 5-year university savings goal
let goal_id = SavingsGoals::create_goal(
    env,
    String::from_small_str("University Fund"),
    500_000_000_000,   // 5,000 USDC
    1830787200,        // 2028-01-01
);

// Monthly contributions
for month in 0..60 {
    let monthly_amount = 83_333_333;  // ~83 USDC/month
    let balance = SavingsGoals::add_to_goal(env, goal_id, monthly_amount);

    let goal = SavingsGoals::get_goal(env, goal_id).unwrap();
    let progress = (goal.current_amount * 100) / goal.target_amount;
    println!("Month {}: {}% complete", month + 1, progress);
}

// Verify completed
assert!(SavingsGoals::is_goal_completed(env, goal_id));
```

### Multiple Concurrent Goals

```rust
// Create diverse goals
let education = SavingsGoals::create_goal(
    env,
    String::from_small_str("Education"),
    500_000_000_000,
    1830787200,
);

let emergency = SavingsGoals::create_goal(
    env,
    String::from_small_str("Emergency Fund"),
    100_000_000,  // 10 USDC emergency buffer
    1830787200,
);

let medical = SavingsGoals::create_goal(
    env,
    String::from_small_str("Medical Fund"),
    200_000_000,  // 20 USDC for healthcare
    1830787200,
);

// Display all goals
let all_goals = SavingsGoals::get_all_goals(env);
println!("Created {} goals", all_goals.len());
```

### Monthly Allocation from Remittance

```rust
// Receive 100 USDC remittance
let remittance = 10_000_000_000;

// Split for savings
let savings_allocation = (remittance * 30) / 100;  // 30% = 30 USDC

// Get all goals
let goals = SavingsGoals::get_all_goals(env);

// Distribute equally among goals
let per_goal = savings_allocation / (goals.len() as i128);

for goal in goals.iter() {
    let balance = SavingsGoals::add_to_goal(env, goal.id, per_goal);
    println!("Added {} to {}", per_goal / 100_000_000, goal.name);
}
```

### Track Progress

```rust
// Monitor goal progress
let goal = SavingsGoals::get_goal(env, goal_id).unwrap();

let progress_pct = (goal.current_amount * 100) / goal.target_amount;
let remaining = goal.target_amount - goal.current_amount;
let months_until_target = if remaining > 0 {
    (remaining * 12) / (goal.current_amount + 1)
} else {
    0
};

println!("Goal: {}", goal.name);
println!("Progress: {}%", progress_pct);
println!("Saved: {} USDC", goal.current_amount / 100_000_000);
println!("Target: {} USDC", goal.target_amount / 100_000_000);
println!("Remaining: {} USDC", remaining / 100_000_000);
println!("Months to target: {}", months_until_target);
```

## Integration Points

### With Remittance Split

- Receives savings allocation
- Automatic contribution each remittance
- Coordinates with other financial obligations

### With Bill Payments

- Prevents savings from covering bills
- Prioritizes bills over savings
- Maintains separate accounts

### With Insurance

- Education goals paired with education insurance
- Emergency savings with emergency coverage
- Layered financial protection

### With Family Wallet

- Track family savings
- Allocate per-member goals (future)
- Monitor household financial progress

## Goal Categories

Recommended goal names and typical amounts:

| Category       | Purpose           | Typical Target      | Timeline   |
| -------------- | ----------------- | ------------------- | ---------- |
| Emergency Fund | Crisis buffer     | 3-6 months expenses | Immediate  |
| Education      | School/university | 1,000-5,000 USDC    | 5-10 years |
| Medical        | Healthcare costs  | 500-2,000 USDC      | 3-5 years  |
| Wedding        | Marriage expenses | 500-2,000 USDC      | 2-3 years  |
| Housing        | Home down payment | 5,000+ USDC         | 5+ years   |
| Vacation       | Travel fund       | 500-1,000 USDC      | 1-2 years  |
| Business       | Startup capital   | 2,000-10,000 USDC   | 3+ years   |

## Best Practices

### 1. Emergency Fund First

```rust
// Create emergency fund before other goals
let emergency = SavingsGoals::create_goal(
    env,
    String::from_small_str("Emergency Fund"),
    100_000_000,  // 10 USDC
    future_date,
);

// Add from every remittance
for remittance in remittances.iter() {
    let emergency_pct = 20;  // 20% for emergency
    let contribution = (remittance * emergency_pct) / 100;
    SavingsGoals::add_to_goal(env, emergency_id, contribution);
}
```

### 2. Prioritized Goals

```rust
// Rank goals by priority
let priority_goals = vec![emergency_id, education_id, medical_id];

// Allocate in priority order
let allocation = 50_000_000;  // 5 USDC per remittance
let per_goal = allocation / priority_goals.len();

for goal_id in priority_goals.iter() {
    SavingsGoals::add_to_goal(env, goal_id, per_goal);
}
```

### 3. Milestone Celebrations

```rust
// Check completion and celebrate
let goals = SavingsGoals::get_all_goals(env);

for goal in goals.iter() {
    let pct = (goal.current_amount * 100) / goal.target_amount;

    if pct >= 100 {
        println!("✓ {}: COMPLETED!", goal.name);
    } else if pct >= 75 {
        println!("► {}: 75% there!", goal.name);
    } else if pct >= 50 {
        println!("► {}: Halfway!", goal.name);
    } else if pct >= 25 {
        println!("► {}: Getting started", goal.name);
    }
}
```

## Security Considerations

1. **Immutable Targets**: Once set, target amount can't change
2. **Forward Only**: Can only add funds, never withdraw
3. **Clear Completion**: Progress is verifiable
4. **No Overwrites**: Each contribution accumulates
5. **Audit Trail**: All contributions tracked (via contract events, future)

## Testing

```rust
#[test]
fn test_create_goal() {
    let env = Env::default();

    let id = SavingsGoals::create_goal(
        env,
        String::from_small_str("Test"),
        100_000_000,
        1830787200,
    );
    assert_eq!(id, 1);
}

#[test]
fn test_add_to_goal() {
    let env = Env::default();

    let id = SavingsGoals::create_goal(
        env,
        String::from_small_str("Test"),
        100_000_000,
        1830787200,
    );

    let balance = SavingsGoals::add_to_goal(env, id, 50_000_000);
    assert_eq!(balance, 50_000_000);
}

#[test]
fn test_goal_completion() {
    let env = Env::default();

    let id = SavingsGoals::create_goal(
        env,
        String::from_small_str("Test"),
        100_000_000,
        1830787200,
    );

    // Not completed yet
    assert!(!SavingsGoals::is_goal_completed(env, id));

    // Add funds
    SavingsGoals::add_to_goal(env, id, 100_000_000);

    // Now completed
    assert!(SavingsGoals::is_goal_completed(env, id));
}

#[test]
fn test_multiple_goals() {
    let env = Env::default();

    let g1 = SavingsGoals::create_goal(env, name1, 100M, date);
    let g2 = SavingsGoals::create_goal(env, name2, 200M, date);

    let all = SavingsGoals::get_all_goals(env);
    assert_eq!(all.len(), 2);
}
```

## Deployment

### Compile

```bash
cd savings_goals
cargo build --release --target wasm32-unknown-unknown
```

### Deploy

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/savings_goals.wasm \
  --network testnet \
  --identity deployer
```

## Contract Size

- **Unoptimized**: ~75KB
- **Optimized**: ~37KB

## Gas Costs (Estimate)

- Create goal: ~4,000 stroops
- Add to goal: ~2,500 stroops
- Get goal: ~1,500 stroops
- Check completion: ~1,000 stroops
- Get all goals: ~2,000 stroops

## Error Scenarios

### Add to Nonexistent Goal

```rust
let result = SavingsGoals::add_to_goal(env, 999, 50_000_000);
// Returns: -1 (goal not found)
```

### Query Nonexistent Goal

```rust
let result = SavingsGoals::get_goal(env, 999);
// Returns: None
```

### Check Nonexistent Goal Completion

```rust
let result = SavingsGoals::is_goal_completed(env, 999);
// Returns: false (not found, so not completed)
```

### Overfunding Goal

```rust
let goal_id = SavingsGoals::create_goal(env, name, 100M, date);

// Add more than target
let total = SavingsGoals::add_to_goal(env, goal_id, 150M);
assert_eq!(total, 150M);  // Allows overfunding

// Still marked as completed
assert!(SavingsGoals::is_goal_completed(env, goal_id));
```

## Future Enhancements

- [ ] Withdraw funds (with permission requirements)
- [ ] Modify goal targets
- [ ] Goal milestones and alerts
- [ ] Projected completion date calculation
- [ ] Goal sharing between family members
- [ ] Automatic contributions from remittances
- [ ] Goal templates and suggestions
- [ ] Bonuses for early completion
- [ ] Impact metrics and reporting

## References

- [Full API Reference](../docs/API_REFERENCE.md#savings-goals-contract)
- [Usage Examples](../docs/USAGE_EXAMPLES.md#savings-goals-examples)
- [Architecture Overview](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT_GUIDE.md)

## Support

For questions or issues:

- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
- [Stellar Documentation](https://developers.stellar.org/)
