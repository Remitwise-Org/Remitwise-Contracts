# Bill Payments Contract

Soroban smart contract for managing bill payments including one-time and recurring bills.

## Overview

The Bill Payments contract provides comprehensive bill management functionality:

- Create bills with customizable amounts and due dates
- Track payment status
- Automatic recurring bill generation
- Query unpaid bills and total obligations

## Key Features

### ✓ Bill Creation

- Support for both one-time and recurring bills
- Customizable due dates
- Automatic next bill creation for recurring bills

### ✓ Payment Tracking

- Mark bills as paid
- Query unpaid bills
- Calculate total outstanding obligations

### ✓ Recurring Automation

- Automatic next bill creation when payment processed
- Configurable frequency (in days)
- Maintains bill history

## Data Structure

### Bill

```rust
pub struct Bill {
    pub id: u32,              // Unique identifier
    pub name: String,         // Bill description
    pub amount: i128,         // Amount in stroops
    pub due_date: u64,        // Unix timestamp
    pub recurring: bool,      // Is recurring?
    pub frequency_days: u32,  // Repeat interval
    pub paid: bool,           // Payment status
}
```

## API Reference

### create_bill

Create a new bill.

```rust
pub fn create_bill(
    env: Env,
    name: String,
    amount: i128,
    due_date: u64,
    recurring: bool,
    frequency_days: u32,
) -> u32
```

**Parameters:**

- `name`: Bill description (e.g., "Electricity")
- `amount`: Payment amount in stroops
- `due_date`: Payment due date (Unix timestamp)
- `recurring`: Whether bill repeats
- `frequency_days`: Repeat interval in days

**Returns:** Bill ID (u32)

**Example:**

```rust
let bill_id = BillPayments::create_bill(
    env,
    String::from_small_str("Electricity"),
    100_000_000,  // 10 USDC
    1704067200,   // 2024-01-01
    true,         // recurring
    30,           // monthly
);
```

### pay_bill

Mark a bill as paid. For recurring bills, creates next bill.

```rust
pub fn pay_bill(env: Env, bill_id: u32) -> bool
```

**Parameters:**

- `bill_id`: ID of bill to pay

**Returns:** `true` if successful, `false` if not found or already paid

**Example:**

```rust
let success = BillPayments::pay_bill(env, bill_id);
if success {
    println!("Bill paid, next bill created");
}
```

### get_bill

Retrieve a specific bill.

```rust
pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill>
```

**Returns:** `Some(Bill)` if found, `None` otherwise

### get_unpaid_bills

Get all unpaid bills.

```rust
pub fn get_unpaid_bills(env: Env) -> Vec<Bill>
```

**Returns:** Vector of unpaid bills

**Example:**

```rust
let unpaid = BillPayments::get_unpaid_bills(env);
for bill in unpaid.iter() {
    println!("Pay {} by {}", bill.name, bill.due_date);
}
```

### get_total_unpaid

Calculate total amount of unpaid bills.

```rust
pub fn get_total_unpaid(env: Env) -> i128
```

**Returns:** Total amount in stroops

## Usage Examples

### Create Monthly Electricity Bill

```rust
let bill_id = BillPayments::create_bill(
    env,
    String::from_small_str("Electricity"),
    50_000_000,     // 5 USDC
    1704067200,     // 2024-01-01
    true,           // recurring monthly
    30,             // every 30 days
);
```

### Track Multiple Bills

```rust
// Create multiple bills
let electric = BillPayments::create_bill(
    env, String::from_small_str("Electric"), 50M, due_date, true, 30
);
let water = BillPayments::create_bill(
    env, String::from_small_str("Water"), 20M, due_date, true, 30
);
let school = BillPayments::create_bill(
    env, String::from_small_str("School"), 300M, due_date, false, 0
);

// Get total obligations
let total = BillPayments::get_total_unpaid(env);
// total = 370M stroops
```

### Payment Processing

```rust
// Get unpaid bills
let unpaid = BillPayments::get_unpaid_bills(env);

// Pay each bill
for bill in unpaid.iter() {
    if budget_available >= bill.amount {
        if BillPayments::pay_bill(env, bill.id) {
            budget_available -= bill.amount;
        }
    }
}
```

## Integration Points

### With Remittance Split

When processing a remittance:

1. RemittanceSplit calculates bills allocation
2. BillPayments receives payment amount
3. Automatically pays all overdue bills

### With Family Wallet

- Track bills for household
- Allocate bill payments to different family members
- Monitor compliance with bill payment schedule

### With Dashboard

- Display all unpaid bills
- Show payment due dates
- Calculate total obligations
- Track payment history

## Units and Conversions

All amounts in **stroops**:

- 1 XLM = 10,000,000 stroops
- 1 USDC (8 decimals) = 100,000,000 stroops

All dates as **Unix timestamps**:

- Convert using: `new Date(timestamp * 1000)` in JavaScript
- Or: `datetime.fromtimestamp(timestamp)` in Python

## Best Practices

### 1. Recurring Bill Setup

```rust
// Setup monthly bill
let monthly_bill = BillPayments::create_bill(
    env,
    name,
    amount,
    start_date,
    true,       // Enable recurring
    30,         // Monthly frequency
);
```

### 2. One-Time Bill

```rust
// Setup one-time bill
let one_time = BillPayments::create_bill(
    env,
    name,
    amount,
    due_date,
    false,      // No recurrence
    0,          // frequency_days ignored
);
```

### 3. Bill Payment Workflow

```rust
// 1. Query unpaid bills
let unpaid = BillPayments::get_unpaid_bills(env);

// 2. Calculate total due
let total_due = BillPayments::get_total_unpaid(env);

// 3. Process payments
for bill in unpaid.iter() {
    BillPayments::pay_bill(env, bill.id);
}

// 4. Verify completion
let remaining = BillPayments::get_total_unpaid(env);
assert_eq!(remaining, 0);
```

## Error Handling

### Pay Nonexistent Bill

```rust
let success = BillPayments::pay_bill(env, 999);
// Returns: false
// Reason: Bill ID 999 doesn't exist
```

### Pay Already Paid Bill

```rust
BillPayments::pay_bill(env, bill_id);  // First payment succeeds
let success = BillPayments::pay_bill(env, bill_id);  // Second call
// Returns: false
// Reason: Bill already marked as paid
```

### Query Nonexistent Bill

```rust
let result = BillPayments::get_bill(env, 999);
// Returns: None
// Reason: Bill ID 999 doesn't exist
```

## Testing

### Unit Tests

```rust
#[test]
fn test_create_bill() {
    let env = Env::default();
    let bill_id = BillPayments::create_bill(
        env,
        String::from_small_str("Test"),
        100_000_000,
        1704067200,
        true,
        30,
    );
    assert_eq!(bill_id, 1);
}

#[test]
fn test_pay_bill_recurring() {
    let env = Env::default();

    // Create recurring bill
    let id1 = BillPayments::create_bill(env, name, 100M, date, true, 30);

    // Pay bill
    assert!(BillPayments::pay_bill(env, id1));

    // Verify next bill created
    let unpaid = BillPayments::get_unpaid_bills(env);
    assert_eq!(unpaid.len(), 1);
}
```

## Deployment

### Compile

```bash
cd bill_payments
cargo build --release --target wasm32-unknown-unknown
```

### Deploy

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/bill_payments.wasm \
  --network testnet \
  --identity deployer
```

## Contract Size

- **Unoptimized**: ~80KB
- **Optimized**: ~40KB

## Gas Costs (Estimate)

- Create bill: ~5,000 stroops
- Pay bill: ~3,000 stroops
- Query unpaid: ~2,000 stroops
- Get total unpaid: ~2,000 stroops

(Actual costs vary with Stellar network conditions)

## Security Considerations

1. **No external calls**: All operations are deterministic
2. **Input validation**: Amounts checked for validity
3. **State isolation**: Each contract instance isolated
4. **Immutable bills**: Once created, bill properties don't change

## Future Enhancements

- [ ] Bill modification after creation
- [ ] Partial payments support
- [ ] Payment reminders via events
- [ ] Bill history/archive
- [ ] Bulk bill operations
- [ ] Late payment penalties

## References

- [Full API Reference](../docs/API_REFERENCE.md#bill-payments-contract)
- [Usage Examples](../docs/USAGE_EXAMPLES.md#bill-payments-examples)
- [Architecture Overview](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT_GUIDE.md)

## Support

For issues, questions, or contributions, visit:

- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
- [Stellar Docs](https://developers.stellar.org/)
