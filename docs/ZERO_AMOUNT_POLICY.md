# Zero-Amount Policy

> **Audience:** downstream integrators — wallet UI, bot, or backend calling RemitWise contracts.
> For the full per-entrypoint breakdown see [AMOUNT_INVARIANTS.md](AMOUNT_INVARIANTS.md).

## TL;DR

| Behavior | When | Example entrypoint |
|---|---|---|
| **Reject** | Most value-moving entrypoints | `create_bill`, `pay_bill`, `execute_remittance_flow` |
| **Accept** | `savings_goals::add_to_goal` only | `add_to_goal(amount: 0)` succeeds as a no-op |
| **Normalize** | Fan-out allocations; `spending_limit` sentinel | Zero leg skipped in `run_remittance_fan_out`; `spending_limit = 0` means unlimited |

## Default Rule

If an entrypoint moves or schedules value, treat `amount = 0` as **rejected** unless this document says otherwise.

## Known Exceptions

### `savings_goals::add_to_goal` — Accepts zero
Calling `add_to_goal` with `amount = 0` succeeds and emits an event with `amount = 0`. It is a no-op that moves no funds. Guard against this if your intent is to skip zero contributions.

```rust
// This succeeds — no funds move, event is emitted
client.add_to_goal(&user, &goal_id, &0i128);
```

### `orchestrator::run_remittance_fan_out` — Normalizes zero allocations
The top-level `total_amount` must be `> 0`. After splitting, any leg with a zero allocation is silently skipped — the downstream contract is never called for that leg.

```rust
// Zero savings allocation → savings_goals::add_to_goal is never called
// Zero bills allocation  → bill_payments::pay_bill is never called
```

### `family_wallet` spending limit — Zero means unlimited
`spending_limit = 0` on a member record is a sentinel for "no cap", not "blocked". `check_spending_limit` always returns `true` when `spending_limit == 0`.

## All Amounts Are i128 Stroops

All `amount` parameters are `i128` stroops (1 XLM = 10,000,000 stroops). There is no shared amount type — each contract validates independently.

## Related Docs

- [AMOUNT_INVARIANTS.md](AMOUNT_INVARIANTS.md) — full per-entrypoint table
- [Remittance Split Rounding & Dust Policy](remittance-split-rounding-policy.md)
- [AUTHORIZATION_MATRIX.md](AUTHORIZATION_MATRIX.md)
