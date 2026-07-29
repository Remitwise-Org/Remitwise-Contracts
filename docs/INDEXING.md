# Remitwise Contract Event Indexing Guide

This guide is designed for **downstream integrators** building off-chain query interfaces, databases, notification engines, or audit trails for the Remitwise smart contract system. It specifies how smart contract events map to off-chain logical tables and entities.

---

## 1. Unified Event Taxonomy

All Remitwise contracts emit events using a standardized topic structure defined in `remitwise-common`:

```
Topic 0: Symbol("Remitwise")
Topic 1: EventCategory (encoded as u32)
Topic 2: EventPriority (encoded as u32)
Topic 3: Action (Symbol, e.g., "created", "paid", "member")
```

Downstream consumers can filter topics globally by subscription or specific contract address using Topic 0 (`Remitwise`).

### Event Category Encoding
- `Transaction` (`0`) – Direct transfers or value movement.
- `State` (`1`) – Changes to configuration or state variables.
- `Alert` (`2`) – Signals for security or threshold notifications.
- `System` (`3`) – Administrative actions (pauses, upgrades).
- `Access` (`4`) – Changes to permissions or memberships.

---

## 2. Event-to-Table Mappings

### Savings Goals (`savings_goals`)
Emitted events update the `savings_goals` entity/table.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `goal_created` | A new savings goal is created | `(goal_id: u32, owner: Address, name: String, target_amount: i128, target_date: u64)` | `INSERT` new goal with `current_amount = 0`, status `locked = 0` |
| `goal_deposit` | Funds deposited into a savings goal | `(goal_id: u32, amount: i128)` | `UPDATE`: Add `amount` to `current_amount` |
| `goal_withdraw`| Funds withdrawn from a savings goal | `(goal_id: u32, amount: i128)` | `UPDATE`: Subtract `amount` from `current_amount` |
| `tags_add` | Tags added to a savings goal | `(entity_id: u32, tags: Vec<String>)` | `UPDATE`: Append tags to JSON array in `tags` |
| `tags_rem` | Tags removed from a savings goal | `(entity_id: u32, tags: Vec<String>)` | `UPDATE`: Filter tags out of JSON array in `tags` |

#### Concrete Payload Example (`goal_created`)
```json
{
  "topic": ["Remitwise", 1, 1, "goal_created"],
  "data": [
    1,
    "GD7U3R...OWNER_ADDRESS",
    "Emergency Fund",
    "10000000000",
    1735689600
  ]
}
```

---

### Bill Payments (`bill_payments`)
Emitted events update the `bills` entity/table.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `bill_created` | A new bill is registered | `(bill_id: u32, owner: Address, name: String, amount: i128, due_date: u64, recurring: bool)` | `INSERT` new bill with `paid = 0` |
| `bill_paid` | A bill is paid in full | `(bill_id: u32, owner: Address)` | `UPDATE`: Set `paid = 1`, `paid_at = timestamp` |
| `bill_cancelled`| A bill is cancelled | `(bill_id: u32, owner: Address)` | `UPDATE`: Delete bill or mark as cancelled |
| `bill_restored` | A cancelled bill is restored | `(bill_id: u32, owner: Address)` | `UPDATE`: Mark bill as active and unpaid |

#### Concrete Payload Example (`bill_paid`)
```json
{
  "topic": ["Remitwise", 0, 2, "bill_paid"],
  "data": [
    42,
    "GD7U3R...OWNER_ADDRESS"
  ]
}
```

---

### Insurance (`insurance`)
Emitted events update the `insurance_policies` entity/table.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `policy_created`| A new policy is created | `(policy_id: u32, owner: Address, name: String, coverage_type: u32, monthly_premium: i128, coverage_amount: i128)` | `INSERT` new policy with `active = 1` |

---

### Remittance Split (`remittance_split`)
Emitted events update the `remittance_splits` entity/table.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `split_created` | A new split config is initialized | `(split_id: u32, owner: Address, name: String, total_amount: i128, recipients: Vec<Address>)` | `INSERT` new split configuration |
| `split_executed`| A split transaction is executed | `(split_id: u32)` | `UPDATE`: Set `executed = 1`, `executed_at = timestamp` |

---

### Family Wallet (`family_wallet`)
Emitted events update governance, membership, and emergency status tables.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `member` | Member added, removed, or role updated | `(member: Address, role: u32, spending_limit: i128)` | `UPSERT` member configuration or mark inactive |
| `limit` | Spending limit is updated | `(member: Address, new_limit: i128)` | `UPDATE` member's spending limit |
| `em_prop` | Emergency withdrawal proposed | `(proposer: Address, recipient: Address, amount: i128)` | `INSERT` proposal into audit trail / queue |
| `archived` | Transaction proposal archived | `(tx_id: u32)` | `UPDATE` proposal state to archived |

---

### Orchestrator (`orchestrator`)
Tracks multi-contract execution flow status.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `flow_ok` | Orchestration flow completes successfully | `(executor: Address, amount: i128)` | `INSERT` success log into execution history |
| `flow_fail` | Orchestration flow fails | `(executor: Address, error_code: u32)` | `INSERT` failure log into execution history |

---

### Emergency Killswitch (`emergency_killswitch`)
Drives immediate system administration and health alerts.

| Action Topic | Event Description | Payload Data Fields | DB Operation |
|:---|:---|:---|:---|
| `paused` | Global pause triggered | `(scope: Symbol)` | `INSERT` pause state and trigger high-priority alerts |
| `unpaused` | Pause status lifted | `(scope: Symbol)` | `INSERT` pause removal event |
| `f_paused` | Individual function paused | `(scope: Symbol, module_id: Symbol, func_name: Symbol)` | `INSERT` function pause entry |
| `m_paused` | Individual contract module paused | `(scope: Symbol, module_id: Symbol)` | `INSERT` module pause entry |

---

## 3. Ordering & Idempotency Rules

Downstream consumers must adhere to the following processing guarantees:

1. **Idempotence**: Stellar ledgers can be replayed. Every database table representing contract events or entities should enforce a unique constraint on the event identifier tuple `(ledger_sequence, transaction_hash, event_topic)`. Use `INSERT OR IGNORE` or `INSERT OR REPLACE` to handle retries without producing duplicate entries.
2. **Sequential Integrity**: Events within the same transaction or block must be processed in the exact order they are emitted. For example, a `goal_deposit` must not be processed before its corresponding `goal_created` event has been fully inserted.
