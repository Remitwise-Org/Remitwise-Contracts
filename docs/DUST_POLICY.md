# Dust Policy and Minimum Transfers

**Target Audience:** Contributors & Maintainers

This document outlines the rationale and implementation of the "Dust Policy" (minimum transfer amounts) across the Remitwise smart contracts.

## 1. Rationale

In the Stellar and Soroban ecosystem, processing extremely small fractional amounts of an asset—commonly referred to as "dust"—can lead to several issues:
1. **Network Bloat:** Micro-transactions cost network fees and bloat ledger state without providing economic value.
2. **Limit Evasion:** Bad actors can spam micro-transactions to artificially bypass rate limit counts or obscure the true volume of spending.
3. **Precision Errors:** Complex rounding in the `remittance_split` can result in dust remainder assignments (which are deterministically swept to the `insurance` allocation).

To mitigate this, the Remitwise contracts enforce minimum transfer bounds.

## 2. Implementation: `min_precision` vs `MIN_TRANSFER`

While conceptually referred to as `MIN_TRANSFER`, the codebase currently enforces this bound via the `min_precision` field on a per-wallet basis within the `family_wallet` crate. 

**There is no single global `MIN_TRANSFER` constant or global token-override map.** Instead, dust limits are evaluated contextually based on the active `SpendingLimit` for the caller.

### The `SpendingLimit` Struct
Within `family_wallet/src/lib.rs`, the `SpendingLimit` contains the `min_precision` parameter (in stroops):
```rust
pub struct SpendingLimit {
    // ...
    pub min_precision: i128, // e.g., 1_0000000 (1 XLM minimum)
}
```
Any transaction with an `amount` less than `min_precision` is rejected with `Error::AmountBelowPrecision`.

## 3. How to Update the Dust Limit

To update the minimum transfer amount (dust policy) for a specific member, a Family Wallet Admin must update that member's spending limit.

### Concrete Example
To set a minimum transfer limit of `0.5 XLM` (5,000,000 stroops) for a specific member, invoke the `update_spending_limit` entrypoint:

```bash
soroban contract invoke \
  --id C_WALLET_ID \
  --source admin_account \
  --network testnet \
  -- \
  update_spending_limit \
  --caller admin_account \
  --member member_account \
  --daily_limit 1000000000 \
  --monthly_limit 5000000000 \
  --max_single_tx 500000000 \
  --min_precision 5000000
```

*Expected Result:* The storage is updated, and any future transfer by `member_account` below `5000000` stroops will immediately revert, neutralizing dust attacks.

## 4. Future Roadmap: Per-Token Overrides

Currently, `min_precision` treats all underlying assets agnostically by their raw integer values. In the future, a dedicated `MIN_TRANSFER` oracle or mapping may be introduced to allow *per-token* dust overrides (e.g., `1 USDC` vs `1 XLM`), ensuring precision limits scale accurately according to the specific asset's market value.
