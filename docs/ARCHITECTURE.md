# Remitwise Contracts - Architecture Documentation

Comprehensive overview of the Remitwise smart contract system architecture, design patterns, and component interactions.

## Table of Contents

- [System Overview](#system-overview)
- [Architecture Diagram](#architecture-diagram)
- [Contract Relationships](#contract-relationships)
- [Data Flow](#data-flow)
- [Storage Model](#storage-model)
- [Design Patterns](#design-patterns)
- [Integration Patterns](#integration-patterns)
- [Security Architecture](#security-architecture)

---

## System Overview

### Purpose

The Remitwise smart contract platform provides a comprehensive financial management system for remittance recipients, enabling:

1. **Automatic Splitting**: Distribute remittances across multiple financial goals
2. **Bill Management**: Track and manage recurring and one-time bills
3. **Insurance Tracking**: Manage insurance policies and premium payments
4. **Family Control**: Implement spending controls for family members
5. **Savings Goals**: Create and track personal savings objectives

### Core Principles

- **Modular Design**: Each contract is independent yet interconnected
- **Atomic Operations**: Transactions are all-or-nothing
- **State Isolation**: Each contract maintains separate state
- **No External Dependencies**: All contracts are self-contained
- **Deterministic**: Same input always produces same output

---

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                     User Applications                            │
│              (Web, Mobile, Backend Services)                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ RPC Calls
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Remitwise Smart Contracts                     │
│                                                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Remittance Entry Point                       │  │
│  │         (Coordinates cross-contract flow)                │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                    │
│        ┌─────────────────────┼─────────────────────┐            │
│        │                     │                     │            │
│        ▼                     ▼                     ▼            │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐        │
│  │  Remittance  │   │  Bill        │   │  Insurance   │        │
│  │  Split       │─┬─│  Payments    │─┬─│  Policies    │        │
│  └──────────────┘ │ └──────────────┘ │ └──────────────┘        │
│                   │                   │                         │
│        ┌──────────┴─────────┬────────┘                          │
│        │                    │                                   │
│        ▼                    ▼                                   │
│  ┌──────────────┐   ┌──────────────┐                           │
│  │  Savings     │   │  Family      │                           │
│  │  Goals       │   │  Wallet      │                           │
│  └──────────────┘   └──────────────┘                           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ Soroban Host
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              Stellar Blockchain Network                          │
│         (Testnet / Mainnet - Soroban VM)                        │
└─────────────────────────────────────────────────────────────────┘
```

---

## Contract Relationships

### 1. Remittance Split Contract (Orchestrator)

**Role**: Entry point and coordinator for incoming remittances

**Relationships**:

- Routes calculated percentages to other contracts
- Reads configuration but maintains independent state
- Called first in remittance processing flow

```
RemittanceSplit
  ├─ Calculates distribution percentages
  ├─ Returns split amounts
  └─ Coordinates with:
     ├─ BillPayments (bills amount)
     ├─ SavingsGoals (savings amount)
     ├─ Insurance (insurance amount)
     └─ FamilyWallet (spending amount)
```

### 2. Bill Payments Contract

**Role**: Manages bill lifecycle and payment tracking

**Relationships**:

- Receives allocation from RemittanceSplit
- Independent state management for bills
- Automatically creates recurring bills
- Used by: RemittanceSplit, User Application

```
BillPayments
  ├─ Creates bills
  ├─ Tracks payment status
  ├─ Auto-creates recurring bills
  └─ Queries:
     ├─ get_bill (by ID)
     ├─ get_unpaid_bills (all unpaid)
     └─ get_total_unpaid (for budgeting)
```

### 3. Insurance Contract

**Role**: Manages insurance policies and premium payments

**Relationships**:

- Receives allocation from RemittanceSplit
- Tracks policy status and payment schedules
- Independent of other contracts
- Used by: RemittanceSplit, User Application

```
Insurance
  ├─ Creates policies
  ├─ Tracks premiums
  ├─ Manages payment schedules
  ├─ Calculates total obligations
  └─ Queries:
     ├─ get_policy (by ID)
     ├─ get_active_policies (all active)
     ├─ get_total_monthly_premium
     └─ deactivate_policy
```

### 4. Savings Goals Contract

**Role**: Tracks personal savings objectives

**Relationships**:

- Receives allocation from RemittanceSplit
- Maintains goal progress independently
- Calculates completion status
- Used by: RemittanceSplit, User Application

```
SavingsGoals
  ├─ Creates goals
  ├─ Tracks contributions
  ├─ Monitors progress
  └─ Queries:
     ├─ get_goal (by ID)
     ├─ get_all_goals
     └─ is_goal_completed
```

### 5. Family Wallet Contract

**Role**: Implements family spending controls

**Relationships**:

- Receives spending allocation from RemittanceSplit
- Enforces spending limits per member
- Role-based access control
- Used by: RemittanceSplit, User Application

```
FamilyWallet
  ├─ Manages family members
  ├─ Enforces spending limits
  ├─ Implements role-based access
  └─ Queries:
     ├─ get_member (by address)
     ├─ get_all_members
     ├─ check_spending_limit
     └─ update_spending_limit
```

---

## Data Flow

### Remittance Processing Flow

```
1. User initiates remittance transfer
                    │
                    ▼
2. Client sends 1000 USDC to platform
                    │
                    ▼
3. RemittanceSplit.calculate_split(1000_000_000) called
   ├─ Retrieves split configuration [50, 30, 15, 5]
   ├─ Calculates amounts:
   │  ├─ Spending: 500_000_000
   │  ├─ Savings: 300_000_000
   │  ├─ Bills: 150_000_000
   │  └─ Insurance: 50_000_000
   └─ Returns distribution vector
                    │
         ┌──────────┼──────────┬────────────┐
         │          │          │            │
         ▼          ▼          ▼            ▼
    4a. Bill     4b. Sav     4c. Insur   4d. Fam
       Pmts       Goals       ance        Wlt
         │          │          │            │
         ▼          ▼          ▼            ▼
    Pay unpaid   Add to    Pay due    Update
    bills       goals      premiums   spending
                                      limits
         │          │          │            │
         └──────────┴──────────┴────────────┘
                    │
                    ▼
5. All allocations complete
                    │
                    ▼
6. User receives confirmation
   ├─ Bills paid
   ├─ Savings allocated
   ├─ Premiums current
   └─ Spending available
```

### Query Flow Example: Weekly Financial Review

```
User requests financial summary
            │
            ▼
1. BillPayments.get_total_unpaid()
   → Returns total unpaid bills
            │
            ▼
2. Insurance.get_total_monthly_premium()
   → Returns total insurance cost
            │
            ▼
3. SavingsGoals.get_all_goals()
   → Returns progress on each goal
            │
            ▼
4. FamilyWallet.get_all_members()
   → Returns family spending status
            │
            ▼
Application aggregates results
            │
            ▼
Display dashboard:
├─ Obligations: $500
├─ Insurance: $70/month
├─ Savings progress: 65%
└─ Family spending: $250/$500 limit
```

---

## Storage Model

### Contract Storage Layout

Each contract maintains independent storage:

```
Contract: BillPayments
├─ BILLS (Map<u32, Bill>)
│  └─ 1: {id: 1, name: "Electric", amount: 50M, ...}
│  └─ 2: {id: 2, name: "Water", amount: 20M, ...}
├─ NEXT_ID (u32)
│  └─ 3

Contract: Insurance
├─ POLICIES (Map<u32, InsurancePolicy>)
│  └─ 1: {id: 1, name: "Health", premium: 50M, ...}
│  └─ 2: {id: 2, name: "Emergency", premium: 20M, ...}
├─ NEXT_ID (u32)
│  └─ 3

Contract: SavingsGoals
├─ GOALS (Map<u32, SavingsGoal>)
│  └─ 1: {id: 1, name: "University", target: 500M, current: 200M, ...}
│  └─ 2: {id: 2, name: "Emergency", target: 100M, current: 75M, ...}
├─ NEXT_ID (u32)
│  └─ 3

Contract: FamilyWallet
├─ MEMBERS (Map<Address, FamilyMember>)
│  └─ GXXXXXX: {address: GXXXXXX, name: "Alice", limit: 10M, role: "sender"}
│  └─ GYYYYYY: {address: GYYYYYY, name: "Parent", limit: 100M, role: "admin"}

Contract: RemittanceSplit
├─ SPLIT (Vec<u32>)
│  └─ [50, 30, 15, 5]  // percentages
```

### State Isolation

- **No cross-contract state sharing**: Each contract has independent storage
- **No global state**: No shared constants or configuration
- **Address-based identity**: FamilyWallet uses Stellar addresses
- **ID-based identity**: Other contracts use numeric IDs

---

## Design Patterns

### 1. Map-Based Registry Pattern

**Used in**: BillPayments, Insurance, SavingsGoals

```rust
// Store items with auto-incrementing IDs
let mut items: Map<u32, Item> = env.storage()
    .instance()
    .get(&STORAGE_KEY)
    .unwrap_or_else(|| Map::new(&env));

let next_id = env.storage()
    .instance()
    .get(&NEXT_ID_KEY)
    .unwrap_or(0u32) + 1;

items.set(next_id, new_item);
env.storage().instance().set(&STORAGE_KEY, &items);
env.storage().instance().set(&NEXT_ID_KEY, &next_id);
```

**Benefits**:

- O(1) lookup by ID
- Maintains insertion order (implicit)
- Efficient iteration

### 2. Address-Based Access Control

**Used in**: FamilyWallet

```rust
// Direct Address -> Member mapping
let mut members: Map<Address, FamilyMember> = env.storage()
    .instance()
    .get(&MEMBERS_KEY)
    .unwrap_or_else(|| Map::new(&env));

// Efficient member lookup
if let Some(member) = members.get(address) {
    // Apply role-based logic
}
```

**Benefits**:

- Direct permission checking
- Efficient role-based access
- No ID lookup required

### 3. Vector-Based Configuration

**Used in**: RemittanceSplit

```rust
// Store configuration as Vec
let split: Vec<u32> = vec![&env, 50, 30, 15, 5];
env.storage().instance().set(&SPLIT_KEY, &split);

// Access by index
let spending_pct = split.get(0).unwrap();
```

**Benefits**:

- Fixed-size configuration
- Efficient batch updates
- Simple semantics

### 4. Query-by-Iteration Pattern

**Used in**: BillPayments, Insurance

```rust
// Get all items matching criteria
let mut result = Vec::new(&env);
for i in 1..=max_id {
    if let Some(item) = items.get(i) {
        if should_include(&item) {
            result.push_back(item);
        }
    }
}
```

**Benefits**:

- Flexible filtering
- Works with any criteria
- No separate index needed

---

## Integration Patterns

### Pattern 1: Sequential Processing

**Used by**: Remittance processing workflow

```
Request → RemittanceSplit → Calculate
            │
            ├─ BillPayments.pay_bill()
            ├─ Insurance.pay_premium()
            ├─ SavingsGoals.add_to_goal()
            └─ FamilyWallet.check_spending_limit()
            │
            └─ Response
```

### Pattern 2: Query Aggregation

**Used by**: Dashboard/analytics

```
Request → Query Multiple Contracts
            │
            ├─ BillPayments.get_total_unpaid()
            ├─ Insurance.get_active_policies()
            ├─ SavingsGoals.get_all_goals()
            └─ FamilyWallet.get_all_members()
            │
            └─ Aggregate and Return
```

### Pattern 3: State Validation

**Used by**: Pre-operation checks

```
Request → FamilyWallet.check_spending_limit()
            │
            ├─ If valid: Execute transfer
            └─ If invalid: Reject
```

---

## Security Architecture

### 1. Input Validation

**Implemented in all contracts**:

```rust
// Percentage validation in RemittanceSplit
if total != 100 {
    return false;  // Reject invalid split
}

// Amount validation
if amount < 0 {
    return false;  // Reject negative amounts
}

// Address validation (implicit in Soroban)
// Invalid addresses rejected by runtime
```

### 2. Access Control

**Role-Based** (FamilyWallet):

```
Admin role:
├─ Can manage all members
├─ Can set spending limits
└─ Can change roles

Sender role:
├─ Can spend up to limit
└─ Cannot manage members

Recipient role:
├─ Can only receive
└─ Cannot initiate transfers
```

**Implicit** (Other contracts):

- Soroban runtime ensures signer authentication
- Only authorized accounts can invoke functions

### 3. State Consistency

**Atomic Operations**:

- All storage updates in single transaction
- All-or-nothing semantics
- No partial state updates

### 4. Overflow Protection

**Rust Type Safety**:

```rust
// i128 used for large amounts
pub amount: i128,  // Supports ±9 quintillion stroops

// Overflow checks enabled in release builds
[profile.release]
overflow-checks = true
```

### 5. Data Isolation

**Storage Key Namespacing**:

```rust
env.storage().instance().set(&symbol_short!("BILLS"), &bills);
// Each contract has separate storage namespace
// No collision possible
```

---

## Error Handling Strategy

### By Contract

**Bill Payments**:

- Returns `false` if bill not found
- Returns `false` if already paid
- Returns `Option<Bill>` from get_bill

**Insurance**:

- Returns `false` if policy not found
- Returns `false` if inactive
- Returns `Option<InsurancePolicy>` from get_policy

**Savings Goals**:

- Returns `-1` if goal not found on add_to_goal
- Returns `Option<SavingsGoal>` from get_goal
- Returns `bool` from is_goal_completed

**Family Wallet**:

- Returns `false` if member not found
- Returns `false` if spending limit exceeded
- Returns `Option<FamilyMember>` from get_member

**Remittance Split**:

- Returns `false` if percentages invalid
- Returns `Vec<u32>` with default if not configured

### Error Categories

1. **Not Found**: Resource doesn't exist
   - Return: `Option::None` or `false`
2. **Invalid State**: Resource exists but in wrong state
   - Return: `false` or error code
3. **Invalid Input**: Input doesn't meet requirements
   - Return: `false` (validation fails)

---

## Performance Characteristics

### Time Complexity

| Operation         | Contract        | Complexity | Notes                  |
| ----------------- | --------------- | ---------- | ---------------------- |
| Create item       | All             | O(1)       | Direct insertion       |
| Get by ID         | Map-based       | O(1)       | Direct lookup          |
| Get all           | Map-based       | O(n)       | Full iteration         |
| Query with filter | All             | O(n)       | Filtered iteration     |
| Update item       | All             | O(1)       | Direct update          |
| Delete item       | Not implemented | -          | Use deactivate pattern |

### Space Complexity

| Storage Type      | Space      | Limit                       |
| ----------------- | ---------- | --------------------------- |
| Individual item   | ~100 bytes | Per contract max ~100 items |
| Map overhead      | ~50 bytes  | Shared                      |
| Vec configuration | ~20 bytes  | Fixed size                  |

---

## Scalability Considerations

### Current Limitations

1. **Item Limit**: ~100 items per contract before performance degrades
2. **Query Complexity**: O(n) queries become expensive at scale
3. **Storage**: Soroban has storage limits per contract

### Future Optimization Options

1. **Pagination**: Implement cursor-based pagination for get_all
2. **Indexing**: Add secondary indexes for common queries
3. **Sharding**: Split data across multiple contracts
4. **Caching**: Client-side caching of frequently accessed data

---

## Testing Architecture

### Unit Tests

Located in each contract's `test.rs`:

```
bill_payments/
├─ src/
│  ├─ lib.rs
│  └─ test.rs    ← Unit tests
insurance/
├─ src/
│  ├─ lib.rs
│  └─ test.rs    ← Unit tests
```

### Integration Tests

Test cross-contract interactions:

```rust
// Test remittance split coordinating multiple contracts
#[test]
fn test_remittance_flow() {
    let env = Env::default();

    // Initialize all contracts
    // Execute remittance split
    // Verify all contracts updated correctly
}
```

### Testing Patterns

- **Isolation**: Each test operates independently
- **Setup/Teardown**: Clean state for each test
- **Assertions**: Verify expected behavior
- **Edge Cases**: Test boundaries and error conditions

---

## Future Architecture Enhancements

### Proposed Improvements

1. **Contract Registry**: Central registry of all Remitwise contracts
2. **Event Emission**: Emit events for off-chain tracking
3. **Multi-sig Support**: Require multiple signatures for critical operations
4. **Upgrade Mechanism**: Allow contract code updates (if Soroban supports)
5. **Cross-contract Calls**: Direct contract-to-contract invocation
6. **Time-lock**: Delay operations for security review period

### Backward Compatibility

- Current contracts will remain compatible
- New features added incrementally
- No breaking changes to public APIs

---

## References

- [Soroban Documentation](https://developers.stellar.org/learn/building-apps/example-application)
- [Stellar Smart Contracts](https://developers.stellar.org/learn)
- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
