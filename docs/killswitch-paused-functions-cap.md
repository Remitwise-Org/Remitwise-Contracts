# Emergency Killswitch: Per-Module Paused-Function Cap

## Overview

`EmergencyKillswitch::pause_function` records individual function pauses per `module_id` under `DataKey::PausedFunctions(module_id)`. Each module maintains its own `Vec<Symbol>` of paused function names. To prevent unbounded storage growth and gas blow-up during an incident, the list is capped at **`MAX_PAUSED_FUNCTIONS` (10)** distinct entries per module.

## Cap Rules

| Behavior | Result |
|----------|--------|
| Pause a new distinct function while count `< MAX_PAUSED_FUNCTIONS` | Success; function is recorded and an `f_paused` event is emitted |
| Pause the (`MAX_PAUSED_FUNCTIONS` + 1)-th distinct function | `Error::LimitExceeded` |
| Pause an already-paused function | No-op (`Ok(())`); does **not** consume an additional slot |
| `unpause_function` on a paused function | Removes the entry and frees one slot for a new distinct pause |
| Same function name on two different `module_id` values | Independent lists; each module has its own cap |

## Storage Key

- `DataKey::PausedFunctions(module_id)` — maps a module symbol to the bounded vector of paused function symbols for that module only.

## Error Code

- `Error::LimitExceeded` (4) — returned when `pause_function` would append an eleventh distinct function to a module's paused list.

## Security Verification and Testing

Cap invariants are verified in `emergency_killswitch/src/lib.rs` (`pause_function_cap_tests`):

1. **`pause_function_exact_cap_succeeds`** — pausing exactly `MAX_PAUSED_FUNCTIONS` distinct functions succeeds and the paused count is exact.
2. **`pause_function_over_cap_returns_limit_exceeded`** — the next distinct function returns `Error::LimitExceeded` without changing storage.
3. **`pause_function_dedup_is_noop`** — re-pausing an existing entry does not free or consume a slot; the cap remains full.
4. **`unpause_function_frees_slot_for_new_pause`** — after filling the cap, unpausing one function allows a new distinct pause to succeed.
5. **`pause_function_cap_is_per_module`** — filling one module to the cap does not block pauses on another module, including the same function symbol name.

Run the focused suite:

```bash
cargo test -p emergency_killswitch pause_function -- --nocapture
```
