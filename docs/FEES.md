# Fees — Computation and Storage

> **Audience:** Contributors touching `remittance_split` (or anything that reads its configuration) who need to know exactly what "fee" means in this codebase before changing it.
> **Goal:** There are two unrelated things in this repo that both get called a "fee." Confusing them is an easy, expensive mistake — this doc pins down what each one is, where it's stored, and exactly how it's computed.

## The two "fees" — do not conflate them

| | Corridor fee | "Fee schedule" (split percentages) |
|---|---|---|
| Type | `remittance_split::Corridor.fee_bps` | `remittance_split::SplitConfig.{spending,savings,bills,insurance}_percent` |
| What it actually is | A per-corridor fee rate, in basis points | A four-way redistribution of the *full* amount — nothing is removed |
| Storage key | `CRIDORS` (instance storage, `Vec<Corridor>`) | `CONFIG` (instance storage, part of `SplitConfig`) |
| Set via | `init_corridors(caller, corridors)` | `initialize_split` / the split-update entrypoint |
| Read via | `get_corridors(env) -> Vec<Corridor>` | `get_split(env) -> Vec<u32>`, `get_config(env) -> Option<SplitConfig>` |
| Applied where | Computable via `Corridor::fee_for(amount)` — **see the gap below** | `calculate_split_amounts` / `distribute_usdc` (the actual money-movement path) |

The second one is called a "fee schedule" in `orchestrator::get_fee_schedule` (`orchestrator/src/lib.rs`), whose doc comment literally says "Get the current fee schedule (split percentages)". Despite the name, calling `get_fee_schedule()` returns `(spending_percent, savings_percent, bills_percent, insurance_percent)` — the split, not a fee. No basis points are deducted from the total for this; every unit of the incoming amount ends up in one of the four buckets.

## Corridor fees

Defined in `remittance_split/src/lib.rs`:

```rust
pub struct Corridor {
    pub id: u32,
    pub source_currency: Symbol,
    pub dest_currency: Symbol,
    pub min_amount: i128,
    pub max_amount: i128,
    pub fee_bps: u32,          // 1 bps = 0.01%
}
```

- **Bound:** `fee_bps` must be `<= params::MAX_FEE_BPS` (`remittance_split/src/params.rs`, currently `1_000` bps = 10%). Enforced in `validate_corridors`, called from `init_corridors`; a corridor exceeding this is rejected with `RemittanceSplitError::CorridorFeeTooHigh` before it's ever stored.
- **Computation:** `Corridor::fee_for(amount)` computes `floor(amount * fee_bps / 10_000)` via `remitwise_common::Rate::from_bps(fee_bps).apply_to(amount)` — one multiply-then-divide step with checked `i128` arithmetic, so it can't silently overflow or wrap.

  Concrete example (from the source doc comment on `fee_for`): a 5.5% corridor (`fee_bps = 550`) on an amount of 200 stroops charges exactly `floor(200 * 550 / 10_000) = 11` stroops — not 10. Before issue #1612, the pipeline went through `Rate::to_percent()` first, which truncates 550 bps down to a whole 5%, undercharging by a stroop. Fractional-bps corridors used to be rejected outright at validation time for this reason (`FeeRounding`); that ban is no longer needed now that `fee_for` computes the exact bps rate directly.

- **⚠️ Current gap:** `Corridor::fee_for` is not called anywhere in `calculate_split_amounts` or `distribute_usdc` — the actual fund-distribution path. Corridors are validated and stored, and `fee_for` is available as a pure computation (e.g. for an off-chain quote), but nothing in this contract currently deducts a corridor fee from a real transfer. Do not assume a configured `fee_bps` is being collected on-chain today — verify against `distribute_usdc` directly if this matters for your change.

## Split percentages (the "fee schedule")

Stored as four `u32` fields on `SplitConfig`, itself stored under the `CONFIG` instance-storage key. Defaults to `[5000, 3000, 1500, 500]` (50% / 30% / 15% / 5%) if `CONFIG` has never been set (`get_split`'s fallback).

`calculate_split_amounts` (private, called by `calculate_split` and `distribute_usdc`) computes:

```rust
spending  = total_amount * spending_percent  / 10_000
savings   = total_amount * savings_percent   / 10_000
bills     = total_amount * bills_percent     / 10_000
insurance = total_amount - spending - savings - bills   // remainder, not its own multiply
```

`insurance` is deliberately the remainder of the other three rather than its own `total * insurance_percent / 10_000` — that's what guarantees `spending + savings + bills + insurance == total_amount` exactly, with no stroop lost to three independent roundings landing the same direction. See [docs/CROSS_CONTRACT_INVARIANTS.md](./CROSS_CONTRACT_INVARIANTS.md) for the conservation invariant this upholds.

## Cross-references

- [remittance_split/README.md](../remittance_split/README.md) — the contract's full API.
- [docs/CROSS_CONTRACT_INVARIANTS.md](./CROSS_CONTRACT_INVARIANTS.md) — split conservation and other cross-contract invariants.
- [docs/AMOUNT_INVARIANTS.md](./AMOUNT_INVARIANTS.md) — amount zero-handling rules referenced by both fee paths.
