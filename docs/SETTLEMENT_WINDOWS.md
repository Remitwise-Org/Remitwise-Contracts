# Invoice Settlement Windows Specification

**Audience:** Contributor (developers building, extending, reviewing, or testing invoice settlement logic in Remitwise contracts).

---

## 1. Domain & Terminology Mapping

In Remitwise contract architecture, an **invoice** represents a monetary payment obligation with a specified due timestamp, amount, and accepted currency. On-chain, invoices are primarily tracked and settled via the **`bill_payments`** contract using the [`Bill`](../bill_payments/src/lib.rs) and [`BillSchedule`](../bill_payments/src/lib.rs) types.

Related settlement window patterns exist in adjacent contracts:
- **`insurance`**: Premium payment schedules (`next_payment_date`).
- **`remittance_split` / `orchestrator`**: Signed authorization execution windows (`MAX_DEADLINE_WINDOW_SECS`).

This document specifies the exact settlement window rules and invariants for invoices (`Bill` / `BillSchedule`).

---

## 2. Settlement Window Parameters per Invoice

Every invoice (`Bill`) defines the following core fields in storage:

```rust
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub due_date: u64,           // Unix timestamp (seconds)
    pub recurring: bool,
    pub frequency_days: u32,     // Interval in days [1..=36_500]
    pub paid: bool,
    pub created_at: u64,         // Creation ledger timestamp
    pub paid_at: Option<u64>,    // Settlement ledger timestamp
    pub currency: String,        // Asset symbol (e.g. "XLM", "USDC")
}
```

### Key Window Timestamps

1. **`created_at`**: Unix timestamp when the invoice was registered via `create_bill`.
2. **`due_date`**: Upper boundary of the primary settlement window.
3. **`paid_at`**: Ledger timestamp recorded when settlement occurs via `pay_bill`.

---

## 3. Invoice Creation Rules (`create_bill`)

When creating an invoice via `create_bill(env, owner, name, amount, due_date, ...)`:

```rust
if due_date == 0 || due_date < current_time {
    return Err(BillPaymentsError::InvalidDueDate);
}
```

### Creation Boundary Conditions

| `due_date` vs `current_ledger_time` | Result | Reason |
|---|---|---|
| `due_date > now` | Accepted | Standard future due date |
| `due_date == now` | **Accepted** | Boundary condition: strict `<` check permits `due_date == now` |
| `due_date < now` | `InvalidDueDate (12)` | Invoices cannot be created overdue |
| `due_date == 0` | `InvalidDueDate (12)` | Zero timestamp is invalid |

---

## 4. Settlement Window Lifecycle & Overdue Semantics

An invoice transitions through distinct lifecycle states based on its `paid` flag and `due_date` relative to `env.ledger().timestamp()`:

```
                  Creation (create_bill)
                            │
                            ▼
              ┌───────────────────────────┐
              │     UNPAID & ON-TIME      │
              │  (now <= bill.due_date)   │
              └─────────────┬─────────────┘
                            │
               ┌────────────┴────────────┐
               │                         │
      Settled before due        Ledger advances
   (pay_bill / now <= due)     (now > bill.due_date)
               │                         │
               ▼                         ▼
    ┌────────────────────┐    ┌────────────────────┐
    │       PAID         │    │ UNPAID & OVERDUE   │
    │   (paid == true)   │    │(now > bill.due_date│
    └────────────────────┘    │  && !bill.paid)    │
                              └──────────┬─────────┘
                                         │
                                   Settled late
                               (pay_bill / now > due)
                                         │
                                         ▼
                              ┌────────────────────┐
                              │    PAID (LATE)     │
                              │   (paid == true)   │
                              └────────────────────┘
```

### Overdue Query Contract

Entrypoints `get_overdue_bills` and `get_overdue_bills_for_owner` filter overdue invoices using:

```rust
if bill.paid || bill.due_date >= current_time {
    continue; // Skip paid or non-overdue bills
}
```

- An unpaid invoice is overdue if and only if `bill.due_date < current_ledger_time`.
- An invoice with `bill.due_date == current_ledger_time` is **not overdue**.

---

## 5. Recurring Invoice Window Advancement & Late Catch-Up

When a recurring invoice (`recurring == true`) is settled via `pay_bill`, a child invoice is automatically generated for the next period.

### Recurrence Interval Calculation

```
period_seconds = frequency_days * 86_400
```

`frequency_days` must be in range `[1, 36_500]` (1 day to 100 years). Values outside this range return `BillPaymentsError::InvalidFrequency`.

### Late Settlement Catch-Up Loop

To prevent child invoices from spawning already overdue when a parent invoice is settled late, `pay_bill` executes a catch-up loop:

```rust
let period = (bill.frequency_days as u64)
    .checked_mul(SECONDS_PER_DAY)
    .ok_or(BillPaymentsError::InvalidFrequency)?;

let mut next_due_date = bill.due_date
    .checked_add(period)
    .ok_or(BillPaymentsError::InvalidDueDate)?;

// Catch-up loop for late settlements
while next_due_date <= current_time {
    next_due_date = next_due_date
        .checked_add(period)
        .ok_or(BillPaymentsError::InvalidDueDate)?;
}
```

### Security Invariant
Child invoices are **guaranteed** to be created with `child.due_date > env.ledger().timestamp()`, regardless of how late the parent invoice was settled.

---

## 6. Shared Defence-In-Depth Settlement Guards

Every invoice settlement operation is guarded by shared validation functions from `remitwise-common`:

### 1. Amount Guard (`require_positive_settlement_amount`)
```rust
pub fn require_positive_settlement_amount(amount: i128) -> Result<(), SettlementAmountError> {
    if amount <= 0 { Err(SettlementAmountError::NonPositiveAmount) } else { Ok(()) }
}
```
- Rejects zero (`amount == 0`) and negative amounts to prevent valueless side-effects or inverted transfers.

### 2. Currency Whitelist Guard (`require_matching_settlement_currency`)
```rust
pub fn require_matching_settlement_currency(inv: &Vec<Symbol>, sym: &Symbol) -> Result<(), SettlementCurrencyError> {
    for accepted in inv.iter() {
        if &accepted == sym { return Ok(()); }
    }
    Err(SettlementCurrencyError::CurrencyNotWhitelisted)
}
```
- Ensures settlement occurs in an asset whitelisted for the target invoice.

### 3. Anti-Dust Guard (`verify_no_dust`)
```rust
pub fn verify_no_dust(amount: i128) -> Result<(), DustError> {
    if amount < MIN_TRANSFER { Err(DustError::AmountTooSmall) } else { Ok(()) }
}
```
- Enforces `amount >= 100` stroops to prevent gas-griefing dust transactions.

---

## 7. Comparative Window Matrix Across Contracts

| Contract | Obligation Type | Window Bounds | Failure Error |
|---|---|---|---|
| **`bill_payments`** | Invoice (`Bill`) Creation | `due_date >= now` (`due_date != 0`) | `InvalidDueDate (12)` |
| **`bill_payments`** | Invoice Recurrence | `child_due_date = parent_due + period` (advanced until `> now`) | `InvalidDueDate (12)` |
| **`insurance`** | Premium Payment | `next_payment_date` (advanced by `30 days` until `> now`) | `PolicyNotFound` / invalid policy state |
| **`remittance_split`** | Signed Execution | `now < deadline <= now + 3600` (1 hour max) | `InvalidDeadline` / `DeadlineExpired` |
| **`orchestrator`** | Signed Flow Execution | `now < deadline <= now + 3600` (1 hour max) | `DeadlineExpired` |

---

## 8. Concrete Contributor Example: Creating & Settling an Invoice

Below is a complete, compilable Soroban test pattern demonstrating invoice creation, overdue checks, and settlement window advancement:

```rust
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, String};
use bill_payments::{BillPaymentsClient, BillPaymentsError};

#[test]
fn test_invoice_settlement_window_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, bill_payments::BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let now = 10_000u64;
    env.ledger().with_mut(|li| li.timestamp = now);

    // 1. Create invoice due in 7 days (7 * 86_400 = 604_800s)
    let due_date = now + 604_800;
    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Server Hosting Invoice"),
        &None,
        &50_000_000, // 5 XLM
        &due_date,
        &true,       // Recurring
        &30,         // Frequency: 30 days
        &None,
        &String::from_str(&env, "XLM"),
    );

    // 2. Verify invoice is not overdue at t = now
    let overdue_page = client.get_overdue_bills(&0, &10);
    assert_eq!(overdue_page.count, 0);

    // 3. Advance ledger timestamp past due date (t = due_date + 1)
    env.ledger().with_mut(|li| li.timestamp = due_date + 1);
    let overdue_page_after = client.get_overdue_bills(&0, &10);
    assert_eq!(overdue_page_after.count, 1);
    assert_eq!(overdue_page_after.items.get(0).unwrap().id, bill_id);

    // 4. Settle invoice (pay_bill)
    assert!(client.pay_bill(&owner, &bill_id).is_ok());

    // 5. Verify child invoice was spawned with due_date strictly in future
    let bill = client.get_bill(&bill_id).unwrap();
    assert!(bill.paid);
    
    // Parent schedule spawned a new bill with due_date > now
    let all_bills = client.get_bills_by_owner(&owner, &0, &10);
    assert_eq!(all_bills.count, 2);
    let child_bill = all_bills.items.get(1).unwrap();
    assert!(child_bill.due_date > due_date + 1);
}
```

---

## Related Documentation

- [Bill Payments — Due Date Semantics](bill-payments-due-date.md)
- [Remittance Split Deadline Window Semantics](remittance-split-deadline-window.md)
- [Orchestrator Signed-Flow Deadline Window Semantics](orchestrator-deadline-window.md)
- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md)
- [Event Taxonomy](EVENT_TAXONOMY.md)
