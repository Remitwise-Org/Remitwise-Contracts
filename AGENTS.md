# Anchored Summary — Remitwise-Contracts Fix Session

## Goal
Fix compilation errors blocking `cargo build --release --target wasm32-unknown-unknown --workspace`

## Constraints & Preferences
- WASM target (`wasm32-unknown-unknown`) with `#![no_std]` constraint
- `panic = "abort"` in release profile (no panic catching)
- `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]` in `remitwise-common`

## Progress
### Done
- Generated `Cargo.lock` via `cargo generate-lockfile` (fixes `check_ci.sh` step 1)
- Fixed `remitwise-common/Cargo.toml`: removed duplicate `[features]` section (lines 13–14, 20–21)
- `remitwise-common/src/lib.rs`: replaced `Vec::with_capacity` / `extend_from_slice` with `soroban_sdk::Bytes::new` + `extend_from_slice`
- `remitwise-common/src/lib.rs`: replaced `soroban_sdk::crypto::ed25519_verify(...)` free-function call with `env.crypto().ed25519_verify(...)` using `BytesN::from_array`
- `remitwise-common/src/tests.rs`: rewrote `ed25519::generate` / `ed25519::sign` helpers with `ed25519-dalek::SigningKey` / `Signer` (dev-dependency only)
- `remitwise-common/src/tests.rs`: updated `verify_signature` tests — invalid signature tests changed from `assert_eq!(..., Err(SignatureError::VerificationFailed))` to `#[should_panic]`
- `bill_payments/src/lib.rs`: fixed `&env` → `env` type mismatch (line 1722), fixed `next_bill` use-after-move (line 1772)
- **This session (Type-Safe Percent Conversion):** Implemented `BPS_PER_PERCENT` / `BASIS_POINTS_PER_PERCENT` constants, `Percent` newtype, `TryFrom<Percent> for Rate`, `Rate::from_percent`, `Rate::to_percent`, and `Rate::has_fractional_percent` in `remitwise-common`. Added 4 unit tests + 1 proptest in `remitwise-common/src/tests.rs` and created `docs/type-safe-percent-conversion.md`.
- `remitwise-common/Cargo.toml`: moved `ed25519-dalek` from dev-deps to regular deps (version `"2"`) to prevent CI resolving to v3.0.0 (incompatible with `soroban-env-host-21.2.1`).
- `insurance/src/lib.rs`: fixed `symbol_short!("reactivated")` (too long, 11 > 9) → `Symbol::new(&env, "reactivated")`; fixed `PolicyAlreadyInactive` duplicate discriminant `12` → `52`; added `clamp_limit` to import; removed `mut` from `let mut active` (no mutation needed); fixed `Vec::new(&env)` → `Vec::new(env)` in `remove_active_policy`.
- `data_migration/src/lib.rs`: fixed `manual_range_contains` clippy lint (`version < MIN || version > MAX` → `!range.contains`); gated `ENCRYPTED_PAYLOAD_PREFIX_V2` with `#[cfg(test)]` (only used in tests).
- `reporting/src/utils.rs`: removed invalid `#![no_std]` (not at crate root).
- `remittance_split/src/lib.rs`: added `#[allow(dead_code)]` to unused `STORAGE_OWNER_SCHED_IDS`.
- **This session (Kill Switch Guard — Issue #1289):** Added `require_no_active_kill_switch()` defence-in-depth guard to block all write operations when the kill switch is active. Implemented `KillSwitchError` (typed `#[contracterror]`, discriminant `WriteBlocked = 1`), `STORAGE_KILL_SWITCH` storage key (`symbol_short!("KILL_SW")`), `is_kill_switch_active`, `require_no_active_kill_switch`, `activate_kill_switch`, and `deactivate_kill_switch` in `remitwise-common/src/lib.rs`. Added 6 unit tests covering happy path, activation, deactivation, idempotency, toggle cycle, and negative test (`test_write_blocked_during_active_kill_switch`).

  Applied the guard to **~50+ write entry points across 7 contracts**:
  - `bill_payments/src/lib.rs`: pay_bill, cancel_bill, restore_bill, batch_pay_bills, execute_due_bill_schedules, add_tags_to_bill, remove_tags_from_bill, pre_upgrade, restore_from_snapshot, discard_snapshot, set_upgrade_admin, set_version, emergency_pause_all, pause, unpause, schedule_unpause, refresh_admin_grant, pause_function, unpause_function
  - `insurance/src/lib.rs`: init, pay_premium, deactivate_policy, archive_policy, restore_policy, batch_pay_premiums, set_pause_admin, pre_upgrade, restore_from_snapshot, discard_snapshot, execute_due_premium_schedules
  - `remittance_split/src/lib.rs`: accept_treasury, pre_upgrade, restore_from_snapshot, discard_snapshot, execute_due_remittance_schedules, pause, unpause
  - `family_wallet/src/lib.rs`: add_family_member, remove_family_member, set_emergency_mode, pre_upgrade, restore_from_snapshot, discard_snapshot, pause, unpause, set_upgrade_admin, set_version, batch_remove_family_members, revalidate_proposals, sign_transaction, propose_policy_cancellation, cancel_transaction, archive_old_transactions, cleanup_expired_pending, set_proposal_expiry
  - `savings_goals/src/lib.rs`: init, pre_upgrade, restore_from_snapshot, discard_snapshot, lock_goal, unlock_goal, archive_goal, restore_goal, execute_due_savings_schedules, cancel_savings_schedule, set_time_lock, add_tags_to_goal, remove_tags_from_goal
  - `orchestrator/src/lib.rs`: bump_actor_epoch, pre_upgrade, restore_from_snapshot, discard_snapshot
  - `reporting/src/lib.rs`: init, accept_admin_rotation

  Uses `panic_with_error!(&env, e)` pattern for Result-returning functions (since `KillSwitchError` is a cross-crate type that can't use `?` with contract-specific error types) and `.is_err()` early-return for non-Result functions.

- **This session (Issue #1715 — Role Lifecycle Security):** Added 3 security-focused tests to `family_wallet/src/test.rs` closing the pre-merge review gaps: `test_role_removal_takes_effect_immediately` (removed member rejected at sign gate), `test_concurrent_role_demotion_strips_signature_and_blocks_signing` (Admin demoted to Viewer → sig stripped, sign blocked), `test_unauthorized_viewer_fails_before_mutation` (Viewer rejected at auth gate with zero side-effects). Created `docs/issue-1715-role-lifecycle-security.md` with full PR description, auth-matrix design, backward-compatibility analysis, rollback notes, and validation checklist.
- **This session (State-Transition Invariants — Emergency Killswitch):** Implemented formal state-transition matrix in `emergency_killswitch/src/lib.rs`. Fixed missing `Error::Overflow = 21` declaration and free helper functions (`checked_add_u64`, `checked_add_u32`, `snapshot_age`, `recovery_ready_at`). Added `KillSwitchState` enum, `TransitionError` contracterror, and `current_state` inspection helper. Applied transition guards to `activate`, `pause_internal`, `unpause`, and `schedule_unpause` ensuring zero partial state on rejection. Replaced unchecked `saturating_add` with `recovery_ready_at()` in `activate`. Added 38 regression tests in `state_transition_invariant_tests` covering legal/illegal edges, idempotency, stale epochs, concurrent lanes, and invariant bounds. Created `docs/state-transition-invariants.md`.

### Verified
- Code review confirms correctness of pattern, consistency, and error bounds.
- 38 new unit tests in `emergency_killswitch/src/lib.rs` covering all legal and illegal transition edges.

### Remaining / Untested
- Needs `cargo check` / `cargo test` on environment with Rust toolchain.
- CI (`check_ci.sh`) not yet run.

## Key Decisions
- Transition guards fire before any mutation (Phase 1 validation) so rejections leave zero partial or orphaned state.
- `clear_emergency_state` intentionally bypasses transition guards as the emergency admin recovery escape hatch.
- `Error::Overflow` assigned discriminant `21` to avoid conflicting with `InvalidCursor = 20`.
- Transition states (`KillSwitchState`) are derived dynamically from storage (`current_state`) rather than stored separately, eliminating state synchronization desync.

## File Changes
- `/emergency_killswitch/src/lib.rs`: Fixed `Error::Overflow = 21`, added free arithmetic/time helper functions, `KillSwitchState`, `TransitionError`, `current_state`, applied entry-point guards, fixed discriminant tests, and added 38 tests in `state_transition_invariant_tests`.
- `/docs/state-transition-invariants.md`: Created comprehensive design and invariants documentation.

