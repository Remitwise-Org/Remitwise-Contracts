# Emergency Killswitch: Pause/Unpause State Machine

Audience: contributors modifying `emergency_killswitch` or wiring a new
contract's `pause`/`unpause` entrypoints to match its conventions.

This complements [Killswitch Trust Model](killswitch-trust-model.md) (who can
act) and [killswitch-timelock.md](killswitch-timelock.md) (why the timelock
exists) by pinning down the states themselves and the exact entrypoint that
drives each transition.

## Global state

```
        pause()                schedule_unpause(t)         unpause()  [only if now >= t]
Unpaused ───────────► Paused ───────────────────► PendingUnpause ───────────► Unpaused
   ▲                     │  ▲                            │
   │                     │  └──────── pause() ────────────┘   (re-pause cancels the
   │                     │             (re-pause)               pending schedule)
   └── clear_emergency_state() ────────┘
        (admin-only bypass, from either Paused or PendingUnpause)
```

- **Unpaused**: `is_paused() == false`. Default state after `initialize`.
- **Paused**: `is_paused() == true`, `get_unpause_schedule() == None`. Entered via `pause()`.
- **PendingUnpause**: `is_paused() == true`, `get_unpause_schedule() == Some(t)`. Entered via `schedule_unpause(t)` while `Paused`. `unpause()` only succeeds once `env.ledger().timestamp() >= t`; calling it earlier returns `Error::Unauthorized`.

Calling `pause()` again while already `Paused` or `PendingUnpause` clears any
pending schedule (see `emergency_killswitch/src/lib.rs::pause`) — this is
intentional so a fresh incident always requires a fresh cool-down, not a
stale one left over from a previous unpause attempt. A consequence: if the
schedule is dropped this way, `unpause()` fails with `Error::InvalidSchedule`
until a new `schedule_unpause` is issued.

`clear_emergency_state()` is the admin-only escape hatch from either `Paused`
or `PendingUnpause` straight back to `Unpaused`, bypassing the timelock. Use
it only to recover from the stuck state above, or genuine emergencies — it
exists precisely because the timelock has no other way to be short-circuited.

## Module and function layers

Independently of the global state above, individual modules and functions
carry their own pause flags:

- `pause_module(module_id)` / `unpause_module(module_id)` — `is_module_paused(module_id)`.
- `pause_function(module_id, func)` / `unpause_function(module_id, func)` — `is_function_paused(module_id, func)`, capped at `MAX_PAUSED_FUNCTIONS` (10) entries per module.

These are independent axes, not sub-states of the global machine above: a
module can be paused while the contract is globally unpaused, and vice
versa. A caller checking whether a specific function is currently callable
must check all three, in this precedence order: **global → module →
function**. `is_module_paused` and `is_function_paused` do not consult
`is_paused()`, so callers must not skip the global check.

## Worked example

```text
initialize(admin)                  -> Unpaused
pause()                            -> Paused
schedule_unpause(now + 3600)       -> PendingUnpause, get_unpause_schedule() == Some(now + 3600)
unpause()  [called at now + 1800]  -> Error::Unauthorized (schedule not reached), still PendingUnpause
unpause()  [called at now + 3600]  -> Unpaused
```

See `emergency_killswitch/tests/test_killswitch.rs` for the executable
version of this and the re-pause/recovery cases.
