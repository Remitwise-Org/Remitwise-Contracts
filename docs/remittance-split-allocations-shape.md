# `get_split_allocations` Shape Contract

## Purpose

`get_split_allocations(env, total_amount)` is a public read-only entrypoint used
by the `reporting` and `orchestrator` contracts as ground truth for per-category
USDC amounts. This document pins the shape contract so downstream consumers can
rely on it without re-reading the implementation.

## Return Shape

On `Ok`, the function **always returns a `Vec<Allocation>` of length 4**, ordered:

| Index | `category` symbol | Percentage source |
|-------|-------------------|-------------------|
| 0 | `SPENDING` | `split[0]` (floor division) |
| 1 | `SAVINGS` | `split[1]` (floor division) |
| 2 | `BILLS` | `split[2]` (floor division) |
| 3 | `INSURANCE` | remainder (dust receiver) |

The ordering is **deterministic and fixed** — it matches the order returned by
`get_split` and the fields in `SplitConfig`.

## Uninitialized Contract (before `initialize_split`)

`get_split` returns the compile-time default `[50, 30, 15, 5]` when no `SPLIT`
key exists in instance storage. Therefore `get_split_allocations` on an
uninitialized contract:

- Does **not** panic.
- Returns the same 4-entry `Vec<Allocation>` using the default percentages.
- Satisfies the conservation invariant using the same dust-to-insurance rule.

## Zero-Amount Input

`total_amount <= 0` returns `Err(RemittanceSplitError::InvalidAmount)` **before**
any allocation is computed. Callers must not pass zero or negative amounts.

## Conservation Invariant

For every valid call (`total_amount > 0`):

```
allocations[0].amount + allocations[1].amount
  + allocations[2].amount + allocations[3].amount == total_amount
```

This is structural (insurance is defined as `total_amount − spending − savings − bills`)
and holds regardless of the configured percentages, including edge configs such as
a single 100% slot.

## Edge Configurations

| Config (sp/sv/bl/ins) | Expected shape |
|-----------------------|----------------|
| Default (50/30/15/5)  | 4 allocations, sum == total |
| Single 100% spending (100/0/0/0) | 4 allocations; `savings=0`, `bills=0`, `insurance=0`; sum == total |
| All-zero except insurance (0/0/0/100) | `spending=0`, `savings=0`, `bills=0`, `insurance=total`; sum == total |
| Large amount (`i128::MAX / 2`) | No overflow; sum == total |

## Overflow

Intermediate `checked_mul(pct)` on `total_amount` can overflow `i128` for very
large amounts (e.g. `i128::MAX` with non-zero percentages). The function returns
`Err(RemittanceSplitError::Overflow)` in that case. `i128::MAX / 2` with any
valid percentage configuration does **not** overflow.

## Related Documents

- [`remittance-split-rounding-policy.md`](remittance-split-rounding-policy.md) —
  full dust/remainder algorithm and conservation proof.
