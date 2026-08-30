# Atomic Rollback and Compensating-Write Guarantees

## Scope

This document covers the atomicity invariants for `emergency_killswitch` —
specifically the `activate()` entry point and the `validate_approvals()` helper.
It describes the two bugs that were fixed, the design pattern that now enforces
atomicity at the application layer, and the test coverage that proves each
invariant.

Audience: contributors touching `emergency_killswitch/src/lib.rs`, security
reviewers, and operators auditing incident records.

---

## Why atomicity matters here

`activate()` is a high-impact, irreversible-until-recovered mutation: it writes
an activation marker (four storage keys) and then sets a pause flag.  If the
function returned an error after writing some but not all keys, the contract
would be left in an inconsistent state with no clean retry path:

- The activation marker keys (`ActivationEpoch`, `ActiveScope`, `RecoveryReadyAt`,
  `ScopeWasPaused`) are present → every subsequent `activate()` fails with
  `ActivationAlreadyActive`.
- The pause flag is absent → callers see the contract as unpaused yet the
  `activate` path is permanently locked.

Soroban transactions roll back at the _host_ level on panic/abort, but they do
_not_ roll back on a normal `Err(...)` return from a contract function.
Application-level errors are committed.  The only reliable way to prevent
partial state is to ensure all validation is complete before the first write.

---

## Bug 1: `activate()` partial-write hazard

### Root cause

The original implementation wrote the four activation metadata keys before
reaching the `LimitExceeded` check for `Function` scopes:

```rust
// OLD — writes first, then validates:
env.storage().instance().set(&DataKey::ActivationEpoch, &epoch);  // write 1
env.storage().instance().set(&DataKey::ActiveScope, &scope);      // write 2
env.storage().instance().set(&DataKey::RecoveryReadyAt, &...);    // write 3
env.storage().instance().set(&DataKey::ScopeWasPaused, &...);     // write 4
// ...
if paused.len() >= MAX_PAUSED_FUNCTIONS {
    return Err(Error::LimitExceeded);  // ← error AFTER 4 writes committed
}
```

When `LimitExceeded` was returned, writes 1–4 were committed but the actual
pause flag was never set.  Subsequent calls to `activate()` would find the
`ActivationEpoch` key present and immediately return `ActivationAlreadyActive`,
permanently blocking new activations.

### Fix: validate-everything then write-everything

The rewritten `activate()` uses a strict two-phase structure:

**Phase 1 — validation only (no writes):**
1. Empty approvals check
2. Epoch match
3. Already-active check
4. `validate_approvals()` quorum/duplicate check
5. Read current scope state (needed for `ScopeWasPaused`)
6. **For `Function` scope**: read the paused list, check capacity, build the
   updated list — return `LimitExceeded` here if needed

**Phase 2 — writes only (all checks passed):**
1. Write `ActivationEpoch`
2. Write `ActiveScope`
3. Write `RecoveryReadyAt`
4. Write `ScopeWasPaused`
5. Write the scope pause flag / updated function list
6. Emit `activated` event

Phase 1 is read-only.  If any check fails, zero writes have occurred and the
contract state is unchanged.  Phase 2 is unconditionally committed because
there are no error returns after it begins.

### Invariant

**After any failed `activate()` call, the following keys must be absent:**
- `DataKey::ActivationEpoch`
- `DataKey::ActiveScope`
- `DataKey::RecoveryReadyAt`
- `DataKey::ScopeWasPaused`

**Proof:** Checked by `activate_function_scope_limit_exceeded_leaves_no_partial_state`
in `emergency_killswitch/tests/test_killswitch.rs`.  The test fills the
function list to capacity, attempts a `Function`-scope activation (which must
return `LimitExceeded`), and then verifies that a valid `Global`-scope
activation succeeds — a result that would be impossible if the activation
marker had been written by the failed attempt.

---

## Bug 2: `validate_approvals()` first-element duplicate skip

### Root cause

The original duplicate-detection loop was gated with `if accepted > 0`:

```rust
// OLD — skips duplicate check for the very first element:
let mut accepted = 0u32;
for approval in approvals.iter() {
    // ...
    if accepted > 0 {           // ← gate: skipped on first iteration (accepted == 0)
        for seen in approvals.iter() {
            // check for duplicates
        }
    }
    accepted += 1;
}
```

On the first iteration `accepted == 0`, so the inner loop was skipped entirely.
A list like `[A, A]` would pass: the first `A` was never checked against itself
or any subsequent `A`.

### Fix: index-based O(n²) comparison without the gate

```rust
// NEW — every element i is compared against all prior elements 0..i:
let n = approvals.len();
for i in 0..n {
    let approval = approvals.get(i).unwrap();
    if !signers.contains(&approval) {
        return Err(Error::SignerNotConfigured);
    }
    for j in 0..i {
        if approvals.get(j).unwrap() == approval {
            return Err(Error::DuplicateApproval);
        }
    }
}
```

The gate is gone.  Every element is compared against every prior element.
The first element (index 0) has no prior elements, so it is checked against
the signer list only — it cannot be a duplicate of itself.  The second element
(index 1) is checked against the first, and so on.

### Invariant

**`[A, A]` must always return `DuplicateApproval`, regardless of position.**

**Proof:** Checked by `validate_approvals_catches_first_element_duplicate` in
`emergency_killswitch/tests/test_killswitch.rs`.

---

## Failure and state invariant matrix

| Failure condition | Phase failed | Writes committed | Activation marker present? |
|---|---|---|---|
| Empty approvals | Phase 1 (early return) | None | No |
| Epoch mismatch | Phase 1 | None | No |
| Already active | Phase 1 | None | Yes (from a prior successful activation) |
| Unknown signer | Phase 1 (`validate_approvals`) | None | No |
| Duplicate approval | Phase 1 (`validate_approvals`) | None | No |
| Below threshold | Phase 1 (`validate_approvals`) | None | No |
| LimitExceeded (Function) | Phase 1 (new) | **None** (was: four keys) | No |
| All checks pass | Phase 2 | All five keys + event | Yes |

---

## Recovery invariants (unchanged)

`recover()` was already implemented with read-before-write semantics:

1. Reads the activation epoch, ready-at timestamp, scope, and `scope_was_paused`.
2. Validates all conditions (active, delay elapsed, approvals).
3. Clears the pause flag / restores scope state.
4. Removes all four activation marker keys in a single atomic pass.
5. Emits the `recovered` event.

This ordering means that if `recover()` returns an error (e.g., `RecoveryTooEarly`,
`EpochMismatch`), no writes have occurred.  The activation marker is only removed
after all checks pass.

---

## Admin rotation invariants (unchanged)

`transfer_admin()` is atomic by construction:
1. Epoch check (guards against replayed authorizations).
2. `admin.require_auth()`.
3. Reject self-address and contract address.
4. Single write: `DataKey::Admin ← new_admin`.
5. Emit `AdminTransferred` event.

There is no partial state possible: the admin key is either the old address or
the new one.  The `KillSwitchEpoch` epoch guard ensures that an authorization
captured at epoch N cannot be replayed after `bump_kill_switch_epoch()` has
advanced the epoch.

---

## Pause/unpause invariants (unchanged)

Admin-only `pause()`, `pause_with_reason()`, `unpause()`, and
`clear_emergency_state()` each follow a simple read-auth-then-write pattern with
no conditional early returns after the first write.  They were not affected by
the atomicity gap fixed here.

---

## Migration and rollback considerations

This change does not add or remove any storage keys.  The storage layout is
identical to the pre-fix contract.  A rollback to the pre-fix binary:

- Would re-introduce the partial-write hazard for new `activate()` calls.
- Would re-introduce the first-element duplicate-skip in `validate_approvals`.
- Would **not** corrupt or misread any state written by the fixed binary.

If a rollback is performed while an activation is active, the activation
markers written by the fixed binary are fully compatible with the pre-fix
recovery path.

---

## Security assumptions

1. **Soroban host atomicity**: the host rolls back all storage changes on
   panic/abort.  Application-level `Err` returns do not trigger host rollback;
   the validate-first pattern is the only guard at the application layer.
2. **No reentrancy**: Soroban does not support reentrant contract calls.  There
   is no window between Phase 1 and Phase 2 in which another call can observe
   intermediate state.
3. **`validate_approvals` is called before every activate**: callers cannot
   bypass the approval check by passing an empty list (caught by the empty-list
   guard before `validate_approvals` is reached).
4. **`LimitExceeded` check in Phase 1 is complete**: the new code reads the
   current paused-function list in Phase 1 and returns `LimitExceeded` if
   capacity is at `MAX_PAUSED_FUNCTIONS` (10).  No write has occurred at the
   point of this return.

---

## Test coverage summary

All regression tests are in `emergency_killswitch/tests/test_killswitch.rs`
under the `Atomic rollback / compensating-write regression tests` section.

| Test name | What it verifies |
|---|---|
| `activate_global_writes_all_metadata_atomically` | Successful Global activation sets the global pause |
| `activate_module_scope_sets_module_paused_flag_atomically` | Successful Module activation sets the module flag |
| `activate_function_scope_adds_function_to_paused_list_atomically` | Successful Function activation adds the function to the list |
| `activate_function_scope_limit_exceeded_leaves_no_partial_state` | **Core fix**: LimitExceeded leaves no activation marker |
| `validate_approvals_catches_first_element_duplicate` | **Core fix**: `[A, A]` is now rejected |
| `validate_approvals_catches_non_first_element_duplicate` | Non-first duplicate continues to be rejected |
| `repeated_activation_always_blocked_after_first_succeeds` | `ActivationAlreadyActive` for all repeated calls |
| `recover_global_scope_leaves_clean_slate_for_new_activation` | Full lifecycle: activate → recover → re-activate |
| `recover_module_scope_restores_pre_activation_paused_state` | Recovery preserves pre-activation module paused state |
| `recover_module_scope_clears_pause_flag_when_not_pre_paused` | Recovery clears module flag when not pre-paused |
| `recover_function_scope_removes_function_from_paused_list_when_not_pre_paused` | Recovery removes function from list |
| `recover_function_scope_preserves_function_in_paused_list_when_pre_paused` | Recovery preserves pre-paused function |
| `activation_with_wrong_epoch_leaves_no_state` | Epoch mismatch: no state written |
| `activation_with_empty_approvals_rejected_before_state_writes` | Empty list: no state written |
| `activation_with_unknown_signer_rejected_before_state_writes` | Unknown signer: no state written |
| `activation_with_insufficient_approvals_rejected_before_state_writes` | Below threshold: no state written |
