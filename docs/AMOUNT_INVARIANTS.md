# Amount Invariants: Zero-Handling Across Entrypoints

## Audience

This document is for **downstream integrators** — teams building a wallet UI,
bot, or backend service that calls these contracts. If you are wiring up a
call to `add_to_goal`, `pay_bill`, `execute_remittance_flow_signed`, or similar,
this tells you whether an amount of `0` will revert, silently succeed as a
no-op, or be treated as a special sentinel value. Getting this wrong either
produces an unexpected `InvalidAmount`/`InvalidPremium` revert in production,
or — worse — a transaction that submits successfully but moves no funds
because the amount was normalized away.

All amounts in this codebase are `i128` stroops (1 XLM = `10_000_000` stroops).
There is no separate "amount type" shared across crates — each contract
validates its own `i128` parameters independently, so the rules below are
per-entrypoint, not global.

## The Three Behaviors

| Behavior | What it means for you |
|---|---|
| **Reject** | Calling with `amount == 0` returns/panics with an error. Your call fails, no state changes, no event emitted. |
| **Accept** | Calling with `amount == 0` succeeds like any other value. State changes and an event is emitted with `amount = 0`. |
| **Normalize** | Zero is treated as a sentinel with different meaning than "zero units" (e.g. "unlimited"), or a zero value in a batch/derived calculation is silently skipped rather than acted on or rejected. |

## Sites That Reject Zero

The large majority of entrypoints that move value validate `amount <= 0` and
return `InvalidAmount` (or a contract-specific equivalent) before doing
anything else. Zero is treated the same as a negative amount here — both are
rejected.

| Crate | Entrypoint | Guard | Error |
|---|---|---|---|
| `remittance_split` | `execute_remittance_flow`, `calculate_split`, `submit_remittance_request` | `total_amount <= 0` | `RemittanceSplitError::InvalidAmount` |
| `orchestrator` | `execute_remittance_flow`, `execute_remittance_flow_signed`, `execute_flow_fanout` | `amount <= 0` / `params.total_amount <= 0` | `OrchestratorError::InvalidAmount` |
| `bill_payments` | `create_bill`, `pay_bill` | `amount <= 0` | `BillPaymentsError::InvalidAmount` |
| `insurance` | `create_policy` (`monthly_premium`, `coverage_amount`), `create_premium_schedule`, `modify_premium_schedule` | `<= 0` | `InsuranceError::InvalidPremium` |
| `family_wallet` | `validate_precision_spending` | `amount <= 0` | `Error::InvalidAmount` |
| `savings_goals` | `create_goal` (`target_amount`), `batch_contribute` (each item's `amount`) | `<= 0` | `SavingsGoalError::InvalidAmount` / `TargetAmountMustBePositive` |

**Concrete example** — `bill_payments::create_bill`:

```rust
// amount is in stroops; 0 is rejected the same as a negative value
let bill_id = client.create_bill(
    &owner,
    &String::from_str(&env, "Electric bill"),
    &0i128,               // amount
    &due_date,
    &false,
    &0u32,
    &None,
    &String::from_str(&env, "XLM"),
    &None,
);
// => Err(BillPaymentsError::InvalidAmount)
```

The equivalent call with `&100_0000000i128` (100 XLM) succeeds and returns a
new bill ID.

## Sites That Accept Zero

`savings_goals::add_to_goal` is the one notable exception in the "moves value"
category. Its guard is `amount < 0`, not `amount <= 0`:

```rust
// savings_goals/src/lib.rs
if amount < 0 {
    Self::append_audit(&env, symbol_short!("add"), &caller, false);
    return Err(SavingsGoalError::InvalidAmount);
}
```

So `add_to_goal(caller, goal_id, 0)` succeeds: it returns `Ok(new_total)`
where `new_total` is unchanged from the previous balance, and it does not
error. **This is inconsistent with the function's own doc comment**, which
states the amount "must be `> 0`" — the accepted behavior is `>= 0`. Treat the
code (not the comment) as the source of truth until the doc comment is
corrected; if you're calling this entrypoint programmatically, a `0` will not
raise an error, so guard against sending it if your intent was "no-op, skip
this call."

`family_wallet::check_spending_limit` is a read-only predicate, not a
value-moving entrypoint, but it's worth knowing about too: it rejects
negative amounts (`amount < 0` → `false`) but a `0` amount always returns
`true` (spending nothing is always "within limit").

## Sites That Normalize Zero

`orchestrator::run_remittance_fan_out` treats a zero *allocation* differently
from a zero *amount at the entrypoint*. The top-level `total_amount` must
still be `> 0` (see the reject table above), but once
`remittance_split::calculate_split` divides that amount into
spending/savings/bills/insurance, each of the three downstream legs is only
invoked if its allocation is strictly positive:

```rust
// orchestrator/src/lib.rs
if savings_amt > 0 {
    // ... calls savings_goals::add_to_goal
}
if bills_amt > 0 {
    // ... calls bill_payments::pay_bill
}
if insurance_amt > 0 {
    // ... calls insurance::pay_premium
}
```

A `0` allocation is neither an error nor a call with `amount = 0` — the
downstream contract is never invoked for that leg at all. Negative allocations
(which should be unreachable given the split math, but are defended against
anyway) still return `InvalidAmount`. See
[Remittance Split Rounding & Dust Policy](remittance-split-rounding-policy.md)
for how allocations are computed and why insurance absorbs the rounding
remainder.

Separately, `family_wallet` overloads `0` on the *limit* side rather than the
amount side: a member's `spending_limit == 0` is normalized to mean
"unlimited," not "blocked." This isn't an amount invariant, but it's a
zero-as-sentinel pattern in the same family and worth knowing if you're
reasoning about `check_spending_limit`.

## Quick Reference

- **Default assumption:** if you're calling an entrypoint that directly moves
  or schedules value (`create_bill`, `pay_bill`, `create_policy`,
  `create_premium_schedule`, `execute_remittance_flow*`, `calculate_split`,
  `validate_precision_spending`, `create_goal`), assume `0` is **rejected**.
- **Known exception:** `savings_goals::add_to_goal` **accepts** `0` as a
  successful no-op.
- **Fan-out allocations:** zero allocations inside
  `orchestrator::run_remittance_fan_out` are **normalized away** (skipped),
  not passed through as zero-amount calls.

## Related Docs

- [Remittance Split Rounding & Dust Policy](remittance-split-rounding-policy.md)
- [Authorization Matrix](AUTHORIZATION_MATRIX.md)