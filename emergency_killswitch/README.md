# Emergency Killswitch Contract

A Soroban smart contract for centralized emergency pause controls across multiple modules/contracts with global, module, and per-function pause granularity.

> **Scope note:** "modules" here means logical groupings *within this contract's own storage* (module/function symbols you define yourself), not the other deployable contracts in this workspace (`bill_payments`, `insurance`, etc.). This contract is not currently cross-called by any of them — pausing it has no effect on their write paths. See [docs/EMERGENCY_SHUTDOWN.md](../docs/EMERGENCY_SHUTDOWN.md) for the repo-wide picture and what actually stops a given contract from accepting writes today.

## Features

- Global pause (all modules/functions)
- Per-module pause
- Per-function pause
- Scheduled unpause
- Admin transfer with safety guardrails
- Event logging for all operations

## Quickstart

```rust
use emergency_killswitch::EmergencyKillswitchClient;

// 1. Initialize
client.initialize(&admin);

// 2. Pause globally
client.pause();

// 3. Schedule unpause for 1 hour from now
let now = env.ledger().timestamp();
client.schedule_unpause(now + 3600);

// 4. Unpause
client.unpause();

// 5. Pause specific function
client.pause_function(symbol_short!("bill_payments"), symbol_short!("pay_bill"));

// 6. Check if paused
assert!(client.is_function_paused(symbol_short!("bill_payments"), symbol_short!("pay_bill")));
```

## API Reference

### Initialization

#### `initialize(env, admin)`

Initializes the killswitch with an admin.

### Admin Management

#### `transfer_admin(env, new_admin)`

Transfers admin authority to a new address.

### Global Controls

#### `pause(env)`

Pauses all functions globally.

#### `schedule_unpause(env, time)`

Schedules an unpause at a future timestamp.

#### `unpause(env)`

Unpauses after scheduled time is reached.

#### `is_paused(env)`

Returns true if globally paused.

### Module Controls

#### `pause_module(env, module_id)`

Pauses an entire module.

#### `unpause_module(env, module_id)`

Unpauses a module.

### Function Controls

#### `pause_function(env, module_id, func)`

Pauses a specific function.

#### `unpause_function(env, module_id, func)`

Unpauses a specific function.

#### `is_function_paused(env, module_id, func)`

Checks if a function is paused (considering global, module, and function-level pauses).

## Running Tests

```bash
cargo test -p emergency_killswitch
```

## Design Documentation

- [Activation and Recovery Policy](ACTIVATION_RECOVERY_POLICY.md) — Epoch semantics, scope invariants, activation/recovery protocol, failure matrix.
- [Atomic Rollback Guarantees](../docs/ATOMIC_ROLLBACK.md) — The validate-then-write pattern enforced by `activate()`, the two bugs that were fixed, and regression test coverage.
- [Killswitch Trust Model](../docs/killswitch-trust-model.md) — Who can trigger/clear, what state is preserved.
- [Pause/Unpause State Machine](../docs/killswitch-pause-state-machine.md) — Global state transitions and the module/function layers.
- [Kill-Switch Recovery Runbook](../docs/KILL_SWITCH_RECOVERY.md) — Operator guide for engaging and recovering the kill switch.


