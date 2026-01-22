# Usage Examples

This document provides practical examples of how to use the RemitWise smart contracts for common remittance management scenarios.

## Prerequisites

Before using these contracts, ensure you have:
- Soroban CLI installed
- Contracts deployed to a Stellar network
- Contract IDs for each deployed contract

## Common Use Cases

### 1. Setting Up Remittance Split for a New User

When a user receives their first remittance, set up automatic fund allocation:

```rust
// Initialize split: 40% spending, 30% savings, 20% bills, 10% insurance
let split_contract = get_contract_id("remittance_split");
let success = split_contract.initialize_split(
    40, // spending_percent
    30, // savings_percent
    20, // bills_percent
    10  // insurance_percent
);
assert!(success);
```

### 2. Processing a Remittance Payment

When remittance arrives, automatically split it according to the configured percentages:

```rust
let total_remittance = 1000; // $1000 received
let split_contract = get_contract_id("remittance_split");
let amounts = split_contract.calculate_split(total_remittance);
// amounts = [400, 300, 200, 100] for spending, savings, bills, insurance
```

### 3. Managing Bill Payments

Set up recurring bills and track payments:

```rust
let bill_contract = get_contract_id("bill_payments");

// Create a monthly electricity bill
let bill_id = bill_contract.create_bill(
    "Electricity".to_string(),
    150, // amount
    1672531200, // due date (Jan 1, 2023)
    true, // recurring
    30 // monthly
);

// Pay the bill
let paid = bill_contract.pay_bill(bill_id);
assert!(paid);

// Check unpaid bills
let unpaid = bill_contract.get_unpaid_bills();
let total_unpaid = bill_contract.get_total_unpaid();
```

### 4. Setting Up Savings Goals

Create and manage savings goals for future expenses:

```rust
let savings_contract = get_contract_id("savings_goals");

// Create education savings goal
let goal_id = savings_contract.create_goal(
    "Children's Education".to_string(),
    5000, // target amount
    1735689600 // target date (Jan 1, 2025)
);

// Add funds from remittance allocation
let new_amount = savings_contract.add_to_goal(goal_id, 300); // from savings split

// Check goal progress
let goal = savings_contract.get_goal(goal_id);
let completed = savings_contract.is_goal_completed(goal_id);
```

### 5. Managing Insurance Policies

Set up micro-insurance for financial protection:

```rust
let insurance_contract = get_contract_id("insurance");

// Create health insurance policy
let policy_id = insurance_contract.create_policy(
    "Family Health Insurance".to_string(),
    "health".to_string(),
    50, // monthly premium
    10000 // coverage amount
);

// Pay monthly premium
let paid = insurance_contract.pay_premium(policy_id);
assert!(paid);

// Check active policies
let active_policies = insurance_contract.get_active_policies();
let total_premium = insurance_contract.get_total_monthly_premium();
```

### 6. Integration Pattern: Complete Remittance Processing

A complete example showing how all contracts work together:

```rust
// 1. Receive remittance
let total_amount = 2000;

// 2. Split the remittance
let split_contract = get_contract_id("remittance_split");
let split_amounts = split_contract.calculate_split(total_amount);
// [800, 600, 400, 200] for spending, savings, bills, insurance

// 3. Allocate to savings goals
let savings_contract = get_contract_id("savings_goals");
let goals = savings_contract.get_all_goals();
for goal in goals {
    if goal.current_amount < goal.target_amount {
        let remaining = goal.target_amount - goal.current_amount;
        let allocate = min(remaining, split_amounts[1]); // from savings portion
        savings_contract.add_to_goal(goal.id, allocate);
        split_amounts[1] -= allocate;
        if split_amounts[1] == 0 { break; }
    }
}

// 4. Pay outstanding bills
let bill_contract = get_contract_id("bill_payments");
let unpaid_bills = bill_contract.get_unpaid_bills();
for bill in unpaid_bills {
    if split_amounts[2] >= bill.amount { // from bills portion
        bill_contract.pay_bill(bill.id);
        split_amounts[2] -= bill.amount;
    }
}

// 5. Pay insurance premiums
let insurance_contract = get_contract_id("insurance");
let active_policies = insurance_contract.get_active_policies();
for policy in active_policies {
    if split_amounts[3] >= policy.monthly_premium { // from insurance portion
        insurance_contract.pay_premium(policy.id);
        split_amounts[3] -= policy.monthly_premium;
    }
}

// Remaining amounts are available for spending
let spending_available = split_amounts[0];
```

### 7. Monitoring and Reporting

Generate reports on financial health:

```rust
// Get comprehensive financial overview
let bill_contract = get_contract_id("bill_payments");
let insurance_contract = get_contract_id("insurance");
let savings_contract = get_contract_id("savings_goals");

let unpaid_bills = bill_contract.get_total_unpaid();
let monthly_premiums = insurance_contract.get_total_monthly_premium();
let savings_goals = savings_contract.get_all_goals();

let total_goals_value = savings_goals.iter().map(|g| g.current_amount).sum();
let completed_goals = savings_goals.iter().filter(|g| g.current_amount >= g.target_amount).count();

// Financial health score calculation
let health_score = calculate_health_score(unpaid_bills, monthly_premiums, total_goals_value);
```

## Error Handling Patterns

Always check return values and handle errors appropriately:

```rust
// Safe bill payment with error handling
let bill_id = 1;
let payment_result = bill_contract.pay_bill(bill_id);
match payment_result {
    true => println!("Bill paid successfully"),
    false => {
        // Check if bill exists
        match bill_contract.get_bill(bill_id) {
            Some(bill) => {
                if bill.paid {
                    println!("Bill already paid");
                } else {
                    println!("Payment failed");
                }
            }
            None => println!("Bill not found"),
        }
    }
}
```

## Best Practices

1. **Initialize contracts in order**: Set up remittance split first, then create goals and policies
2. **Regular monitoring**: Check unpaid bills and due premiums regularly
3. **Goal prioritization**: Allocate savings to goals based on urgency and target dates
4. **Error handling**: Always check return values and handle failure cases
5. **Batch operations**: Process multiple bills or goals in single transactions when possible