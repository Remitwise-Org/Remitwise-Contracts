//! Request-key (idempotency-nonce) regression coverage for `data_migration`.
//!
//! Issue #1766 requires every operation to be bound to a durable request key
//! or nonce: a safe retry returns a deterministic result and conflicting reuse
//! is rejected, with rejected, stale, repeated, and failed operations leaving
//! no unauthorized or partial state. These tests prove the invariant at the
//! actual integration boundary — the public `import_*_with_request_key`
//! functions and the `SharedMigrationTracker` concurrency facade.
//!
//! The invariant under test, for every workload:
//!
//! > A given request key commits at most one effect. The first submission
//! > records its outcome; a retry with the same key and the same bytes returns
//! > the recorded outcome deterministically (without re-processing), and
//! > reusing the key with different bytes is rejected with no state change.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;

use data_migration::{
    export_to_binary, export_to_json, import_from_binary_with_request_key,
    import_from_json_with_request_key, ExportFormat, ExportSnapshot, ImportResult, JsonValue,
    MigrationError, MigrationTracker, SharedMigrationTracker, SnapshotPayload,
};

fn generic_payload(value: &str) -> SnapshotPayload {
    let mut entries = BTreeMap::new();
    entries.insert("payload".into(), JsonValue::from(serde_json::json!(value)));
    SnapshotPayload::Generic(entries)
}

fn json_bytes(payload: SnapshotPayload) -> Vec<u8> {
    export_to_json(&ExportSnapshot::new(payload, ExportFormat::Json)).expect("snapshot exports")
}

fn binary_bytes(payload: SnapshotPayload) -> Vec<u8> {
    export_to_binary(&ExportSnapshot::new(payload, ExportFormat::Binary)).expect("snapshot exports")
}

// ---------------------------------------------------------------------------
// Duplicate / retry / one committed effect
// ---------------------------------------------------------------------------

#[test]
fn duplicate_request_key_retry_is_deterministic_and_commits_once() {
    let bytes = json_bytes(generic_payload("settlement-batch-1"));
    let mut tracker = MigrationTracker::new();

    let first =
        import_from_json_with_request_key(&bytes, &mut tracker, "req-settlement-1", 1_000).unwrap();
    assert_eq!(first, ImportResult::Applied);

    // Simulated retry of a message whose response was lost: same key, same
    // bytes. Must not re-apply.
    let retry =
        import_from_json_with_request_key(&bytes, &mut tracker, "req-settlement-1", 2_000).unwrap();
    assert_eq!(retry, ImportResult::AlreadyCommitted);

    // Exactly one committed effect.
    assert_eq!(tracker.imported_count(), 1);
    assert_eq!(tracker.request_key_count(), 1);
    assert_eq!(tracker.attempt_history().len(), 1);

    let record = tracker.request_record("req-settlement-1").expect("record");
    assert!(record.applied);
    assert!(record.failure.is_none());
    assert_eq!(record.submitted_at_ms, 1_000);
}

#[test]
fn timeout_retry_after_commit_never_appends_state() {
    let bytes = json_bytes(generic_payload("timeout-sensitive"));
    let mut tracker = MigrationTracker::new();

    import_from_json_with_request_key(&bytes, &mut tracker, "req-timeout", 5_000).unwrap();

    // A client that timed out and re-sent the same request many times sees the
    // same deterministic result and no growth in committed state.
    for t in 5_100..5_200 {
        let result =
            import_from_json_with_request_key(&bytes, &mut tracker, "req-timeout", t).unwrap();
        assert_eq!(result, ImportResult::AlreadyCommitted);
    }

    assert_eq!(tracker.imported_count(), 1);
    assert_eq!(tracker.attempt_history().len(), 1);
    assert_eq!(tracker.request_key_count(), 1);
}

#[test]
fn request_key_outcome_is_durable_across_tracker_serialization() {
    let bytes = json_bytes(generic_payload("durable-key"));
    let mut tracker = MigrationTracker::new();
    import_from_json_with_request_key(&bytes, &mut tracker, "req-durable", 7_000).unwrap();

    // Persist the tracker (e.g. checkpoint between worker restarts) and
    // reload it in a fresh process state.
    let persisted = bincode::serialize(&tracker).expect("serialize tracker");
    let mut reloaded: MigrationTracker = bincode::deserialize(&persisted).expect("deserialize");

    let retry =
        import_from_json_with_request_key(&bytes, &mut reloaded, "req-durable", 8_000).unwrap();
    assert_eq!(retry, ImportResult::AlreadyCommitted);
    assert_eq!(reloaded.imported_count(), 1);
    assert_eq!(reloaded.request_key_count(), 1);
    assert!(
        reloaded
            .request_record("req-durable")
            .expect("record")
            .applied
    );
}

// ---------------------------------------------------------------------------
// Conflicting reuse and no-partial-state guarantees
// ---------------------------------------------------------------------------

#[test]
fn conflicting_key_reuse_is_rejected_without_state_change() {
    let bytes_a = json_bytes(generic_payload("operation-A"));
    let bytes_b = json_bytes(generic_payload("operation-B"));
    let mut tracker = MigrationTracker::new();

    import_from_json_with_request_key(&bytes_a, &mut tracker, "req-conflict", 1_000).unwrap();

    let conflict = import_from_json_with_request_key(&bytes_b, &mut tracker, "req-conflict", 2_000)
        .unwrap_err();
    assert!(matches!(
        conflict,
        MigrationError::RequestKeyReuseConflict { .. }
    ));

    // Rejected reuse must leave no partial state: the original effect is
    // untouched and no second identity is recorded.
    assert_eq!(tracker.imported_count(), 1);
    assert_eq!(tracker.request_key_count(), 1);
    assert_eq!(tracker.attempt_history().len(), 1);

    // The safe retry of the *original* operation still succeeds after the
    // conflict was rejected.
    let retry =
        import_from_json_with_request_key(&bytes_a, &mut tracker, "req-conflict", 3_000).unwrap();
    assert_eq!(retry, ImportResult::AlreadyCommitted);
}

#[test]
fn failed_first_submission_is_replayed_deterministically_and_leaves_no_state() {
    let corrupt = b"definitely-not-a-valid-snapshot-envelope";
    let mut tracker = MigrationTracker::new();

    let first =
        import_from_json_with_request_key(corrupt, &mut tracker, "req-bad", 1_000).unwrap_err();
    assert!(matches!(first, MigrationError::DeserializeError(_)));

    // The failure is recorded but no import state was committed.
    assert_eq!(tracker.imported_count(), 0);
    let record = tracker.request_record("req-bad").expect("record");
    assert!(!record.applied);
    assert!(record.failure.is_some());

    // A timeout-retry replays the recorded failure instead of re-processing.
    let retry =
        import_from_json_with_request_key(corrupt, &mut tracker, "req-bad", 2_000).unwrap_err();
    assert!(matches!(
        retry,
        MigrationError::ReplayedFailedRequest { .. }
    ));
    assert_eq!(tracker.imported_count(), 0);
    assert_eq!(tracker.attempt_history().len(), 0);
}

#[test]
fn failed_validation_submission_is_replayed_and_leaves_no_state() {
    let mut snapshot = ExportSnapshot::new(generic_payload("tampered"), ExportFormat::Json);
    snapshot.header.checksum = "tampered".to_string();
    let bytes = export_to_json(&snapshot).expect("snapshot exports");

    let mut tracker = MigrationTracker::new();

    let first =
        import_from_json_with_request_key(&bytes, &mut tracker, "req-tampered", 1_000).unwrap_err();
    assert!(matches!(first, MigrationError::ChecksumMismatch));

    assert_eq!(tracker.imported_count(), 0);
    let record = tracker.request_record("req-tampered").expect("record");
    assert!(!record.applied);

    let retry =
        import_from_json_with_request_key(&bytes, &mut tracker, "req-tampered", 2_000).unwrap_err();
    assert!(matches!(
        retry,
        MigrationError::ReplayedFailedRequest { .. }
    ));
    assert_eq!(tracker.imported_count(), 0);
}

// ---------------------------------------------------------------------------
// Reordering and cross-key isolation
// ---------------------------------------------------------------------------

#[test]
fn reordered_keys_converge_to_the_same_deterministic_state() {
    let bytes_x = json_bytes(generic_payload("snapshot-X"));
    let bytes_y = json_bytes(generic_payload("snapshot-Y"));

    let build = |first: &[u8], second: &[u8]| {
        let mut tracker = MigrationTracker::new();
        import_from_json_with_request_key(first, &mut tracker, "req-first", 1_000).unwrap();
        import_from_json_with_request_key(second, &mut tracker, "req-second", 2_000).unwrap();
        tracker.imported_records()
    };

    let order_xy = build(&bytes_x, &bytes_y);
    let order_yx = build(&bytes_y, &bytes_x);

    // The committed *identity* set is schedule-independent (import timestamps
    // legitimately reflect application order and are not part of the
    // reconciliation identity).
    let identities = |records: Vec<data_migration::ImportRecord>| {
        records
            .into_iter()
            .map(|r| (r.checksum, r.version))
            .collect::<BTreeMap<_, _>>()
    };
    let set_xy = identities(order_xy);
    let set_yx = identities(order_yx);
    assert_eq!(set_xy, set_yx);
    assert_eq!(set_xy.len(), 2);
}

#[test]
fn same_bytes_under_a_different_key_are_rejected_as_duplicate() {
    let bytes = json_bytes(generic_payload("one-true-effect"));
    let mut tracker = MigrationTracker::new();

    import_from_json_with_request_key(&bytes, &mut tracker, "key-a", 1_000).unwrap();

    // A fresh key carrying the same payload cannot double-apply: identity-level
    // replay protection still holds on top of request-key idempotency.
    let dup = import_from_json_with_request_key(&bytes, &mut tracker, "key-b", 2_000).unwrap_err();
    assert!(matches!(dup, MigrationError::DuplicateImport));

    assert_eq!(tracker.imported_count(), 1);
    assert!(!tracker.request_record("key-b").expect("record").applied);
}

// ---------------------------------------------------------------------------
// Binary path parity
// ---------------------------------------------------------------------------

#[test]
fn binary_path_has_the_same_idempotency_contract() {
    let bytes = binary_bytes(generic_payload("binary-batch"));
    let other = binary_bytes(generic_payload("binary-other"));
    let mut tracker = MigrationTracker::new();

    assert_eq!(
        import_from_binary_with_request_key(&bytes, &mut tracker, "req-bin", 1_000).unwrap(),
        ImportResult::Applied
    );
    assert_eq!(
        import_from_binary_with_request_key(&bytes, &mut tracker, "req-bin", 2_000).unwrap(),
        ImportResult::AlreadyCommitted
    );
    assert!(matches!(
        import_from_binary_with_request_key(&other, &mut tracker, "req-bin", 3_000).unwrap_err(),
        MigrationError::RequestKeyReuseConflict { .. }
    ));

    assert_eq!(tracker.imported_count(), 1);
    assert_eq!(tracker.request_key_count(), 1);
}

// ---------------------------------------------------------------------------
// Concurrency facade: one committed effect per request key
// ---------------------------------------------------------------------------

#[test]
fn shared_tracker_concurrent_same_key_commits_exactly_once() {
    let bytes = json_bytes(generic_payload("concurrent-race"));
    let shared = Arc::new(SharedMigrationTracker::new());
    let mut handles = Vec::new();

    for _ in 0..16 {
        let shared = Arc::clone(&shared);
        let bytes = bytes.clone();
        handles.push(thread::spawn(move || {
            shared.import_from_json_with_request_key(&bytes, "req-race", 1_000)
        }));
    }

    let mut applied = 0usize;
    let mut already_committed = 0usize;
    for handle in handles {
        match handle.join().expect("worker") {
            Ok(ImportResult::Applied) => applied += 1,
            Ok(ImportResult::AlreadyCommitted) => already_committed += 1,
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    // Exactly one submission wins the race; every other caller observes the
    // recorded outcome. Total effect: one committed import.
    assert_eq!(applied, 1);
    assert_eq!(already_committed, 15);
    assert_eq!(shared.imported_count(), 1);
    assert_eq!(shared.request_key_count(), 1);
    assert!(shared.request_record("req-race").expect("record").applied);
}
