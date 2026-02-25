# Bill Payments Contract

A Soroban smart contract for managing bill payments with support for recurring bills, payment tracking, and access control.

## Overview

The Bill Payments contract allows users to create, manage, and pay bills. It supports both one-time and recurring bills, tracks payment history, and provides comprehensive querying capabilities.

## Features

- Create one-time or recurring bills
- Mark bills as paid with automatic recurring bill generation
- Query unpaid, overdue, and all bills
- Access control ensuring only owners can manage their bills
- Event emission for audit trails
- Storage TTL management for efficiency

## Quickstart

This section provides a minimal example of how to interact with the Bill Payments contract. 

**Gotchas:** 
- The contract uses a paginated API for most list queries natively.
- Bill amounts are specified in the lowest denomination (e.g., stroops for XLM).
- If a bill is marked as `recurring`, paying it automatically generates the next bill.

### Write Example: Creating a Bill
*Note: This is pseudo-code demonstrating the Soroban Rust SDK CLI or client approach.*
```rust

let bill_id = client.create_bill(
    &owner_address,
    &String::from_str(&env, "Internet Bill"),
    &500_0000000,                           
    &(env.ledger().timestamp() + 2592000), 
    &false,                                
    &0,                                     
    &String::from_str(&env, "XLM")          
);

```

### Read Example: Fetching Unpaid Bills
```rust

let limit = 10;
let cursor = 0; 
let page = client.get_unpaid_bills(&owner_address, &cursor, &limit);

```

## API Reference

### Data Structures

#### Bill
```rust
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub created_at: u64,
    pub paid_at: Option<u64>,
}
```

#### Error Codes
- `BillNotFound = 1`: Bill with specified ID doesn't exist
- `BillAlreadyPaid = 2`: Attempting to pay an already paid bill
- `InvalidAmount = 3`: Amount is zero or negative
- `InvalidFrequency = 4`: Recurring bill has zero frequency
- `Unauthorized = 5`: Caller is not the bill owner

### Functions

#### `create_bill(env, owner, name, amount, due_date, recurring, frequency_days) -> Result<u32, Error>`
Creates a new bill.

**Parameters:**
- `owner`: Address of the bill owner (must authorize)
- `name`: Bill name (e.g., "Electricity", "School Fees")
- `amount`: Payment amount (must be positive)
- `due_date`: Due date as Unix timestamp
- `recurring`: Whether this is a recurring bill
- `frequency_days`: Frequency in days for recurring bills (> 0 if recurring)

**Returns:** Bill ID on success

**Errors:** InvalidAmount, InvalidFrequency

#### `pay_bill(env, caller, bill_id) -> Result<(), Error>`
Marks a bill as paid.

**Parameters:**
- `caller`: Address of the caller (must be bill owner)
- `bill_id`: ID of the bill to pay

**Returns:** Ok(()) on success

**Errors:** BillNotFound, BillAlreadyPaid, Unauthorized

#### `get_bill(env, bill_id) -> Option<Bill>`
Retrieves a bill by ID.

**Parameters:**
- `bill_id`: ID of the bill

**Returns:** Bill struct or None if not found

#### `get_unpaid_bills(env, owner) -> Vec<Bill>`
Gets all unpaid bills for an owner.

**Parameters:**
- `owner`: Address of the bill owner

**Returns:** Vector of unpaid Bill structs

#### `get_overdue_bills(env, owner) -> Vec<Bill>`
Gets all overdue unpaid bills for a specific owner.

**Parameters:**
- `owner`: Address of the bill owner

**Returns:** Vector of overdue Bill structs belonging to the owner

#### `get_total_unpaid(env, owner) -> i128`
Calculates total amount of unpaid bills for an owner.

**Parameters:**
- `owner`: Address of the bill owner

**Returns:** Total unpaid amount

#### `cancel_bill(env, bill_id) -> Result<(), Error>`
Cancels/deletes a bill.

**Parameters:**
- `bill_id`: ID of the bill to cancel

**Returns:** Ok(()) on success

**Errors:** BillNotFound

#### `get_all_bills(env) -> Vec<Bill>`
Gets all bills (paid and unpaid).

**Returns:** Vector of all Bill structs

## Usage Examples

### Creating a One-Time Bill
```rust
// Create a one-time electricity bill due in 30 days
let bill_id = bill_payments::create_bill(
    env,
    user_address,
    "Electricity Bill".into(),
    150_0000000, // 150 XLM in stroops
    env.ledger().timestamp() + (30 * 86400), // 30 days from now
    false, // not recurring
    0, // frequency not needed
)?;
```

### Creating a Recurring Bill
```rust
// Create a monthly insurance bill
let bill_id = bill_payments::create_bill(
    env,
    user_address,
    "Insurance Premium".into(),
    50_0000000, // 50 XLM
    env.ledger().timestamp() + (30 * 86400), // due in 30 days
    true, // recurring
    30, // every 30 days
)?;
```

### Paying a Bill
```rust
// Pay the bill (caller must be the owner)
bill_payments::pay_bill(env, user_address, bill_id)?;
```

### Querying Bills
```rust
// Get all unpaid bills for a user
let unpaid = bill_payments::get_unpaid_bills(env, user_address);

// Get total unpaid amount
let total = bill_payments::get_total_unpaid(env, user_address);

// Check for overdue bills
let overdue = bill_payments::get_overdue_bills(env, user_address);
```

## Events

The contract emits events for audit trails:
- `BillEvent::Created`: When a bill is created
- `BillEvent::Paid`: When a bill is paid

## Integration Patterns

### With Remittance Split
The bill payments contract integrates with the remittance split contract to automatically allocate funds to bill payments:

```rust
// Calculate split amounts
let split_amounts = remittance_split::calculate_split(env, total_remittance);

// Allocate to bills
let bills_allocation = split_amounts.get(2).unwrap(); // bills percentage

// Create bill payment entries based on allocation
```

### With Insurance Contract
Bills can represent insurance premiums, working alongside the insurance contract for comprehensive financial management.

## Security Considerations

- All functions require proper authorization
- Owners can only manage their own bills
- Input validation prevents invalid states
- Storage TTL is managed to prevent bloat