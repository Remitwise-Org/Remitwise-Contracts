# Settlement Currency Policy — How Currencies Are Chosen and Enforced

**Audience:** Contributor (developers building, extending, reviewing, or testing settlement currency logic in Remitwise contracts).

---

## 1. Domain & Terminology

In the Remitwise contract ecosystem, a **settlement currency** is the asset in which a financial obligation (bill, premium, remittance leg) is denominated and ultimately discharged. Every monetary obligation tracked on-chain carries a currency code that governs:

- **Denomination** — what asset the obligation is measured in (e.g., `"XLM"`, `"USDC"`, `"EURC"`).
- **Validation** — whether a proposed settlement matches the obligation's accepted currency.
- **Normalisation** — how raw user-supplied currency strings are canonicalised before storage and comparison.

This document is the **authoritative reference** for:

1. Which currencies the platform recognises.
2. How currency codes are canonicalised at entry.
3. How settlement currency enforcement gates each settlement entry point.
4. How to add or remove supported currencies.

---

## 2. Supported Currencies

Every token recognised by the platform is declared in [`remitwise-common/src/tokens.rs`](../remitwise-common/src/tokens.rs) as a variant of the [`SupportedToken`](../remitwise-common/src/tokens.rs) enum.

### 2.1 Canonical Token Registry

| Token | Code | Decimals | Minor Units / Major Unit | XDR Asset |
|-------|------|----------|--------------------------|-----------|
| Stellar Lumens | `"XLM"` | 7 | `10_000_000` stroops | Native |
| USD Coin | `"USDC"` | 6 | `1_000_000` base units | Stellar Asset Contract |
| Euro Coin | `"EURC"` | 7 | `10_000_000` base units | Stellar Asset Contract |

### 2.2 Key Constants

```rust
// remitwise-common/src/tokens.rs

/// Default currency code used when no currency is specified.
pub const DEFAULT_CURRENCY: &str = "XLM";

/// Maximum byte length of a user-supplied currency code string.
pub const MAX_CURRENCY_LEN: u32 = 10;
```

### 2.3 Adding a Token

When adding a token:

1. **Append** a new `#[repr(u32)]` variant at the **end** of `SupportedToken` to preserve existing discriminant stability across contract upgrades.
2. Implement the `decimals()`, `base_units_per_unit()`, and `currency_code()` methods.
3. Add an arm to `from_currency_code()`.
4. The compiler will force every exhaustive `match` on `SupportedToken` in the workspace to handle the new variant.

```rust
// Example: adding a new token
pub enum SupportedToken {
    XLM = 1,
    USDC = 2,
    EURC = 3,
    // NEW_TOKEN = 4,  // <-- append here
}
```

> **See also:** [docs/DECIMAL_CATALOGUE.md](DECIMAL_CATALOGUE.md) for the decimal reference table for every canonical token.

---

## 3. Currency Code Canonicalisation

User-supplied currency strings undergo strict canonicalisation before storage. This is implemented in the `bill_payments` contract via `validate_and_normalize_currency()`.

### 3.1 Transformation Pipeline

```
Raw input string
    │
    ▼
Empty check ──────────────────────► "" → return "XLM"
    │
    ▼
Length check ─────────────────────► len > 10 → Err(InvalidCurrency)
    │
    ▼
Trim leading/trailing ASCII spaces ──► "  NGN  " → "NGN"
    │
    ▼
Whitespace-only check ────────────► "   " → return "XLM"
    │
    ▼
Charset check (ASCII alphabetic) ─► non-alpha → Err(InvalidCurrency)
    │
    ▼
Uppercase ────────────────────────► "xlm" → "XLM"
    │
    ▼
Stored as canonical String
```

### 3.2 Validation Rules — Quick Reference

| Input | Output | Error? |
|-------|--------|--------|
| `""` (empty) | `"XLM"` | No — defaults to XLM |
| `"   "` (spaces only) | `"XLM"` | No — defaults to XLM |
| `"xlm"` | `"XLM"` | No |
| `"UsDc"` | `"USDC"` | No |
| `"  NGN  "` | `"NGN"` | No |
| `"XLM1"` | — | `InvalidCurrency (15)` |
| `"US-D"` | — | `InvalidCurrency (15)` |
| `"XLM!"` | — | `InvalidCurrency (15)` |
| `"VERYLONGCURRENCY"` (11+ chars) | — | `InvalidCurrency (15)` |

### 3.3 Legacy Path: `normalize_currency`

A legacy wrapper `normalize_currency()` exists for backward compatibility. It delegates to `validate_and_normalize_currency()` but silently falls back to `"XLM"` on error. **New code must use `validate_and_normalize_currency` directly** to surface errors.

```rust
// bill_payments/src/lib.rs

/// ❌ Legacy — silently falls back to "XLM" on invalid input
fn normalize_currency(env: &Env, currency: &String) -> String { ... }

/// ✅ Preferred — returns Err(InvalidCurrency) for bad input
fn validate_and_normalize_currency(env: &Env, currency: &String) -> Result<String, BillPaymentsError> { ... }
```

---

## 4. How Currencies Are Chosen

### 4.1 At Bill Creation (`create_bill`)

When a bill is created via `create_bill(env, owner, name, amount, due_date, recurring, frequency_days, external_ref, currency)`:

1. The `currency` argument is passed through `validate_and_normalize_currency()`.
2. The normalised currency string is stored in `Bill.currency`.
3. The stored currency is immutable for the lifetime of the bill.

```rust
// bill_payments/src/lib.rs — create_bill entry point
let resolved_currency = Self::validate_and_normalize_currency(&env, &currency)?;

let bill = Bill {
    id:         next_id,
    owner:      owner.clone(),
    name:       validated_name,
    amount,
    due_date,
    recurring,
    frequency_days: interval,
    paid:       false,
    created_at: env.ledger().timestamp(),
    paid_at:    None,
    currency:   resolved_currency,
};
```

### 4.2 At Bill Schedule Creation (`create_bill_schedule`)

Bill schedules follow the same validation path:

```rust
// bill_payments/src/lib.rs — create_bill_schedule entry point
let resolved_currency = Self::validate_and_normalize_currency(&env, &currency)?;
```

### 4.3 At Insurance Policy Creation (`create_policy`)

Insurance policies accept a `premium_currency` string that follows the same canonicalisation rules (uppercase normalisation, length check). The premium is paid in the stored currency.

### 4.4 Recurring Bills Preserve Currency

When a recurring bill generates its child via `pay_bill`, the child inherits the parent's currency verbatim — no re-validation or re-canonicalisation is applied because the parent's currency was already validated at creation time.

---

## 5. How Currencies Are Enforced at Settlement

Every settlement entry point is gated by shared defence-in-depth guards from `remitwise-common`.

### 5.1 Currency Whitelist Guard (`require_matching_settlement_currency`)

```rust
// remitwise-common/src/lib.rs

pub fn require_matching_settlement_currency(
    inv: &soroban_sdk::Vec<Symbol>,
    sym: &Symbol,
) -> Result<(), SettlementCurrencyError> {
    for accepted in inv.iter() {
        if &accepted == sym {
            return Ok(());
        }
    }
    Err(SettlementCurrencyError::CurrencyNotWhitelisted)
}
```

This guard ensures that a settlement's currency is one of the currencies the obligation is willing to accept. It performs a **linear scan** of the whitelist (expected to be a handful of currencies at most).

### 5.2 Threat Model

If a settlement entry point accepts a currency without checking it against the obligation's whitelist:

- An attacker can discharge the full face amount of an obligation while paying in an illiquid, depegged, or low-value asset — cheating the payee and corrupting downstream accounting.
- An empty whitelist **fails closed** (returns `CurrencyNotWhitelisted`), never acting as a wildcard that silently accepts any currency.

### 5.3 Error Variants

| Error | Variant | When |
|-------|---------|------|
| `SettlementCurrencyError` | `CurrencyNotWhitelisted (1)` | Settlement currency is not in the obligation's accepted list |
| `BillPaymentsError` | `InvalidCurrency (15)` | Raw currency string fails validation rules (non-alpha, too long, etc.) |

### 5.4 Complementary Guards

Settlement currency enforcement is always applied **in addition to** — not instead of — other shared settlement guards:

| Guard | Function | What It Rejects |
|-------|----------|-----------------|
| **Anti-negative / anti-zero** | `require_positive_settlement_amount()` | `amount <= 0` |
| **Anti-dust** | `verify_no_dust()` | `amount < 100` stroops |
| **Currency whitelist** | `require_matching_settlement_currency()` | Unlisted settlement currency |

All three must pass for a settlement to proceed.

---

## 6. Where Currencies Appear — Entry Point Matrix

| Contract | Entry Point | Currency Argument | Validation |
|----------|-------------|-------------------|------------|
| **`bill_payments`** | `create_bill` | `currency: String` | `validate_and_normalize_currency` |
| **`bill_payments`** | `create_bill_schedule` | `currency: String` | `validate_and_normalize_currency` |
| **`bill_payments`** | `pay_bill` | None (uses stored `Bill.currency`) | Settler provides matching asset |
| **`bill_payments`** | `get_unpaid_bills_by_currency` | `currency: String` | `normalize_currency` (legacy, query only) |
| **`bill_payments`** | `get_total_unpaid_by_currency` | `currency: String` | `normalize_currency` (legacy, query only) |
| **`insurance`** | `create_policy` | `premium_currency: String` | Upper-cased, length-checked |
| **`insurance`** | `pay_premium` | None (uses stored `Policy.currency`) | Settler provides matching asset |
| **`orchestrator`** | `execute_settlement_flow` | Coordinates multi-contract settlement | Enforces via downstream contracts |

---

## 7. Comparative Matrix Across Contracts

| Contract | Obligation Type | Currency Stored In | Default If Empty | Settled In |
|----------|----------------|---------------------|------------------|------------|
| **`bill_payments`** | `Bill` | `Bill.currency` (String) | `"XLM"` | Settler provides matching asset |
| **`bill_payments`** | `BillSchedule` | Embedded in `Schedule.currency` | `"XLM"` | Bill inherits at creation |
| **`insurance`** | `Policy` | `Policy.premium_currency` | Not defaulted — must be valid | Settler provides matching asset |
| **`savings_goals`** | `Goal` | N/A (goals are currency-agnostic) | N/A | Contribution asset decided at transfer |
| **`remittance_split`** | Split configuration | N/A (USDC-only distribution) | N/A | USDC via Stellar Asset Contract |

---

## 8. Concrete Contributor Example: Creating a Bill with Currency Validation

Below is a complete, compilable Soroban test pattern demonstrating currency validation at bill creation:

```rust
use soroban_sdk::{testutils::Address as _, Address, Env, String};
use bill_payments::{BillPaymentsClient, BillPaymentsError};

#[test]
fn test_settlement_currency_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, bill_payments::BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    let owner = Address::generate(&env);
    let now = 10_000u64;
    env.ledger().with_mut(|li| li.timestamp = now);

    // ── Test 1: Valid currency (USDC, lowercase → normalised) ──
    let bill_id = client.create_bill(
        &owner,
        &String::from_str(&env, "Cloud Hosting"),
        &None,
        &500_000_000,  // 500 USDC (6 decimals)
        &now + 604_800, // 7 days
        &false,
        &0,
        &None,
        &String::from_str(&env, "usdc"),
    );

    let bill = client.get_bill(&bill_id).unwrap();
    assert_eq!(bill.currency, String::from_str(&env, "USDC")); // normalised

    // ── Test 2: Empty currency defaults to XLM ──
    let bill_id_2 = client.create_bill(
        &owner,
        &String::from_str(&env, "Rent"),
        &None,
        &100_000_000,  // 10 XLM (7 decimals)
        &now + 604_800,
        &false,
        &0,
        &None,
        &String::from_str(&env, ""),
    );

    let bill_2 = client.get_bill(&bill_id_2).unwrap();
    assert_eq!(bill_2.currency, String::from_str(&env, "XLM")); // defaulted

    // ── Test 3: Invalid currency with numbers → rejected ──
    let result = client.try_create_bill(
        &owner,
        &String::from_str(&env, "Bad Bill"),
        &None,
        &50_000_000,
        &(now + 604_800),
        &false,
        &0,
        &None,
        &String::from_str(&env, "XLM1"),
    );
    assert_eq!(result, Err(Ok(BillPaymentsError::InvalidCurrency)));
}

#[test]
fn test_settlement_currency_whitelist_guard() {
    use remitwise_common::{
        require_matching_settlement_currency,
        SettlementCurrencyError,
    };
    use soroban_sdk::{symbol_short, Env, Symbol};

    let env = Env::default();

    // Invoice accepts only USDC and EURC
    let whitelist = soroban_sdk::Vec::from_array(
        &env,
        [symbol_short!("USDC"), symbol_short!("EURC")],
    );

    // ── Settlement in USDC (whitelisted) → passes ──
    assert!(require_matching_settlement_currency(
        &whitelist,
        &symbol_short!("USDC"),
    ).is_ok());

    // ── Settlement in XLM (not whitelisted) → rejected ──
    assert_eq!(
        require_matching_settlement_currency(
            &whitelist,
            &symbol_short!("XLM"),
        ),
        Err(SettlementCurrencyError::CurrencyNotWhitelisted),
    );

    // ── Empty whitelist rejects everything ──
    let empty: soroban_sdk::Vec<Symbol> = soroban_sdk::Vec::new(&env);
    assert_eq!(
        require_matching_settlement_currency(&empty, &symbol_short!("XLM")),
        Err(SettlementCurrencyError::CurrencyNotWhitelisted),
    );
}
```

---

## 9. Design Principles

### 9.1 Fail Closed

Every currency guard fails **closed** — an empty whitelist rejects all currencies, an invalid string returns an error, and an unlisted currency cannot settle an obligation. There is no implicit "accept all" wildcard.

### 9.2 Canonicalise Early, Compare Canonical

Currency strings are normalised to uppercase at the **earliest possible point** (bill/policy creation). Downstream comparisons operate on canonical forms, avoiding case-sensitivity bugs.

### 9.3 Default Is Explicit

When no currency is specified, the system defaults to `"XLM"` — the Stellar network's native asset. This default is explicit (`DEFAULT_CURRENCY`) and centralised in the shared `remitwise-common` crate.

### 9.4 Defence in Depth

Currency validation is one layer of a multi-layer defence. Amount validation (`require_positive_settlement_amount`) and dust guards (`verify_no_dust`) operate independently, so a caller cannot bypass currency checks by manipulating other fields.

---

## 10. Adding a New Supported Currency — Checklist

1. [ ] **Add variant** to `SupportedToken` enum in `remitwise-common/src/tokens.rs`.
2. [ ] **Implement metadata** (`decimals`, `base_units_per_unit`, `currency_code`, `from_currency_code` arm).
3. [ ] **Add constants** (e.g., `BASE_UNITS_PER_NEW_TOKEN`, `NEW_TOKEN_DECIMALS`).
4. [ ] **Update downstream matches** — the compiler will force every exhaustive match to handle the new variant.
5. [ ] **Update whitelists** in any bill or invoice that should accept the new currency.
6. [ ] **Add tests** covering:
    - Currency code round-trip via `from_currency_code`.
    - Decimal precision matches specification.
    - The currency can be used in `create_bill` and `create_policy`.
7. [ ] **Update documentation**:
    - This document (§2.1 — Canonical Token Registry).
    - [DECIMAL_CATALOGUE.md](DECIMAL_CATALOGUE.md).
    - Any contract-specific README if the new currency changes default behaviour.

---

## Related Documentation

- [String and Bytes Canonicalisation](CANONICALISATION.md) — §2 Currency codes: trim, uppercase, and default
- [Invoice Settlement Windows Specification](SETTLEMENT_WINDOWS.md) — §6 Shared defence-in-depth settlement guards
- [Token Decimal Catalogue](DECIMAL_CATALOGUE.md) — Reference table of decimals per canonical token
- [Amount Invariants](AMOUNT_INVARIANTS.md) — Zero-handling rules across entry points
- [Dust Policy](DUST_POLICY.md) — Minimum transfer thresholds
- [Currency Validation Implementation — SC-015](../SC-015_CURRENCY_VALIDATION.md) — Original implementation details
- [Contributor Overview](CONTRIBUTOR_OVERVIEW.md) — Onboarding guide for new contributors
- [Event Taxonomy](EVENT_TAXONOMY.md) — Event schema for settlement events
