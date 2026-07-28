# Period Validation Helper

`remitwise-common::validate_period` is the canonical validation utility for checking the logical ordering of start and end timestamps/dates in range reads across Remitwise contracts.

The contract is:
- Returns `Ok(())` if `start <= end`.
- Returns `Err(TimeError::InvalidPeriod)` if `start > end`.
- Executed as an $O(1)$ stateless check.
- Prevents redundant range validation implementation across different contracts.

Callers should import and use this helper to standardize range validation and ensure consistent error handling when reading time-indexed data.

## Temporal Boundaries: Current, Future, Past

The table below summarizes how `validate_period(start, end)` and `Timestamp::seconds_until(now, target)` evaluate across temporal states:

| Temporal Classification | Input Condition | `validate_period(start, end)` | `Timestamp::seconds_until(now, target)` |
| :--- | :--- | :--- | :--- |
| **Current** | `start == end` / `now == target` | `Ok(())` | `0` |
| **Future** | `start < end` / `now < target` | `Ok(())` | `target - now` (`> 0`) |
| **Past** | `start > end` / `now > target` | `Err(TimeError::InvalidPeriod)` | `0` (saturating subtraction) |

### Executable Examples

```rust
use remitwise_common::{validate_period, Timestamp, TimeError};

// Current (equal timestamps)
assert_eq!(validate_period(1000, 1000), Ok(()));
assert_eq!(Timestamp::seconds_until(1000, 1000), 0);

// Future (target / end ahead in time)
assert_eq!(validate_period(1000, 1500), Ok(()));
assert_eq!(Timestamp::seconds_until(1000, 1500), 500);

// Past (target / end behind in time)
assert_eq!(validate_period(1500, 1000), Err(TimeError::InvalidPeriod));
assert_eq!(Timestamp::seconds_until(1500, 1000), 0);
```
