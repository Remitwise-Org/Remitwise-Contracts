# Events and Audit Parity for Data Migration

## 1. Overview

During contract upgrades and data migrations, maintaining total audit parity and preserving payment and settlement records without cursor gaps or phantom states is essential for post-migration financial reconciliation. Downstream indexers and accounting services require a deterministic guarantee that audit events match committed contract states bit-for-bit.

This document details the design, schema, invariants, and guarantees implemented in `data_migration` for event emission and audit reconciliation.

---

## 2. Invariants & Guarantees

1. **Committed Transitions Only**:
   - Audit events are emitted, and the monotonic sequence number `event_seq` is incremented, **only** when a migration transition successfully commits.
   - Pre-validation failures (corrupted checksum, incompatible version, size limit breach, semantic invalidity), capacity limits, and duplicate imports return errors immediately without mutating tracker state, appending audit records, or advancing the sequence counter.

2. **Deterministic, Monotonic Event Sequence**:
   - Every audit record receives a 1-indexed, strictly monotonic sequence number (`seq: 1, 2, 3, ...`).
   - Every audit record includes a deterministic correlation identifier of the format:
     ```text
     mig:seq:<seq>:<checksum[..8]>
     ```
     e.g., `mig:seq:1:a1b2c3d4`.

3. **Deterministic, Gap-Free Pagination**:
   - The query API `query_events(from_seq: Option<u64>, limit: usize) -> EventPage` verifies that all returned records are contiguous without gaps (`gap_free: true`).
   - Pagination cursors (`next_cursor`) allow indexers to stream records sequentially without missing or duplicating entries.

4. **Bit-for-Bit Payment & Settlement Preservation**:
   - Canonical payload types `SnapshotPayload::Payments` and `SnapshotPayload::Settlements` represent payment schedules and settlement transactions.
   - Export, import, and semantic validation enforce business invariants (e.g. positive amounts, non-empty identifiers, proper timestamp records for paid bills).
   - Snapshot reconciliation reports (`ExportSnapshot::reconciliation_report`) sort logical keys and guarantee gap-free ordinals.

5. **Poisoned-Lock Resilient Concurrency**:
   - `SharedMigrationTracker` synchronizes concurrent imports across threads via an internal `Mutex<MigrationTracker>`.
   - In the event of a worker panic, `lock()` recovers from mutex poisoning via `poisoned.into_inner()`, preserving all committed audit records and maintaining atomic execution.

---

## 3. Audit Event & Schema Specifications

### Lifecycle Event Types (`MigrationEventType`)

| Event Type | Trigger | Terminal State |
| :--- | :--- | :--- |
| `ImportStarted` | `begin_import` invoked with valid snapshot | `InProgress` |
| `ProgressUpdated` | `record_progress` invoked with valid record offset | `InProgress` |
| `ImportCommitted` | `mark_imported` completes snapshot application | `Completed` |
| `ImportFailed` | `fail_import` explicitly records import failure | `Failed` |
| `RollbackRestored` | `restore_rollback` reverts uncommitted or failed state | `RolledBack` |
| `MigrationCompleted` | `mark_completed` / `mark_completed_at` signals all migrations done | `Completed` |

### Audit Event Structure (`MigrationAuditEventV1`)

```rust
pub struct MigrationAuditEventV1 {
    pub version: u32,
    pub seq: u64,
    pub correlation_id: String,
    pub event_type: MigrationEventType,
    pub checksum: String,
    pub snapshot_version: u32,
    pub payload_type: String,
    pub records_count: usize,
    pub timestamp_ms: u64,
    pub status: MigrationAttemptStatus,
    pub details: Option<String>,
}
```

### Event Page Structure (`EventPage`)

```rust
pub struct EventPage {
    pub events: Vec<MigrationEvent>,
    pub next_cursor: Option<u64>,
    pub has_more: bool,
    pub total_count: usize,
    pub gap_free: bool,
}
```

---

## 4. Payment & Settlement Payloads

### Payments Export (`PaymentsExport` & `PaymentExport`)

```rust
pub struct PaymentsExport {
    pub next_id: u32,
    pub bills: Vec<PaymentExport>,
}

pub struct PaymentExport {
    pub id: u32,
    pub owner: String,
    pub name: String,
    pub amount: i128,
    pub currency: String,
    pub due_date: u64,
    pub recurring: bool,
    pub frequency_days: u32,
    pub paid: bool,
    pub paid_at: Option<u64>,
    pub external_ref: Option<String>,
}
```

### Settlements Export (`SettlementsExport` & `SettlementExport`)

```rust
pub struct SettlementsExport {
    pub next_id: u32,
    pub settlements: Vec<SettlementExport>,
}

pub struct SettlementExport {
    pub id: u32,
    pub sender: String,
    pub recipient: String,
    pub amount: i128,
    pub currency: String,
    pub status: String,
    pub timestamp_ms: u64,
}
```

---

## 5. Verification & Test Matrix

The integration test suite in `data_migration/tests/events_audit_parity.rs` and unit tests in `data_migration/src/lib.rs` validate the following scenarios:

| Test Case | Verification Target | Status |
| :--- | :--- | :--- |
| `test_audit_event_emission_on_committed_transitions_only` | Audit records emitted only upon state commits; rejected ops emit nothing | Passed |
| `test_deterministic_gap_free_pagination` | `query_events` returns gap-free pages with correct cursors | Passed |
| `test_fail_import_and_rollback_parity` | Failure and rollback lifecycles record audit events with matching statuses | Passed |
| `test_payments_and_settlements_export_reconciliation` | Payments & Settlements snapshot reconciliation reports are gap-free and sorted | Passed |
| `test_concurrent_audit_parity_and_gap_free_sequence` | 16-thread concurrent burst maintains gap-free sequence and deterministic correlation IDs | Passed |
