# Token Decimal Catalogue

This document serves as a reference for the expected decimal precision of canonical tokens used across the RemitWise ecosystem. 

By standardizing this information, we ensure consistent parsing, formatting, and mathematical operations across all Soroban contracts and off-chain indexing services.

## Audience: Integrators and Contributors

If you are writing a new contract or building a frontend that interacts with RemitWise, use these decimal values to properly shift amounts (e.g., multiplying by `10^decimals` before sending to a contract).

## Canonical Tokens

All canonical Stellar assets wrapped for Soroban use 7 decimals by default.

| Token Symbol | Asset Name | Decimals | Example (1.00 unit in on-chain integer) |
|--------------|------------|----------|-----------------------------------------|
| **XLM**      | Stellar Lumens | 7        | `10_000_000` (1 XLM) |
| **USDC**     | USD Coin | 7        | `10_000_000` (1 USDC) |
| **EURC**     | Euro Coin | 7        | `10_000_000` (1 EURC) |
| **NGN**      | Nigerian Naira | 7        | `10_000_000` (1 NGN) |
| **GBP**      | British Pound | 7        | `10_000_000` (1 GBP) |
| **JPY**      | Japanese Yen | 7        | `10_000_000` (1 JPY) |

## Example Usage (Rust)

When invoking a token transfer in a test or writing an integration, remember to scale the amount:

```rust
// 50 USDC (with 7 decimals)
let amount_usdc: i128 = 50 * 10_000_000; 

// Initializing a goal for 1,000 XLM
let target_amount: i128 = 1_000 * 10_000_000; 
```

*Note: The RemitWise contracts expect all token amounts to be passed as fully scaled integers (`i128`). Do not pass floating-point numbers or unscaled integers.*
