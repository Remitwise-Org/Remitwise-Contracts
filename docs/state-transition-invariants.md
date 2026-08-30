# Emergency Kill-Switch — State-Transition Invariants

**Added in:** this PR (closes #state-transition-invariants)  
**Contract version:** `CONTRACT_VERSION = 2`  
**File:** `emergency_killswitch/src/lib.rs`

---

## 1  Design

### 1.1  State machine

The kill switch has exactly four observable states, derived at call-time from two
boolean storage keys (`ActivationEpoch` presence and `GlobalPaused` value):

```
                         pause / pause_with_reason
                        ┌──────────────────────────────────┐
                        │ (idempotent; second call ok)      │
                        ▼                                   │
             ┌──────────────────┐                           │
 initialize  │                  │  clear_emergency_state    │
 ───────────▶│      Idle        │◀──────────────────────────┤
             │                  │                           │
             └────────┬────┬────┘                           │
                      │    │                                │
            activate  │    │ pause / pause_with_reason      │
          (threshold) │    └────────────────────────────────┘
                      │                                     ▲
                      ▼                                     │
             ┌──────────────────┐    unpause /              │
             │                  │    clear_emergency_state  │
             │ ThresholdActive  │───────────────────────────┘
             │                  │
             └────────┬─────────┘
                      │ recover (after RECOVERY_DELAY)
                      ▼
                    Idle
```

**AdminPaused** is the intermediate state between `Idle` and the bottom arc:
`Idle →(pause)→ AdminPaused →(unpause | clear)→ Idle`.

Full four-state enumeration (derived, not stored):

| State | `ActivationEpoch` present | `GlobalPaused` |
|---|:---:|:---:|
| `Idle` | no | false |
| `AdminPaused` | no | true |
| `ThresholdActive` | yes | false |
| `ThresholdActiveAndAdminPaused` | yes | true |

### 1.2  Transition matrix

| Operation | Legal from | Illegal from | Error on illegal |
|---|---|---|---|
| `pause` / `pause_with_reason` | `Idle`, `AdminPaused` | `ThresholdActive`, `ThresholdActiveAndAdminPaused` | `ActivationAlreadyActive` |
| `unpause` | `AdminPaused` | `Idle` (→ `NotActive`), `ThresholdActive`, `ThresholdActiveAndAdminPaused` | `ActivationAlreadyActive` (threshold), `NotActive` (idle) |
| `schedule_unpause` | `AdminPaused` | `Idle`, `ThresholdActive`, `ThresholdActiveAndAdminPaused` | `NotActive` / `ActivationAlreadyActive` |
| `activate` | `Idle` | `AdminPaused`, `ThresholdActive`, `ThresholdActiveAndAdminPaused` | `ActivationAlreadyActive` |
| `recover` | `ThresholdActive`, `ThresholdActiveAndAdminPaused` | `Idle`, `AdminPaused` | `NotActive` |
| `clear_emergency_state` | **any** (admin bypass) | — | — |
| `transfer_admin` | **any** (epoch-gated) | — | `EpochMismatch` on stale epoch |
| `configure_signers*` | **any** | — | — |
| `pause_module/function` | **any** | — | — |
| `unpause_module/function` | **any** | — | — |

### 1.3  Enforcement point

Each guarded entry point calls `current_state(&env)` as the **first** statement
after `require_auth()`.  The state read is pure (no writes); the guard fires
before any mutation so a rejection leaves **zero orphaned storage keys**.

The implementation follows the pre-existing validate-first / write-second
pattern already present in `activate`.

---

## 2  New public surface

### `KillSwitchState` enum

```rust
pub enum KillSwitchState { Idle, AdminPaused, ThresholdActive, ThresholdActiveAndAdminPaused }
```

Not stored on-chain. Derived by `current_state(&env)`. Available to off-chain
tooling via the crate's test interface.

### `TransitionError` contracterror

```rust
#[contracterror]
pub enum TransitionError { IllegalTransition = 1 }
```

A separate `#[contracterror]` type so the existing `Error` enum ABI is
unchanged. Currently used internally; may be propagated in future versions.

> **Note:** The guards currently return `Error::ActivationAlreadyActive` instead
> of `TransitionError::IllegalTransition` so that callers using the existing
> numeric error codes are unaffected. `TransitionError` is defined now for
> future use and is regression-tested by its discriminant assertion.

### Free helper functions

```rust
pub fn checked_add_u64(a: u64, b: u64) -> Result<u64, Error>
pub fn checked_add_u32(a: u32, b: u32) -> Result<u32, Error>
pub fn snapshot_age(now: u64, taken_at: u64) -> Result<u64, Error>
pub fn recovery_ready_at(now: u64) -> Result<u64, Error>
pub fn current_state(env: &Env) -> KillSwitchState
```

All return `Error::Overflow` on arithmetic wrap; `snapshot_age` returns
`Overflow` on inverted clock (`now < taken_at`).

---

## 3  Error variant change — `Error::Overflow`

| Variant | Old discriminant | New discriminant |
|---|:---:|:---:|
| `InvalidCursor` | 20 | **20** (unchanged) |
| `Overflow` | _missing (compile error)_ | **21** |

`Error::Overflow` was referenced throughout the codebase but never declared.
Two test assertions incorrectly stated its value as `20` (same as
`InvalidCursor`, which is a compile error in Rust). Both assertions are
corrected to `21`.

**Compatibility impact:** The previous WASM did not compile; there is no
deployed version with `Overflow` at `20`. This is a bug fix, not a breaking
change.

---

## 4  Activate — overflow fix for recovery deadline

`activate` previously computed the recovery deadline as:

```rust
env.ledger().timestamp().saturating_add(RECOVERY_DELAY)
```

`saturating_add` silently caps at `u64::MAX` rather than signalling overflow,
which can produce a recovery deadline that is already in the past for
timestamps near `u64::MAX`.  This is now:

```rust
let ready_at = recovery_ready_at(env.ledger().timestamp())?;  // → Err(Overflow) near u64::MAX
```

The existing test `activate_overflows_near_u64_max_without_writing_markers` in
`threshold_tests.rs` continues to pass — the `Overflow` error is the same
result, just now signalled via checked arithmetic rather than saturation.

---

## 5  Failure behavior

| Failure scenario | State after failure |
|---|---|
| `activate` from `AdminPaused` | `AdminPaused` — no activation markers written |
| `activate` with bad approvals | `Idle` or `AdminPaused` — no activation markers written |
| `activate` function cap exceeded | Unchanged — no activation markers, function list unchanged |
| `pause` from `ThresholdActive` | `ThresholdActive` — activation markers intact, global pause unchanged |
| `unpause` from `ThresholdActive` | `ThresholdActive` — activation markers intact, global pause unchanged |
| `recover` with stale epoch | Activation markers preserved; pause state unchanged |
| `recover` before `ready_at` | Activation markers preserved; pause state unchanged |
| `transfer_admin` with stale epoch | `DataKey::Admin` unchanged |
| `configure_signers` with duplicate | `SignerEpoch` unchanged |

All failures are enforced before the first storage write. Soroban's atomic
transaction semantics provide a further backstop: any unexpected panic after a
write has begun rolls back the entire transaction.

---

## 6  Migration / rollback considerations

- No on-chain key layout change.  `STORAGE_VERSION` is unchanged.
- `DataKey` enum is unchanged — no migration required.
- The new free functions and types are additive.
- The `Error::Overflow = 21` addition is additive (new discriminant, no rename).
- The two corrected test assertions (`Overflow as u32, 20 → 21`) are test-only;
  they do not affect the on-chain ABI.

---

## 7  Operational limitations

- **`clear_emergency_state` intentionally bypasses all transition guards.**
  It is the admin's last resort and must remain unrestricted.
- The `current_state` probe is a point-in-time read.  In a multi-transaction
  sequence an external actor can change state between the probe and the write;
  Soroban's single-transaction atomicity means this cannot happen within a
  single invocation.
- `pause_module` / `unpause_module` / `pause_function` / `unpause_function` are
  not guarded by the transition matrix: they operate on scopes independent of
  the global pause and do not conflict with threshold-quorum activation.

---

## 8  Security assumptions

1. The admin private key is not compromised.  `transfer_admin` and
   `bump_kill_switch_epoch` are the admin's tools to rotate credentials.
2. A threshold quorum (`>= threshold` of configured signers) is needed to
   activate or recover.  An attacker controlling fewer than `threshold` signers
   cannot trigger or clear an activation.
3. The `RECOVERY_DELAY` (3 600 s / 1 hour) provides a mandatory observation
   window between quorum-triggered activation and recovery, during which the
   admin may intervene via `clear_emergency_state`.
4. Signer-epoch rotation invalidates any outstanding approval bundle
   immediately; stale approvals cannot be replayed.

---

## 9  Test coverage added

New module: `state_transition_invariant_tests` in `src/lib.rs`.

| Category | Count |
|---|:---:|
| Legal transition edges | 6 |
| Illegal transition edges (with invariant probes) | 5 |
| Repeated / idempotent | 4 |
| Stale / out-of-order | 4 |
| Concurrent / interleave | 3 |
| Failure-path invariants (partial state probes) | 4 |
| `KillSwitchState` derivation | 4 |
| Helper function unit tests | 8 |
| **Total new tests** | **38** |
