# Bill Payments — Due Date Semantics

## 1. Acceptance Rule for `create_bill`

Guard in `lib.rs` (`create_bill`):

```rust
if due_date == 0 || due_date < current_time {
    return Err(BillPaymentsError::InvalidDueDate);
}
```

The comparison is **strict less-than**, so `due_date == now` passes.

| `due_date` vs `now`  | Outcome                    |
|----------------------|----------------------------|
| `due_date > now`     | Accepted                   |
| `due_date == now`    | Accepted (`<` is strict)   |
| `due_date < now`     | `InvalidDueDate (12)` error |
| `due_date == 0`      | `InvalidDueDate (12)` error |

## 2. Recurring Next-Due Formula

```
child_due_date = parent_due_date + (frequency_days × 86_400)
```

If the result is still `<= current_time` (extremely late payment), the contract
advances by one additional period repeatedly until the child is strictly in the future:

```rust
let mut next_due_date = bill.due_date + period;
while next_due_date <= current_time {
    next_due_date = next_due_date + period;
}
```

Key properties:

- Formula is **independent of `paid_at`** timestamp.
- `frequency_days` must be in `[1, MAX_FREQUENCY_DAYS]`; otherwise `InvalidFrequency (4)`.
- A child bill's `due_date` is always strictly greater than the parent's `due_date`.
- The catch-up loop guarantees the child is **never born overdue** regardless of payment lateness.

## 3. Security Invariant

A recurring child bill MUST NOT be created with a `due_date` in the past relative to
the ledger timestamp at time of `pay_bill`. This is guaranteed by the catch-up loop:
the loop exits only when `next_due_date > current_time`, so the child is always strictly
in the future at the moment of creation.

Formally: `child.due_date > env.ledger().timestamp()` at the point `pay_bill` executes.

## 4. Overflow Protection

The recurring period arithmetic uses `checked_mul` and `checked_add`:

```rust
let period = (bill.frequency_days as u64)
    .checked_mul(SECONDS_PER_DAY)
    .ok_or(Error::InvalidFrequency)?;
let mut next_due_date = bill.due_date
    .checked_add(period)
    .ok_or(Error::InvalidDueDate)?;
```

- `frequency_days` overflow on `× 86_400`: returns `InvalidFrequency (4)`.
- `due_date` overflow on `+ period`: returns `InvalidDueDate (12)`.
- The catch-up loop also uses `checked_add`, returning `InvalidDueDate` on overflow.

`MAX_FREQUENCY_DAYS = 36_500` (100 years). The maximum period in seconds is
`36_500 × 86_400 = 3_153_600_000`, well within `u64` range.

## 5. Edge Cases

| Case | Behaviour |
|------|-----------|
| `due_date == 0` | Always rejected with `InvalidDueDate (12)`. |
| `due_date == u64::MAX` | Accepted by `create_bill` if `u64::MAX >= now`. On `pay_bill`, `checked_add(period)` overflows → `InvalidDueDate (12)`. |
| `frequency_days == 0` | `InvalidFrequency (4)`; would produce a zero-advance child. |
| `frequency_days == MAX_FREQUENCY_DAYS` (36_500) | Accepted. |
| `frequency_days > MAX_FREQUENCY_DAYS` | `InvalidFrequency (4)`. |
| Payment so late that `parent_due_date + period <= now` | Catch-up loop advances until child is in the future. |
