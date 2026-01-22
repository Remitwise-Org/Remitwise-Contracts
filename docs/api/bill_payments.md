# Bill Payments Contract API Reference

## Overview

The Bill Payments contract manages bill tracking, payment scheduling, and recurring bill automation for the RemitWise platform.

## Data Structures

### Bill

```rust
pub struct Bill {
    pub id: u32,
    pub name: String,
    pub amount: i128,
    pub due_date: u64, // Unix timestamp
    pub recurring: bool,
    pub frequency_days: u32, // For recurring bills (e.g., 30 for monthly)
    pub paid: bool,
}
```

## Functions

### create_bill

Creates a new bill.

**Signature:**
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
- `name`: Name of the bill (e.g., "Electricity", "School Fees")
- `amount`: Amount to pay (must be positive)
- `due_date`: Due date as Unix timestamp
- `recurring`: Whether this is a recurring bill
- `frequency_days`: Frequency in days for recurring bills (ignored if not recurring)

**Returns:** The ID of the created bill.

**Errors:** This function does not return errors; it always succeeds by creating a new bill.

### pay_bill

Marks a bill as paid.

**Signature:**
```rust
pub fn pay_bill(env: Env, bill_id: u32) -> bool
```

**Parameters:**
- `bill_id`: ID of the bill to mark as paid

**Returns:**
- `true`: Bill was successfully marked as paid. If recurring, a new bill is created for the next period.
- `false`: Bill not found or already paid.

**Errors:** No explicit errors; returns false for invalid operations.

### get_bill

Retrieves a bill by ID.

**Signature:**
```rust
pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill>
```

**Parameters:**
- `bill_id`: ID of the bill to retrieve

**Returns:**
- `Some(Bill)`: The bill struct if found
- `None`: If the bill does not exist

**Errors:** No errors; returns None for non-existent bills.

### get_unpaid_bills

Gets all unpaid bills.

**Signature:**
```rust
pub fn get_unpaid_bills(env: Env) -> Vec<Bill>
```

**Returns:** A vector of all unpaid `Bill` structs. Returns an empty vector if no unpaid bills exist.

**Errors:** No errors; always returns a vector.

### get_total_unpaid

Gets the total amount of unpaid bills.

**Signature:**
```rust
pub fn get_total_unpaid(env: Env) -> i128
```

**Returns:** The total amount (i128) of all unpaid bills. Returns 0 if no unpaid bills exist.

**Errors:** No errors; always returns a valid amount.