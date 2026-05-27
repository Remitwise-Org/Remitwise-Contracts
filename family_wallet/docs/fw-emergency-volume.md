# Emergency Transfer Daily Volume Cap

## Overview

The `FamilyWallet` emergency path allows direct token transfers when `EM_MODE`
is `true`, bypassing the normal multisig flow. A daily volume cap prevents the
wallet from being fully drained during a single incident.

## Storage Keys

| Key       | Type            | Description                                              |
|-----------|-----------------|----------------------------------------------------------|
| `EM_CONF` | `EmergencyConfig` | Per-transfer max, cooldown, min-balance, daily limit   |
| `EM_MODE` | `bool`          | Whether emergency mode is active                         |
| `EM_LAST` | `u64`           | Ledger timestamp of the last completed emergency transfer |
| `EM_VOL`  | `i128`          | Accumulated transfer volume for the current UTC day      |

## Day-Boundary Rollover

`EM_VOL` resets to zero when a transfer occurs in a **later UTC calendar day**
than the one recorded in `EM_LAST`:

```
is_new_day = (now / 86_400) > (EM_LAST / 86_400)
```

Integer division truncates to midnight UTC, so:

| Timestamp | Day index (`ts / 86400`) |
|-----------|--------------------------|
| 86 399    | 0 (last second of day 0) |
| 86 400    | 1 (first second of day 1) — triggers reset |
| 172 799   | 1 |
| 172 800   | 2 — triggers reset |

This anchors the window to calendar-day boundaries rather than a rolling 24-hour
window, preventing the attack where transfers at `T` and `T + 86_401` would both
sit within their own 24-hour windows, effectively allowing `2 × daily_limit` in
two back-to-back calls.

## Checked Arithmetic

`EM_VOL` accumulation uses `i128::checked_add` instead of `saturating_add`.
Saturation would silently cap at `i128::MAX`, which can pass the `> daily_limit`
comparison if both operands are near the maximum. `checked_add` panics on
overflow, surfacing the error explicitly rather than masking it.

## Execution Flow

```
propose_emergency_transfer  (EM_MODE = true)
  └─ execute_emergency_transfer_now
       ├─ amount > 0
       ├─ amount ≤ EM_CONF.max_amount
       ├─ cooldown elapsed  (now ≥ EM_LAST + EM_CONF.cooldown)
       ├─ check_and_update_emergency_volume          ← this document
       │    ├─ read EM_LAST, EM_VOL
       │    ├─ (now / 86_400) > (EM_LAST / 86_400) ? reset vol to 0
       │    ├─ checked_add(vol, amount)  — panic on overflow
       │    ├─ assert new_vol ≤ EM_CONF.daily_limit
       │    └─ persist EM_VOL = new_vol
       ├─ balance ≥ EM_CONF.min_balance
       ├─ token.transfer(proposer → recipient, amount)
       └─ EM_LAST = now
```

`EM_VOL` is written **before** the token transfer. If the transfer reverts,
Soroban rolls back the `EM_VOL` write atomically — no phantom volume is recorded.

## Security Notes

- **Fresh deployment (`EM_LAST = 0`)**: the last transfer is considered to be in
  day 0 (epoch). Any transfer at timestamp ≥ 86 400 triggers a rollover first,
  which is the correct safe default.
- **`daily_limit = 0`**: effectively disables all emergency transfers since any
  positive `amount` will exceed the cap.
- **Cooldown + volume cap are independent**: cooldown prevents high-frequency
  transfers; the volume cap prevents high-total-volume transfers. Both must pass.

## Test Coverage

| Test | Scenario |
|------|----------|
| `test_emergency_volume_same_day_accumulation` | Multiple transfers in one day stack up |
| `test_emergency_volume_over_cap_rejected` | Transfer exceeding cumulative cap fails |
| `test_emergency_volume_cross_day_reset` | Day 2 transfer sees a fresh EM_VOL = 0 |
| `test_emergency_volume_exactly_at_cap_succeeds` | Transfer equal to cap passes |
| `test_emergency_volume_one_stroop_over_cap_rejected` | Cap + 1 stroop fails |
| `test_emergency_volume_boundary_timestamp_resets_counter` | ts=86400 resets day-0 volume |
| `test_emergency_mode_disabled_skips_volume_cap` | EM_MODE=false uses multisig, no cap check |

## Running the tests

```bash
cargo test -p family_wallet
```