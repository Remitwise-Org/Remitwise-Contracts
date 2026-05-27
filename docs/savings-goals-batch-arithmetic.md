# Savings Goals Batch Arithmetic

`batch_add_to_goals` processes a `Vec<ContributionItem>` in input order and applies each item to the referenced goal immediately.

## Contract Rules

- The batch length must be at most `MAX_BATCH_SIZE` (50).
- Each `ContributionItem.amount` must be strictly positive.
- Duplicate `goal_id` values are allowed.
- When a goal appears multiple times in one batch, later items see the updated `current_amount` from earlier items in that same batch.
- Balance updates use checked arithmetic and return `SavingsGoalError::Overflow` instead of allowing a host-level panic.
- Oversized batches return `SavingsGoalError::BatchTooLarge`.

## Security Notes

- The checked addition path is required because `overflow-checks = true` would otherwise abort on raw i128 overflow.
- The contract keeps overflow as a regular error path so callers can handle it explicitly and tests can assert the failure mode.
- Invalid amounts are rejected before any balance mutation is persisted.

## Regression Coverage

The stress suite covers:

- overflow on large duplicate contributions
- duplicate goal IDs in a single batch
- zero and negative amounts
- 51-item batch rejection
