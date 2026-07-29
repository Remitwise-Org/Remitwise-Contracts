# Bill Payments Pause Hierarchy

The Bill Payments contract implements a granular pause mechanism with administrative controls to safely manage incident containment and recovery.

## Overview

The contract provides three levels of pause controls:
1. **Global Pause** (`pause` / `unpause`): Suspends all state-mutating operations across the entire contract.
2. **Function-Level Pause** (`pause_function` / `unpause_function`): Allows targeted suspension of specific operations (e.g., `CREATE_BILL`, `PAY_BILL`) while allowing others to continue.
3. **Emergency Pause** (`emergency_pause_all`): A "killswitch" that immediately invokes the global pause and simultaneously pauses all function-level flags.
4. **Scheduled Unpause** (`schedule_unpause`): Time-locks the global unpause mechanism, preventing the contract from being globally unpaused before a specific future `at_timestamp`.

## Timing and Precedence

- **Global vs Function**: If the global pause flag is set, all state-mutating functions are blocked (`ContractPaused` error), regardless of their individual function-level pause state.
- **Function Pauses**: If the global pause is not set, individual functions are evaluated. If a function is paused at the function level, it returns a `FunctionPaused` error.
- **Emergency Pause Override**: Calling `emergency_pause_all` guarantees that even if individual functions were explicitly unpaused (`unpause_function`), they are re-paused. Thus, it acts as a blanket override.
- **Scheduled Unpause**: A scheduled unpause sets an `at_timestamp`. Any attempt to call `unpause` before the `ledger().timestamp()` reaches or exceeds `at_timestamp` will be rejected with a `ContractPaused` error. The unpause becomes callable exactly at or after `at_timestamp`.

## Access Control

All pause-related functions are strictly gated to the `PAUSE_ADM` (Pause Admin). Any unauthorized calls will revert with `UnauthorizedPause`.

See [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) for more details on the administrative roles and permissions.
