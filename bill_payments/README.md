# Bill Payments Contract

A Soroban smart contract for managing bill payments with support for recurring bills, payment tracking, access control, and currency management.

## Overview

The Bill Payments contract allows users to create, manage, and pay bills in various currencies. It supports both one-time and recurring bills, tracks payment history, provides comprehensive querying capabilities, and includes currency normalization and validation.

## Features

- Create one-time or recurring bills with currency specification
- Currency normalization (case-insensitive, whitespace trimming, defaults to "XLM")
- Currency validation (alphanumeric, 1-12 characters)
- Mark bills as paid with automatic recurring bill generation
- Query unpaid, overdue, and all bills by currency
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
    pub currency: String, // Currency code (e.g., "XLM", "USDC", "NGN")
}
```

#### ArchivedBill
```rust
pub struct ArchivedBill {
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
    pub archived_at: u64,
    pub currency: String, // Currency code carried over from original bill
}
```

#### Error Codes
- `BillNotFound = 1`: Bill with specified ID doesn't exist
- `BillAlreadyPaid = 2`: Attempting to pay an already paid bill
- `InvalidAmount = 3`: Amount is zero or negative
- `InvalidFrequency = 4`: Recurring bill has zero frequency
- `Unauthorized = 5`: Caller is not the bill owner
- `InvalidCurrency = 15`: Currency code is invalid (non-alphanumeric or wrong length)

### Functions

#### `create_bill(env, owner, name, amount, due_date, recurring, frequency_days, currency) -> Result<u32, Error>`
Creates a new bill with currency specification.

**Parameters:**
- `owner`: Address of the bill owner (must authorize)
- `name`: Bill name (e.g., "Electricity", "School Fees")
- `amount`: Payment amount (must be positive)
- `due_date`: Due date as Unix timestamp
- `recurring`: Whether this is a recurring bill
- `frequency_days`: Frequency in days for recurring bills (> 0 if recurring)
- `currency`: Currency code (e.g., "XLM", "USDC", "NGN"). Case-insensitive, whitespace trimmed, defaults to "XLM" if empty.

**Returns:** Bill ID on success

**Errors:** InvalidAmount, InvalidFrequency, InvalidCurrency

**Currency Normalization:**
- Converts to uppercase (e.g., "usdc" → "USDC")
- Trims whitespace (e.g., " XLM " → "XLM")
- Empty string defaults to "XLM"
- Validates: 1-12 alphanumeric characters only

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

#### `get_bills_by_currency(env, owner, currency, cursor, limit) -> Page<Bill>`
Gets a page of ALL bills (paid + unpaid) for an owner that match a specific currency.

**Parameters:**
- `owner`: Address of the bill owner
- `currency`: Currency code to filter by (case-insensitive)
- `cursor`: Start after this bill ID (0 for first page)
- `limit`: Maximum number of bills to return (1-100, defaults to 10)

**Returns:** Page struct with bills and next cursor

**Currency Comparison:** Case-insensitive (e.g., "usdc", "USDC", "UsDc" all match)

#### `get_unpaid_bills_by_currency(env, owner, currency, cursor, limit) -> Page<Bill>`
Gets a page of unpaid bills for an owner that match a specific currency.

**Parameters:**
- `owner`: Address of the bill owner
- `currency`: Currency code to filter by (case-insensitive)
- `cursor`: Start after this bill ID (0 for first page)
- `limit`: Maximum number of bills to return (1-100, defaults to 10)

**Returns:** Page struct with bills and next cursor

**Currency Comparison:** Case-insensitive (e.g., "usdc", "USDC", "UsDc" all match)

#### `get_total_unpaid_by_currency(env, owner, currency) -> i128`
Calculates total amount of unpaid bills for an owner in a specific currency.

**Parameters:**
- `owner`: Address of the bill owner
- `currency`: Currency code to filter by (case-insensitive)

**Returns:** Total unpaid amount in the specified currency

**Currency Comparison:** Case-insensitive (e.g., "usdc", "USDC", "UsDc" all match)

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

### Creating a One-Time Bill with Currency
```rust
// Create a one-time electricity bill due in 30 days in USDC
let bill_id = bill_payments::create_bill(
    env,
    user_address,
    "Electricity Bill".into(),
    150_0000000, // 150 USDC in smallest units
    env.ledger().timestamp() + (30 * 86400), // 30 days from now
    false, // not recurring
    0, // frequency not needed
    "USDC".into(), // currency code (case-insensitive)
)?;
```

### Creating a Recurring Bill with Currency
```rust
// Create a monthly insurance bill in XLM
let bill_id = bill_payments::create_bill(
    env,
    user_address,
    "Insurance Premium".into(),
    50_0000000, // 50 XLM
    env.ledger().timestamp() + (30 * 86400), // due in 30 days
    true, // recurring
    30, // every 30 days
    "xlm".into(), // lowercase works, will be normalized to "XLM"
)?;
```

### Querying Bills by Currency
```rust
// Get all unpaid USDC bills for a user (case-insensitive)
let unpaid_usdc = bill_payments::get_unpaid_bills_by_currency(
    env, 
    user_address, 
    "usdc".into(), // lowercase query
    0, // start from beginning
    10 // limit to 10 results
);

// Get total unpaid amount in XLM
let total_xlm = bill_payments::get_total_unpaid_by_currency(
    env,
    user_address,
    "XLM".into()
);

// Get all bills (paid + unpaid) in a specific currency
let all_usdc_bills = bill_payments::get_bills_by_currency(
    env,
    user_address,
    "USDC".into(),
    0,
    100
);
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

The contract emits **typed, versioned events** using the `RemitwiseEvents` helper from `remitwise-common`. Every event follows a standardized schema to ensure downstream indexers and consumers can reliably decode event data across contract upgrades.

### Topic Convention

All events use a 4-topic tuple:

```text
("Remitwise", category: u32, priority: u32, action: Symbol)
```

| Position | Field      | Description                                        |
|----------|------------|----------------------------------------------------|
| 0        | Namespace  | Always `"Remitwise"` — immutable across versions  |
| 1        | Category   | `0`=Transaction, `1`=State, `3`=System             |
| 2        | Priority   | `0`=Low, `1`=Medium, `2`=High                     |
| 3        | Action     | Short symbol: `"created"`, `"paid"`, `"canceled"`, etc |

### Event Types

| Operation              | Event Struct          | Action Symbol | Category    | Priority |
|------------------------|-----------------------|---------------|-------------|----------|
| `create_bill`          | `BillCreatedEvent`    | `"created"`   | State       | Medium   |
| `pay_bill`             | `BillPaidEvent`       | `"paid"`      | Transaction | High     |
| `cancel_bill`          | `BillCancelledEvent`  | `"canceled"`  | State       | Medium   |
| `archive_paid_bills`   | `BillsArchivedEvent`  | `"archived"`  | System      | Low      |
| `restore_bill`         | `BillRestoredEvent`   | `"restored"`  | State       | Medium   |
| `set_version`          | `VersionUpgradeEvent` | `"upgraded"`  | System      | High     |
| `batch_pay_bills`      | `BillPaidEvent` × N   | `"paid"`      | Transaction | High     |
| `pause`                | `()`                  | `"paused"`    | System      | High     |
| `unpause`              | `()`                  | `"unpaused"`  | System      | High     |

### Schema Versioning & Backward Compatibility

Every event struct includes a `schema_version` field (currently `1`) that:

1. Allows downstream consumers to branch decoding logic per version.
2. Guarantees that **field ordering is append-only** — new fields are always added at the end.
3. Is enforced at **compile time** via `assert_min_fields!` macros in `events.rs`.

**Guarantees:**
- Topic symbols (e.g., `"created"`, `"paid"`) are **never renamed** across versions.
- The 4-topic structure `(Namespace, Category, Priority, Action)` is **immutable**.
- Existing fields are **never removed or reordered** — only new optional fields may be appended.
- All events are **deterministically reproducible** from the same contract state.


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

- All functions require proper authorization (`require_auth()`)
- Owners can only manage their own bills (enforced by explicit owner check)
- Input validation prevents invalid states (amount, frequency, due_date, currency)
- Currency codes are validated (1-12 alphanumeric chars) and normalized
- Event payloads contain only bill metadata — no sensitive data leakage
- Storage TTL is managed to prevent bloat
- Schema version in events prevents silent breaking changes to consumers
- Compile-time `assert_min_fields!` macros catch accidental field-count regressions