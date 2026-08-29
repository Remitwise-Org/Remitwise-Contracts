Data Migration Rollback Runbook

Overview

The `data_migration` crate provides `RollbackMetadata` to capture the pre-import state
and deterministically restore it if an import fails partway. This file documents the
operator workflow for safe imports and recovery.

Capture

1. Before performing any irreversible mutation (writes), call `RollbackMetadata::capture(current_state, attempted_snapshot, timestamp_ms)`.
2. Persist the returned `RollbackMetadata` in an operator-local safe place (off-chain storage, logs or operator UI).

Import

1. Validate the snapshot using `ExportSnapshot::validate_for_import()`.
2. Start an observable attempt with `MigrationTracker::begin_import()`.
3. Apply the snapshot to the contract state in bounded batches, recording each
   successful checkpoint with `MigrationTracker::record_progress()`.
4. Mark the snapshot as imported in the `MigrationTracker` via `mark_imported()`.
5. Once the full migration is coherent, call `mark_completed()` so guarded
   writes can resume.
6. If the import completes successfully, discard the persisted `RollbackMetadata`.

Failure & Restore

1. If the import fails at any point after capture (validation error, checksum, downstream error), retrieve the `RollbackMetadata` persisted earlier.
2. Call `RollbackMetadata::restore(&rollback, &mut state, &mut tracker)` to:
   - Revert the in-memory representation of the contract to the captured pre-import snapshot.
   - Remove the replay-tracking entry for the attempted (failed) import so retries are permitted.
   - Mark any active matching attempt as `RolledBack` in tracker history.

Notes & Guarantees

- `RollbackMetadata::restore` is idempotent: it is safe to call it multiple times.
- The design preserves previously-recorded imported snapshots; restore only removes the marker for the attempted failed import.
- Progress checkpoints are monotonic and bounded by the snapshot record count.
- `fail_import` records a failed attempt without creating a replay marker, so a
  later retry with the same snapshot remains possible.
- Operators should prefer capturing before any mutation and discarding the metadata only after a fully successful import.

Examples

See `data_migration::RollbackMetadata::capture` and `restore` in the crate source for usage patterns and tests demonstrating successful and failed flows.
