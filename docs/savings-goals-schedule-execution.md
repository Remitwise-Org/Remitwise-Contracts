# Savings Goals Schedule Execution

This document defines `execute_due_savings_schedules` behavior for safety and determinism.

## Execution Rules

- A schedule executes only when:
- `active == true`
- `next_due <= now`
- idempotency guard passes (`last_executed < next_due` when present)
- Goal exists and can be credited safely.

## Idempotency And Double-Credit Protection

- Re-running within the same ledger window does not double-credit the same due window.
- One-shot schedules deactivate after first execution.
- Recurring schedules advance `next_due` past `now`, preventing immediate re-execution.

## Missed Windows

- For recurring schedules, each skipped interval increments `missed_count`.
- `next_due` is set to the first future anchor after catch-up.
- `SavingsEvent::ScheduleMissed` is emitted with the missed interval count.

## Security Note

- Schedule advancement and crediting are persisted in one transaction context; a repeated call observes updated schedule state and cannot credit the same window twice.
