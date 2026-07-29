use std::collections::BTreeMap;

use data_migration::{
    export_to_json, import_from_json, ExportFormat, ExportSnapshot, JsonValue, MigrationError,
    MigrationTracker, RemittanceSplitExport, SavingsGoalExport, SavingsGoalsExport,
    SnapshotPayload,
};

fn remittance_split() -> SnapshotPayload {
    SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
        owner: "GOWNER".into(),
        spending_percent: 50,
        savings_percent: 30,
        bills_percent: 15,
        insurance_percent: 5,
    })
}

fn savings_goals() -> SnapshotPayload {
    SnapshotPayload::SavingsGoals(SavingsGoalsExport {
        next_id: 2,
        goals: vec![SavingsGoalExport {
            id: 1,
            owner: "GOWNER".into(),
            name: "Emergency fund".into(),
            target_amount: 5_000,
            current_amount: 1_000,
            target_date: 2_000_000_000,
            locked: false,
        }],
    })
}

fn generic_payload(value: &str) -> SnapshotPayload {
    let mut entries = BTreeMap::new();
    entries.insert("payload".into(), JsonValue::from(serde_json::json!(value)));
    SnapshotPayload::Generic(entries)
}

fn assert_duplicate_import(payload: SnapshotPayload) {
    let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
    let bytes = export_to_json(&snapshot).expect("valid snapshot exports to JSON");
    let mut tracker = MigrationTracker::new();

    assert!(import_from_json(&bytes, &mut tracker, 1).is_ok());
    assert!(matches!(
        import_from_json(&bytes, &mut tracker, 2),
        Err(MigrationError::DuplicateImport)
    ));
}

#[test]
fn repeated_imports_are_rejected_for_every_snapshot_payload_type() {
    assert_duplicate_import(remittance_split());
    assert_duplicate_import(savings_goals());
    assert_duplicate_import(generic_payload("first"));
}

#[test]
fn tracker_identity_includes_snapshot_version() {
    let snapshot = ExportSnapshot::new(generic_payload("same payload"), ExportFormat::Json);
    let mut next_version = snapshot.clone();
    next_version.header.version += 1;
    let mut tracker = MigrationTracker::new();

    assert!(tracker.mark_imported(&snapshot, 1).is_ok());
    assert!(tracker.mark_imported(&next_version, 2).is_ok());
}

#[test]
fn different_same_size_payloads_do_not_collide() {
    let first = ExportSnapshot::new(generic_payload("alpha"), ExportFormat::Json);
    let second = ExportSnapshot::new(generic_payload("bravo"), ExportFormat::Json);
    let mut tracker = MigrationTracker::new();

    assert!(tracker.mark_imported(&first, 1).is_ok());
    assert!(tracker.mark_imported(&second, 2).is_ok());
}
