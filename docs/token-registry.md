# Token Registry

> Single source of truth for every token the Remitwise platform supports.

## Motivation

Token metadata (decimals, stroop multipliers, currency codes) was previously
scattered across contracts as magic numbers and string literals. Adding a new
token required editing multiple files with no compiler guarantee that every
consumer was updated.

The `SupportedToken` enum in `remitwise-common/src/tokens.rs` centralises this.
Adding a new token means adding one enum variant; the Rust compiler then forces
every exhaustive `match` in the workspace to handle it.

## Supported tokens

| Variant   | Currency code | Decimals | Minor units per major unit |
|-----------|---------------|----------|---------------------------|
| `XLM`     | `"XLM"`       | 7        | 10 000 000 (stroops)      |
| `USDC`    | `"USDC"`      | 6        | 1 000 000                 |
| `EURC`    | `"EURC"`      | 7        | 10 000 000                |

## How to add a new token

1. Add a new `#[repr(u32)]` variant **at the end** of `SupportedToken` in
   `remitwise-common/src/tokens.rs`.
2. Implement the four metadata methods (`decimals`, `base_units_per_unit`,
   `currency_code`, `from_currency_code`) for the new variant.
3. Add the corresponding `*_DECIMALS` and `BASE_UNITS_PER_*` constants.
4. The compiler will produce errors for every `match` in the workspace that
   does not yet handle the new variant — fix each one.
5. Add tests in `remitwise-common/src/tokens.rs` for the new token.
6. Update this document.

## Constants exported from `remitwise_common`

| Constant              | Value         | Description                         |
|-----------------------|---------------|-------------------------------------|
| `XLM_DECIMALS`        | `7`           | Minor-unit exponent for XLM         |
| `USDC_DECIMALS`       | `6`           | Minor-unit exponent for USDC        |
| `EURC_DECIMALS`       | `7`           | Minor-unit exponent for EURC        |
| `STROOPS_PER_XLM`     | `10_000_000`  | Minor units per XLM major unit      |
| `BASE_UNITS_PER_USDC` | `1_000_000`   | Minor units per USDC major unit     |
| `BASE_UNITS_PER_EURC` | `10_000_000`  | Minor units per EURC major unit     |
| `DEFAULT_CURRENCY`    | `"XLM"`       | Fallback currency when none given   |
| `MAX_CURRENCY_LEN`    | `10`          | Max byte length of currency strings |

## Usage in contracts

### bill_payments

Uses `DEFAULT_CURRENCY` and `MAX_CURRENCY_LEN` from `remitwise_common` instead
of local constants. Currency validation normalises to uppercase and falls back
to `DEFAULT_CURRENCY` for empty/whitespace-only input.

### family_wallet

Uses `STROOPS_PER_XLM` to define named default limits:

```rust
const DEFAULT_MULTISIG_SPENDING_LIMIT: i128 = 1_000 * STROOPS_PER_XLM;
const DEFAULT_EMERGENCY_MAX_AMOUNT: i128 = 10_000 * STROOPS_PER_XLM;
const DEFAULT_EMERGENCY_DAILY_LIMIT: i128 = 100_000 * STROOPS_PER_XLM;
```

### insurance

All premium/coverage bounds are expressed in stroops. The doc comments now
reference `remitwise_common::STROOPS_PER_XLM` as the canonical source.

## Migration guide (for existing contracts)

| Before                                    | After                                      |
|-------------------------------------------|--------------------------------------------|
| `const MAX_CURRENCY_LEN: u32 = 10;`      | `use remitwise_common::MAX_CURRENCY_LEN;`  |
| `String::from_str(env, "XLM")`           | `String::from_str(env, DEFAULT_CURRENCY)`  |
| `1000_0000000` (magic number)             | `1_000 * STROOPS_PER_XLM`                 |
| `10000_0000000` (magic number)            | `10_000 * STROOPS_PER_XLM`                |
| `100000_0000000` (magic number)           | `100_000 * STROOPS_PER_XLM`               |

No breaking ABI changes — all constants are compile-time only.
