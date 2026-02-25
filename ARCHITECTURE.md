# Architecture Overview

## System Architecture

The Remitwise Contracts suite implements a comprehensive financial management system on the Stellar network using Soroban smart contracts. The architecture follows a modular design with clear separation of concerns and integrated data flow.

## Contract Relationships

```
┌─────────────────┐    ┌─────────────────┐
│   Remittance   │────│   Bill Payments │
│     Split      │    │                 │
│                 │    └─────────────────┘
└─────────┬───────┘             │
          │                     │
          │                     │
          v                     v
┌─────────────────┐    ┌─────────────────┐
│  Savings Goals  │    │    Insurance    │
│                 │    │                 │
└─────────────────┘    └─────────────────┘
          │                     │
          │                     │
          └──────────┬──────────┘
                     │
                     v
            ┌─────────────────┐
            │    Reporting    │
            │   (Aggregator)  │
            └─────────────────┘
```

## Data Flow Architecture

### Remittance Processing Flow

```
Incoming Remittance
        │
        ▼
┌─────────────────┐
│ Remittance Split│  Calculate allocation percentages
│   Contract      │  [spending, savings, bills, insurance]
└───────┬─────────┘
        │
        ├─────────────┐
        │             │
        ▼             ▼
┌─────────────┐ ┌─────────────┐
│Savings Goals│ │Bill Payments│
│             │ │             │
└─────────────┘ └──────┬──────┘
                       │
                       ▼
              ┌─────────────┐
              │  Insurance  │
              │             │
              └─────────────┘
```

## Contract Details

### 1. Remittance Split Contract

**Purpose:** Central allocation engine for incoming remittances

**Key Features:**

- Percentage-based fund allocation
- Owner-controlled configuration
- Backward-compatible storage
- Event-driven audit trail

**Storage Structure:**

```
Instance Storage:
├── CONFIG: SplitConfig { owner, percentages, initialized }
├── SPLIT: Vec<u32> [spending, savings, bills, insurance]
```

**Relationships:**

- **Provides:** Allocation ratios to other contracts
- **Consumes:** None (entry point for remittances)

### 2. Bill Payments Contract

**Purpose:** Manage recurring and one-time bill payments

**Key Features:**

- Bill creation with due dates
- Payment tracking and status
- Recurring bill automation
- Overdue bill identification

**Storage Structure:**

```
Instance Storage:
├── BILLS: Map<u32, Bill>
├── NEXT_ID: u32
├── ARCH_BILL: Map<u32, ArchivedBill>
├── STOR_STAT: StorageStats
```

**Relationships:**

- **Provides:** Bill payment tracking
- **Consumes:** Allocation amounts from Remittance Split
- **Integrates:** With Insurance for premium bills

### 3. Insurance Contract

**Purpose:** Insurance policy management and premium tracking

**Key Features:**

- Policy creation and activation
- Monthly premium scheduling
- Payment tracking
- Policy deactivation

**Storage Structure:**

```
Instance Storage:
├── POLICIES: Map<u32, InsurancePolicy>
├── NEXT_ID: u32
├── ARCH_POL: Map<u32, ArchivedPolicy>
├── STOR_STAT: StorageStats
```

**Relationships:**

- **Provides:** Insurance premium amounts
- **Consumes:** Allocation amounts from Remittance Split
- **Integrates:** With Bill Payments for premium tracking

### 4. Savings Goals Contract

**Purpose:** Goal-based savings management

**Key Features:**

- Goal creation with targets
- Fund addition/withdrawal
- Goal locking mechanism
- Progress tracking

**Storage Structure:**

```
Instance Storage:
├── GOALS: Map<u32, SavingsGoal>
├── NEXT_ID: u32
├── ARCH_GOAL: Map<u32, ArchivedGoal>
├── STOR_STAT: StorageStats
```

**Relationships:**

- **Provides:** Savings allocation management
- **Consumes:** Allocation amounts from Remittance Split

### 5. Reporting Contract

**Purpose:** Cross-contract data aggregation and comprehensive financial reporting

**Key Features:**

- Cross-contract data queries
- Financial health score calculation
- Multiple report types (remittance, savings, bills, insurance)
- Trend analysis and period comparisons
- Report storage and retrieval
- Category-wise breakdowns

**Storage Structure:**

```
Instance Storage:
├── ADMIN: Address
├── ADDRS: ContractAddresses
├── REPORTS: Map<(Address, u64), FinancialHealthReport>
├── ARCH_RPT: Map<(Address, u64), ArchivedReport>
├── STOR_STAT: StorageStats
```

**Relationships:**

- **Provides:** Aggregated financial insights and reports
- **Consumes:** Data from all other contracts via cross-contract calls
- **Integrates:** With remittance_split, savings_goals, bill_payments, insurance, family_wallet

## Integration Patterns

### Automated Remittance Processing

```rust
fn process_remittance(env: Env, user: Address, amount: i128) {
    // 1. Calculate allocations
    let allocations = remittance_split::calculate_split(env, amount);

    // 2. Allocate to savings
    savings_goals::add_to_goal(env, user, primary_goal, allocations[1]);

    // 3. Create bill payments
    bill_payments::create_bill(env, user, "Monthly Bills", allocations[2], due_date, false, 0);

    // 4. Pay insurance premiums
    insurance::pay_premium(env, user, active_policy);
}
```

### Cross-Contract Queries

```rust
fn get_financial_overview(env: Env, user: Address) -> FinancialOverview {
    let unpaid_bills = bill_payments::get_total_unpaid(env, user);
    let monthly_premium = insurance::get_total_monthly_premium(env, user);
    let savings_goals = savings_goals::get_all_goals(env, user);
    let split_config = remittance_split::get_config(env);

    FinancialOverview {
        unpaid_bills,
        monthly_premium,
        savings_goals,
        split_config,
    }
}
```

### Reporting Integration

The Reporting contract aggregates data from all contracts:

```rust
fn generate_financial_health_report(env: Env, user: Address) -> FinancialHealthReport {
    // Query remittance split configuration
    let split_client = RemittanceSplitClient::new(&env, &split_address);
    let split_config = split_client.get_split(&env);

    // Query savings progress
    let savings_client = SavingsGoalsClient::new(&env, &savings_address);
    let goals = savings_client.get_all_goals(user.clone());

    // Query bill compliance
    let bill_client = BillPaymentsClient::new(&env, &bills_address);
    let unpaid_bills = bill_client.get_unpaid_bills(user.clone());

    // Query insurance coverage
    let insurance_client = InsuranceClient::new(&env, &insurance_address);
    let policies = insurance_client.get_active_policies(user);

    // Calculate health score and generate report
    calculate_health_score_and_report(split_config, goals, unpaid_bills, policies)
}
```

## Security Architecture

### Access Control

- **Owner Authorization:** All operations require owner signature
- **Contract Isolation:** Each user has isolated data
- **Input Validation:** Comprehensive parameter validation
- **State Consistency:** Atomic operations prevent inconsistent states

### Storage Security

- **TTL Management:** Automatic storage cleanup
- **Instance Storage:** Efficient data organization
- **Event Logging:** Complete audit trail
- **Panic Handling:** Fail-fast on invalid operations

## Event Architecture

### Event Naming Conventions

All event topics and storage keys follow standardized naming conventions documented in:
- **Full Conventions**: [`docs/naming-conventions.md`](docs/naming-conventions.md)
- **Quick Reference**: [`docs/naming-quick-reference.md`](docs/naming-quick-reference.md)
- **Audit & Action Plan**: [`docs/naming-audit-action-plan.md`](docs/naming-audit-action-plan.md)

**Key Principles**:
- Event topics: lowercase, max 8 characters, past tense for actions
- Storage keys: UPPERCASE, underscores for multi-word, max 8 characters
- Event enums: PascalCase, descriptive names

### Event Types

```
Bill Payments Events:
├── Namespace: "bills"
├── BillEvent::Created
├── BillEvent::Paid

Insurance Events:
├── Namespace: "insure"
├── InsuranceEvent::PolicyCreated
├── InsuranceEvent::PremiumPaid
├── InsuranceEvent::PolicyDeactivated

Remittance Split Events:
├── Namespace: "split"
├── SplitEvent::Initialized
├── SplitEvent::Updated
├── SplitEvent::Calculated

Savings Goals Events:
├── Namespace: "savings"
├── SavingsEvent::GoalCreated
├── SavingsEvent::FundsAdded
├── SavingsEvent::FundsWithdrawn
├── SavingsEvent::GoalCompleted
├── SavingsEvent::GoalLocked
├── SavingsEvent::GoalUnlocked

Reporting Events:
├── Namespace: "report"
├── ReportEvent::ReportGenerated
├── ReportEvent::ReportStored
├── ReportEvent::AddressesConfigured
```

### Event Flow

```
User Action → Contract Function → State Change → Event Emission → Off-chain Processing
```

### Event Publication Pattern

```rust
// Standard pattern used across all contracts
env.events().publish(
    (symbol_short!("<namespace>"), EventEnum::Variant),
    event_data
);

// Example from Savings Goals
env.events().publish(
    (symbol_short!("savings"), SavingsEvent::GoalCreated),
    (goal_id, owner)
);
```

## Scalability Considerations

### Storage Optimization

- **Instance Storage:** Used for frequently accessed data
- **TTL Extension:** Prevents storage bloat
- **Efficient Maps:** O(1) access patterns
- **Minimal Data Duplication:** Shared storage keys
- **Tiered TTL Strategy:** Active data (~30 days), archived data (~180 days)
- **Data Archival:** Completed/inactive records moved to compressed archive storage
- **Bulk Cleanup:** Functions to permanently delete old archives
- **Storage Monitoring:** `StorageStats` struct tracks active/archived counts in all contracts

### Data Archival System

Each contract implements a comprehensive archival system:

```
Active Storage                    Archive Storage
├── GOALS (Map<u32, SavingsGoal>)  → ARCH_GOAL (Map<u32, ArchivedGoal>)
├── BILLS (Map<u32, Bill>)         → ARCH_BILL (Map<u32, ArchivedBill>)
├── POLICIES (Map<u32, Policy>)    → ARCH_POL (Map<u32, ArchivedPolicy>)
├── PEND_TXS (Map<u64, PendingTx>) → ARCH_TX (Map<u64, ArchivedTransaction>)
└── REPORTS (Map<(Addr,u64), Rpt>) → ARCH_RPT (Map<(Addr,u64), ArchivedReport>)
```

**Archival Flow:**
1. Archive functions move completed/inactive records to archive storage
2. Archived records use compressed structs with essential fields only
3. Archive storage uses longer TTL (6 months) for cost efficiency
4. Cleanup functions permanently delete old archives when no longer needed
5. Restore functions can move archived records back to active storage

**Archived Data Compression:**
- `ArchivedGoal`: Removes `locked`, `target_date` (no longer relevant)
- `ArchivedBill`: Removes `due_date`, `recurring`, `frequency_days`, `created_at`
- `ArchivedPolicy`: Removes `monthly_premium`, `coverage_amount`, `next_payment_date`
- `ArchivedTransaction`: Stores only `tx_id`, `tx_type`, `proposer`, timestamps
- `ArchivedReport`: Stores only `user`, `period_key`, `health_score`, timestamps

### Performance Patterns

- **Batch Operations:** Minimize cross-contract calls
- **Caching:** Client-side caching of configurations
- **Pagination:** For large result sets
- **Async Processing:** Event-driven architecture

## Error Handling

### Error Propagation

```
Contract Function
    ├── Success → Return Result
    ├── Validation Error → Panic with message
    ├── Access Error → Panic with message
    └── Storage Error → Panic with message
```

### Error Codes

- **Bill Payments:** BillNotFound, BillAlreadyPaid, InvalidAmount, etc.
- **Insurance:** Policy not found, unauthorized, inactive policy
- **Remittance Split:** Invalid percentages, not initialized
- **Savings Goals:** Goal not found, insufficient balance, locked goal

## Testing Architecture

### Unit Tests

Each contract includes comprehensive unit tests covering:

- Happy path scenarios
- Error conditions
- Edge cases
- Integration scenarios

### Integration Tests

Cross-contract functionality testing:

- Remittance processing workflows
- Multi-contract state consistency
- Event emission verification
- Access control validation

## Deployment Architecture

### Network Deployment

```
Development → Testnet → Mainnet
     │           │         │
     ├── Unit Tests       │
     └── Integration Tests└── Production Monitoring
```

### Contract Dependencies

```
Remittance Split ← Bill Payments
Remittance Split ← Insurance
Remittance Split ← Savings Goals
All Contracts ← Reporting (read-only queries)
```

No circular dependencies ensure clean deployment order. The Reporting contract should be deployed last after all other contracts are deployed and their addresses are known.

## Future Extensibility

### Contract Extensions

- **Multi-currency support**
- **Advanced scheduling**
- **Automated payments**
- **Reporting dashboards**
- **Third-party integrations**

### Architecture Evolution

- **Plugin system** for custom allocation rules
- **Sub-contracts** for specialized functionality
- **Cross-chain bridges** for multi-network support
- **Governance mechanisms** for protocol upgrades
