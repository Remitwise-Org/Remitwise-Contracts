# Remitwise Contracts - API Reference

Complete API documentation for all Remitwise smart contracts.

## Table of Contents

- [Bill Payments Contract](#bill-payments-contract)
- [Family Wallet Contract](#family-wallet-contract)
- [Insurance Contract](#insurance-contract)
- [Remittance Split Contract](#remittance-split-contract)
- [Savings Goals Contract](#savings-goals-contract)

---

## Bill Payments Contract

**Module**: `bill_payments`  
**Purpose**: Manage bill payments including creation, tracking, and payment of bills. Supports both one-time and recurring bills with automatic renewal.

### Data Types

#### Bill

```rust
pub struct Bill {
    pub id: u32,              // Unique identifier
    pub name: String,         // Bill description
    pub amount: i128,         // Payment amount in stroops
    pub due_date: u64,        // Unix timestamp
    pub recurring: bool,      // Is recurring bill?
    pub frequency_days: u32,  // Interval for recurring (e.g., 30 for monthly)
    pub paid: bool,           // Payment status
}
```

### Functions

#### create_bill

Creates a new bill entry.

**Signature**:

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

**Parameters**:

- `env`: Soroban environment context
- `name`: Bill description (e.g., "Electricity")
- `amount`: Payment amount in stroops
- `due_date`: Payment due date as Unix timestamp
- `recurring`: Whether bill repeats
- `frequency_days`: Repeat interval in days (30 for monthly)

**Returns**: `u32` - Created bill ID

**Error Codes**: None (always succeeds)

---

#### pay_bill

Mark a bill as paid. For recurring bills, automatically creates next bill.

**Signature**:

```rust
pub fn pay_bill(env: Env, bill_id: u32) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `bill_id`: ID of bill to mark as paid

**Returns**: `bool`

- `true`: Payment successful, next bill created if recurring
- `false`: Bill not found or already paid

**Error Codes**:

- Bill not found → returns false
- Bill already paid → returns false

---

#### get_bill

Retrieve a specific bill.

**Signature**:

```rust
pub fn get_bill(env: Env, bill_id: u32) -> Option<Bill>
```

**Parameters**:

- `env`: Soroban environment context
- `bill_id`: ID of bill to retrieve

**Returns**: `Option<Bill>`

- `Some(Bill)`: Bill found
- `None`: Bill not found

---

#### get_unpaid_bills

Get all unpaid bills.

**Signature**:

```rust
pub fn get_unpaid_bills(env: Env) -> Vec<Bill>
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `Vec<Bill>` - Vector of all unpaid bills

---

#### get_total_unpaid

Get total amount of all unpaid bills.

**Signature**:

```rust
pub fn get_total_unpaid(env: Env) -> i128
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `i128` - Total unpaid amount in stroops

---

## Family Wallet Contract

**Module**: `family_wallet`  
**Purpose**: Manage family member accounts with individual spending limits and role-based access control.

### Data Types

#### FamilyMember

```rust
pub struct FamilyMember {
    pub address: Address,       // Stellar wallet address
    pub name: String,           // Member name
    pub spending_limit: i128,   // Spending limit in stroops
    pub role: String,           // "sender", "recipient", or "admin"
}
```

**Roles**:

- `"sender"`: Can initiate transfers up to spending limit
- `"recipient"`: Can receive transfers
- `"admin"`: Full access, manages other members

### Functions

#### add_member

Add a new family member.

**Signature**:

```rust
pub fn add_member(
    env: Env,
    address: Address,
    name: String,
    spending_limit: i128,
    role: String,
) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `address`: Stellar address of family member
- `name`: Member's name
- `spending_limit`: Daily/monthly limit in stroops
- `role`: Role assignment

**Returns**: `bool` - Always true on success

---

#### get_member

Retrieve a specific family member.

**Signature**:

```rust
pub fn get_member(env: Env, address: Address) -> Option<FamilyMember>
```

**Parameters**:

- `env`: Soroban environment context
- `address`: Stellar address of member

**Returns**: `Option<FamilyMember>`

- `Some(FamilyMember)`: Member found
- `None`: Member not found

---

#### get_all_members

Get all family members.

**Signature**:

```rust
pub fn get_all_members(env: Env) -> Vec<FamilyMember>
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `Vec<FamilyMember>` - All registered members

---

#### update_spending_limit

Update a member's spending limit.

**Signature**:

```rust
pub fn update_spending_limit(
    env: Env,
    address: Address,
    new_limit: i128,
) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `address`: Member's Stellar address
- `new_limit`: New spending limit in stroops

**Returns**: `bool`

- `true`: Update successful
- `false`: Member not found

---

#### check_spending_limit

Validate spending against member's limit.

**Signature**:

```rust
pub fn check_spending_limit(
    env: Env,
    address: Address,
    amount: i128,
) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `address`: Member's Stellar address
- `amount`: Amount to validate in stroops

**Returns**: `bool`

- `true`: Amount within limit
- `false`: Over limit or member not found

---

## Insurance Contract

**Module**: `insurance`  
**Purpose**: Manage insurance policies with monthly premium payments and coverage tracking.

### Data Types

#### InsurancePolicy

```rust
pub struct InsurancePolicy {
    pub id: u32,                // Unique identifier
    pub name: String,           // Policy name
    pub coverage_type: String,  // Type: "health", "emergency", etc.
    pub monthly_premium: i128,  // Monthly premium in stroops
    pub coverage_amount: i128,  // Total coverage in stroops
    pub active: bool,           // Policy status
    pub next_payment_date: u64, // Next payment due (Unix timestamp)
}
```

### Functions

#### create_policy

Create a new insurance policy.

**Signature**:

```rust
pub fn create_policy(
    env: Env,
    name: String,
    coverage_type: String,
    monthly_premium: i128,
    coverage_amount: i128,
) -> u32
```

**Parameters**:

- `env`: Soroban environment context
- `name`: Policy name (e.g., "Family Health Insurance")
- `coverage_type`: Type of coverage (e.g., "health", "emergency")
- `monthly_premium`: Monthly premium in stroops
- `coverage_amount`: Total coverage amount in stroops

**Returns**: `u32` - Created policy ID

**Notes**: First payment due date set to 30 days from now

---

#### pay_premium

Pay monthly premium for a policy.

**Signature**:

```rust
pub fn pay_premium(env: Env, policy_id: u32) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `policy_id`: ID of policy to pay

**Returns**: `bool`

- `true`: Payment successful, next date advanced 30 days
- `false`: Policy not found or inactive

**Error Codes**:

- Policy not found → returns false
- Policy inactive → returns false

---

#### get_policy

Retrieve a specific policy.

**Signature**:

```rust
pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy>
```

**Parameters**:

- `env`: Soroban environment context
- `policy_id`: ID of policy to retrieve

**Returns**: `Option<InsurancePolicy>`

- `Some(InsurancePolicy)`: Policy found
- `None`: Policy not found

---

#### get_active_policies

Get all active policies.

**Signature**:

```rust
pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy>
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `Vec<InsurancePolicy>` - All active policies

---

#### get_total_monthly_premium

Get combined monthly premium for all active policies.

**Signature**:

```rust
pub fn get_total_monthly_premium(env: Env) -> i128
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `i128` - Total premium in stroops

---

#### deactivate_policy

Deactivate an insurance policy.

**Signature**:

```rust
pub fn deactivate_policy(env: Env, policy_id: u32) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `policy_id`: ID of policy to deactivate

**Returns**: `bool`

- `true`: Deactivation successful
- `false`: Policy not found

---

## Remittance Split Contract

**Module**: `remittance_split`  
**Purpose**: Automatically distribute incoming remittances based on configurable percentage allocations.

### Functions

#### initialize_split

Configure remittance distribution percentages.

**Signature**:

```rust
pub fn initialize_split(
    env: Env,
    spending_percent: u32,
    savings_percent: u32,
    bills_percent: u32,
    insurance_percent: u32,
) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `spending_percent`: Percentage for daily spending (0-100)
- `savings_percent`: Percentage for savings (0-100)
- `bills_percent`: Percentage for bills (0-100)
- `insurance_percent`: Percentage for insurance (0-100)

**Returns**: `bool`

- `true`: Configuration successful
- `false`: Percentages don't sum to 100

**Error Codes**:

- Invalid total (not 100) → returns false

**Example**: `initialize_split(env, 50, 30, 15, 5)` = 50% spending, 30% savings, 15% bills, 5% insurance

---

#### get_split

Retrieve current split configuration.

**Signature**:

```rust
pub fn get_split(env: Env) -> Vec<u32>
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `Vec<u32>` - Percentages [spending, savings, bills, insurance]

**Default**: `[50, 30, 15, 5]` if not configured

---

#### calculate_split

Calculate split amounts from total remittance.

**Signature**:

```rust
pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>
```

**Parameters**:

- `env`: Soroban environment context
- `total_amount`: Total remittance amount in stroops

**Returns**: `Vec<i128>` - Amounts [spending, savings, bills, insurance]

**Notes**: Remainder from rounding allocated to insurance

**Example**: With default split and 100,000,000 stroops:

- Spending: 50,000,000
- Savings: 30,000,000
- Bills: 15,000,000
- Insurance: 5,000,000

---

## Savings Goals Contract

**Module**: `savings_goals`  
**Purpose**: Create and track personal savings goals with target amounts and completion dates.

### Data Types

#### SavingsGoal

```rust
pub struct SavingsGoal {
    pub id: u32,              // Unique identifier
    pub name: String,         // Goal name
    pub target_amount: i128,  // Target amount in stroops
    pub current_amount: i128, // Currently saved in stroops
    pub target_date: u64,     // Target completion date (Unix timestamp)
    pub locked: bool,         // Funds locked until target date?
}
```

### Functions

#### create_goal

Create a new savings goal.

**Signature**:

```rust
pub fn create_goal(
    env: Env,
    name: String,
    target_amount: i128,
    target_date: u64,
) -> u32
```

**Parameters**:

- `env`: Soroban environment context
- `name`: Goal name (e.g., "Education", "Medical Emergency")
- `target_amount`: Target amount in stroops
- `target_date`: Target date as Unix timestamp

**Returns**: `u32` - Created goal ID

**Notes**: Starts with current_amount = 0, locked = true

---

#### add_to_goal

Deposit funds into a savings goal.

**Signature**:

```rust
pub fn add_to_goal(env: Env, goal_id: u32, amount: i128) -> i128
```

**Parameters**:

- `env`: Soroban environment context
- `goal_id`: ID of goal
- `amount`: Amount to add in stroops

**Returns**: `i128`

- Updated current_amount if successful
- `-1` if goal not found

**Error Codes**:

- Goal not found → returns -1

---

#### get_goal

Retrieve a specific savings goal.

**Signature**:

```rust
pub fn get_goal(env: Env, goal_id: u32) -> Option<SavingsGoal>
```

**Parameters**:

- `env`: Soroban environment context
- `goal_id`: ID of goal to retrieve

**Returns**: `Option<SavingsGoal>`

- `Some(SavingsGoal)`: Goal found
- `None`: Goal not found

---

#### get_all_goals

Get all savings goals.

**Signature**:

```rust
pub fn get_all_goals(env: Env) -> Vec<SavingsGoal>
```

**Parameters**:

- `env`: Soroban environment context

**Returns**: `Vec<SavingsGoal>` - All goals

---

#### is_goal_completed

Check if a goal has reached its target.

**Signature**:

```rust
pub fn is_goal_completed(env: Env, goal_id: u32) -> bool
```

**Parameters**:

- `env`: Soroban environment context
- `goal_id`: ID of goal to check

**Returns**: `bool`

- `true`: current_amount >= target_amount
- `false`: Not completed or goal not found

---

## Common Units and Conventions

### Amount Units

All amounts are in **stroops** (smallest unit of Stellar assets):

- 1 XLM = 10,000,000 stroops
- 1 USDC (8 decimals) = 100,000,000 stroops

### Time Units

All timestamps are **Unix timestamps** (seconds since January 1, 1970 UTC)

### Return Values

- Functions returning `bool` indicate success/failure
- Functions returning `Option<T>` may return `None` if not found
- Functions returning `Vec<T>` return empty vectors if no results

---

## Error Handling

All contracts use implicit error handling:

- Operations returning `bool` return `false` on error
- Operations returning `Option<T>` return `None` if not found
- No exceptions thrown; all errors are return values
