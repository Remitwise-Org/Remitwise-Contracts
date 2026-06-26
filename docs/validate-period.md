# Period Validation Helper

`remitwise-common::validate_period` is the canonical validation utility for checking the logical ordering of start and end timestamps/dates in range reads across Remitwise contracts.

The contract is:
- Returns `Ok(())` if `start <= end`.
- Returns `Err(TimeError::InvalidPeriod)` if `start > end`.
- Executed as an $O(1)$ stateless check.
- Prevents redundant range validation implementation across different contracts.

Callers should import and use this helper to standardize range validation and ensure consistent error handling when reading time-indexed data.
