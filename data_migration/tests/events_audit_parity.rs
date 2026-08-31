use data_migration::{
    build_payments_snapshot, build_settlements_snapshot, EventPage, ExportFormat,
    ExportSnapshot, MigrationAttemptStatus, MigrationError, MigrationEvent,
    MigrationEventType, MigrationTracker, PaymentExport, PaymentsExport,
    SettlementExport, SettlementsExport, SharedMigrationTracker, SnapshotPayload,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Barrier};
use std::thread;

fn sample_payments() -> PaymentsExport {
    PaymentsExport {
        next_id: 3,
        bills: vec![
            PaymentExport {
                id: 1,
                owner: "alice_wallet".to_string(),
                name: "Internet Bill".to_string(),
                amount: 50_000,
                currency: "USDC".to_string(),
                due_date: 1_700_000_000,
                recurring: false,
                frequency_days: 0,
                paid: true,
                paid_at: Some(1_699_000_000),
                external_ref: Some("INV-001".to_string()),
            },
            PaymentExport {
                id: 2,
                owner: "bob_wallet".to_string(),
                name: "Electricity Bill".to_string(),
                amount: 120_000,
                currency: "EURC".to_string(),
                due_date: 1_700_500_000,
                recurring: true,
                frequency_days: 30,
                paid: false,
                paid_at: None,
                external_ref: None,
            },
        ],
    }
}

fn sample_settlements() -> SettlementsExport {
    SettlementsExport {
        next_id: 2,
        settlements: vec![SettlementExport {
            id: 1,
            sender: "sender_pubkey_1".to_string(),
            recipient: "recipient_pubkey_2".to_string(),
            amount: 75_000,
            currency: "USDC".to_string(),
            status: "completed".to_string(),
            timestamp_ms: 1_700_100_000,
        }],
    }
}

fn sample_generic(key: &str, val: &str) -> ExportSnapshot {
    let mut entries = BTreeMap::new();
    entries.insert(key.to_string(), serde_json::json!(val).into());
    ExportSnapshot::new(SnapshotPayload::Generic(entries), ExportFormat::Json)
}

#[test]
fn test_payments_and_settlements_export_reconciliation() {
    let payments_snapshot = build_payments_snapshot(sample_payments(), ExportFormat::Json);
    payments_snapshot
        .validate_for_import()
        .expect("payments snapshot must validate");

    let report = payments_snapshot
        .reconciliation_report()
        .expect("payments reconciliation report");
    assert!(report.gap_free);
    assert_eq!(report.total_records, 2);
    assert_eq!(report.payload_type, "payments");
    assert_eq!(report.records.len(), 2);
    assert_eq!(report.records[0].ordinal, 0);
    assert_eq!(report.records[1].ordinal, 1);

    let settlements_snapshot =
        build_settlements_snapshot(sample_settlements(), ExportFormat::Json);
    settlements_snapshot
        .validate_for_import()
        .expect("settlements snapshot must validate");

    let s_report = settlements_snapshot
        .reconciliation_report()
        .expect("settlements reconciliation report");
    assert!(s_report.gap_free);
    assert_eq!(s_report.total_records, 1);
    assert_eq!(s_report.payload_type, "settlements");
    assert_eq!(s_report.records.len(), 1);
    assert_eq!(s_report.records[0].ordinal, 0);
}

#[test]
fn test_audit_event_emission_on_committed_transitions_only() {
    let mut tracker = MigrationTracker::new();
    assert_eq!(tracker.get_event_seq(), 0);
    assert!(tracker.events().is_empty());

    let snapshot = sample_generic("payment_batch_1", "records_payload");

    // 1. Legal begin_import emits MigrationEventType::ImportStarted
    tracker
        .begin_import(&snapshot, 1_000)
        .expect("begin_import succeeds");
    assert_eq!(tracker.get_event_seq(), 1);
    let events = tracker.events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 1);
            assert_eq!(ev.event_type, MigrationEventType::ImportStarted);
            assert_eq!(ev.checksum, snapshot.header.checksum);
            assert_eq!(ev.snapshot_version, snapshot.header.version);
            assert_eq!(ev.status, MigrationAttemptStatus::InProgress);
            assert_eq!(ev.timestamp_ms, 1_000);
            assert_eq!(ev.correlation_id, format!("mig:seq:1:{}", &snapshot.header.checksum[..8]));
        }
        _ => panic!("expected AuditV1 event"),
    }

    // 2. Illegal begin_import during active attempt fails and DOES NOT emit or increment seq
    let dup_res = tracker.begin_import(&snapshot, 1_050);
    assert!(matches!(dup_res, Err(MigrationError::MigrationAlreadyInProgress)));
    assert_eq!(tracker.get_event_seq(), 1);
    assert_eq!(tracker.events().len(), 1);

    // 3. Legal record_progress emits MigrationEventType::ProgressUpdated
    tracker
        .record_progress(&snapshot, 1, 1_100)
        .expect("record_progress succeeds");
    assert_eq!(tracker.get_event_seq(), 2);
    let events = tracker.events();
    assert_eq!(events.len(), 2);
    match &events[1] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 2);
            assert_eq!(ev.event_type, MigrationEventType::ProgressUpdated);
            assert_eq!(ev.records_count, 1);
            assert_eq!(ev.status, MigrationAttemptStatus::InProgress);
            assert_eq!(ev.timestamp_ms, 1_100);
            assert_eq!(ev.correlation_id, format!("mig:seq:2:{}", &snapshot.header.checksum[..8]));
        }
        _ => panic!("expected AuditV1 event"),
    }

    // 4. Out-of-bounds progress fails and DOES NOT emit or increment seq
    let err_prog = tracker.record_progress(&snapshot, 999, 1_150);
    assert!(matches!(err_prog, Err(MigrationError::MigrationProgressOutOfBounds { .. })));
    assert_eq!(tracker.get_event_seq(), 2);
    assert_eq!(tracker.events().len(), 2);

    // 5. Legal mark_imported commits attempt and emits MigrationEventType::ImportCommitted
    tracker
        .mark_imported(&snapshot, 1_200)
        .expect("mark_imported succeeds");
    assert_eq!(tracker.get_event_seq(), 3);
    let events = tracker.events();
    assert_eq!(events.len(), 3);
    match &events[2] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 3);
            assert_eq!(ev.event_type, MigrationEventType::ImportCommitted);
            assert_eq!(ev.status, MigrationAttemptStatus::Completed);
            assert_eq!(ev.timestamp_ms, 1_200);
        }
        _ => panic!("expected AuditV1 event"),
    }

    // 6. Duplicate import fails and DOES NOT emit or increment seq
    let dup_import = tracker.mark_imported(&snapshot, 1_300);
    assert!(matches!(dup_import, Err(MigrationError::DuplicateImport)));
    assert_eq!(tracker.get_event_seq(), 3);
    assert_eq!(tracker.events().len(), 3);

    // 7. Legal mark_completed_at emits MigrationEventType::MigrationCompleted
    tracker.mark_completed_at(1_500);
    assert_eq!(tracker.get_event_seq(), 4);
    let events = tracker.events();
    assert_eq!(events.len(), 4);
    match &events[3] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 4);
            assert_eq!(ev.event_type, MigrationEventType::MigrationCompleted);
            assert_eq!(ev.status, MigrationAttemptStatus::Completed);
            assert_eq!(ev.timestamp_ms, 1_500);
        }
        _ => panic!("expected AuditV1 event"),
    }
}

#[test]
fn test_deterministic_gap_free_pagination() {
    let mut tracker = MigrationTracker::new();
    let snapshot1 = sample_generic("k1", "v1");
    let snapshot2 = sample_generic("k2", "v2");
    let snapshot3 = sample_generic("k3", "v3");

    tracker.mark_imported(&snapshot1, 100).unwrap(); // seq 1
    tracker.mark_imported(&snapshot2, 200).unwrap(); // seq 2
    tracker.mark_imported(&snapshot3, 300).unwrap(); // seq 3
    tracker.mark_completed_at(400); // seq 4

    assert_eq!(tracker.get_event_seq(), 4);

    // Page 1: limit 2 starting from beginning (seq 1)
    let page1: EventPage = tracker.query_events(None, 2);
    assert!(page1.gap_free);
    assert_eq!(page1.events.len(), 2);
    assert_eq!(page1.total_count, 4);
    assert!(page1.has_more);
    assert_eq!(page1.next_cursor, Some(3));
    match (&page1.events[0], &page1.events[1]) {
        (MigrationEvent::AuditV1(e1), MigrationEvent::AuditV1(e2)) => {
            assert_eq!(e1.seq, 1);
            assert_eq!(e2.seq, 2);
        }
        _ => panic!("expected AuditV1 events"),
    }

    // Page 2: limit 2 starting from cursor 3
    let page2: EventPage = tracker.query_events(page1.next_cursor, 2);
    assert!(page2.gap_free);
    assert_eq!(page2.events.len(), 2);
    assert_eq!(page2.total_count, 2);
    assert!(!page2.has_more);
    assert_eq!(page2.next_cursor, None);
    match (&page2.events[0], &page2.events[1]) {
        (MigrationEvent::AuditV1(e3), MigrationEvent::AuditV1(e4)) => {
            assert_eq!(e3.seq, 3);
            assert_eq!(e4.seq, 4);
        }
        _ => panic!("expected AuditV1 events"),
    }

    // Out of range query (from_seq = 10)
    let page_empty = tracker.query_events(Some(10), 10);
    assert!(page_empty.gap_free);
    assert!(page_empty.events.is_empty());
    assert!(!page_empty.has_more);
    assert_eq!(page_empty.next_cursor, None);
}

#[test]
fn test_fail_import_and_rollback_parity() {
    let mut tracker = MigrationTracker::new();
    let snapshot = sample_generic("transient_job", "data");

    // Begin import -> seq 1
    tracker.begin_import(&snapshot, 100).unwrap();
    assert_eq!(tracker.get_event_seq(), 1);

    // Fail import -> seq 2
    tracker.fail_import(&snapshot, 200).unwrap();
    assert_eq!(tracker.get_event_seq(), 2);

    let events = tracker.events();
    assert_eq!(events.len(), 2);
    match &events[1] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 2);
            assert_eq!(ev.event_type, MigrationEventType::ImportFailed);
            assert_eq!(ev.status, MigrationAttemptStatus::Failed);
        }
        _ => panic!("expected AuditV1 event"),
    }

    // Retry begin import -> seq 3
    tracker.begin_import(&snapshot, 300).unwrap();
    assert_eq!(tracker.get_event_seq(), 3);

    // Rollback by identity -> seq 4
    tracker.mark_rolled_back_by_identity(&snapshot.header.checksum, snapshot.header.version, 400);
    assert_eq!(tracker.get_event_seq(), 4);

    let events = tracker.events();
    assert_eq!(events.len(), 4);
    match &events[3] {
        MigrationEvent::AuditV1(ev) => {
            assert_eq!(ev.seq, 4);
            assert_eq!(ev.event_type, MigrationEventType::RollbackRestored);
            assert_eq!(ev.status, MigrationAttemptStatus::RolledBack);
        }
        _ => panic!("expected AuditV1 event"),
    }
}

#[test]
fn test_concurrent_audit_parity_and_gap_free_sequence() {
    const THREAD_COUNT: usize = 16;
    let shared = Arc::new(SharedMigrationTracker::new());
    let barrier = Arc::new(Barrier::new(THREAD_COUNT));

    let mut handles = Vec::with_capacity(THREAD_COUNT);
    for i in 0..THREAD_COUNT {
        let shared = Arc::clone(&shared);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let snapshot = sample_generic(&format!("concurrent_{i}"), "data");
            let json_bytes = serde_json::to_vec(&snapshot).unwrap();
            barrier.wait();
            shared.import_from_json(&json_bytes, (i as u64 + 1) * 100)
        }));
    }

    for handle in handles {
        handle.join().unwrap().unwrap();
    }

    shared.mark_completed_at(99_999);

    assert_eq!(shared.imported_count(), THREAD_COUNT);
    assert!(shared.is_completed());

    // Total events = THREAD_COUNT (ImportCommitted) + 1 (MigrationCompleted)
    let total_expected_events = THREAD_COUNT + 1;
    assert_eq!(shared.get_event_seq(), total_expected_events as u64);

    let all_events = shared.events();
    assert_eq!(all_events.len(), total_expected_events);

    // Verify gap-free sequence numbers across concurrent emissions
    for (idx, event) in all_events.iter().enumerate() {
        let expected_seq = (idx + 1) as u64;
        match event {
            MigrationEvent::AuditV1(audit) => {
                assert_eq!(audit.seq, expected_seq, "sequence must be strictly gap-free");
                assert_eq!(
                    audit.correlation_id,
                    format!(
                        "mig:seq:{expected_seq}:{}",
                        &audit.checksum[..audit.checksum.len().min(8)]
                    )
                );
            }
            _ => panic!("expected AuditV1 event"),
        }
    }

    // Verify pagination across concurrent events is gap-free
    let mut current_cursor = None;
    let mut paginated_events = Vec::new();
    loop {
        let page = shared.query_events(current_cursor, 4);
        assert!(page.gap_free, "each page must maintain gap_free invariant");
        paginated_events.extend(page.events);
        if !page.has_more {
            break;
        }
        current_cursor = page.next_cursor;
    }
    assert_eq!(paginated_events.len(), total_expected_events);
}
