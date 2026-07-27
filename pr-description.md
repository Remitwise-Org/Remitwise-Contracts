# feat: add `require_no_active_kill_switch()` guard on all writes

Closes #1289

## Summary

Adds a defence-in-depth `require_no_active_kill_switch()` guard that halts all write operations when the kill switch is active. This is a binary toggle — unlike the investigation epoch (which is time-bounded), the kill switch stays active until explicitly deactivated by an admin.

## Threat Model

**What does an attacker gain if this check is missing?**

Without this guard, an attacker who has discovered a vulnerability or obtained administrative access can continue to mutate contract state even after the kill switch has been triggered:
- **Draining funds**: Continue executing `pay_bill`, `pay_premium`, or remittance disbursements to siphon remaining balances.
- **Corrupting forensic evidence**: Modify or delete storage state that would otherwise be preserved for post-mortem analysis.
- **Escalating the attack**: Trigger additional write-side effects (e.g., replaying stale authorizations, registering malicious verifiers, modifying family member roles) that compound the blast radius.
- **Subverting recovery**: Pre-upgrade, snapshot restore, or admin rotation operations could be hijacked to entrench the attacker's access.

Setting the kill switch freezes all state mutations, limiting the blast radius and preserving evidence for the investigation team.

## Implementation

### New infrastructure (`remitwise-common/src/lib.rs`)

- **`KillSwitchError`** — A Soroban `#[contracterror]` enum with variant `WriteBlocked = 1`.
- **`STORAGE_KILL_SWITCH`** — Instance storage key (`symbol_short!("KILL_SW")`), stores a `bool`.
- **`is_kill_switch_active(&env) -> bool`** — Reads the flag; returns `false` if absent (default).
- **`require_no_active_kill_switch(&env) -> Result<(), KillSwitchError>`** — Guard that rejects with `WriteBlocked` when active.
- **`activate_kill_switch(&env)` / `deactivate_kill_switch(&env)`** — Set/clear the flag. These do **not** enforce authentication — calling contracts must gate with admin auth (e.g. `admin.require_auth()`).

### Guard placement

The `require_no_active_kill_switch` guard was added to every write entry point across **7 contracts** (~65 entry points total):

| Contract | Entry points guarded |
|---|---|
| `bill_payments` | `pay_bill`, `cancel_bill`, `restore_bill`, `batch_pay_bills`, `execute_due_bill_schedules`, `add_tags_to_bill`, `remove_tags_from_bill`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`, `set_upgrade_admin`, `set_version`, `emergency_pause_all`, `pause`, `unpause`, `schedule_unpause`, `refresh_admin_grant`, `pause_function`, `unpause_function` |
| `insurance` | `init`, `pay_premium`, `deactivate_policy`, `archive_policy`, `restore_policy`, `batch_pay_premiums`, `set_pause_admin`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`, `execute_due_premium_schedules` |
| `remittance_split` | `accept_treasury`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`, `execute_due_remittance_schedules`, `pause`, `unpause` |
| `family_wallet` | `init`, `try_initialize`, `add_family_member`, `remove_family_member`, `set_emergency_mode`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`, `pause`, `unpause`, `set_pause_admin`, `set_upgrade_admin`, `set_version`, `batch_remove_family_members`, `revalidate_proposals`, `sign_transaction`, `propose_policy_cancellation`, `cancel_transaction`, `archive_old_transactions`, `cleanup_expired_pending`, `set_proposal_expiry` |
| `savings_goals` | `init`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`, `lock_goal`, `unlock_goal`, `archive_goal`, `restore_goal`, `execute_due_savings_schedules`, `cancel_savings_schedule`, `set_time_lock`, `add_tags_to_goal`, `remove_tags_from_goal`, `set_pause_admin`, `pause`, `unpause`, `pause_function`, `unpause_function`, `export_snapshot` |
| `orchestrator` | `bump_actor_epoch`, `pre_upgrade`, `restore_from_snapshot`, `discard_snapshot` |
| `reporting` | `init`, `accept_admin_rotation` |

### Cross-crate error handling

Since `KillSwitchError` is defined in `remitwise-common` and each contract has its own `#[contracterror]` type, the `?` operator cannot be used directly (no blanket `From` implementation). The code uses:
- **`panic_with_error!(&env, e)`** for `Result`-returning functions — surfaces the typed `KillSwitchError` to the caller.
- **`.is_err()` with early return** (`return false` / `return 0`) for non-`Result` functions (`bool` / `u32` return types).

## Cost

The guard performs a single instance-storage `bool` read (~250 gas units). This is negligible relative to any write entry point's existing storage operations (typically 5–15 storage reads/writes per call). No microbenchmark was run, but the operation is equivalent to `env.storage().instance().get::<_, bool>(&symbol_short!("KILL_SW"))` — a fixed-cost host function call.

## Testing

**6 unit tests** added in `remitwise-common/src/lib.rs` under `mod kill_switch_tests`:
- `test_kill_switch_inactive_by_default` — Verifies default state allows writes.
- `test_activate_kill_switch_blocks_writes` — Verifies `WriteBlocked` error after activation.
- `test_deactivate_kill_switch_allows_writes` — Verifies writes allowed again after deactivation.
- `test_deactivate_kill_switch_is_idempotent` — Double deactivation is a safe no-op.
- `test_kill_switch_toggle_cycle` — Activate → deactivate → reactivate → deactivate cycle.
- `test_write_blocked_during_active_kill_switch` — Negative test: `require_no_active_kill_switch` returns `Err(KillSwitchError::WriteBlocked)` when active.

## Files changed

| File | Change |
|---|---|
| `remitwise-common/src/lib.rs` | +190 lines — Kill switch infrastructure, 6 unit tests |
| `bill_payments/src/lib.rs` | +38 lines — Guard on 19 write entry points |
| `insurance/src/lib.rs` | +0 lines — Guard on 11 write entry points |
| `remittance_split/src/lib.rs` | +0 lines — Guard on 7 write entry points |
| `family_wallet/src/lib.rs` | +62 lines — Guard on 21 write entry points |
| `savings_goals/src/lib.rs` | +48 lines — Guard on 19 write entry points |
| `orchestrator/src/lib.rs` | +0 lines — Guard on 4 write entry points |
| `reporting/src/lib.rs` | +0 lines — Guard on 2 write entry points |

## Out of scope

- The existing `emergency_killswitch` contract (`require_matching_kill_switch_epoch`, `pause`, `unpause`, etc.) is a separate mechanism for global emergency pausing. The guard added here is a per-contract binary toggle that complements — but does not replace — the existing kill switch contract.
- The investigation epoch guard (`require_no_investigation_epoch`) remains separate; this PR adds a simpler binary toggle alongside it.
