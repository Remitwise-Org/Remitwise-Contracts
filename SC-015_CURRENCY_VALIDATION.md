# Currency Validation Implementation - SC-015

## Summary
This document describes the implementation of strict currency code validation for the Bill Payments contract.

## Changes Made

### 1. Added Constants ([lib.rs:27-28](bill_payments/src/lib.rs#L27-L28))
```rust
/// Maximum length for currency codes (ISO 4217 is 3 letters)
const MAX_CURRENCY_LEN: u32 = 10;
```

### 2. Added Validation Helper ([lib.rs:30-33](bill_payments/src/lib.rs#L30-L33))
```rust
/// Validates that a currency string contains only ASCII alphabetic characters.
/// Returns true if the string is valid (all ASCII letters A-Z or a-z).
fn is_valid_currency_chars(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(|&b| b.is_ascii_alphabetic())
}
```

### 3. New Validation Function ([lib.rs:177-232](bill_payments/src/lib.rs#L177-L232))
- **Name**: `validate_and_normalize_currency`
- **Validation Rules**:
  - Empty strings → default to "XLM"
  - Strings longer than `MAX_CURRENCY_LEN` (10) → `InvalidCurrency` error
  - Non-alphabetic characters (numbers, symbols, spaces) → `InvalidCurrency` error
  - Whitespace-only strings → default to "XLM"
  - Valid strings → normalized to uppercase

### 4. Updated create_bill Function ([lib.rs:590-591](bill_payments/src/lib.rs#L590-L591))
Changed from:
```rust
let resolved_currency = Self::normalize_currency(&env, &currency);
```
To:
```rust
let resolved_currency = Self::validate_and_normalize_currency(&env, &currency)?;
```

### 5. Added Comprehensive Tests ([test.rs:2860-2998](bill_payments/src/test.rs#L2860-L2998))
- `test_create_bill_valid_currency_xlm` - Valid XLM
- `test_create_bill_valid_currency_usdc` - Valid USDC
- `test_create_bill_valid_currency_ngn` - Valid NGN
- `test_create_bill_currency_lowercase_normalized` - lowercase → uppercase
- `test_create_bill_currency_mixed_case_normalized` - mixed case → uppercase
- `test_create_bill_currency_with_whitespace` - whitespace trimmed
- `test_create_bill_empty_currency_defaults_to_xlm` - empty → XLM default
- `test_create_bill_invalid_currency_with_numbers` - numbers rejected
- `test_create_bill_invalid_currency_with_special_chars` - special chars rejected
- `test_create_bill_invalid_currency_with_dash` - dashes rejected
- `test_create_bill_invalid_currency_too_long` - too long rejected
- `test_create_bill_invalid_currency_only_spaces` - spaces → XLM default
- `test_create_bill_valid_currency_eur` - EUR test
- `test_create_bill_valid_currency_gbp` - GBP test
- `test_create_bill_valid_currency_jpy` - JPY test
- `test_recurring_bill_preserves_currency` - currency preserved in recurring
- `test_create_bill_currency_with_leading_spaces` - leading spaces trimmed
- `test_create_bill_currency_with_trailing_spaces` - trailing spaces trimmed

## Test Coverage
18 new currency validation tests added, covering:
- ✅ Valid currency codes (XLM, USDC, NGN, EUR, GBP, JPY)
- ✅ Case normalization (lowercase, mixed case)
- ✅ Whitespace handling (leading, trailing, only spaces)
- ✅ Empty string defaults
- ✅ Invalid characters (numbers, special chars, dashes)
- ✅ Length validation (too long)
- ✅ Recurring bill currency preservation

## Error Code
The existing `BillPaymentsError::InvalidCurrency = 15` is used for validation failures.

## How to Test

### Step 1: Install Prerequisites
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli
```

### Step 2: Run Tests
```bash
cd /home/semi/Documents/d/Remitwise-Contracts

# Run all bill_payments tests
cargo test -p bill_payments

# Run only currency validation tests
cargo test -p bill_payments currency
```

### Step 3: Verify Coverage
```bash
# Run with coverage (requires tarpaulin)
cargo tarpaulin -p bill_payments --output-format html
```

### Step 4: Expected Results
All 18 new tests should pass:
- 16 tests for valid currencies (pass)
- 5 tests for invalid currencies (return `InvalidCurrency` error)

### Step 5: Integration Test
```bash
# Run full test suite
cargo test

# Run integration tests
cargo test -p integration_tests
```

## Validation Rules Summary
| Input | Output | Error? |
|-------|--------|--------|
| "" (empty) | "XLM" | No |
| "   " (spaces only) | "XLM" | No |
| "xlm" | "XLM" | No |
| "UsDc" | "USDC" | No |
| "  XLM  " | "XLM" | No |
| "XLM1" | - | Yes |
| "XLM!" | - | Yes |
| "US-D" | - | Yes |
| "VERYLONGCURRENCYCODE" | - | Yes |

## Backward Compatibility
The legacy `normalize_currency` function is preserved for backward compatibility but now delegates to the validation function with error handling.