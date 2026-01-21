# Remitwise Contracts - Usage Examples

Practical examples for integrating and using Remitwise smart contracts.

## Table of Contents

- [Bill Payments Examples](#bill-payments-examples)
- [Family Wallet Examples](#family-wallet-examples)
- [Insurance Examples](#insurance-examples)
- [Remittance Split Examples](#remittance-split-examples)
- [Savings Goals Examples](#savings-goals-examples)
- [Complete Integration Example](#complete-integration-example)

---

## Bill Payments Examples

### Use Case 1: Track Monthly Electricity Bill

```rust
use bill_payments::BillPayments;

// Create a recurring electricity bill (100 USDC, due monthly)
let bill_id = BillPayments::create_bill(
    env,
    String::from_small_str("Electricity"),
    100_000_000,        // 100 USDC (8 decimals)
    1704067200,         // 2024-01-01
    true,               // recurring
    30,                 // every 30 days
);

// Check unpaid bills
let unpaid = BillPayments::get_unpaid_bills(env);
assert_eq!(unpaid.len(), 1);

// When bill is paid
let success = BillPayments::pay_bill(env, bill_id);
assert!(success);

// New bill automatically created for next month
let next_unpaid = BillPayments::get_unpaid_bills(env);
assert_eq!(next_unpaid.len(), 1);
```

### Use Case 2: Budget Planning with Multiple Bills

```rust
// Create multiple bills
let electric = BillPayments::create_bill(
    env,
    String::from_small_str("Electricity"),
    50_000_000,
    1704067200,
    true,
    30,
);

let water = BillPayments::create_bill(
    env,
    String::from_small_str("Water"),
    20_000_000,
    1704067200,
    true,
    30,
);

let school = BillPayments::create_bill(
    env,
    String::from_small_str("School Fees"),
    300_000_000,
    1704494400,  // Different due date
    false,       // One-time bill
    0,
);

// Get total obligations
let total_unpaid = BillPayments::get_total_unpaid(env);
// total_unpaid = 370_000_000 stroops

// Dashboard query - show upcoming bills
let upcoming = BillPayments::get_unpaid_bills(env);
for bill in upcoming.iter() {
    println!("Pay {} by {} ({})",
        bill.name,
        bill.due_date,
        if bill.recurring { "Monthly" } else { "Once" }
    );
}
```

---

## Family Wallet Examples

### Use Case 1: Create Family with Different Roles

```rust
use family_wallet::FamilyWallet;

let parent_address = Address::from_contract_id(&env, &parent_contract_id);
let child_address = Address::from_contract_id(&env, &child_contract_id);

// Add parent as admin
FamilyWallet::add_member(
    env,
    parent_address.clone(),
    String::from_small_str("Parent"),
    50_000_000,     // 5 USDC daily limit (unused for admin)
    String::from_small_str("admin"),
);

// Add child with limited spending
FamilyWallet::add_member(
    env,
    child_address.clone(),
    String::from_small_str("Alice"),
    10_000_000,     // 1 USDC daily limit
    String::from_small_str("sender"),
);
```

### Use Case 2: Enforce Spending Limits

```rust
// Verify spending limit before allowing transaction
let child_address = Address::from_contract_id(&env, &child_contract_id);
let requested_amount = 15_000_000;  // 1.5 USDC

let within_limit = FamilyWallet::check_spending_limit(
    env,
    child_address,
    requested_amount,
);

if within_limit {
    // Process transaction
    execute_transfer(child_address, requested_amount);
} else {
    // Reject transaction
    println!("Spending exceeds limit");
}
```

### Use Case 3: Update Allowance

```rust
// Parent increases child's weekly allowance
let new_limit = 20_000_000;  // 2 USDC

FamilyWallet::update_spending_limit(
    env,
    child_address,
    new_limit,
);

// Verify update
let member = FamilyWallet::get_member(env, child_address).unwrap();
assert_eq!(member.spending_limit, new_limit);
```

---

## Insurance Examples

### Use Case 1: Create Health Insurance Policy

```rust
use insurance::Insurance;

let policy_id = Insurance::create_policy(
    env,
    String::from_small_str("Family Health Insurance"),
    String::from_small_str("health"),
    50_000_000,     // 5 USDC monthly premium
    1_000_000_000,  // 100 USDC coverage
);

// Policy created with first payment due 30 days from now
let policy = Insurance::get_policy(env, policy_id).unwrap();
assert!(policy.active);
```

### Use Case 2: Monthly Premium Payment

```rust
// Simulate monthly premium payment
let success = Insurance::pay_premium(env, policy_id);
assert!(success);

// Check updated policy
let policy = Insurance::get_policy(env, policy_id).unwrap();
// next_payment_date advanced 30 days
```

### Use Case 3: Total Monthly Budget

```rust
// Create multiple insurance policies
let health = Insurance::create_policy(
    env,
    String::from_small_str("Health"),
    String::from_small_str("health"),
    50_000_000,
    1_000_000_000,
);

let emergency = Insurance::create_policy(
    env,
    String::from_small_str("Emergency"),
    String::from_small_str("emergency"),
    20_000_000,
    500_000_000,
);

let education = Insurance::create_policy(
    env,
    String::from_small_str("Education"),
    String::from_small_str("education"),
    30_000_000,
    300_000_000,
);

// Calculate total insurance budget
let total_premium = Insurance::get_total_monthly_premium(env);
assert_eq!(total_premium, 100_000_000);  // 10 USDC total

// Get all policies for dashboard
let policies = Insurance::get_active_policies(env);
for policy in policies.iter() {
    println!("{}: {} USDC/month",
        policy.name,
        policy.monthly_premium / 100_000_000
    );
}
```

### Use Case 4: Cancel Insurance Policy

```rust
// Deactivate education insurance
let success = Insurance::deactivate_policy(env, education);
assert!(success);

// New total excludes education
let new_total = Insurance::get_total_monthly_premium(env);
assert_eq!(new_total, 70_000_000);  // 7 USDC
```

---

## Remittance Split Examples

### Use Case 1: Configure Personal Split Preferences

```rust
use remittance_split::RemittanceSplit;

// Setup: 50% spending, 30% savings, 15% bills, 5% insurance
let success = RemittanceSplit::initialize_split(
    env,
    50,   // spending
    30,   // savings
    15,   // bills
    5,    // insurance
);
assert!(success);

// Verify configuration
let split = RemittanceSplit::get_split(env);
assert_eq!(split.get(0).unwrap(), 50);  // spending
assert_eq!(split.get(1).unwrap(), 30);  // savings
assert_eq!(split.get(2).unwrap(), 15);  // bills
assert_eq!(split.get(3).unwrap(), 5);   // insurance
```

### Use Case 2: Automatic Money Distribution

```rust
// Receive remittance of 100 USDC
let remittance = 100_000_000;  // 100 USDC

// Calculate distribution
let distribution = RemittanceSplit::calculate_split(env, remittance);
// Result: [50_000_000, 30_000_000, 15_000_000, 5_000_000]

let spending_amount = distribution.get(0).unwrap();  // 50 USDC
let savings_amount = distribution.get(1).unwrap();   // 30 USDC
let bills_amount = distribution.get(2).unwrap();     // 15 USDC
let insurance_amount = distribution.get(3).unwrap(); // 5 USDC

// Transfer to respective accounts
transfer_to_spending_wallet(spending_amount);
transfer_to_savings_wallet(savings_amount);
transfer_to_bills_wallet(bills_amount);
transfer_to_insurance_wallet(insurance_amount);
```

### Use Case 3: Adjust Split Based on Life Changes

```rust
// After getting married: increase savings, reduce bills
let success = RemittanceSplit::initialize_split(
    env,
    40,   // spending (reduced from 50)
    40,   // savings (increased from 30)
    15,   // bills (same)
    5,    // insurance (same)
);
assert!(success);

// New distribution for 100 USDC remittance
let new_distribution = RemittanceSplit::calculate_split(env, 100_000_000);
// Result: [40_000_000, 40_000_000, 15_000_000, 5_000_000]
```

---

## Savings Goals Examples

### Use Case 1: Education Fund for Child

```rust
use savings_goals::SavingsGoals;

// Create 5-year goal: 5,000 USDC for university
let target_date = 1830787200;  // January 1, 2028

let education_goal = SavingsGoals::create_goal(
    env,
    String::from_small_str("University Fund"),
    500_000_000_000,    // 5,000 USDC
    target_date,
);

// Monthly contributions
for month in 0..60 {
    let contribution = 83_333_333;  // ~83.33 USDC per month
    let updated_amount = SavingsGoals::add_to_goal(
        env,
        education_goal,
        contribution,
    );

    // Check progress
    let goal = SavingsGoals::get_goal(env, education_goal).unwrap();
    let progress = (goal.current_amount * 100) / goal.target_amount;
    println!("Goal progress: {}%", progress);
}

// Verify goal completed
let completed = SavingsGoals::is_goal_completed(env, education_goal);
assert!(completed);
```

### Use Case 2: Emergency Fund

```rust
// Create emergency fund goal
let emergency_goal = SavingsGoals::create_goal(
    env,
    String::from_small_str("Emergency Fund"),
    100_000_000,  // 10 USDC (3-month buffer)
    1830787200,   // Far future date
);

// Add savings from remittance split
let emergency_amount = 30_000_000;  // From split calculation
SavingsGoals::add_to_goal(env, emergency_goal, emergency_amount);

// Track emergency fund status
let goal = SavingsGoals::get_goal(env, emergency_goal).unwrap();
if goal.current_amount >= goal.target_amount {
    println!("Emergency fund complete - can handle 3-month expenses");
}
```

### Use Case 3: Multiple Goals Dashboard

```rust
// Create multiple savings goals
let education = SavingsGoals::create_goal(env, /* ... */);
let emergency = SavingsGoals::create_goal(env, /* ... */);
let vacation = SavingsGoals::create_goal(env, /* ... */);

// Display all goals with progress
let all_goals = SavingsGoals::get_all_goals(env);
for goal in all_goals.iter() {
    let progress_pct = (goal.current_amount * 100) / goal.target_amount;
    let remaining = goal.target_amount - goal.current_amount;

    println!("Goal: {}", goal.name);
    println!("  Progress: {}% ({}/{})",
        progress_pct,
        goal.current_amount / 100_000_000,  // In USDC
        goal.target_amount / 100_000_000
    );
    println!("  Remaining: {} USDC", remaining / 100_000_000);
    println!("  Target date: {}", goal.target_date);
}
```

---

## Complete Integration Example

### Scenario: Complete Financial Flow

User receives 1,000 USDC remittance and needs to manage money across all categories.

```rust
use bill_payments::BillPayments;
use family_wallet::FamilyWallet;
use insurance::Insurance;
use remittance_split::RemittanceSplit;
use savings_goals::SavingsGoals;

fn process_remittance(env: Env, amount: i128) {
    // Step 1: Split remittance according to preferences
    let distribution = RemittanceSplit::calculate_split(env, amount);
    let spending = distribution.get(0).unwrap();
    let savings = distribution.get(1).unwrap();
    let bills = distribution.get(2).unwrap();
    let insurance_amt = distribution.get(3).unwrap();

    // Step 2: Add to spending budget (for family wallet)
    let spending_account = get_primary_account();
    FamilyWallet::check_spending_limit(env, spending_account, spending);

    // Step 3: Pay overdue bills
    let unpaid_bills = BillPayments::get_unpaid_bills(env);
    let mut bills_paid = 0i128;
    for bill in unpaid_bills.iter() {
        if bills_paid + bill.amount <= bills {
            if BillPayments::pay_bill(env, bill.id) {
                bills_paid += bill.amount;
            }
        }
    }

    // Step 4: Pay insurance premiums
    let insurance_policies = Insurance::get_active_policies(env);
    for policy in insurance_policies.iter() {
        if Insurance::pay_premium(env, policy.id) {
            // Premium paid
        }
    }

    // Step 5: Contribute to savings goals
    let goals = SavingsGoals::get_all_goals(env);
    let per_goal = savings / (goals.len() as i128);
    for goal in goals.iter() {
        SavingsGoals::add_to_goal(env, goal.id, per_goal);
    }

    // Step 6: Final report
    println!("Remittance processing complete:");
    println!("  Spending: {} USDC", spending / 100_000_000);
    println!("  Savings: {} USDC", savings / 100_000_000);
    println!("  Bills paid: {} USDC", bills_paid / 100_000_000);
    println!("  Insurance: {} USDC", insurance_amt / 100_000_000);
}
```

### Integration Best Practices

1. **Atomic Operations**: Group related operations
2. **Validation**: Check spending limits and policy status before transactions
3. **Error Handling**: Always check return values
4. **Auditing**: Log all significant financial transactions
5. **Rate Limiting**: Implement rate limiting for frequent operations
6. **Access Control**: Enforce role-based permissions in FamilyWallet

---

## Testing Patterns

### Unit Test Example

```rust
#[test]
fn test_bill_payment_flow() {
    let env = Env::default();

    // Create bill
    let bill_id = BillPayments::create_bill(
        env,
        String::from_small_str("Test Bill"),
        100_000_000,
        1704067200,
        true,
        30,
    );

    // Verify created
    assert!(BillPayments::get_bill(env, bill_id).is_some());

    // Pay bill
    let success = BillPayments::pay_bill(env, bill_id);
    assert!(success);

    // Verify recurring bill created
    assert_eq!(BillPayments::get_unpaid_bills(env).len(), 1);
}
```

---

## Common Integration Patterns

### Pattern 1: Weekly Budget Check

```rust
fn weekly_financial_check(env: Env) {
    // Get financial snapshot
    let unpaid_bills = BillPayments::get_total_unpaid(env);
    let insurance_premium = Insurance::get_total_monthly_premium(env);
    let goals = SavingsGoals::get_all_goals(env);

    // Calculate required funds
    let total_obligations = unpaid_bills + insurance_premium;

    // Recommend weekly allocation
    let weekly_allocation = total_obligations / 4;
    println!("Recommended weekly savings: {} stroops", weekly_allocation);
}
```

### Pattern 2: Member Expense Tracking

```rust
fn track_member_spending(
    env: Env,
    member_address: Address,
    transaction_amount: i128,
) -> bool {
    // Check if spending is allowed
    let within_limit = FamilyWallet::check_spending_limit(
        env,
        member_address,
        transaction_amount,
    );

    if within_limit {
        // Execute transaction
        process_payment(member_address, transaction_amount);
        true
    } else {
        // Alert and reject
        alert_spending_limit_exceeded(member_address);
        false
    }
}
```
