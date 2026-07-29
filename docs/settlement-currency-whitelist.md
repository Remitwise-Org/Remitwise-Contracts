# Settlement Currency Whitelist — Contributor Guide

**Audience:** Contributors (developers building, extending, reviewing, or testing settlement currency whitelist logic in Remitwise contracts).

---

## 1. Overview

Every invoice in Remitwise represents a payment obligation with a specific **amount** and **currency**. The settlement currency whitelist is a per-invoice list of currencies the payee is willing to accept in settlement. When a payer attempts to settle an invoice, the proposed currency must appear in that invoice's whitelist; otherwise the settlement is rejected.

This defence-in-depth check prevents an attacker (or a compromised off-chain settlement relayer) from discharging the full face amount of an obligation while paying in a currency the payee never agreed to hold — an illiquid, depegged, or otherwise low-value asset — while the contract's ledger still records the obligation as "settled in full".

### Flow summary

```
Invoice owner              Settlement caller
     │                            │
     │  add_accepted_currency     │
     │──────────────────────────> │  (mutates whitelist)
     │                            │
     │  remove_accepted_currency  │
     │──────────────────────────> │  (mutates whitelist)
     │                            │
     │                            │  pay_bill(invoice, currency)
     │                            │  ──> require_matching_settlement_currency
     │                            │      ──> Ok / CurrencyNotWhitelisted
```

### Trust boundary

| Actor | Can mutate whitelist? | Can settle? |
|-------|----------------------|-------------|
| Invoice owner | Yes | Yes (their own invoice) |
| Off-chain relayer | No | Yes (for approved currency) |
| Any address | No | Only if currency is whitelisted |

An empty whitelist is treated as accepting **nothing**, never as a wildcard, so a mis-provisioned invoice fails closed instead of silently accepting any currency.

---

## 2. Data model

The whitelist is stored as a `soroban_sdk::Vec<Symbol>` — an ordered, iterable collection of currency symbols — embedded in the invoice struct.

```rust
pub struct Bill {
    pub id: u32,
    pub owner: Address,
    pub amount: i128,
    pub currency: String,            // Primary invoice currency
    pub accepted_currencies: Vec<Symbol>,  // Whitelist for settlement
    // ... other fields
}
```

**Constraints:**
- Each entry is a `Symbol` (max 9 bytes, uppercase asset code).
- Duplicate entries are **not** checked at insert time — the linear scan in `require_matching_settlement_currency` treats duplicates as harmless (first match wins).
- The whitelist is independent of `currency` (the primary invoice currency). The primary currency is a convention for display and default settlement; the whitelist defines which currencies are actually accepted.

---

## 3. Mutation entry points

### 3.1 `add_accepted_currency`

Appends a currency to the invoice's whitelist.

```rust
pub fn add_accepted_currency(
    env: Env,
    caller: Address,
    bill_id: u32,
    currency: Symbol,
) -> Result<(), BillPaymentsError>
```

**Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `caller` | `Address` | Must authenticate as the bill owner |
| `bill_id` | `u32` | Target invoice ID |
| `currency` | `Symbol` | Currency to add (e.g. `symbol_short!("USDC")`) |

**Access control:** `caller.require_auth()` + `caller == bill.owner`.

**Rejections:**

| Condition | Error |
|-----------|-------|
| Bill does not exist | `BillNotFound` |
| Caller is not bill owner | `Unauthorized` |
| Contract is paused (global or per-function) | `ContractPaused` / `FunctionPaused` |

**Concrete example:**

```rust
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Symbol};

fn test_add_accepted_currency() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let bill_id = client.create_bill(
        &owner, &"Rent".into(), &None, &100_000_000,
        &(env.ledger().timestamp() + 86400), &false, &0, &None,
        &"XLM".into(), &None,
    );

    // Add USDC as an accepted settlement currency
    client.add_accepted_currency(&owner, &bill_id, &symbol_short!("USDC"));

    // EURC is not yet in the whitelist
    let whitelist = client.get_accepted_currencies(&bill_id);
    assert_eq!(whitelist.len(), 1);
    assert_eq!(whitelist.get(0).unwrap(), symbol_short!("USDC"));
}
```

### 3.2 `remove_accepted_currency`

Removes a currency from the invoice's whitelist.

```rust
pub fn remove_accepted_currency(
    env: Env,
    caller: Address,
    bill_id: u32,
    currency: Symbol,
) -> Result<(), BillPaymentsError>
```

**Parameters:** Same as `add_accepted_currency`.

**Access control:** Same as `add_accepted_currency`.

**Behaviour:** Scans the whitelist for the given currency and removes the first matching entry. If the currency is not present, the call succeeds (idempotent removal — no error).

**Concrete example:**

```rust
fn test_remove_accepted_currency() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let bill_id = client.create_bill(
        &owner, &"Rent".into(), &None, &100_000_000,
        &(env.ledger().timestamp() + 86400), &false, &0, &None,
        &"XLM".into(), &None,
    );

    client.add_accepted_currency(&owner, &bill_id, &symbol_short!("USDC"));
    client.add_accepted_currency(&owner, &bill_id, &symbol_short!("EURC"));

    // Remove USDC
    client.remove_accepted_currency(&owner, &bill_id, &symbol_short!("USDC"));

    let whitelist = client.get_accepted_currencies(&bill_id);
    assert_eq!(whitelist.len(), 1);
    assert_eq!(whitelist.get(0).unwrap(), symbol_short!("EURC"));

    // Idempotent removal of already-absent currency
    client.remove_accepted_currency(&owner, &bill_id, &symbol_short!("USDC"));
    assert_eq!(client.get_accepted_currencies(&bill_id).len(), 1);
}
```

### 3.3 `get_accepted_currencies`

Read-only query. Returns the current whitelist for an invoice.

```rust
pub fn get_accepted_currencies(
    env: Env,
    bill_id: u32,
) -> Vec<Symbol>
```

---

## 4. Guard check during settlement

Every settlement entry point calls `require_matching_settlement_currency` early, before any value transfer, as a defence-in-depth check.

```rust
use remitwise_common::require_matching_settlement_currency;

pub fn pay_bill(env: Env, caller: Address, bill_id: u32) -> Result<(), BillPaymentsError> {
    Self::require_not_paused(&env, pause_functions::FUNCTION_SYMBOL)?;

    let (mut bill, _bills) = Self::read_bill(&env, bill_id)?;
    bill.owner.require_auth();
    // ...

    // Guard: proposed currency must be in the whitelist
    let settlement_sym = Symbol::new(&env, &bill.currency);
    require_matching_settlement_currency(&bill.accepted_currencies, &settlement_sym)
        .map_err(|_| BillPaymentsError::CurrencyNotWhitelisted)?;

    // ... proceed with settlement
}
```

If the whitelist is empty, every settlement is rejected:

```rust
fn test_empty_whitelist_rejects_all() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let bill_id = client.create_bill(
        &owner, &"Rent".into(), &None, &100_000_000,
        &(env.ledger().timestamp() + 86400), &false, &0, &None,
        &"XLM".into(), &None,
    );

    // pay_bill fails because whitelist is empty
    let result = client.try_pay_bill(&owner, &bill_id);
    assert!(result.is_err());
}
```

---

## 5. Contract integration checklist

To add settlement currency whitelist support to a contract:

1. **Add `accepted_currencies: Vec<Symbol>` to the invoice struct** (e.g. `Bill`, `Policy`, `RemittanceLeg`).
2. **Add mutation entry points**: `add_accepted_currency`, `remove_accepted_currency`, `get_accepted_currencies`.
3. **Wire the guard**: Call `require_matching_settlement_currency` at the top of every entry point that settles the obligation.
4. **Seed the default whitelist**: On invoice creation, populate `accepted_currencies` with `[Symbol::new(&env, &currency)]` so the invoice's own currency is accepted by default.
5. **Write tests**: Cover add, remove, guard-accept, guard-reject, and empty-whitelist behaviour.

---

## 6. Shared utility reference

### `remitwise_common::require_matching_settlement_currency`

**Location:** `remitwise-common/src/lib.rs:417`

```rust
pub fn require_matching_settlement_currency(
    inv: &soroban_sdk::Vec<Symbol>,
    sym: &Symbol,
) -> Result<(), SettlementCurrencyError>
```

**Returns:**
- `Ok(())` if `sym` appears in `inv`.
- `Err(SettlementCurrencyError::CurrencyNotWhitelisted)` if `sym` is absent.

**Cost:** Linear scan of `inv` — negligible for typical whitelist sizes (expected 1–5 currencies).

### `remitwise_common::SettlementCurrencyError`

```rust
#[contracterror]
#[repr(u32)]
pub enum SettlementCurrencyError {
    CurrencyNotWhitelisted = 1,
}
```

---

## 7. Existing test coverage

The shared utility has four unit tests in `remitwise-common/src/lib.rs`:

| Test | Scenario |
|------|----------|
| `rejects_currency_not_in_whitelist` | USDC/EURC whitelist rejects XLM |
| `rejects_against_empty_whitelist` | Empty whitelist rejects XLM (fails closed) |
| `accepts_currency_present_in_whitelist` | USDC/EURC whitelist accepts both |
| `accepts_sole_whitelisted_currency` | Single-entry whitelist accepts that entry |

Run them with:

```bash
cargo test -p remitwise-common settlement_currency
```

---

## 8. Threat model

| Threat | Mitigation |
|--------|-----------|
| Attacker settles with unapproved low-value asset | `require_matching_settlement_currency` rejects currencies not in whitelist |
| Compromised off-chain relayer submits settlement in wrong currency | Same guard — relayer cannot bypass |
| Invoice owner accidentally removes all currencies | Fails closed: no settlement possible until at least one currency is added |
| Duplicate whitelist entries | Harmless — first match wins in linear scan |
| Gas grief via oversized whitelist | Whitelist size bounded by practical limits (expected 1–5 entries); caller pays gas for insert |

---

## 9. Related documentation

- [Settlement Windows](SETTLEMENT_WINDOWS.md) — Invoice settlement window lifecycle (Section 6.2 documents the guard function)
- [Settler Whitelist](SETTLER_WHITELIST.md) — Admin address management (separate concern from currency whitelist)
- [Remitwise Common](../remitwise-common/README.md) — Shared types and utilities
- [Threat Model](../THREAT_MODEL.md) — Full system threat model
- [Amount Invariants](AMOUNT_INVARIANTS.md) — Zero-handling rules across entry points
