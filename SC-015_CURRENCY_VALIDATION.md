# Currency Validation Implementation - SC-015 (Issue #1420)

## Summary
Strict currency code validation and normalization for the Bill Payments contract. 
Prevents inconsistent data, reduces off-chain parsing risk, and ensures only 
supported stable assets can be used as bill currencies.

## Changes Made

### 1. Currency Validation & Normalization (`bill_payments/src/lib.rs`)
- **`is_valid_currency_chars`**: Validates that currency strings contain only ASCII alphabetic characters (first-pass sanity check)
- **`validate_and_normalize_currency`**: Full validation pipeline:
  - Empty strings → default to `"XLM"`
  - Strings > `MAX_CURRENCY_LEN` (10) → `InvalidCurrency` error
  - Whitespace trimmed (leading/trailing)
  - Non-alphabetic characters → `InvalidCurrency` error
  - Whitespace-only strings → default to `"XLM"`
  - Valid strings → normalized to uppercase
  - Final check: validated against `STABLE_CURRENCIES` allowlist → `UnsupportedCurrency` if not recognized
- **`normalize_currency`**: Legacy backward-compatible helper (silent fallback to XLM on error)

### 2. Error Codes (`BillPaymentsError`)
- `InvalidCurrency = 11`: Currency code is invalid (too long, wrong characters)
- `UnsupportedCurrency = 12`: Currency not in the stable asset allowlist
- Full error enum expanded from 9 to 33 variants to support all contract operations

### 3. New Types
- **`ArchivedBill`**: Cold-storage bill representation with archival timestamp
- **`ArchivedBillPage`**: Paginated result for archived bill queries with `first()` helper

### 4. Wallet-Friendly Query Methods
- **`get_total_unpaid_by_currency(owner, currency)`**: Total unpaid amount filtered by currency
- **`get_unpaid_bills_by_currency(owner, currency, cursor, limit)`**: Paginated unpaid bills filtered by currency

### 5. Integration
- `create_bill` and `create_bill_schedule` entrypoints use `validate_and_normalize_currency` for strict validation
- Read-only query functions (`get_bills_by_currency`, `get_unpaid_bills_by_currency`, `get_total_unpaid_by_currency`) use `normalize_currency` for lenient normalization

## Supported Currencies
The stable currency allowlist (`STABLE_CURRENCIES`):
`USDC`, `USDT`, `USDP`, `BUSD`, `GUSD`, `TUSD`, `USDD`, `EURC`, `EURS`, `DAI`, `XLM`

## Test Coverage (121 tests, all passing)
### New Currency Validation Tests (6 tests):
- `test_currency_valid_xlm`: Valid XLM currency
- `test_currency_empty_defaults_to_xlm`: Empty string defaults to XLM
- `test_currency_lowercase_normalized`: Lowercase → uppercase normalization
- `test_currency_invalid_with_numbers`: Numbers rejected (InvalidCurrency)
- `test_currency_invalid_too_long`: Too-long codes rejected (InvalidCurrency)
- `test_currency_unsupported_rejected`: Non-whitelisted currency rejected (UnsupportedCurrency)

## Validation Rules Summary

| Input | Output | Error |
|-------|--------|-------|
| "" (empty) | "XLM" | No |
| "   " (spaces only) | "XLM" | No |
| "xlm" | "XLM" | No |
| "UsDc" | "USDC" | No |
| "  XLM  " | "XLM" | No |
| "XLM1" | - | InvalidCurrency |
| "XLM!" | - | InvalidCurrency |
| "US-D" | - | InvalidCurrency |
| "VERYLONGCURRENCYCODE" | - | InvalidCurrency |
| "NGN" | - | UnsupportedCurrency |

## Backward Compatibility
- Legacy `normalize_currency` function preserved for query operations
- `DEFAULT_CURRENCY = "XLM"` for existing bills without explicit currency
- `MAX_CURRENCY_LEN = 10` maintained from `remitwise-common`

## How to Test
```bash
# Run all bill_payments tests
cargo test -p bill_payments --lib

# Run only currency validation tests
cargo test -p bill_payments --lib currency
```
