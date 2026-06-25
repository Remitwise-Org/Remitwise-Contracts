# Emergency Killswitch Contract

A Soroban smart contract for centralized emergency pause controls across multiple modules/contracts with global, module, and per-function pause granularity.

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

