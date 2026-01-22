# Bill Payments Contract

[![Contract](https://img.shields.io/badge/Contract-Bill_Payments-blue)](https://github.com/your-org/remitwise-contracts/tree/main/bill_payments)

A Soroban smart contract for managing bill payments with support for recurring bills and automated scheduling.

## Overview

The Bill Payments contract enables users to track and manage their financial obligations, including utility bills, school fees, and other recurring expenses. It supports both one-time and recurring payments, automatically creating future bill instances for recurring obligations.

## Features

- ✅ **Bill Creation**: Create bills with custom names, amounts, and due dates
- ✅ **Recurring Bills**: Set up bills that repeat at specified intervals
- ✅ **Payment Tracking**: Mark bills as paid and track payment history
- ✅ **Automated Scheduling**: Auto-create next recurring bill after payment
- ✅ **Bulk Operations**: Retrieve all unpaid bills and calculate totals

## Data Structures

### Bill

```rust
pub struct Bill {
    pub id: u32,              // Unique bill identifier
    pub name: String,         // Bill description (e.g., "Electricity", "School Fees")
    pub amount: i128,         // Payment amount
    pub due_date: u64,        // Due date as Unix timestamp
    pub recurring: bool,      // Whether this is a recurring bill
    pub frequency_days: u32,  // Recurrence interval in days
    pub paid: bool,           // Payment status
}
```

## Functions

### Core Functions

| Function | Description |
|----------|-------------|
| `create_bill` | Create a new bill with payment details |
| `pay_bill` | Mark a bill as paid and handle recurring logic |
| `get_bill` | Retrieve bill details by ID |
| `get_unpaid_bills` | Get all unpaid bills |
| `get_total_unpaid` | Calculate total amount of unpaid bills |

### Usage Examples

#### Creating a Monthly Electricity Bill

```rust
let bill_id = contract.create_bill(
    "Electricity Bill".to_string(),
    150,                    // amount
    1672531200,            // due date (Jan 1, 2023)
    true,                   // recurring
    30                      // monthly
);
```

#### Processing a Bill Payment

```rust
let success = contract.pay_bill(bill_id);
if success {
    // Bill marked as paid, next recurring bill created if applicable
}
```

#### Checking Outstanding Bills

```rust
let unpaid_bills = contract.get_unpaid_bills();
let total_amount = contract.get_total_unpaid();
```

## Integration Patterns

### With Remittance Split

The Bill Payments contract integrates seamlessly with the Remittance Split contract:

```rust
// 1. Calculate split from remittance
let split_amounts = remittance_split.calculate_split(total_remittance);

// 2. Allocate to bills (split_amounts[2] is bills portion)
let unpaid_bills = bill_payments.get_unpaid_bills();

// 3. Pay bills from allocated amount
for bill in unpaid_bills {
    if split_amounts[2] >= bill.amount {
        bill_payments.pay_bill(bill.id);
        split_amounts[2] -= bill.amount;
    }
}
```

### Recurring Bill Management

```rust
// Set up multiple recurring bills
let bills = vec![
    ("Electricity", 150, 30),
    ("Water", 50, 30),
    ("Internet", 80, 30),
    ("School Fees", 500, 365),  // Annual
];

for (name, amount, freq) in bills {
    contract.create_bill(
        name.to_string(),
        amount,
        current_timestamp + (freq as u64 * 86400),
        true,
        freq
    );
}
```

## Testing

Run the contract tests:

```bash
cd bill_payments
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
`target/wasm32-unknown-unknown/release/bill_payments.wasm`

## API Reference

For complete API documentation, see [Bill Payments API](../../docs/api/bill_payments.md).

## Error Handling

The contract uses boolean returns to indicate success/failure:

- `create_bill`: Always succeeds (returns new bill ID)
- `pay_bill`: Returns `false` if bill not found or already paid
- `get_bill`: Returns `None` if bill doesn't exist
- `get_unpaid_bills`: Always succeeds (may return empty vector)
- `get_total_unpaid`: Always succeeds (may return 0)

## Security Considerations

- All financial amounts are validated as positive values
- Bill IDs are auto-incremented to prevent conflicts
- Recurring bills are created with future dates only
- No direct token transfers (handled by frontend integration)

## Future Enhancements

- [ ] Bill payment scheduling with notifications
- [ ] Integration with external payment processors
- [ ] Bill categories and prioritization
- [ ] Payment history and analytics
- [ ] Multi-currency support