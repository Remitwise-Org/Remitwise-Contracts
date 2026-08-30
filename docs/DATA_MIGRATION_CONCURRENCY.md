# Data Migration: Concurrency, Conflict, and Reconciliation Contract

This document records the design and the reviewable guarantee behind the
concurrency hardening of `data_migration` (tracker determinism,
`SharedMigrationTracker`, reconciliation queries). It complements the rustdoc
on `SharedMigrationTracker` and the crate-level docs in
`data_migration/src/lib.rs`.

## Background

Migration runners and indexers import payment/settlement snapshots
(`ExportSnapshot`) through `import_from_json` / `import_from_binary`, guarded by
a `MigrationTracker` whose job is to make double-application impossible.
Two gaps existed at that boundary:

1. **No concurrency-safe shared tracker.** `MigrationTracker`'s mutation API
   requires `&mut self`, so workers on different threads could not share one
   tracker without hand-rolled synchronization — the classic place where
   check-then-set splits apart and two concurrent imports of the *same*
   snapshot both observe "not imported" and both apply (double settlement).
   The crate also did not define what a client should do when two requests
   race: what the conflict response is, and whether to retry it.
2. **No deterministic reconciliation surface.** The tracker stored identities
   in a `HashMap`, so iterating or serializing tracker state produced a
   randomized order (measured: 50/50 distinct byte strings for logically
   identical state), and no API existed to enumerate the applied set at all.
   Post-upgrade reconciliation — "prove every settlement record migrated,
   exactly once, with nothing missing" — was not possible from the crate.

## Design

### Serialization (conflict) behavior for concurrent requests

`SharedMigrationTracker` wraps `Mutex<MigrationTracker>` and each import
method holds the lock across **validation and the replay commit as one atomic
section**. Concurrent calls serialize through the lock, so for a given
`(checksum, version)` snapshot identity:

- exactly one caller observes `Ok(snapshot)`;
- every other concurrent caller deterministically receives
  `Err(MigrationError::DuplicateImport)`;
- a rejected caller mutates no tracker state under any interleaving.

The tracker content after any concurrent burst is therefore
schedule-independent: it is exactly the set of distinct valid identities
submitted. Response shapes are unchanged — the existing
`MigrationError::DuplicateImport` variant is the conflict response; no new
error variant was introduced.

### Client retry contract

| Outcome | Meaning | Client action |
|---|---|---|
| `Ok(snapshot)` | Validated and applied exactly once | None — do not resubmit |
| `Err(DuplicateImport)` | Identity already applied by this or a concurrent request | Treat as idempotent success; **do not retry** the same bytes — the rejection is deterministic and permanent |
| `Err(_)` (version / size / checksum / semantic) | Permanent rejection; tracker state untouched | **Do not retry** identical bytes; fix the payload, then submit (the fixed payload is a new identity) |
| After `RollbackMetadata::restore` / `restore_rollback` | The failed identity was un-marked | **Retry permitted** — the only path that makes a recorded identity importable again |

### Deterministic, gap-free reconciliation queries

- `MigrationTracker::imported_records()` (also on the shared facade) enumerates
  every applied identity exactly once, sorted by `(checksum, version)` —
  independent of application order, thread scheduling, and process. A record
  exists **iff** a fully validated import committed; rejected, rolled-back,
  and failed operations leave no entry. Comparing the returned set against the
  import manifest proves both directions: a missing identity was never applied
  (safe to retry), a present identity was applied exactly once.
- `imported_count()` / `contains_identity()` provide the cheap checks.
- Tracker storage moved from `HashMap` to `BTreeMap`, so serde output is
  deterministic: logically identical trackers serialize to identical bytes and
  persisted reconciliation artifacts can be diffed/reviewed across runs.

## Invariants

1. **Unique application.** No `(checksum, version)` identity is ever recorded
   twice; an `ImportRecord` implies a fully validated, committed import.
2. **No partial state.** Rejected (invalid, oversized, incompatible),
   conflicting, and failed operations insert nothing and remove nothing.
3. **Deterministic enumeration and serialization.** Equal logical state →
   equal `imported_records()` and equal serialized bytes.
4. **Retry contract stability.** Re-submitting an applied identity is always
   `DuplicateImport`, forever, unless an explicit rollback un-marks it.
5. **Completion gating.** `verify_completed` returns
   `MigrationNotCompleted` until `mark_completed` is called; gate operations
   never create import state.

## Failure behavior

| Failure | Observable behavior |
|---|---|
| Forged/tampered snapshot (bad checksum) | `ChecksumMismatch`; tracker count unchanged; retrying with corrected bytes succeeds |
| Oversized envelope / payload | `SnapshotTooLarge` / `PayloadTooLarge` pre-deserialization; no state change |
| Incompatible version | `IncompatibleVersion`; no state change |
| Semantic invariant violation (split ≠ 100, wound-back `next_id`, `current > target`) | `ValidationFailed`; no state change |
| Panic while a worker holds the lock | Mutex is recovered (see below); recovered tracker is consistent because every mutation under the lock is a single atomic check-then-set with no multi-step invariants |
| Apply failure after commit (downstream step) | `RollbackMetadata::capture` before mutation + `restore`/`restore_rollback` on failure: state returns to the captured pre-image, the identity is un-marked (retryable), restore is idempotent |

## Compatibility impact

- **Public API:** additive only. New items: `SharedMigrationTracker`,
  `ImportRecord`, `MigrationTracker::{imported_records, imported_count,
  contains_identity}`. No existing signature, error variant, or behavior
  changed; `MigrationTracker` field layout was already private.
- **Serialized tracker bytes:** the on-the-wire shape (map of
  `(String, u32) → u64`, plus `completed` flag) is unchanged; only the emitted
  entry order is now sorted. Bytes written by the previous representation
  deserialize into the current one (order is irrelevant on decode), and vice
  versa — verified by `test_tracker_deserializes_legacy_hashmap_layout`.
- **Migration needed:** none. Existing persisted trackers load unmodified.

## Migration / rollback considerations

- Rollback semantics are unchanged: capture before mutation, restore on
  failure, restore is idempotent, restore un-marks only the attempted identity.
- `SharedMigrationTracker::restore_rollback` performs restore under the same
  lock as imports, so a rollback cannot interleave with an in-flight import of
  the same identity.
- A tracker can be snapshotted (`snapshot()`), persisted deterministically,
  and re-adopted later via `SharedMigrationTracker::from_tracker` — the
  supported save/restart path for long-running migrations (incl. ledger
  upgrades that pause and resume settlement imports).

## Operational limitations

- Imports serialize through a single lock. Validation is CPU-bound and
  snapshot size is capped (`MAX_MIGRATION_SNAPSHOT_BYTES`), so lock hold time
  is small; the type is designed for correctness under contention, not maximal
  import throughput. Batch workers should shard by disjoint payload sets if
  throughput ever becomes the bottleneck.
- `imported_at_ms` is caller-supplied; the enumeration order is **identity**
  order, not time order, and equal/zero timestamps cannot perturb it.
- The untracked import helpers (`import_from_json_untracked`,
  `import_from_binary_untracked`) remain one-shot conveniences with no
  cross-call replay protection; the concurrency contract does not extend to
  them. Use the shared facade for any concurrent or retryable flow.

## Security assumptions

- The checksum binding (version + format + canonical payload) is unchanged;
  this change neither strengthens nor weakens tamper detection, and the
  `enc:v1:` format remains an encoding-only marker (no cryptography).
- Poisoned-lock recovery is sound **only because** tracker mutations are
  single check-then-set operations. If a future change introduces multi-step
  invariants under the lock, recovery must be revisited (or the change should
  roll back partial work before unwinding).
- Duplicate detection is scoped to one tracker instance. Callers distributing
  imports across processes must partition by identity or persist/share one
  tracker; two independent trackers cannot detect duplicates across instances.

## Verification

- Unit tests (`data_migration/src/lib.rs`): determinism of enumeration and
  serialization, legacy-layout deserialization, poison recovery, rollback
  retry, and a sequential model property test
  (`test_tracker_matches_set_model_under_random_op_sequences`) that proves
  rejected/stale/repeated/failed operations leave no partial state over random
  operation sequences (proptest, 256 cases per run).
- Integration tests (`data_migration/tests/concurrency_conflict.rs`):
  barrier-synchronized 16-thread bursts proving exactly-one-winner for
  identical payloads, gap-free application of distinct payloads, deterministic
  conflict responses, retry-after-conflict stability, rollback-permitted
  retry, mixed-workload final-state exactness, JSON/binary non-interference,
  completion-gate consistency, byte-identical reconciliation artifacts across
  runs, and bit-for-bit settlement payload preservation.
- Stability: the concurrency suite passed 30 consecutive runs (300 test
  executions) with zero failures at implementation time; assertions target
  schedule-invariant quantities (counts, error variants, final content), so
  the suite is non-flaky by construction.
