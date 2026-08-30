# Emergency Killswitch: Integer Precision and Overflow

## Overview

The `emergency_killswitch` contract has **no token amounts**. Pause, recovery,
threshold activation, and administrator rotation use unsigned integers only:
ledger timestamps (seconds), epoch counters, signer thresholds, paused-function
counts, and snapshot TTL.

This document is the reviewable guarantee that those values are added and
compared with exact integer rules, that overflow is rejected before storage
writes, and that saturating arithmetic cannot shorten an incident window.

## Integer model (scale, sign, fractional)

| Rule | Contract behavior |
| --- | --- |
| Scale | Seconds and counts. `RECOVERY_DELAY` is exactly `3600` seconds (one hour). There is no millisecond scale, no percent, and no division. |
| Sign | Public numeric arguments are `u64` or `u32`. Zero threshold and empty approvals are rejected. Negative values cannot appear on this ABI. |
| Fractional | None. `recovery_ready_at(now) = now + 3600` with `checked_add`. No rounding or truncation. |

Helpers (pure, crate-visible):

- `checked_add_u64` / `checked_add_u32` → `Error::Overflow` on wrap
- `recovery_ready_at(now)` → `now.checked_add(RECOVERY_DELAY)`
- `snapshot_age(now, snapshot_ts)` → `now.checked_sub(snapshot_ts)` (inverted clock → `Overflow`)

## Invariants

1. **Fail before write.** `activate` computes the recovery deadline and the
   function-pause cap **before** writing `ActivationEpoch`, `ActiveScope`,
   `RecoveryReadyAt`, or `ScopeWasPaused`.
2. **No saturating delay.** `timestamp.saturating_add(RECOVERY_DELAY)` is not
   used. Near `u64::MAX`, saturation would set `ready_at = u64::MAX` and allow
   immediate recovery (`now >= ready_at`). Checked add returns `Overflow`
   instead and leaves state unchanged.
3. **Epoch bumps** (`bump_kill_switch_epoch`, `configure_signers`) use
   `checked_add_u64(..., 1)` and return `Overflow`. They do not map wrap to
   `InvalidAdmin` or `EpochMismatch`.
4. **Inclusive time boundaries (unchanged public semantics):**
   - `recover` succeeds when `now >= RecoveryReadyAt`
   - `schedule_unpause` rejects `time < now` (`time == now` is allowed)
   - `unpause` rejects `now < schedule`
   - snapshot restore succeeds when age `<= SNAPSHOT_TTL` (expired when `>`)
5. **Approval and migration counters** increment with `checked_add_u32`.

## Failure matrix

| Attempt | Error | Mutable state |
| --- | --- | --- |
| `now + 3600` overflows | `Overflow` (20) | unchanged |
| Function activation would exceed `MAX_PAUSED_FUNCTIONS` | `LimitExceeded` | unchanged |
| Kill-switch or signer epoch is `u64::MAX` then bump | `Overflow` | epoch unchanged |
| Snapshot timestamp is after ledger `now` | `Overflow` | snapshot and pause state unchanged |
| Snapshot age `> SNAPSHOT_TTL` | `SnapshotExpired` | unchanged |
| Recover at `ready_at - 1` | `RecoveryTooEarly` | pause retained |
| Recover at `ready_at` | success | markers cleared |
| Repeat recover | `NotActive` | unchanged |
| Second activate while active | `ActivationAlreadyActive` | existing scope retained |
| Stale signer epoch on recover | `EpochMismatch` | pause retained |

## Compatibility

- Successful pause, unpause, transfer-admin, and in-range activate/recover
  paths are unchanged.
- **Error-shape change:** arithmetic wrap now returns `Error::Overflow = 20`
  instead of `InvalidAdmin` (kill-switch epoch bump) or `EpochMismatch`
  (signer-epoch bump). Discriminants `1`–`19` are unchanged.
- Additive ABI: new getter `get_recovery_ready_at() -> Option<u64>`.
- No storage layout change. `STORAGE_VERSION` is not bumped.

## Migration and rollback

No on-chain migration is required. Rolling back WASM restores the old overflow
error variants for the wrap path only (practically unreachable except tests
that inject `u64::MAX`). Live pause and signer state remain compatible.

## Operational limitations

If the ledger timestamp is greater than `u64::MAX - 3600`, threshold
`activate` fails closed with `Overflow`. Operators cannot open a recovery
window that would wrap. Admin-only `pause` / `clear_emergency_state` still
work; they do not add `RECOVERY_DELAY`.

## Security assumptions

- Overflow must not collapse the recovery delay into an immediate unpause of
  a quorum activation.
- Cap rejection must not leave an activation marker even if a future SDK
  stopped trapping on `Result::Err`.
- This contract still does not pause other workspace contracts by itself;
  dependents must check pause state on their write paths.
- Quorum `activate` / `recover` still validate the approval address list
  against the configured signers; they do not replace Soroban
  `require_auth` on each signer (unchanged; out of scope for this change).

## Validation

```bash
cargo test -p emergency_killswitch
cargo test -p emergency_killswitch overflow
cargo test -p emergency_killswitch numeric_precision
cargo clippy -p emergency_killswitch --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Focused coverage: `numeric_precision_overflow_tests` in
`emergency_killswitch/src/lib.rs`, recovery/cap/oracle tests in
`emergency_killswitch/src/threshold_tests.rs`, and
`overflow_guard_fires_at_u64_max`.

Commands were not executed in the implementation environment (`cargo` is not
installed). Re-run the commands above on Rust 1.88+ and paste the summaries
into the PR. `./check_ci.sh` remains the full-repo gate.
