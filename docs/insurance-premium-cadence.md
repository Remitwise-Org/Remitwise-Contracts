# Insurance Premium Cadence

This document defines how `next_payment_date` advances for:

- `pay_premium`
- `batch_pay_premiums`

## Cadence Rule

- Period: fixed 30-day interval (`30 * 86_400`).
- Early payment (`now < previous_due`): `next_payment_date = previous_due + period`.
- On-time or late payment (`now >= previous_due`): advance by enough whole periods so the new due date is strictly greater than `now`.

## Guarantees

- No past-dated due date after payment.
- `PremiumPaidEvent.next_payment_date` always equals the stored policy value.
- Batch processing advances each policy independently from its own previous due date.
- `batch_pay_premiums` returns the count of policies actually advanced.
