# Type-Safe Percent → Basis-Points Conversion

## Overview

`remitwise-common` provides a type-safe conversion model for converting whole percentages into basis-points representation (`Rate`).

In the Remitwise platform:
- Whole percentages are expressed as `1% = 100 basis points`.
- 100% is equivalent to `10,000 basis points` (`BASIS_POINTS`).
- 1 basis point (bps) represents `0.01%`.

Previously, operators, downstream contracts, and frontend tooling often performed manual unchecked arithmetic (`percent * 100`), which risked scale confusion or integer overflow when manipulating rates. The `Percent` struct and `Rate` conversion methods eliminate these manual workarounds.

---

## Canonical Constants

Defined in `remitwise-common/src/lib.rs`:

| Constant | Value | Description |
| --- | --- | --- |
| `BASIS_POINTS` | `10_000` | Total basis points representing 100%. |
| `BPS_PER_PERCENT` | `100` | Basis points in 1 percentage point (1% = 100 bps). |
| `BASIS_POINTS_PER_PERCENT` | `100` | Alias for `BPS_PER_PERCENT`. |

---

## Types and Methods

### `Percent`

`#[contracttype]` newtype wrapping a `u32` integer representing whole percentages.

```rust
use remitwise_common::{Percent, Rate, RateError};

// Construction
let p = Percent::from_percentage(5); // 5%
assert_eq!(p.to_percentage(), 5);

// Checked conversion to Rate or raw basis points
let rate: Result<Rate, RateError> = p.to_rate(); // Ok(Rate(500))
let bps: Result<u32, RateError> = p.to_bps();   // Ok(500)
```

`TryFrom<Percent>` is also implemented for `Rate`:

```rust
let rate: Rate = Percent::from_percentage(50).try_into()?; // 5000 bps
```

### `Rate` Enhancements

`Rate` supports direct percent conversion helpers:

```rust
use remitwise_common::{Rate, RateError};

// Construct Rate directly from whole percentage (u32)
let rate = Rate::from_percent(15)?; // Ok(Rate(1500))

// Convert Rate back to whole percentage (truncates fractional bps)
assert_eq!(rate.to_percent(), 15);

// Check if rate contains fractional percentage (e.g. 550 bps = 5.5%)
assert_eq!(rate.has_fractional_percent(), false);
let fractional = Rate::from_bps(550);
assert_eq!(fractional.has_fractional_percent(), true);
```

---

## Safety & Invariants

1. **Checked Arithmetic**: All conversions check for multiplication overflow against `u32::MAX`. Values exceeding `u32::MAX / 100` return `Err(RateError::Overflow)`.
2. **Backwards Compatibility**: Existing `Rate::from_bps(u32)` and `BASIS_POINTS` remain unchanged and fully supported.
3. **`no_std` Compliant**: Safe for inclusion across all WASM target Soroban smart contracts.

---

## Testing & Verification

Comprehensive unit tests and property-based tests are locked in `remitwise-common/src/tests.rs`:

```bash
cargo test -p remitwise-common test_rate_from_percent
cargo test -p remitwise-common test_percent_type_conversions
cargo test -p remitwise-common proptest_percent_rate_roundtrip
```
