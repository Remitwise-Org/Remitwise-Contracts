# Currency Support Policy

How the Remitwise protocol decides which currencies to whitelist for
on-chain settlement, billing, and remittance operations.

## Audience

Contributors and integrators who need to understand which currencies are
accepted, how whitelisting works, and how to add a new currency.

---

## Whitelisting levels

The protocol uses two defence-in-depth layers:

| Level | Scope | Enforcement | Location |
|-------|-------|-------------|----------|
| **Supported token registry** | Protocol-wide set of tokens with known decimals | Compile-time enum + match arms | `remitwise-common/src/tokens.rs` |
| **Stable currency allowlist** | Contract-level allowlist of currency codes accepted in bill/invoice flows | Runtime `require_stable_currency()` check | `remitwise-common/src/lib.rs` |

### 1. Supported token registry

The `SupportedToken` enum in `remitwise-common/src/tokens.rs` defines the
canonical set of tokens the protocol natively supports:

| Variant | Discriminant | Currency code | Decimals | Base units per unit |
|---------|-------------|---------------|----------|-------------------|
| `XLM`   | 1           | `"XLM"`       | 7        | 10_000_000        |
| `USDC`  | 2           | `"USDC"`      | 6        | 1_000_000         |
| `EURC`  | 3           | `"EURC"`      | 7        | 10_000_000        |

Each variant implements:
- `decimals()` — returns the number of decimal places
- `base_units_per_unit()` — returns the scaling factor for 1 whole unit
- `currency_code()` — returns the three-to-four-letter ISO-style code as `Symbol`
- `from_currency_code()` — reverse lookup from a `Symbol` to a `SupportedToken`

Adding a new token requires:
1. Adding a new variant to the `SupportedToken` enum
2. Implementing all four methods for the new variant
3. Updating every match arm that exhaustively covers `SupportedToken`

### 2. Stable currency allowlist

The `STABLE_CURRENCIES` constant in `remitwise-common/src/lib.rs` is the
runtime allowlist:

```rust
pub const STABLE_CURRENCIES: &[&str] = &[
    "USDC", "USDT", "USDP", "BUSD", "GUSD", "TUSD", "USDD",
    "EURC", "EURS", "DAI", "XLM",
];
```

**Validation flow** (`require_stable_currency`):

1. Input is trimmed of leading/trailing whitespace
2. Input is uppercased
3. Length is checked against `MAX_CURRENCY_LEN` (10 characters)
4. Each character is verified as ASCII alphabetic
5. The result is matched against `STABLE_CURRENCIES` (case-insensitive)
6. If no match, the call panics with `StableCurrencyError::UnsupportedCurrency`

**Design invariants:**
- **Default currency:** `"XLM"` (lowest friction for new users)
- **Fail closed:** unknown codes are rejected, not silently defaulted
- **Case-insensitive:** `"usdc"`, `"Usdc"`, and `"USDC"` all resolve to USDC
- **Defence in depth:** the allowlist is checked independently of the
  `SupportedToken` enum — a token must pass both checks to be usable

---

## Currency canonicalisation pipeline

The `validate_and_normalize_currency()` function in
`bill_payments/src/lib.rs` applies the full pipeline:

```
raw input
  → trim whitespace
  → uppercase
  → validate characters (ASCII alphabetic only)
  → validate length (≤ MAX_CURRENCY_LEN)
  → require stable currency (allowlist match)
  → return normalized Symbol
```

Entry points that accept currency input:
- `create_bill`
- `create_bill_schedule`

Both call `validate_and_normalize_currency` at the boundary before any
storage writes, ensuring malformed or unsupported currency codes are
rejected early.

---

## Currency index system

`bill_payments` maintains an on-chain currency index for efficient
per-currency queries (`get_bills_by_owner_currency`):

- `get_currency_index()` — returns the current cursor for a currency
- `index_add_currency()` — appends a bill ID to a currency's index
- `index_remove_currency()` — removes a bill ID from a currency's index
- `get_bills_by_owner_currency()` — paginated read of bills filtered by
  currency

The index is kept in sync automatically by `create_bill`, which calls
`validate_and_normalize_currency` followed by `index_add_currency`.

---

## How to add a new currency

1. **Add to `SupportedToken` enum** (`remitwise-common/src/tokens.rs`):
   - Add a new variant with the next available discriminant
   - Implement `decimals()`, `base_units_per_unit()`, `currency_code()`,
     and `from_currency_code()`
   - Update all exhaustive match arms across the workspace

2. **Add to `STABLE_CURRENCIES`** (`remitwise-common/src/lib.rs`):
   - Append the currency code (uppercase, 3-10 characters)

3. **Update documentation:**
   - This file (`CURRENCY_SUPPORT_POLICY.md`)
   - `docs/STABLE_CURRENCIES.md`
   - `docs/token-registry.md`

4. **Write tests:**
   - `require_stable_currency` accepts the new code (case-insensitive)
   - `SupportedToken::from_currency_code` resolves the new code
   - Existing entry points accept the new currency
   - Paginated reads filter correctly by the new currency

---

## Related documentation

| Document | Content |
|----------|---------|
| `docs/STABLE_CURRENCIES.md` | Stable currency allowlist rationale and full list |
| `docs/SETTLEMENT_CURRENCY_POLICY.md` | Per-invoice settlement currency whitelist (planned) |
| `docs/token-registry.md` | Centralized token registry reference |
| `docs/DECIMAL_CATALOGUE.md` | Token decimal catalogue |
| `docs/CANONICALISATION.md` | Currency trim/uppercase canonicalisation |
| `docs/whitelisting-test-coverage.md` | Test coverage for whitelisting |
| `docs/bill-payments-unpaid-by-currency-pagination.md` | Currency-indexed pagination |
| `SC-015_CURRENCY_VALIDATION.md` | Original currency validation implementation doc |
| `remitwise-common/src/tokens.rs` | `SupportedToken` enum source |
| `remitwise-common/src/lib.rs` | `STABLE_CURRENCIES`, `require_stable_currency` source |
| `bill_payments/src/lib.rs` | `validate_and_normalize_currency` source |
