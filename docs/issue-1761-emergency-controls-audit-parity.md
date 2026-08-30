# Issue #1761 — Emergency & Administrator Controls: Events and Audit Parity

**Status:** Implemented  \
**Contract:** `emergency_killswitch`  \
**Issue:** https://github.com/Remitwise-Org/Remitwise-Contracts/issues/1761  \
**Area:** authorization / resilience — events and audit parity for pause, recovery, threshold approval, and administrator rotation.

---

## 1. Objective

Make pause, recovery, threshold approval, and administrator rotation **bounded,
auditable, and safe during incidents**, by providing a deterministic,
reviewable guarantee that the on-chain event log **matches committed state**
under normal, invalid, repeated, concurrent, and failure conditions.

The concrete deliverables from the issue:

1. **Versioned, complete records only for committed transitions, with
   correlation identifiers and documented ordering.**
2. **Preserve compatible public behavior**; make any migration, error, or
   response-shape change explicit.
3. **Rejected, stale, repeated, and failed operations leave no unauthorized or
   partial state** (and emit no events).
4. **Focused regression coverage** proving the invariant at the actual
   integration boundary.

---

## 2. Design

### 2.1 New versioned control/audit event stream

Every **committed** emergency transition now publishes a versioned,
correlation-tagged record on a fixed topic:

```
topic: ("emergency", "control")
data:  ControlEvent { version, seq, kind, actor, timestamp }
```

| Field | Type | Meaning |
| ----- | ---- | ------- |
| `version` | `u32` | Event schema version (`EVENT_VERSION`, currently `1`). Bump only via the documented upgrade workflow when the on-wire shape changes. |
| `seq` | `u64` | Monotonic per-contract correlation identifier / ordering key. |
| `kind` | `Symbol` | Operation symbol (`init`, `pause`, `unpause`, `schedule`, `cleared`, `mpause`, `munpause`, `fpause`, `funpause`, `signers`, `activated`, `recovered`, `epch_bump`, `admn_xfer`, `migr`, `snap_pre`, `snap_rst`, `snap_dsc`). |
| `actor` | `Option<Address>` | The authorizing principal; `None` for consensus-driven transitions (threshold activation / recovery) where no single address is the actor. |
| `timestamp` | `u64` | Ledger timestamp at commit. |

### 2.2 Correlation identifier and documented ordering

A new instance-storage key `DataKey::EventSeq` holds a monotonic counter.
`next_event_seq()` reads, increments, and persists it; every `ControlEvent`
consumes the next value. Because the counter is persisted and strictly
increasing:

- each record is **uniquely correlated** (`seq` is the correlation id), and
- the audit stream has a **deterministic global ordering**, observable
  off-chain via the new read-only `get_event_seq()` view.

Ordering rules:

- Within one transition: the granular per-transition event (e.g. `paused_v2`)
  is published first, then the `control` record.
- Across transitions: strictly ordered by `seq` (which also equals the order
  in which state mutations committed).

### 2.3 Committed-only emission

`emit_control_event` is invoked **only at the commit point** — after every
state mutation for the transition has succeeded and immediately before the
entry point returns `Ok`. All validation, authorization, and error paths return
`Err` (or panic, which Soroban rolls back atomically) **before** reaching the
emit call. Therefore:

- Rejected operations (bad auth, invalid args, wrong epoch, too-early
  recovery, duplicate init, invalid schedule, self-transfer, …) emit **no**
  control event and **do not advance** `seq`.
- Repeated no-op operations (re-pausing an already-paused function,
  unpausing a non-paused function) emit nothing, matching the granular events.
- Failed operations leave no unauthorized or partial state; Soroban's atomic
  transaction semantics roll back any partial writes when the top-level call
  returns `Err` or panics.

### 2.4 Where the record is emitted

Every write entry point emits one `control` record on success:

`initialize`, `configure_signers`, `activate`, `recover`,
`bump_kill_switch_epoch`, `transfer_admin`, `pause` / `pause_with_reason`,
`unpause`, `schedule_unpause`, `clear_emergency_state`, `pause_module`,
`unpause_module`, `pause_function`, `unpause_function`, `migrate_storage`,
`pre_upgrade`, `restore_from_snapshot`, `discard_snapshot`.

`pause_function` / `unpause_function` emit only when the transition actually
changes state (idempotent repeats emit nothing). `schedule_unpause`, which
previously had **no** event at all, is now auditable via the control stream
(`kind = "schedule"`).

---

## 3. Invariants

1. **State↔event parity:** for every committed transition, the `control`
   record's `timestamp`/`actor`/`kind` match the observable committed state
   (e.g. `pause.timestamp == get_paused_since()`, `admn_xfer.actor == old
   admin`).
2. **Strict ordering:** `seq` starts at 1 and strictly increases with each
   committed transition; `get_event_seq()` equals the last emitted `seq`.
3. **Versioning:** every record carries `version == EVENT_VERSION`.
4. **No-emission on rejection:** rejected / stale / repeated / failed
   operations emit no `control` record, do not advance `seq`, and leave no
   partial state.
5. **Consensus actor semantics:** `activate` / `recover` records carry
   `actor = None` (quorum-driven, no single principal); all admin-driven
   transitions carry the authorizing admin.

---

## 4. Compatibility & migration

### 4.1 Public behavior

- **No public function signature changes.** Every existing entry point keeps
  its exact signature, return type, and error behavior.
- **Additive events only.** The `("emergency", "control")` topic is new; all
  legacy granular events (`paused_v2`, `admn_xfer`, `signers_set`, …) are
  emitted unchanged, so existing indexers keep working.
- **`CONTRACT_VERSION` bumped `1 → 2`** to let off-chain tooling detect the
  new audit-event behavior. `version()` is a pure read; no auth required.
- **`STORAGE_VERSION` unchanged (`1`)**. The new `EventSeq` key is additive
  and lazily initialized (`get().unwrap_or(0)`), so existing deployments need
  **no migration** and read/write the counter correctly without a
  `migrate_storage` call. A `STORAGE_VERSION` bump is deferred until a
  genuinely breaking layout change occurs.

### 4.2 Snapshot encoding change (pre-upgrade snapshots)

`EmergencyStateSnapshot.active_scope` was previously `Option<PauseScope>`.
`soroban-sdk`'s `#[contracttype]` spec only derives fallible `TryFrom` for
custom contract types, while `Option<T>` requires infallible `From<T>` — so
`Option` of a custom enum/struct is **not spec-serializable** in this SDK and
the contract's test build failed to compile (pre-existing breakage on `main`).
The field is now encoded as spec-scalar fields:

```rust
pub scope_kind: u32,        // 0 = none, 1 = global, 2 = module, 3 = function
pub scope_module: Symbol,   // module id for module/function scopes
pub scope_function: Symbol, // function id for function scopes
```

`pre_upgrade` encodes the active `PauseScope` into these fields and
`restore_from_snapshot` reconstructs it. Semantics are unchanged.

**Migration note for operators:** snapshots are transient (24 h TTL) and are
meant to be captured immediately before an upgrade and restored immediately
after. A snapshot captured by a pre-change build and restored by this build
may fail to deserialize (the old XDR has no `scope_kind` fields). Use
`discard_snapshot()` after upgrading and capture a fresh snapshot if a
rollback plan is needed.

### 4.3 Gas impact

Each committed transition now performs one instance-storage read/write
(`EventSeq`) plus one event publish. Gas benchmarks in
`emergency_killswitch/tests/gas_bench.rs` were re-measured and their baselines
updated to the new steady-state values (write paths grew ~10–40%; read-only
views drifted with the toolchain). See section 6 for the re-measurement
procedure.

### 4.4 Rollback

Reverting this change removes the `("emergency", "control")` stream and the
`EventSeq` key. No data depends on the counter; the contract remains fully
functional without it. The snapshot struct change and `CONTRACT_VERSION`
bump revert cleanly with the code.

---

## 5. Security assumptions & operational limitations

- **Auth is unchanged and enforced before any mutation:** admin-gated
  entry points call `require_auth()` before touching state; threshold
  entry points validate the signer quorum and epoch.
- **The audit stream is not a substitute for authorization.** It makes
  committed transitions reviewable and deterministic; it does not grant or
  restrict permissions.
- **Consensus transitions have no single actor** (`actor = None`); the
  approving signer set remains visible via `get_signer_epoch` /
  `get_signer_threshold` and the granular `activated`/`recovered` events.
- **Event ordering is per-contract.** `seq` is scoped to this contract's
  instance storage; cross-contract correlation is out of scope.
- **Rejected attempts are not recorded.** If monitoring rejected attempts is
  required, that must be a separate, explicit opt-in surface (out of scope).
- **`get_event_seq()` is read-only and unauthenticated** — it exposes the
  counter only; no privileged state.

---

## 6. Validation evidence

### 6.1 Focused regression tests — `emergency_killswitch/tests/audit_parity.rs`

Drive the real contract entry points and decode the events Soroban actually
recorded, proving the invariants at the integration boundary:

| Test | Proves |
| ---- | ------ |
| `lifecycle_emits_versioned_ordered_control_events_matching_state` | one versioned record per committed transition; `seq` = 1..4 strictly monotonic; kinds in order; actor + timestamp match committed state (`get_paused_since()`) |
| `rejected_operations_emit_no_control_event_and_leave_no_state` | double-init, schedule-less unpause, self-transfer, past-schedule — all rejected with **zero** control events, `seq` unchanged, no partial state |
| `repeated_function_pause_is_idempotent_and_emits_nothing` | repeated pause/unpause of the same function emits nothing and does not advance `seq` |
| `activation_recovery_emit_actor_none_and_rejections_emit_nothing` | threshold activation/recovery record `actor = None`; wrong-epoch and too-early rejections emit nothing and leave no partial activation state |
| `transfer_admin_records_old_admin_then_new_admin_acts` | admin rotation records the outgoing admin as `actor`; the new admin is the actor for subsequent ops |
| `module_pause_and_clear_emit_admin_actor_control_events` | module pause/unpause and `clear_emergency_state` emit admin-actor records |
| `epoch_bump_snapshot_and_migration_controls_emit_control_events` | epoch bump, snapshot pre/restore/discard all emit committed control events |

### 6.2 Commands & results (rustc 1.98.0, stable, wasm32 targets installed)

```bash
# Unit + integration + gas benchmarks for the contract
cargo test -p emergency_killswitch
#   72 passed (lib) / 7 passed (audit_parity) / 15 passed (gas_bench) / 41 passed (test_killswitch)

# Clippy (all targets, warnings denied) + unwrap/expect ban (lib)
cargo clippy -p emergency_killswitch --all-targets -- -D warnings
cargo clippy -p emergency_killswitch --lib -- -D clippy::unwrap_used -D clippy::expect_used

# Format
cargo fmt -p emergency_killswitch -- --check

# WASM release build
cargo build --release --target wasm32-unknown-unknown -p emergency_killswitch
#   -> emergency_killswitch.wasm (~71 KB)
```

Gas baseline re-measurement (the intended workflow for deliberate cost
increases):

```bash
cargo test -p emergency_killswitch --test gas_bench -- --nocapture
# copy the printed GAS_BENCH_RESULT lines into the RegressionSpec constants
```

### 6.3 Pre-existing breakage fixed on the way (documented, in-contract)

The `emergency_killswitch` test build did not compile on `main` (all of the
following predate this change and were verified by stashing this change):

- `client.configure_signers(&admin, &signers, 2)` passed an unborrowed `u32`
  in four lib tests → changed to `&2` / `&1`.
- `EmergencyStateSnapshot.active_scope: Option<PauseScope>` is not
  spec-serializable in soroban-sdk → encoded as spec-scalar fields (see §4.2).
- `migration_progress_reflects_completed_state` assumed a nonzero default
  ledger timestamp (default is 0) → sets a nonzero base timestamp.
- `restore_from_snapshot_fails_after_ttl` triggered a
  "ledger.try_borrow failed" host quirk in soroban-env-host 21.2.1 when
  advancing from timestamp 0 via `with_mut` → uses `set_timestamp` from a
  nonzero base.
- Gas baselines were stale for the current toolchain and for the new audit
  cost → re-measured and updated.

**Known pre-existing issues outside this contract** (present on `main`,
unrelated to #1761): `bill_payments/src/test.rs` contains a parse error
(`cargo fmt --all` fails), `data_migration` test build has comparison errors,
and other crates may fail their own test builds. These are out of scope for
this focused change.
