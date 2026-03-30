# Remitwise Contracts Event Schema Documentation

This document defines the complete event schema for all Remitwise smart contracts. Events are emitted on-chain and can be indexed by external systems (indexers, frontends, analytics platforms) for reliable data parsing and monitoring.

**Version:** 1.0  
**Last Updated:** 2026-02-25  
**Compatibility:** Soroban SDK v21+

---

## Table of Contents

1. [Event Architecture](#event-architecture)
2. [Bill Payments Contract](#bill-payments-contract)
3. [Savings Goals Contract](#savings-goals-contract)
4. [Insurance Contract](#insurance-contract)
5. [Remittance Split Contract](#remittance-split-contract)
6. [Family Wallet Contract](#family-wallet-contract)
7. [Orchestrator Contract](#orchestrator-contract)
8. [Reporting Contract](#reporting-contract)
9. [Version Compatibility](#version-compatibility)

---

## Event Architecture

### Event Publishing Pattern

All Remitwise contracts use Soroban's `env.events().publish()` mechanism with a consistent topic structure:

```rust
// Primary event (indexed by topic)
env.events().publish((topic_symbol,), event_data);

// Secondary event (for categorization)
env.events().publish((contract_name, event_category), event_data);
```

### Data Types

- **Address**: Soroban Address type (32 bytes)
- **Symbol**: Short symbol (up to 12 characters, encoded as u64)
- **i128**: Signed 128-bit integer (stroops for amounts)
- **u32**: Unsigned 32-bit integer (IDs, counts)
- **u64**: Unsigned 64-bit integer (timestamps, dates)
- **String**: UTF-8 encoded string
- **bool**: Boolean flag

### Timestamp Convention

All timestamps are in **Unix epoch seconds** (seconds since 1970-01-01 00:00:00 UTC).

---

## Bill Payments Contract

**Contract Name:** `bill_payments`  
**Primary Topic Prefix:** `"Remitwise"`

### Event: Bill Created

**Topic:** `"Remitwise"` (category: Transaction, priority: Medium)  
**Action Symbol:** `"crt_bill"`

**Data Structure:**
```rust
pub struct Bill {
    pub id: u32,                    // Unique bill ID
    pub owner: Address,             // Bill owner address
    pub name: String,               // Bill name (e.g., "Electricity")
    pub amount: i128,               // Amount in stroops
    pub due_date: u64,              // Unix timestamp of due date
    pub recurring: bool,            // Whether bill recurs
    pub frequency_days: u32,        // Recurrence frequency in days (0 if non-recurring)
    pub paid: bool,                 // Payment status
    pub created_at: u64,            // Creation timestamp
    pub paid_at: Option<u64>,       // Payment timestamp (null if unpaid)
    pub schedule_id: Option<u32>,   // Associated schedule ID (null if none)
    pub currency: String,           // Currency code (e.g., "XLM", "USDC")
}
```

**Example Event:**
```json
{
  "id": 1,
  "owner": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "name": "Electricity",
  "amount": 1000,
  "due_date": 1234567890,
  "recurring": false,
  "frequency_days": 0,
  "paid": false,
  "created_at": 1234567800,
  "paid_at": null,
  "schedule_id": null,
  "currency": "XLM"
}
```

### Event: Bill Paid

**Topic:** `"Remitwise"` (category: Transaction, priority: High)  
**Action Symbol:** `"pay_bill"`

**Data Structure:**
```rust
pub struct BillPaidEvent {
    pub bill_id: u32,               // ID of paid bill
    pub owner: Address,             // Bill owner
    pub amount: i128,               // Amount paid
    pub paid_at: u64,               // Payment timestamp
}
```

**Example Event:**
```json
{
  "bill_id": 1,
  "owner": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "amount": 1000,
  "paid_at": 1234567850
}
```

### Event: Bill Cancelled

**Topic:** `"Remitwise"` (category: State, priority: Medium)  
**Action Symbol:** `"can_bill"`

**Data Structure:**
```rust
pub struct BillCancelledEvent {
    pub bill_id: u32,               // ID of cancelled bill
    pub owner: Address,             // Bill owner
    pub cancelled_at: u64,          // Cancellation timestamp
}
```

### Event: Bills Archived

**Topic:** `"Remitwise"` (category: System, priority: Low)  
**Action Symbol:** `"archive"`

**Data Structure:**
```rust
pub struct BillsArchivedEvent {
    pub count: u32,                 // Number of bills archived
    pub archived_at: u64,           // Archive timestamp
}
```

### Event: Bill Restored

**Topic:** `"Remitwise"` (category: State, priority: Medium)  
**Action Symbol:** `"restore"`

**Data Structure:**
```rust
pub struct BillRestoredEvent {
    pub bill_id: u32,               // ID of restored bill
    pub owner: Address,             // Bill owner
    pub restored_at: u64,           // Restoration timestamp
}
```

### Event: Contract Paused/Unpaused

**Topic:** `"Remitwise"` (category: System, priority: High)  
**Action Symbol:** `"paused"` or `"unpaused"`

**Data:** Empty tuple `()`

### Event: Contract Upgraded

**Topic:** `"Remitwise"` (category: System, priority: High)  
**Action Symbol:** `"upgraded"`

**Data Structure:**
```rust
pub struct VersionUpgradeEvent {
    pub previous_version: u32,      // Previous contract version
    pub new_version: u32,           // New contract version
}
```

---

## Savings Goals Contract

**Contract Name:** `savings_goals`  
**Primary Topic Prefix:** `"savings"`

### Event: Goal Created

**Topic:** `"created"` (primary)  
**Secondary Topic:** `("savings", SavingsEvent::GoalCreated)`

**Data Structure:**
```rust
pub struct GoalCreatedEvent {
    pub goal_id: u32,               // Unique goal ID
    pub name: String,               // Goal name (e.g., "Emergency Fund")
    pub target_amount: i128,        // Target amount in stroops
    pub target_date: u64,           // Target completion date (Unix timestamp)
    pub timestamp: u64,             // Event timestamp
}
```

**Example Event:**
```json
{
  "goal_id": 1,
  "name": "Emergency Fund",
  "target_amount": 50000,
  "target_date": 1735689600,
  "timestamp": 1234567800
}
```

### Event: Funds Added

**Topic:** `"added"` (primary)  
**Secondary Topic:** `("savings", SavingsEvent::FundsAdded)`

**Data Structure:**
```rust
pub struct FundsAddedEvent {
    pub goal_id: u32,               // Goal ID
    pub amount: i128,               // Amount added in stroops
    pub new_total: i128,            // New total in goal
    pub timestamp: u64,             // Event timestamp
}
```

**Example Event:**
```json
{
  "goal_id": 1,
  "amount": 5000,
  "new_total": 15000,
  "timestamp": 1234567850
}
```

### Event: Goal Completed

**Topic:** `"completed"` (primary)  
**Secondary Topic:** `("savings", SavingsEvent::GoalCompleted)`

**Data Structure:**
```rust
pub struct GoalCompletedEvent {
    pub goal_id: u32,               // Goal ID
    pub name: String,               // Goal name
    pub final_amount: i128,         // Final amount in goal
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Funds Withdrawn

**Topic:** `("savings", SavingsEvent::FundsWithdrawn)`

**Data Structure:**
```rust
pub struct FundsWithdrawnEvent {
    pub goal_id: u32,               // Goal ID
    pub amount: i128,               // Amount withdrawn
    pub remaining: i128,            // Remaining amount
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Goal Locked/Unlocked

**Topic:** `("savings", SavingsEvent::GoalLocked)` or `("savings", SavingsEvent::GoalUnlocked)`

**Data Structure:**
```rust
pub struct GoalLockEvent {
    pub goal_id: u32,               // Goal ID
    pub locked: bool,               // Lock status
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Savings Schedule Created

**Topic:** `("savings", SavingsEvent::ScheduleCreated)`

**Data Structure:**
```rust
pub struct SavingsScheduleCreatedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub goal_id: u32,               // Associated goal ID
    pub amount: i128,               // Recurring amount
    pub next_due: u64,              // Next execution timestamp
    pub interval: u64,              // Interval in seconds
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Contract Upgraded

**Topic:** `("savings", "upgraded")`

**Data Structure:**
```rust
pub struct VersionUpgradeEvent {
    pub previous_version: u32,
    pub new_version: u32,
}
```

---

## Insurance Contract

**Contract Name:** `insurance`  
**Primary Topic Prefix:** `"insure"`

### Event: Policy Created

**Topic:** `"created"` (primary)  
**Secondary Topic:** `("insure", InsuranceEvent::PolicyCreated)`

**Data Structure:**
```rust
pub struct PolicyCreatedEvent {
    pub policy_id: u32,             // Unique policy ID
    pub name: String,               // Policy name (e.g., "Life Insurance")
    pub coverage_type: String,      // Coverage type (e.g., "Term", "Whole")
    pub monthly_premium: i128,      // Monthly premium in stroops
    pub coverage_amount: i128,      // Total coverage amount in stroops
    pub timestamp: u64,             // Event timestamp
}
```

**Example Event:**
```json
{
  "policy_id": 1,
  "name": "Life Insurance",
  "coverage_type": "Term",
  "monthly_premium": 2000,
  "coverage_amount": 500000,
  "timestamp": 1234567800
}
```

### Event: Premium Paid

**Topic:** `"paid"` (primary)  
**Secondary Topic:** `("insure", InsuranceEvent::PremiumPaid)`

**Data Structure:**
```rust
pub struct PremiumPaidEvent {
    pub policy_id: u32,             // Policy ID
    pub name: String,               // Policy name
    pub amount: i128,               // Premium amount paid
    pub next_payment_date: u64,     // Next payment due date
    pub timestamp: u64,             // Event timestamp
}
```

**Example Event:**
```json
{
  "policy_id": 1,
  "name": "Life Insurance",
  "amount": 2000,
  "next_payment_date": 1237246200,
  "timestamp": 1234567850
}
```

### Event: Policy Deactivated

**Topic:** `"deactive"` (primary)  
**Secondary Topic:** `("insure", InsuranceEvent::PolicyDeactivated)`

**Data Structure:**
```rust
pub struct PolicyDeactivatedEvent {
    pub policy_id: u32,             // Policy ID
    pub name: String,               // Policy name
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Premium Schedule Created

**Topic:** `("insure", InsuranceEvent::ScheduleCreated)`

**Data Structure:**
```rust
pub struct PremiumScheduleCreatedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub policy_id: u32,             // Associated policy ID
    pub next_due: u64,              // Next execution timestamp
    pub interval: u64,              // Interval in seconds
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Premium Schedule Executed

**Topic:** `("insure", InsuranceEvent::ScheduleExecuted)`

**Data Structure:**
```rust
pub struct ScheduleExecutedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub policy_id: u32,             // Policy ID
    pub amount: i128,               // Amount processed
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Premium Schedule Missed

**Topic:** `("insure", InsuranceEvent::ScheduleMissed)`

**Data Structure:**
```rust
pub struct ScheduleMissedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub policy_id: u32,             // Policy ID
    pub missed_count: u32,          // Total missed count
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Contract Upgraded

**Topic:** `("insure", "upgraded")`

**Data Structure:**
```rust
pub struct VersionUpgradeEvent {
    pub previous_version: u32,
    pub new_version: u32,
}
```

---

## Remittance Split Contract

**Contract Name:** `remittance_split`  
**Primary Topic Prefix:** `"split"`

### Event: Split Initialized

**Topic:** `"init"` (primary)  
**Secondary Topic:** `("split", SplitEvent::Initialized)`

**Data Structure:**
```rust
pub struct SplitInitializedEvent {
    pub spending_percent: u32,      // Spending allocation percentage (0-100)
    pub savings_percent: u32,       // Savings allocation percentage (0-100)
    pub bills_percent: u32,         // Bills allocation percentage (0-100)
    pub insurance_percent: u32,     // Insurance allocation percentage (0-100)
    pub timestamp: u64,             // Event timestamp
}
```

**Constraint:** `spending_percent + savings_percent + bills_percent + insurance_percent == 100`

**Example Event:**
```json
{
  "spending_percent": 50,
  "savings_percent": 30,
  "bills_percent": 15,
  "insurance_percent": 5,
  "timestamp": 1234567800
}
```

### Event: Split Calculated

**Topic:** `"calc"` (primary)  
**Secondary Topic:** `("split", SplitEvent::Calculated)`

**Data Structure:**
```rust
pub struct SplitCalculatedEvent {
    pub total_amount: i128,         // Total amount to split
    pub spending_amount: i128,      // Calculated spending amount
    pub savings_amount: i128,       // Calculated savings amount
    pub bills_amount: i128,         // Calculated bills amount
    pub insurance_amount: i128,     // Calculated insurance amount
    pub timestamp: u64,             // Event timestamp
}
```

**Constraint:** `spending_amount + savings_amount + bills_amount + insurance_amount == total_amount`

**Example Event:**
```json
{
  "total_amount": 10000,
  "spending_amount": 5000,
  "savings_amount": 3000,
  "bills_amount": 1500,
  "insurance_amount": 500,
  "timestamp": 1234567850
}
```

### Event: Split Updated

**Topic:** `("split", SplitEvent::Updated)`

**Data Structure:** Same as `SplitInitializedEvent`

### Event: Remittance Schedule Created

**Topic:** `("schedule", ScheduleEvent::Created)`

**Data Structure:**
```rust
pub struct RemittanceScheduleCreatedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub owner: Address,             // Schedule owner
    pub amount: i128,               // Remittance amount
    pub next_due: u64,              // Next execution timestamp
    pub interval: u64,              // Interval in seconds
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Remittance Schedule Executed

**Topic:** `("schedule", ScheduleEvent::Executed)`

**Data Structure:**
```rust
pub struct ScheduleExecutedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub amount: i128,               // Amount processed
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Remittance Schedule Modified

**Topic:** `("schedule", ScheduleEvent::Modified)`

**Data Structure:**
```rust
pub struct ScheduleModifiedEvent {
    pub schedule_id: u32,           // Schedule ID
    pub new_amount: i128,           // Updated amount
    pub new_next_due: u64,          // Updated next due date
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Remittance Schedule Cancelled

**Topic:** `("schedule", ScheduleEvent::Cancelled)`

**Data Structure:**
```rust
pub struct ScheduleCancelledEvent {
    pub schedule_id: u32,           // Schedule ID
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Contract Paused/Unpaused

**Topic:** `("split", "paused")` or `("split", "unpaused")`

**Data:** Empty tuple `()`

### Event: Contract Upgraded

**Topic:** `("split", "upgraded")`

**Data Structure:**
```rust
pub struct VersionUpgradeEvent {
    pub previous_version: u32,
    pub new_version: u32,
}
```

---

## Family Wallet Contract

**Contract Name:** `family_wallet`  
**Primary Topic Prefix:** `"family"`

### Event: Member Added

**Topic:** `("family", "member_added")`

**Data Structure:**
```rust
pub struct MemberAddedEvent {
    pub member: Address,            // Member address
    pub role: FamilyRole,           // Member role (Owner, Admin, Member)
    pub spending_limit: i128,       // Spending limit in stroops
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Spending Limit Updated

**Topic:** `("family", "limit_updated")`

**Data Structure:**
```rust
pub struct SpendingLimitUpdatedEvent {
    pub member: Address,            // Member address
    pub old_limit: i128,            // Previous limit
    pub new_limit: i128,            // New limit
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Transaction Proposed

**Topic:** `("family", "tx_proposed")`

**Data Structure:**
```rust
pub struct TransactionProposedEvent {
    pub tx_id: u64,                 // Transaction ID
    pub proposer: Address,          // Proposer address
    pub amount: i128,               // Transaction amount
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Transaction Executed

**Topic:** `("family", "tx_executed")`

**Data Structure:**
```rust
pub struct TransactionExecutedEvent {
    pub tx_id: u64,                 // Transaction ID
    pub amount: i128,               // Amount executed
    pub executed_at: u64,           // Execution timestamp
}
```

### Event: Emergency Mode Activated

**Topic:** `("family", "emergency_on")`

**Data Structure:**
```rust
pub struct EmergencyModeEvent {
    pub activated_by: Address,      // Address that activated emergency mode
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Emergency Mode Deactivated

**Topic:** `("family", "emergency_off")`

**Data Structure:**
```rust
pub struct EmergencyModeEvent {
    pub deactivated_by: Address,    // Address that deactivated emergency mode
    pub timestamp: u64,             // Event timestamp
}
```

---

## Orchestrator Contract

**Contract Name:** `orchestrator`  
**Primary Topic Prefix:** `"orchestrator"`

### Event: Remittance Flow Completed

**Topic:** `("orchestrator", "flow_complete")`

**Data Structure:**
```rust
pub struct RemittanceFlowEvent {
    pub caller: Address,            // Address that initiated flow
    pub total_amount: i128,         // Total amount processed
    pub allocations: Vec<i128>,     // [spending, savings, bills, insurance]
    pub timestamp: u64,             // Event timestamp
}
```

**Example Event:**
```json
{
  "caller": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4",
  "total_amount": 10000,
  "allocations": [5000, 3000, 1500, 500],
  "timestamp": 1234567850
}
```

### Event: Remittance Flow Failed

**Topic:** `("orchestrator", "flow_error")`

**Data Structure:**
```rust
pub struct RemittanceFlowErrorEvent {
    pub caller: Address,            // Address that initiated flow
    pub failed_step: Symbol,        // Step that failed (e.g., "perm_chk", "savings", "bills", "insurance")
    pub error_code: u32,            // Error code from OrchestratorError
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Execution Statistics Updated

**Topic:** `("orchestrator", "stats_updated")`

**Data Structure:**
```rust
pub struct ExecutionStats {
    pub total_flows_executed: u64,  // Total successful flows
    pub total_flows_failed: u64,    // Total failed flows
    pub total_amount_processed: i128, // Total amount processed
    pub last_execution: u64,        // Last execution timestamp
}
```

### Event: Audit Log Entry

**Topic:** `("orchestrator", "audit")`

**Data Structure:**
```rust
pub struct OrchestratorAuditEntry {
    pub caller: Address,            // Address that initiated operation
    pub operation: Symbol,          // Operation (e.g., "exec_flow", "exec_save", "exec_bill")
    pub amount: i128,               // Amount involved
    pub success: bool,              // Operation success status
    pub timestamp: u64,             // Event timestamp
    pub error_code: Option<u32>,    // Error code if failed
}
```

---

## Reporting Contract

**Contract Name:** `reporting`  
**Primary Topic Prefix:** `"reporting"`

### Event: Report Generated

**Topic:** `("reporting", ReportEvent::ReportGenerated)`

**Data Structure:**
```rust
pub struct ReportGeneratedEvent {
    pub user: Address,              // User address
    pub report_type: Symbol,        // Report type (e.g., "financial", "health", "insurance")
    pub period_key: u64,            // Period identifier
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Report Stored

**Topic:** `("reporting", ReportEvent::ReportStored)`

**Data Structure:**
```rust
pub struct ReportStoredEvent {
    pub user: Address,              // User address
    pub report_type: Symbol,        // Report type
    pub period_key: u64,            // Period identifier
    pub stored_at: u64,             // Storage timestamp
}
```

### Event: Addresses Configured

**Topic:** `("reporting", ReportEvent::AddressesConfigured)`

**Data Structure:**
```rust
pub struct AddressesConfiguredEvent {
    pub configured_by: Address,     // Address that configured
    pub timestamp: u64,             // Event timestamp
}
```

### Event: Reports Archived

**Topic:** `("reporting", ReportEvent::ReportsArchived)`

**Data Structure:**
```rust
pub struct ReportsArchivedEvent {
    pub count: u32,                 // Number of reports archived
    pub archived_at: u64,           // Archive timestamp
}
```

---

## Version Compatibility

### Contract Versioning

Each contract maintains a version number that can be queried via `get_version()`. Version changes are emitted as upgrade events.

**Current Versions:**
- Bill Payments: v1
- Savings Goals: v1
- Insurance: v1
- Remittance Split: v1
- Family Wallet: v1
- Orchestrator: v1
- Reporting: v1

### Event Format Stability

**Backward Compatibility Guarantees:**
- Event topics (primary and secondary) are immutable
- Event data structures are append-only (new fields added at the end)
- Existing fields maintain their type and position
- Deprecated fields are marked but not removed

**Breaking Changes:**
- Major version bumps indicate potential event schema changes
- Indexers should monitor `upgraded` events for version changes
- Contract upgrades are announced via `set_version()` calls

### Migration Path

When upgrading contracts:
1. New event types are added with new topic symbols
2. Old event types continue to be emitted for backward compatibility
3. Indexers can subscribe to both old and new topics during transition period
4. After deprecation period, old events may be phased out (announced in advance)

---

## Indexer Integration Guide

### Recommended Indexing Strategy

1. **Subscribe to all contract topics:**
   ```
   "Remitwise", "savings", "insure", "split", "family", "orchestrator", "reporting"
   ```

2. **Parse events by topic structure:**
   - Primary topic: Main event type
   - Secondary topics: Category and priority information
   - Data: Strongly-typed event payload

3. **Handle optional fields:**
   - Use `null` for missing optional values
   - Validate presence before processing

4. **Monitor version events:**
   - Track `upgraded` events to detect schema changes
   - Update parsers when new versions are detected

### Example Event Parsing (Pseudocode)

```javascript
function parseEvent(topics, data) {
  const [primary, ...secondary] = topics;
  
  switch(primary) {
    case "Remitwise":
      return parseBillPaymentEvent(secondary, data);
    case "savings":
      return parseSavingsEvent(secondary, data);
    case "insure":
      return parseInsuranceEvent(secondary, data);
    case "split":
      return parseSplitEvent(secondary, data);
    // ... other contracts
  }
}
```

---

## FAQ

**Q: Why are there both primary and secondary topics?**  
A: Primary topics enable efficient filtering by event type. Secondary topics provide categorization for analytics and monitoring.

**Q: How do I handle optional fields in events?**  
A: Optional fields are represented as `null` in JSON. Check for null before accessing.

**Q: What happens if a contract is upgraded?**  
A: An `upgraded` event is emitted with the old and new version numbers. Indexers should monitor these events.

**Q: Are events immutable?**  
A: Yes. Once emitted, events are immutable on-chain. They cannot be modified or deleted.

**Q: How do I correlate events across contracts?**  
A: Use the `caller` or `owner` address field to trace operations across contracts. The orchestrator contract emits flow events that reference multiple sub-contracts.

---

## Support & Updates

For questions or to report event schema issues, please open an issue in the repository with the `events` label.

Event schema updates will be documented in `CHANGELOG_CONTRACTS.md` with version numbers and migration guidance.
