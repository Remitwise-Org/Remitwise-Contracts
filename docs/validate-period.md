# Period Validation Helper

`remitwise-common::validate_period` is the canonical validation utility for checking the logical ordering of start and end timestamps/dates in range reads across Remitwise contracts.

The contract is:
- Returns `Ok(())` if `start <= end`.
- Returns `Err(TimeError::InvalidPeriod)` if `start > end`.
- Executed as an $O(1)$ stateless check.
- Prevents redundant range validation implementation across different contracts.

Callers should import and use this helper to standardize range validation and ensure consistent error handling when reading time-indexed data.

## Related Helper: `Timestamp::seconds_until(now, target)`

`remitwise-common::Timestamp::seconds_until` standardizes overflow-safe future-distance
calculation for timestamp-based checks.

The contract is:
- Returns `target - now` when `target > now`.
- Returns `0` when `target <= now`.
- Uses saturating subtraction, so it never underflows.
- Fits window checks such as deadline validation and maximum lead-time enforcement.
