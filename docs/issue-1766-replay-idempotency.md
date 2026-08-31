# PR — Issue #1766: Migration, Pagination & Settlement Data: Replay and Idempotency (durable request keys)

Closes #1766.

## Summary

`data_migration` already prevented the *same snapshot* from being applied twice
via a `(checksum, version)` identity registry. That guard is necessary but not
sufficient for production-grade idempotency: a caller whose response is lost
(timeout, worker crash, transport retry) cannot tell "did my operation commit?"
from the payload alone, and a duplicated or reordered message must never apply a
business operation twice.

This PR adds a **durable request-key (idempotency nonce) layer** at the actual
integration boundary — the public `import_from_json` / `import_from_binary`
entry points and the `SharedMigrationTracker` concurrency facade:

| Entry point | Behavior |
|---|---|
| `import_from_json_with_request_key(bytes, tracker, key, ts)` | Bind the operation to `key`; first submission commits + records; safe retry returns the recorded result; conflicting reuse is rejected. |
| `import_from_binary_with_request_key(bytes, tracker, key, ts)` | Same contract for the binary format. |
| `SharedMigrationTracker::import_from_*_with_request_key(...)` | Same contract, serialized through the mutex so concurrent timeout-retries commit exactly once. |

The base `data_migration` crate did not compile (missing braces, an undefined
`ImportRecord`, a `#[test]` malformed inside a `proptest!` macro, and an
unclosed macro block); the compile errors were repaired to obtain a testable
baseline, and two latent test failures caused by the previously-added
`active_attempt` / `attempt_history` fields (bincode determinism and legacy
deserialization) were fixed with a custom `Serialize` / `Deserialize` impl.

## Design and invariants

The registry lives inside `MigrationTracker` (field `request_keys:
BTreeMap<String, RequestRecord>`), so it is durable: it persists with the
tracker across serialization/deserialization, meaning a key committed before a
crash still returns `AlreadyCommitted` after a restart.

```rust
pub struct RequestRecord {
    pub fingerprint: String,      // SHA-256 hex of the raw request bytes
    pub applied: bool,            // did the first submission commit?
    pub failure: Option<String>,  // deterministic failure text, if first attempt failed
    pub submitted_at_ms: u64,
}
```

Pipeline (`import_bytes_with_request_key` → `MigrationTracker::apply_with_request_key`):

1. **Size gate** — oversized envelopes are rejected before hashing/parsing
   (unchanged DoS protection).
2. **Fingerprint** = SHA-256 hex of the raw request bytes. Identical bytes ⇒
   identical fingerprint ⇒ the safe-retry/conflict classifier is exact.
3. **Registry lookup by key**:
   - key unseen ⇒ process as new work;
   - key seen, same fingerprint, `applied == true` ⇒ return
     `ImportResult::AlreadyCommitted` (no re-processing, no new history entry);
   - key seen, same fingerprint, `applied == false` ⇒ return
     `MigrationError::ReplayedFailedRequest` (deterministic replay of the
     recorded failure);
   - key seen, different fingerprint ⇒ return
     `MigrationError::RequestKeyReuseConflict` (no state touched).
4. **First submission** — decode + `validate_for_import` + `mark_imported`.
   The outcome is recorded under the key on *every* path: success records
   `applied: true`; a decode/validation/mark error records `applied: false`
   with the error text. A failed first submission therefore never mutates
   tracker state, and its retry replays the recorded failure.

### Invariants

- **One committed effect per request key.** A key applied once is applied
  once; retries return the recorded result and append no history entry.
- **Conflicting reuse is rejected with no partial state.** `imported_count`,
  `attempt_history`, and the recorded fingerprint are unchanged.
- **Rejected / stale / repeated / failed operations leave no unauthorized or
  partial state.** Failed first submissions record `applied: false` and commit
  nothing; identity-level replay protection (`DuplicateImport`) still holds for
  the same bytes arriving under a *different* key.
- **Deterministic.** The registry is a `BTreeMap` and is serialized in sorted
  order; the committed identity set is schedule-independent under reordering and
  concurrency.

## Failure behavior and compatibility impact

**Additive API only.** No existing public function, return type, or error
discriminant changed; existing callers of `import_from_json`,
`import_from_binary`, `mark_imported`, `SharedMigrationTracker`, and the
reconciliation/rollback surface are unaffected.

Two new error variants were added to `MigrationError`:

```rust
RequestKeyReuseConflict { request_key, previous_fingerprint, current_fingerprint }
ReplayedFailedRequest   { request_key, recorded_failure }
```

**Serde compatibility (documented behavior change).** `MigrationTracker` now has
a custom `Serialize` / `Deserialize` impl to satisfy two guarantees:

1. *Byte determinism* — `attempt_history` is serialized in a deterministic
   (sorted) order so two trackers with the same logical state serialize to
   identical bytes (the previously-`HashMap`-ordered Vec broke this when
   `attempt_history` was added).
2. *Backward compatibility* — trackers serialized by the older two-field layout
   (`imported_payloads` + `completed`) still deserialize: `Deserialize` falls
   back to defaults for the attempt-tracking and request-key fields when a
   positional stream ends early. The wire shape for the base fields is
   unchanged.

## Migration / rollback considerations

- **No storage migration.** Existing serialized trackers deserialize into the
  new type unchanged (defaults for the new fields).
- **Rollback compatibility.** `RollbackMetadata::restore` is unaffected. Note:
  request-key records are *not* cleared by rollback — a failed-and-rolled-back
  request key retains its recorded outcome; operators retrying corrected content
  must mint a new key. This is the documented idempotency-key rule (a key binds
  to one exact request).

## Operational limitations

- A request key binds to the **exact bytes** of a request. Fixing a rejected
  payload and resubmitting requires a **new** key; the old key permanently
  replays its recorded failure/conflict.
- The registry grows by one record per distinct key (a `BTreeMap<String,
  RequestRecord>`). For long-running processes, operators should prune/archive
  records out of band (keys have no TTL by design — durability is the point).
- `MAX_MIGRATION_ATTEMPT_HISTORY` still bounds `attempt_history`; request-key
  records are separate and unbounded by that constant.

## Security assumptions

- The fingerprint is SHA-256 over raw request bytes: identical requests are
  indistinguishable from themselves (safe retry) and different requests are
  distinct (conflict). SHA-256 is collision-resistant for this threat model.
- The request key is an *idempotency* key, **not** an authorization or
  authentication mechanism. Callers must still gate who may invoke the migration
  entry points (the surrounding contract layer does this).
- Decode/validation failures are recorded under the key before any state
  mutation; there is no path where a failed submission commits partial state.

## Regression coverage (integration boundary)

`data_migration/tests/idempotency_request_keys.rs` (10 tests):

| Test | Criterion |
|---|---|
| `duplicate_request_key_retry_is_deterministic_and_commits_once` | duplicate |
| `timeout_retry_after_commit_never_appends_state` | timeout-retry |
| `request_key_outcome_is_durable_across_tracker_serialization` | durability (bincode round-trip) |
| `conflicting_key_reuse_is_rejected_without_state_change` | conflicting-key |
| `failed_first_submission_is_replayed_deterministically_and_leaves_no_state` | failed / no partial state |
| `failed_validation_submission_is_replayed_and_leaves_no_state` | stale/failed / no partial state |
| `reordered_keys_converge_to_the_same_deterministic_state` | reordered |
| `same_bytes_under_a_different_key_are_rejected_as_duplicate` | cross-key identity replay |
| `binary_path_has_the_same_idempotency_contract` | binary format parity |
| `shared_tracker_concurrent_same_key_commits_exactly_once` | one committed effect under concurrency |

Each test asserts counts, error variants, and final-state content (quantities
invariant under thread scheduling), so the suite is deterministic.

## Validation evidence

Commands run (workdir: repository root):

| Command | Result |
|---|---|
| `cargo build -p data_migration` | clean |
| `cargo check -p data_migration` | clean |
| `cargo clippy -p data_migration --all-targets` | no warnings |
| `cargo fmt -p data_migration -- --check` | formatted |
| `cargo test -p data_migration` | **297 passed, 0 failed** (273 unit + 10 concurrency + 10 new idempotency + 3 replay + 1 csv) |

Notes:
- `cargo check --workspace` reports pre-existing compile errors in
  `savings_goals` / `family_wallet` that exist on the base and are outside this
  issue's scope (`data_migration` is the designated starting point); they were
  not modified.
- `Cargo.lock` was reverted after builds; no generated artifacts, secrets, or
  disabled checks are included.
