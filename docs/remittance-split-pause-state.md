# Remittance Split Pause State Machine

Audience: operators and downstream integrators who need to reason about the `remittance_split` emergency stop without reading the full contract.

Source of truth: `remittance_split/src/lib.rs`.

## Storage Keys

| Key | Shape | Writer | Meaning |
| --- | --- | --- | --- |
| `PAUSED` | `bool` | `pause`, `unpause` | Global emergency-stop flag. Missing means `false`. |
| `PAUSED_AT` | `u64` | `pause`, `unpause` | Ledger timestamp captured when the split is paused. Removed on unpause. |
| `PAUSE_ADM` | `Address` | `set_pause_admin` | Optional pause admin. If unset, the split owner from `CONFIG` acts as pause admin. |
| `CONFIG` | `SplitConfig` | `initialize_split` | Provides the owner fallback and proves the split was initialized. |

## States

| State | Storage condition | Observable views | Mutating behavior |
| --- | --- | --- | --- |
| Uninitialized | `CONFIG` is missing | pause/unpause admin resolution fails with `NotInitialized` | Pause controls cannot proceed. |
| Active | `PAUSED` is missing or `false` | `is_paused() == false`, `get_paused_since() == None` | Normal mutating entrypoints can proceed after their own auth and validation checks. |
| Paused | `PAUSED == true` and `PAUSED_AT` is present | `is_paused() == true`, `get_paused_since() == Some(timestamp)` | Entry points guarded by `require_not_paused` return `Unauthorized`; `unpause` remains callable by the active pause admin. |

## Transitions

| Transition | Entrypoint | Required caller | Storage effects | Event |
| --- | --- | --- | --- | --- |
| Active -> Paused | `pause(caller)` | `PAUSE_ADM`, or `CONFIG.owner` when no pause admin is set | sets `PAUSED = true`; sets `PAUSED_AT = env.ledger().timestamp()` | high-priority paused event (`ACTION_PAUSED_V2`) |
| Paused -> Active | `unpause(caller)` | same active pause admin resolution as `pause` | sets `PAUSED = false`; removes `PAUSED_AT` | high-priority unpaused event (`ACTION_UNPAUSED_V2`) |
| Active -> Active with new admin | `set_pause_admin(caller, new_admin)` | `CONFIG.owner` only | writes `PAUSE_ADM = new_admin` | `adm_xfr` audit event |

`pause` calls `require_not_paused`, so calling `pause` while already paused returns `Unauthorized`. `set_pause_admin` also calls `require_not_paused`, so the owner cannot rotate the pause admin while the contract is paused. `unpause` intentionally does not call `require_not_paused`; otherwise the contract could not recover.

## Guarded Entry Points

The current implementation calls `require_not_paused` in these mutating entrypoints:

- `set_pause_admin`
- `pause`
- `propose_treasury`
- `set_version`
- `initialize_split`
- `update_split`
- `init_corridors`
- `distribute_usdc`
- `import_snapshot`
- `create_remittance_schedule`
- `modify_remittance_schedule`
- `cancel_remittance_schedule`

Read-only views such as `is_paused`, `get_paused_since`, `get_pause_state`, `get_config`, `get_split`, `calculate_split`, and snapshot/export style readers remain available so operators and clients can diagnose pause state on-chain.

## Client Guidance

Pause rejections reuse `RemittanceSplitError::Unauthorized`. When a caller receives `Unauthorized` from a guarded mutating entrypoint, clients that need a user-facing explanation should read `get_pause_state()` before assuming the signer is wrong.