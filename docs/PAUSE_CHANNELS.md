# Pause Channels — Operator Guide

## Overview

The Remitwise contracts use multiple **independent** pause channels for incident
containment. Each channel has a distinct blast radius — understanding what each
channel guards is critical for choosing the right one during an incident.

This guide is written for **operators** who need to contain incidents without
causing unnecessary collateral damage.

**Core principle:** pause the narrowest channel that stops the threat. A
sledgehammer global pause stops everything but also blocks routine operations —
use it only when the threat is systemic.

> **Cross-reference:** see [docs/SETTLER_WHITELIST.md](SETTLER_WHITELIST.md) for
> how pause admin addresses are added, rotated, and revoked, and
> [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) for the full
> function-level access matrix.

---

## Channel inventory

### Channel 1: Killswitch (centralised emergency pause)

**Contract:** `emergency_killswitch`
**Admin:** `DataKey::Admin` (set at `initialize`, rotated via `transfer_admin`)
**Granularity:** four levels — global, module, function, timelocked unpause

| Level | Entrypoint | Blast radius |
|---|---|---|
| Global | `pause()` / `unpause()` | Killswitch contract itself. Does **not** cascade to other contracts. |
| Global recovery | `clear_emergency_state()` | Immediately clears global pause (bypasses timelock). Admin-only. |
| Module | `pause_module(module_id)` / `unpause_module(module_id)` | One named module within the killswitch (e.g. `"remittance"`, `"bills"`) |
| Function | `pause_function(module_id, func)` / `unpause_function(module_id, func)` | One function in one module. Capped at 10 paused functions per module. |
| Scheduled unpause | `schedule_unpause(timestamp)` | Sets a future timestamp before which `unpause()` is rejected. |

**Precedence within the killswitch** (for `is_function_paused`):
```
global → module → function
```
A function blocked at a higher level remains blocked even if its own flag is clear.

**The killswitch is standalone.** No downstream contract checks the killswitch
programmatically. Each contract checks its **own** local `PAUSED` flag. To
achieve full coverage, pause both the killswitch and the relevant per-contract
pause channels.

```rust
// Pause the killswitch globally
killswitch.pause();

// Does NOT automatically pause bill_payments — you must also call:
bills.pause(&pause_admin);

// Module-level: pause only the remittance module
killswitch.pause_module(&symbol_short!("remittance"));

// Function-level: pause only pay_bill within the bills module
killswitch.pause_function(&symbol_short!("bills"), &symbol_short!("pay_bill"));

// Recovery: clear global pause immediately (bypasses timelock)
killswitch.clear_emergency_state();
```

---

### Channel 2: Bill Payments pause

**Contract:** `bill_payments`
**Admin:** `PAUSE_ADM` (set via `set_pause_admin`)
**Granularity:** three levels — global, function, emergency

| Level | Entrypoint | What it guards |
|---|---|---|
| Global | `pause()` / `unpause()` | All 12 named functions in `pause_functions` plus `execute_due_bill_schedules` (13 total affected). `execute_due_bill_schedules` returns an empty `Vec` when paused (silent no-op) instead of a typed error — unlike other mutators. |
| Function | `pause_function(func)` / `unpause_function(func)` | One named function (e.g. `CREATE_BILL`, `PAY_BILL`, `CANCEL_BILL`). See `bill_payments::pause_functions` for the full symbol list. |
| Emergency | `emergency_pause_all()` | Global pause + all function-level flags set to `true`. Blanket override. |
| Scheduled unpause | `schedule_unpause(timestamp)` | Timelocks `unpause()` until the ledger reaches `timestamp`. |

**Read-only functions** (unaffected by any pause):
`get_bill`, `get_unpaid_bills`, `get_all_bills_for_owner`, `get_overdue_bills`,
`get_total_unpaid`, `get_storage_stats`, `get_bills_by_currency`, `is_paused`,
`get_version`, `get_bill_schedules`, `get_bill_schedule`.

```rust
// Pause all bill operations
bills.pause(&pause_admin);

// Pause only bill creation — payments still work
bills.pause_function(&pause_admin, &symbol_short!("crt_bill"));

// Emergency override — pauses everything in one call
bills.emergency_pause_all(&pause_admin);

// Schedule unpause with 24-hour cooldown
let now = env.ledger().timestamp();
bills.schedule_unpause(&pause_admin, &(now + 86_400));
```

**Blast radius:** 13 state-mutating functions (12 named in `pause_functions` + `execute_due_bill_schedules`).
`execute_due_bill_schedules` returns an empty `Vec` instead of a typed error when paused — operators should not rely on error-based monitoring for this function.
See [docs/bill-payments-pause-hierarchy.md](bill-payments-pause-hierarchy.md)
for the full pause hierarchy.

---

### Channel 3: Remittance Split pause

**Contract:** `remittance_split`
**Admin:** `PAUSE_ADM` (set via `set_pause_admin` by the owner)
**Granularity:** single global flag

| Level | Entrypoint | What it guards |
|---|---|---|
| Global | `pause()` / `unpause()` | `update_split`, `create_remittance_schedule`, `modify_remittance_schedule`, `cancel_remittance_schedule`, `import_snapshot`. **Note:** `initialize_split` has its own guard (rejects if already initialised). |

**Notable:** `distribute_usdc` is intentionally not guarded by the pause flag.
It is protected by the token contract trust check (`UntrustedTokenContract`) and
owner auth instead. This design keeps settlement operational during incidents
that only affect configuration.

**Read-only functions** (unaffected): `get_split_config`, `get_split`,
`calculate_split`, `get_remittance_schedule`, `export_snapshot`,
`execute_due_remittance_schedules`, `get_split_nonce`, `is_paused`.

```rust
split.pause(&pause_admin);
split.unpause(&pause_admin);
```

**Blast radius:** 5 state-mutating functions (`update_split`, schedule lifecycle, `import_snapshot`).
`initialize_split` and `distribute_usdc` are not pause-guarded by design.
See [docs/remittance-split-pause-coverage.md](remittance-split-pause-coverage.md)
for the full coverage matrix.

---

### Channel 4: Savings Goals pause

**Contract:** `savings_goals`
**Admin:** `PAUSE_ADM` (self-nomination on first call, then admin-only transfer)
**Granularity:** global + function-level

| Level | Entrypoint | What it guards |
|---|---|---|
| Global | `pause()` / `unpause()` | All goal mutators: `create_goal`, `add_to_goal`, `batch_add_to_goals`, `withdraw_from_goal`, `lock_goal`, `unlock_goal`, tag management, schedule lifecycle, snapshot import |
| Function | `pause_function(func)` / `unpause_function(func)` | One named function within `savings_goals::pause_functions` |
| Scheduled unpause | `schedule_unpause(timestamp)` | Timelocks `unpause()` |

```rust
savings.pause(&pause_admin);
savings.unpause(&pause_admin);
```

**Blast radius:** all goal creation, contribution, withdrawal, locking, tagging,
schedule lifecycle, and snapshot import. See `savings_goals::pause_functions`
for the full function-level symbol list.

---

### Channel 5: Insurance pause

**Contract:** `insurance`
**Admin:** `PAUSE_ADM` (self-nomination on first call)
**Granularity:** global + function + emergency

| Level | Entrypoint | What it guards |
|---|---|---|
| Global | `pause()` / `unpause()` | `create_policy`, `pay_premium`, `batch_pay_premiums`, `deactivate_policy`, schedule lifecycle |
| Function | `pause_function(func)` / `unpause_function(func)` | One named function |
| Emergency | `emergency_pause_all()` | Global + all function-level flags |

```rust
insurance.pause(&pause_admin);
insurance.emergency_pause_all(&pause_admin);
```

**Blast radius:** all policy creation, premium payment, deactivation, and
schedule lifecycle. See `insurance::pause_functions` for the full
function-level symbol list.

---

### Channel 6: Family Wallet pause

**Contract:** `family_wallet`
**Admin:** `PAUSE_ADM` (set by owner via `set_pause_admin`)
**Granularity:** single global flag

| Level | Entrypoint | What it guards |
|---|---|---|
| Global | `pause()` / `unpause()` | `add_member`, `remove_family_member`, `update_spending_limit`, `configure_multisig`, `propose_transaction`, `sign_transaction`, `withdraw`, `propose_emergency_transfer`, `configure_emergency`, `set_emergency_mode`, `batch_add_family_members`, `batch_remove_family_members`, `set_role_expiry`, and all other state mutators. |

**Critical exemption:** `pause`, `unpause`, and `set_pause_admin` deliberately
bypass the pause check so the contract can always be unpaused.

**Read-only functions** (unaffected): all getters, queries, and audit functions
(see [docs/fw-pause-matrix.md](fw-pause-matrix.md) for the full list).

```rust
// Pause stops all member-initiated operations
wallet.pause(&pause_admin);

// pause/unpause/set_pause_admin remain callable during pause
wallet.unpause(&pause_admin);
```

**Blast radius:** 25+ state-mutating entrypoints.
See [docs/fw-pause-matrix.md](fw-pause-matrix.md) for the full per-function matrix.

---

### Channel 7: Orchestrator execution lock

**Contract:** `orchestrator`
**Owner:** `OWNER` (set at `init`, immutable)
**Granularity:** single execution lock (not a user-facing pause)

The orchestrator does **not** have a traditional pause channel. Instead, it uses
an `EXEC_LOCK` (reentrancy guard) that serialises `execute_remittance_flow` and
`claim_rewards_summary_external` calls.

| Mechanism | What it guards |
|---|---|
| `EXEC_LOCK` | Prevents reentrant execution of settlement flows and reward claims |
| Actor epoch bump (`bump_actor_epoch`) | Invalidates all stale actor tokens without pausing the contract |

```rust
// Invalidate all actor tokens (e.g. compromised signing service)
orch.bump_actor_epoch(&owner);
```

The owner can also upgrade the contract or restore from a snapshot to recover
from incidents.

---

## Pause channel comparison

| Channel | Contract | Granularity | Timelock? | Emergency override? | Blast radius |
|---|---|---|---|---|---|
| Killswitch global | `emergency_killswitch` | Global | Yes (`schedule_unpause`) | `clear_emergency_state` | Killswitch only |
| Killswitch module | `emergency_killswitch` | Per-module | No | — | One module |
| Killswitch function | `emergency_killswitch` | Per-function | No | — | One function (max 10) |
| Bill Payments | `bill_payments` | Global + function | Yes | `emergency_pause_all` | 12 mutators |
| Remittance Split | `remittance_split` | Global | No | — | 5 mutators |
| Savings Goals | `savings_goals` | Global + function | Yes | — | All goal mutators |
| Insurance | `insurance` | Global + function | Yes | `emergency_pause_all` | All policy mutators |
| Family Wallet | `family_wallet` | Global | No | — | 25+ mutators |
| Orchestrator lock | `orchestrator` | Single-lock | No | `bump_actor_epoch` | Settlement flows |

---

## Incident response playbook

### Scenario 1: Suspicious bill creation

**Threat:** an attacker is flooding the system with bogus bills.

**Response:** function-level pause on `bill_payments`.

```rust
// Stop bill creation only — payments and queries still work
bills.pause_function(&pause_admin, &symbol_short!("crt_bill"));
```

### Scenario 2: Compromised pause admin key

**Threat:** a pause admin key has been compromised.

**Response:** rotate the pause admin immediately.

```rust
// Remittance split: owner rotates pause admin
split.set_pause_admin(&owner, &new_pause_admin);

// Bill payments: current pause admin rotates
bills.set_pause_admin(&old_pause_admin, &new_pause_admin);
```

### Scenario 3: Systemic exploit across multiple contracts

**Threat:** a vulnerability affects multiple contracts simultaneously.

**Response:** per-contract global pause on every affected contract, plus
killswitch global pause for defence-in-depth.

```rust
// Layer 1: per-contract pauses
bills.pause(&bills_pause_admin);
split.pause(&split_pause_admin);

// Layer 2: killswitch global pause (independent, does not cascade)
killswitch.pause();

// Remediate...
// ...

// Unpause in reverse order
killswitch.clear_emergency_state();
split.unpause(&split_pause_admin);
bills.unpause(&bills_pause_admin);
```

### Scenario 4: Compromised actor signing service (orchestrator)

**Threat:** an attacker can forge actor tokens.

**Response:** bump the actor epoch to invalidate all existing tokens.

```rust
let new_epoch = orch.bump_actor_epoch(&owner);
// All actor tokens with epoch < new_epoch are now rejected.
```

---

## Event monitoring

Every pause/unpause action emits an event. Operators should monitor for:

| Event | Meaning | Urgency |
|---|---|---|
| `paused_v2` | Global pause (killswitch or per-contract) | Critical |
| `unpaused_v2` | Global unpause | High — verify intent |
| `m_paused_v2` / `m_unpause_v2` | Module pause toggled | Medium |
| `f_paused_v2` / `f_unpause_v2` | Function pause toggled | Low |
| `epch_bump` (orchestrator) | Actor epoch bumped | Medium |
| `admn_xfer` | Admin transferred | Low — routine rotation |

Refer to [docs/EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) for the full event schema.

---

## References

- [docs/SETTLER_WHITELIST.md](SETTLER_WHITELIST.md) — how pause admin addresses are managed
- [docs/bill-payments-pause-hierarchy.md](bill-payments-pause-hierarchy.md) — bill payments pause details
- [docs/fw-pause-matrix.md](fw-pause-matrix.md) — family wallet per-function pause matrix
- [docs/remittance-split-pause-coverage.md](remittance-split-pause-coverage.md) — remittance split pause coverage
- [docs/killswitch-timelock.md](killswitch-timelock.md) — killswitch timelock design
- [docs/killswitch-trust-model.md](killswitch-trust-model.md) — killswitch trust boundaries
- [docs/killswitch-paused-functions-cap.md](killswitch-paused-functions-cap.md) — killswitch function cap
- [ACCESS_CONTROL_MATRIX.md](../ACCESS_CONTROL_MATRIX.md) — full access control matrix
- [docs/EVENT_TAXONOMY.md](EVENT_TAXONOMY.md) — event taxonomy and schema

---

*Document written for Remitwise operators managing incident response via pause channels.*
