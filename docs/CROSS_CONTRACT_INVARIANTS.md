# Cross-Contract Invariants

## Audience

This document is for **contributors** and **auditors**. It captures every invariant that spans two or more contracts in the RemitWise workspace. Each invariant names the contracts it applies to, the property that must hold, what breaks if it is violated, and where the enforcement lives in code.

This knowledge was previously tribal; writing it down lets reviewers verify behaviour against documented intent without reading every commit, and lets the support team answer common questions without paging an engineer.

---

## 1. Remittance-Split Allocation Conservation

**Contracts involved:** `remittance_split`, `savings_goals`, `bill_payments`, `insurance`

**Invariant:** The sum of every category allocation produced by `remittance_split::calculate_split` must equal the `total_amount` passed in. Specifically:

```
spending + savings + bills + insurance == total_amount
```

**Why it matters:** The orchestrator routes each allocation to its target contract (`add_to_goal`, `pay_bill`, `pay_premium`). If the split does not conserve the total, one or more downstream contracts receive less than the user intended, causing silent fund loss that is invisible on-chain.

**Where it is enforced:**
- `remittance_split/src/lib.rs` — `calculate_split` assigns the rounding remainder to the last category so the sum is always exact.
- `remittance_split` tests assert `sum(allocations) == total_amount`.
- `scripts/verify_cross_contract_invariants.py` — offline verification script that simulates the invariant and can be run in CI.

**Common breakage pattern:** Introducing a new category without updating the remainder-assignment logic. The four-category constant `10_000` (basis points) must be updated in lockstep with any new category column.

---

## 2. Split Percentage Normalisation (Basis Points = 10 000)

**Contracts involved:** `remittance_split`, `data_migration`

**Invariant:** All category percentages are stored and transmitted as basis points, and must satisfy:

```
spending_percent + savings_percent + bills_percent + insurance_percent == 10_000
```

**Why it matters:** `data_migration::validate_payload_semantics` rejects any `RemittanceSplitExport` that does not sum to exactly `10_000`. Importing a malformed split would seed corrupt on-chain state that the live contract enforces at write-time, producing a disagreement between stored state and the live validation rule.

**Where it is enforced:**
- `remittance_split/src/lib.rs` — `initialize_split` / `update_split` reject splits that do not sum to 10 000.
- `data_migration/src/lib.rs` — `validate_payload_semantics` enforces the same rule at import time.

**Common breakage pattern:** Adding a new category without subtracting from an existing one, or importing a snapshot exported from an older schema that used whole-percent values (0–100) instead of basis points.

---

## 3. Savings-Goal ID Counter Monotonicity

**Contracts involved:** `savings_goals`, `data_migration`

**Invariant:** `SavingsGoalsExport::next_id >= max(goal.id)` for all exported goals. The ID counter must never be wound back below the highest assigned goal ID.

**Why it matters:** A rolled-back or forged `next_id` would allow a newly created goal to receive an ID that already exists in on-chain storage, causing a silent overwrite of the original goal's data.

**Where it is enforced:**
- `data_migration/src/lib.rs` — `validate_payload_semantics` checks this at import time and returns `MigrationError::ValidationFailed` if the counter is below the maximum observed ID.

**Common breakage pattern:** Manually editing a snapshot file to reduce `next_id` (e.g. to "compact" IDs) without regenerating checksums.

---

## 4. Goal Completion Coherence

**Contracts involved:** `savings_goals`, `reporting`

**Invariant:** `current_amount <= target_amount` for every active savings goal. A goal whose current amount exceeds its target is incoherent — the `is_goal_completed` predicate would return `true` but no `GoalCompletedEvent` would have been emitted.

**Where it is enforced:**
- `savings_goals/src/lib.rs` — `add_to_goal` caps contributions at `target_amount - current_amount` and emits `GoalCompletedEvent` when the goal reaches its target.
- `data_migration/src/lib.rs` — `validate_payload_semantics` rejects any import where a goal's `current_amount > target_amount`.

**Common breakage pattern:** Directly adjusting `target_amount` downward via a migration without simultaneously checking that no goal's current balance already exceeds the new target.

---

## 5. Replay-Protection Consistency

**Contracts involved:** `data_migration`, any on-chain contract that consumes migration imports

**Invariant:** A snapshot with identity `(checksum, version)` can be applied to on-chain state at most once. The `MigrationTracker` records the timestamp of every successfully applied snapshot and rejects a second application of the same identity with `MigrationError::DuplicateImport`.

**Why it matters:** Double-application of the same migration payload produces double-counted balances, double-created goals, or double-paid bills — none of which can be reversed without a rollback.

**Where it is enforced:**
- `data_migration/src/lib.rs` — `MigrationTracker::mark_imported` / `import_from_json` / `import_from_binary`.
- `data_migration/src/lib.rs` — `verify_migration_completed` guards write entrypoints against proceeding before a migration has been marked complete; see §9 below.

**Common breakage pattern:** Using `import_from_json_untracked` / `import_from_binary_untracked` in a context where the same payload might be seen more than once. These helpers use a throwaway tracker and provide no cross-call duplicate protection.

---

## 6. Orchestrator Epoch Guard

**Contracts involved:** `orchestrator`

**Invariant:** A signed remittance flow request is only accepted when `actor_epoch == current_epoch` (strict equality). Any stale (`< current`) or future (`> current`) epoch value is rejected with `OrchestratorError::EpochMismatch`.

**Why it matters:** The epoch is a replay-barrier that invalidates all previously signed request hashes when the contract state changes in a breaking way (e.g. a contract upgrade or a key rotation). Without strict equality, a signed request captured before an upgrade could be replayed after it.

**Where it is enforced:**
- `orchestrator/src/lib.rs` — `verify_matching_epoch` (called from `execute_remittance_flow_signed`).
- `orchestrator/tests/dispute_epoch_guard.rs` — boundary tests covering current, prior (−1), ancient (0 after many bumps), future (+1), and a sweep including `u64::MAX`.

**Common breakage pattern:** Changing `==` to `>=` or adding a "staleness window" in the epoch check, which would allow prior-epoch tokens to be replayed.

---

## 7. Migration Schema Version Boundary

**Contracts involved:** `data_migration`, any consumer of migration imports

**Invariant:** Only schema versions in the inclusive range `[MIN_SUPPORTED_VERSION, SCHEMA_VERSION]` are accepted for import. Versions outside that range are rejected with `MigrationError::IncompatibleVersion`.

**Where it is enforced:**
- `data_migration/src/lib.rs` — `check_version_compatibility` and `ExportSnapshot::is_version_compatible`.
- `data_migration/src/lib.rs` — `upgrade_epoch_guard` tests in `#[cfg(test)] mod upgrade_epoch_guard_tests` cover the current boundary, one below (`MIN - 1`), ancient (0), one above (`SCHEMA_VERSION + 1`), and a representative sweep.

**Common breakage pattern:** Bumping `SCHEMA_VERSION` without updating `MIN_SUPPORTED_VERSION` (accepting ancient snapshots forever) or forgetting to bump `MIN_SUPPORTED_VERSION` when dropping support for old formats (accepting incompatible snapshots).

---

## 8. Killswitch Pause Propagation

**Contracts involved:** `emergency_killswitch`, `remittance_split`, `savings_goals`, `bill_payments`, `insurance`, `family_wallet`, `orchestrator`

**Invariant:** When the `emergency_killswitch` contract is in the `Paused` state, every downstream contract that calls `is_paused` must refuse all state-mutating entrypoints. No downstream contract may execute a write while the killswitch is active.

**Why it matters:** The killswitch is the last line of defence in an active incident. A contract that does not honour the pause signal can continue to drain funds or corrupt state even after operators have triggered the emergency stop.

**Where it is enforced:**
- Each downstream contract calls `emergency_killswitch::is_paused` at the top of its mutating entrypoints and returns a `ContractPaused` / `EmergencyPause` error if the result is `true`.
- `docs/PAUSE_PLAYBOOK.md` — operational runbook for triggering and clearing a pause.
- `docs/killswitch-trust-model.md` — who can pause, who can clear, what state is preserved.

**Common breakage pattern:** Adding a new mutating entrypoint and forgetting to add the pause check at the top of its body.

---

## 9. Migration-Completion Write Gate (`verify_migration_completed`)

**Contracts involved:** `data_migration`, any contract that applies migration imports

**Invariant:** Write operations on migrated state must not proceed until the migration has been explicitly marked complete. `verify_migration_completed` checks a completion flag on the `MigrationTracker` and returns `MigrationError::MigrationNotCompleted` if the flag is not set.

**Why it matters:** Without a completion gate, a contract could begin accepting live writes over a partially-applied migration, producing an inconsistent mix of migrated and un-migrated data. This is especially dangerous during a batched or multi-step migration that is interrupted mid-way.

**Where it is enforced:**
- `data_migration/src/lib.rs` — `MigrationTracker::mark_completed` / `verify_migration_completed`.

**Common breakage pattern:** Calling `verify_migration_completed` only on the happy path but omitting it from error-recovery paths, allowing a retried import to bypass the gate.

---

## Reviewer Checklist

Before merging any PR that touches two or more contracts, verify:

- [ ] The remittance split allocations still sum to `total_amount` (Invariant 1).
- [ ] All category percentages still sum to `10_000` in both the live contract and `data_migration` (Invariant 2).
- [ ] No new category was added without updating the remainder-assignment and the validation constant.
- [ ] `next_id` is never set below the highest assigned ID in any snapshot export path (Invariant 3).
- [ ] No goal's `current_amount` can exceed `target_amount` after the change (Invariant 4).
- [ ] Tracked import variants are used wherever replay protection is required (Invariant 5).
- [ ] The orchestrator epoch check still uses strict equality — not `>=` or `<=` (Invariant 6).
- [ ] `SCHEMA_VERSION` and `MIN_SUPPORTED_VERSION` are updated in lockstep (Invariant 7).
- [ ] Every new mutating entrypoint calls the killswitch pause check first (Invariant 8).
- [ ] Write paths that depend on a completed migration call `verify_migration_completed` (Invariant 9).

---

## See Also

- [`docs/PERIOD_INVARIANTS.md`](PERIOD_INVARIANTS.md) — time-bound period invariants and ledger timestamp rules
- [`docs/CROSS_CONTRACT_TIME.md`](CROSS_CONTRACT_TIME.md) — how ledger time is shared across contracts and why the host clock is authoritative
- [`docs/AMOUNT_INVARIANTS.md`](AMOUNT_INVARIANTS.md) — zero-amount handling across contract entrypoints
- [`docs/AUTHORIZATION_MATRIX.md`](AUTHORIZATION_MATRIX.md) — per-entrypoint caller authorization requirements
- [`docs/MIGRATIONS.md`](MIGRATIONS.md) — how to bump a contract spec without breaking existing storage
- [`scripts/verify_cross_contract_invariants.py`](../scripts/verify_cross_contract_invariants.py) — offline invariant verification script
