# Remittance Split Deadline Window Semantics

## Overview
The `distribute_usdc_signed` function enforces a deadline on every signed
request to prevent replay attacks and stale request execution.

## Deadline Validation Rules
| Condition                                  | Result           |
|--------------------------------------------|------------------|
| deadline == 0                              | InvalidDeadline  |
| deadline < now                             | DeadlineExpired  |
| deadline == now                            | DeadlineExpired  |
| deadline == now + 1                        | Accepted         |
| deadline <= now + MAX_DEADLINE_WINDOW_SECS | Accepted         |
| deadline > now + MAX_DEADLINE_WINDOW_SECS  | InvalidDeadline  |

## Constants
- `MAX_DEADLINE_WINDOW_SECS` = 3,600 (1 hour)

## Security Properties
- Expired or invalid deadlines never advance the nonce
- Deadline is checked before require_auth side effects
- The comparison is strictly greater than (deadline > now required)

## Replay Window
Requests are valid for at most 1 hour from the ledger timestamp at
submission time. Requests with deadlines beyond this window are rejected
as InvalidDeadline to prevent unbounded replay windows.
