//! Concurrency, conflict, and retry contract tests for `data_migration`.
//!
//! These tests prove the guarantees documented on [`SharedMigrationTracker`]
//! at the actual integration boundary: multiple OS threads driving the public
//! crate API against one shared tracker.
//!
//! The invariant under test, for every workload:
//!
//! > For a given `(checksum, version)` snapshot identity, exactly one
//! > concurrent import commits. Every conflicting, rejected, stale, or failed
//! > operation deterministically returns the same error and leaves no partial
//! > tracker state; the final set of applied identities is schedule-independent.
//!
//! Determinism note: assertions target *counts, error variants, and final
//! state content* — quantities that are invariant under every thread
//! schedule — so these tests are stable (not flaky) by construction.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Barrier};
use std::thread;

use data_migration::{
    export_to_binary, export_to_json, ExportFormat, ExportSnapshot, JsonValue, MigrationError,
    MigrationTracker, RollbackMetadata, SavingsGoalExport, SavingsGoalsExport,
    SharedMigrationTracker, SnapshotPayload,
};

/// Number of worker threads used for contention bursts: high enough to
/// over-subscribe typical CI cores so interleavings actually overlap.
const THREADS: usize = 16;

/// How outcomes are bucketed after a concurrent burst. Bucketing (not order)
/// is what the contract pins down, so results are schedule-independent.
#[derive(Debug, Default)]
struct Outcomes {
    ok: usize,
    duplicate: usize,
    other_errors: Vec<MigrationError>,
}

impl Outcomes {
    fn classify(results: impl IntoIterator<Item = Result<ExportSnapshot, MigrationError>>) -> Self {
        let mut outcomes = Outcomes::default();
        for result in results {
            match result {
                Ok(_) => outcomes.ok += 1,
                Err(MigrationError::DuplicateImport) => outcomes.duplicate += 1,
                Err(e) => outcomes.other_errors.push(e),
            }
        }
        outcomes
    }
}

fn generic_payload(value: &str) -> SnapshotPayload {
    let mut entries = BTreeMap::new();
    entries.insert("payload".into(), JsonValue::from(serde_json::json!(value)));
    SnapshotPayload::Generic(entries)
}

fn savings_goals_payload(tag: &str) -> SnapshotPayload {
    SnapshotPayload::SavingsGoals(SavingsGoalsExport {
        next_id: 2,
        goals: vec![SavingsGoalExport {
            id: 1,
            owner: format!("owner-{tag}"),
            name: "Settlement reserve".into(),
            target_amount: 5_000,
            current_amount: 1_000,
            target_date: 2_000_000_000,
            locked: false,
        }],
    })
}

fn json_bytes(payload: SnapshotPayload) -> (ExportSnapshot, Vec<u8>) {
    let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
    let bytes = export_to_json(&snapshot).expect("valid snapshot must export");
    (snapshot, bytes)
}

fn identity_set(tracker: &MigrationTracker) -> BTreeSet<(String, u32)> {
    tracker
        .imported_records()
        .iter()
        .map(|r| (r.checksum.clone(), r.version))
        .collect()
}

fn assert_records_sorted(tracker: &MigrationTracker) {
    let records = tracker.imported_records();
    assert!(
        records
            .windows(2)
            .all(|w| (&w[0].checksum, w[0].version) < (&w[1].checksum, w[1].version)),
        "reconciliation records must be strictly sorted by identity"
    );
}

/// Run `THREADS` barrier-synchronized import closures against the shared
/// tracker and collect their results. The barrier forces maximal overlap at
/// the commit section instead of letting threads finish before peers start.
fn burst<F>(
    tracker: &Arc<SharedMigrationTracker>,
    make_job: F,
) -> Vec<Result<ExportSnapshot, MigrationError>>
where
    F: Fn(usize) -> Vec<u8>,
{
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for i in 0..THREADS {
        let tracker = Arc::clone(tracker);
        let barrier = Arc::clone(&barrier);
        let bytes = make_job(i);
        handles.push(thread::spawn(move || {
            barrier.wait();
            tracker.import_from_json(&bytes, i as u64)
        }));
    }
    handles
        .into_iter()
        .map(|h| h.join().expect("worker thread must not panic"))
        .collect()
}

/// Deterministic parallel test: `THREADS` threads import the *same* payment
/// snapshot. Exactly one wins; the rest receive the documented conflict
/// response. The final tracker state is the single applied identity — no
/// double-apply is possible under any interleaving.
#[test]
fn concurrent_same_payload_exactly_one_winner() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let (snapshot, bytes) = json_bytes(generic_payload("settlement-batch-1"));

    let outcomes = Outcomes::classify(burst(&tracker, |_| bytes.clone()));

    assert_eq!(outcomes.ok, 1, "exactly one concurrent import may commit");
    assert_eq!(
        outcomes.duplicate,
        THREADS - 1,
        "every conflicting import must observe DuplicateImport"
    );
    assert!(
        outcomes.other_errors.is_empty(),
        "no other error variant may surface: {:?}",
        outcomes.other_errors
    );

    // Final-state assertions.
    assert_eq!(tracker.imported_count(), 1);
    assert!(tracker.is_imported(&snapshot));
    let records = tracker.imported_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].checksum, snapshot.header.checksum);
    assert_eq!(records[0].version, snapshot.header.version);
}

/// Deterministic parallel test: `THREADS` threads import `THREADS` *distinct*
/// settlement snapshots. All commit; reconciliation enumerates exactly the
/// submitted set — no gaps, no duplicates, deterministic order.
#[test]
fn concurrent_distinct_payloads_all_applied_gap_free() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let prepared: Vec<(ExportSnapshot, Vec<u8>)> = (0..THREADS)
        .map(|i| json_bytes(savings_goals_payload(&format!("settlement-{i}"))))
        .collect();
    let expected_identities: BTreeSet<(String, u32)> = prepared
        .iter()
        .map(|(s, _)| (s.header.checksum.clone(), s.header.version))
        .collect();
    let bytes: Vec<Vec<u8>> = prepared.into_iter().map(|(_, b)| b).collect();

    let outcomes = Outcomes::classify(burst(&tracker, |i| bytes[i].clone()));

    assert_eq!(outcomes.ok, THREADS);
    assert_eq!(outcomes.duplicate, 0);
    assert!(outcomes.other_errors.is_empty());

    // Final-state assertions: exact set equality (gap-free).
    let view = tracker.snapshot();
    assert_eq!(identity_set(&view), expected_identities);
    assert_eq!(view.imported_count(), THREADS);
    assert_records_sorted(&view);
}

/// Contention test: a burst of *invalid* (forged-checksum) snapshots can never
/// create tracker state, and the rejection is deterministic; a subsequent
/// valid retry of the corrected payload then succeeds. Rejected and failed
/// operations leave no partial state.
#[test]
fn concurrent_rejected_payloads_leave_no_state_and_retry_succeeds() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let (mut forged, _) = json_bytes(generic_payload("forged-checksum"));
    forged.header.checksum = "forged".into();
    let forged_bytes = serde_json::to_vec(&forged).expect("forged snapshot serializes");

    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for _ in 0..THREADS {
        let tracker = Arc::clone(&tracker);
        let barrier = Arc::clone(&barrier);
        let bytes = forged_bytes.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            tracker.import_from_json(&bytes, 0)
        }));
    }
    for handle in handles {
        let outcome = handle.join().expect("worker must not panic");
        assert!(
            matches!(outcome, Err(MigrationError::ChecksumMismatch)),
            "every forged import must deterministically fail with ChecksumMismatch, got {outcome:?}"
        );
    }

    // Rejections under contention left zero partial state.
    assert_eq!(tracker.imported_count(), 0);
    assert!(tracker.imported_records().is_empty());

    // The corrected payload (valid checksum for the same logical content) is
    // still importable — the earlier rejections did not burn the identity or
    // any other state.
    let (snapshot, valid_bytes) = json_bytes(generic_payload("forged-checksum"));
    tracker
        .import_from_json(&valid_bytes, 1)
        .expect("corrected retry must succeed after rejected attempts");
    assert_eq!(tracker.imported_count(), 1);
    assert!(tracker.is_imported(&snapshot));
}

/// Retry-after-conflict test: once an identity is applied, an arbitrarily
/// large concurrent retry burst deterministically re-receives
/// `DuplicateImport`, and the tracker content never flaps. This pins the
/// client-facing "conflict means idempotent success — do not retry" contract.
#[test]
fn retry_after_conflict_is_deterministic_and_stable() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let (_, bytes) = json_bytes(generic_payload("idempotent-settlement"));

    tracker
        .import_from_json(&bytes, 0)
        .expect("initial import must succeed");

    for round in 0..3 {
        let outcomes = Outcomes::classify(burst(&tracker, |_| bytes.clone()));
        assert_eq!(
            outcomes.ok, 0,
            "round {round}: a committed identity never re-commits"
        );
        assert_eq!(
            outcomes.duplicate, THREADS,
            "round {round}: deterministic conflict response"
        );
        assert!(outcomes.other_errors.is_empty());
        assert_eq!(
            tracker.imported_count(),
            1,
            "round {round}: state must not flap under retries"
        );
    }
}

/// Retry-after-failure test: a failed apply restored via rollback un-marks the
/// identity, so a retry burst commits exactly once and lands on the exact
/// pre-failure logical content.
#[test]
fn rollback_after_failure_permits_single_retry_commit() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let (snapshot, bytes) = json_bytes(savings_goals_payload("retryable"));

    // Capture before any irreversible step, then simulate a downstream failure
    // after the import commit.
    let mut state: Option<ExportSnapshot> = None;
    let rollback = RollbackMetadata::capture(state.as_ref(), &snapshot, 500);
    tracker
        .import_from_json(&bytes, 1)
        .expect("first apply commits before the simulated downstream failure");

    // Downstream failure → restore. The stale identity marker must be removed.
    tracker
        .restore_rollback(&rollback, &mut state)
        .expect("restore must succeed");
    assert!(state.is_none());
    assert_eq!(tracker.imported_count(), 0);
    assert!(!tracker.is_imported(&snapshot));

    // Retry burst for the same identity: exactly one commit.
    let outcomes = Outcomes::classify(burst(&tracker, |_| bytes.clone()));
    assert_eq!(outcomes.ok, 1);
    assert_eq!(outcomes.duplicate, THREADS - 1);
    assert!(outcomes.other_errors.is_empty());
    assert_eq!(tracker.imported_count(), 1);
    assert!(tracker.is_imported(&snapshot));
}

/// Contention test with a mixed workload — distinct valids, duplicates, and
/// forged payloads racing — asserting the exact final content set. Under every
/// schedule, exactly the distinct valid identities are applied, exactly once.
#[test]
fn concurrent_mixed_workload_final_state_is_exact() {
    let tracker = Arc::new(SharedMigrationTracker::new());

    let distinct: Vec<Vec<u8>> = (0..8)
        .map(|i| json_bytes(generic_payload(&format!("valid-{i}"))).1)
        .collect();
    let expected_identities: BTreeSet<(String, u32)> = (0..8)
        .map(|i| {
            let (s, _) = json_bytes(generic_payload(&format!("valid-{i}")));
            (s.header.checksum, s.header.version)
        })
        .collect();
    let (_, dup_of_first) = json_bytes(generic_payload("valid-0"));
    let (mut forged, _) = json_bytes(generic_payload("forged"));
    forged.header.checksum = "forged".into();
    let forged_bytes = serde_json::to_vec(&forged).unwrap();

    // Workload mix: first 8 threads submit distinct valids, the next 4
    // resubmit valid-0 (conflicts), the last 4 submit a forged payload.
    let jobs: Vec<Vec<u8>> = (0..THREADS)
        .map(|i| match i {
            0..=7 => distinct[i].clone(),
            8..=11 => dup_of_first.clone(),
            _ => forged_bytes.clone(),
        })
        .collect();

    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for (i, bytes) in jobs.into_iter().enumerate() {
        let tracker = Arc::clone(&tracker);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            tracker.import_from_json(&bytes, i as u64)
        }));
    }
    let results: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("worker must not panic"))
        .collect();

    let mut ok = 0;
    let mut dup = 0;
    let mut checksum_fail = 0;
    for result in results {
        match result {
            Ok(_) => ok += 1,
            Err(MigrationError::DuplicateImport) => dup += 1,
            Err(MigrationError::ChecksumMismatch) => checksum_fail += 1,
            Err(e) => panic!("unexpected error variant under contention: {e:?}"),
        }
    }
    assert_eq!(ok, 8, "all distinct valid payloads commit");
    assert_eq!(dup, 4, "resubmissions conflict deterministically");
    assert_eq!(checksum_fail, 4, "forged payloads reject deterministically");

    // Final-state assertion: exactly the 8 valid identities, nothing else.
    let view = tracker.snapshot();
    assert_eq!(identity_set(&view), expected_identities);
    assert_eq!(view.imported_count(), 8);
}

/// Mixed-format contention: JSON and binary imports of distinct identities
/// racing on one tracker must not interfere.
#[test]
fn concurrent_json_and_binary_imports_do_not_interfere() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let json_jobs: Vec<Vec<u8>> = (0..8)
        .map(|i| json_bytes(generic_payload(&format!("json-{i}"))).1)
        .collect();
    let binary_jobs: Vec<Vec<u8>> = (0..8)
        .map(|i| {
            let snapshot = ExportSnapshot::new(
                generic_payload(&format!("binary-{i}")),
                ExportFormat::Binary,
            );
            export_to_binary(&snapshot).expect("valid binary export")
        })
        .collect();

    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for i in 0..THREADS {
        let tracker = Arc::clone(&tracker);
        let barrier = Arc::clone(&barrier);
        let use_json = i % 2 == 0;
        let bytes = if use_json {
            json_jobs[i / 2].clone()
        } else {
            binary_jobs[i / 2].clone()
        };
        handles.push(thread::spawn(move || {
            barrier.wait();
            if use_json {
                tracker.import_from_json(&bytes, i as u64)
            } else {
                tracker.import_from_binary(&bytes, i as u64)
            }
        }));
    }
    for handle in handles {
        handle
            .join()
            .expect("worker must not panic")
            .expect("every distinct mixed-format import commits");
    }
    assert_eq!(tracker.imported_count(), THREADS);
}

/// The write-completion gate must be observable consistently under
/// concurrency: closed before any mark, open for *every* observer once the
/// racing marks have been joined. A mid-flight check may legitimately observe
/// either state (that observation is not synchronized with the marks), so the
/// deterministic assertions are the pre-burst and post-join states only.
#[test]
fn completion_gate_is_consistent_under_concurrency() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    assert_eq!(
        tracker.verify_completed().unwrap_err(),
        MigrationError::MigrationNotCompleted
    );

    // Burst 1: racing completion marks (idempotent under every interleaving).
    let barrier = Arc::new(Barrier::new(THREADS / 2));
    let mut handles = Vec::with_capacity(THREADS / 2);
    for _ in 0..THREADS / 2 {
        let tracker = Arc::clone(&tracker);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            tracker.mark_completed();
        }));
    }
    for handle in handles {
        handle.join().expect("worker must not panic");
    }

    // Post-join, every observer sees the gate open.
    let barrier = Arc::new(Barrier::new(THREADS));
    let mut handles = Vec::with_capacity(THREADS);
    for _ in 0..THREADS {
        let tracker = Arc::clone(&tracker);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            tracker.verify_completed()
        }));
    }
    for handle in handles {
        handle
            .join()
            .expect("worker must not panic")
            .expect("after joined completion marks the gate must be open for every observer");
    }
    assert!(tracker.is_completed());
    assert_eq!(
        tracker.imported_count(),
        0,
        "gate operations must not create import state"
    );
}

/// Reconciliation artifacts are byte-deterministic: two trackers that applied
/// the same identities — even in different application orders and even under
/// thread contention — serialize identically and enumerate identically. This
/// is what makes post-upgrade financial reconciliation reviewable.
#[test]
fn reconciliation_artifacts_are_byte_deterministic_across_runs() {
    let build = |reverse_order: bool| -> MigrationTracker {
        let tracker = Arc::new(SharedMigrationTracker::new());
        let order: Vec<usize> = if reverse_order {
            (0..THREADS).rev().collect()
        } else {
            (0..THREADS).collect()
        };
        let barrier = Arc::new(Barrier::new(THREADS));
        let mut handles = Vec::with_capacity(THREADS);
        for i in order {
            let tracker = Arc::clone(&tracker);
            let barrier = Arc::clone(&barrier);
            let (_, bytes) = json_bytes(generic_payload(&format!("recon-{i}")));
            // Timestamps are payload-derived (not submission-order-derived) so
            // both runs record the same (identity → timestamp) mapping.
            handles.push(thread::spawn(move || {
                barrier.wait();
                tracker.import_from_json(&bytes, i as u64)
            }));
        }
        for handle in handles {
            handle
                .join()
                .expect("worker must not panic")
                .expect("distinct payloads commit");
        }
        match Arc::try_unwrap(tracker) {
            Ok(shared) => shared.into_inner(),
            Err(_) => panic!("no outstanding references may remain after join"),
        }
    };

    let forward = build(false);
    let reverse = build(true);

    assert_eq!(
        forward.imported_records(),
        reverse.imported_records(),
        "enumeration must be identical regardless of application order"
    );
    assert_eq!(
        bincode::serialize(&forward.imported_records()).expect("serialize forward"),
        bincode::serialize(&reverse.imported_records()).expect("serialize reverse"),
        "persisted reconciliation state must be byte-identical across runs"
    );
}

/// Payment/settlement payload data survives the concurrent import path
/// bit-for-bit: the winning imports return exactly the exported payloads,
/// proving no record was lost or rewritten by racing imports.
#[test]
fn concurrent_imports_preserve_settlement_data_exactly() {
    let tracker = Arc::new(SharedMigrationTracker::new());
    let prepared: Vec<(ExportSnapshot, Vec<u8>)> = (0..THREADS)
        .map(|i| json_bytes(savings_goals_payload(&format!("preserve-{i}"))))
        .collect();
    let expected: BTreeSet<String> = prepared
        .iter()
        .map(|(s, _)| serde_json::to_string(&s.payload).expect("payload serializes"))
        .collect();
    let bytes: Vec<Vec<u8>> = prepared.into_iter().map(|(_, b)| b).collect();

    let imported: BTreeSet<String> = burst(&tracker, |i| bytes[i].clone())
        .into_iter()
        .map(|r| {
            let snapshot = r.expect("distinct settlement payloads commit");
            serde_json::to_string(&snapshot.payload).expect("payload serializes")
        })
        .collect();

    assert_eq!(
        imported, expected,
        "no settlement record may be lost or rewritten under concurrency"
    );
    assert_eq!(tracker.imported_count(), THREADS);
}
