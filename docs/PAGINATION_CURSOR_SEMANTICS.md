# Deterministic Pagination and Cursor Semantics for Migration & Reconciliation

## Overview
During contract upgrades, large-scale backups, and post-migration auditing, financial payment and settlement data must be preserved and verifiable across batches. Reconciling migration records without pagination risks unbounded payload sizes and out-of-memory errors on large migration runs. Conversely, non-deterministic pagination or cursor gaps cause skipped records, duplicate accounting, and broken financial reconciliation.

This specification documents the deterministic, gap-free pagination and cursor semantics implemented in `data_migration` across [`MigrationTracker`], [`SharedMigrationTracker`], and [`ExportSnapshot`].

---

## Core Invariants

### 1. Gap-Free Partition Invariant
For any applied dataset of size $N$, traversing successive pages starting from `cursor = None` until `is_last_page == true` produces a sequence of items $S = P_1 \cup P_2 \cup \dots \cup P_k$ such that:
1. **Completeness**: $|S| = N$, containing every committed record identity.
2. **Disjointness**: $P_i \cap P_j = \emptyset$ for all $i \neq j$ (zero duplicates across page boundaries).
3. **Equivalence**: $S$ exactly equals the output of the unbounded [`MigrationTracker::imported_records`].

### 2. Strict Deterministic Ordering
- `ImportRecord` items are ordered strictly by composite key `(checksum, version)` in ascending lexicographical order.
- `SnapshotRecordRef` items are ordered strictly by canonical logical key (e.g. `savings_goal:<owner>:<id>`, `remittance_split:<owner>`), assigned sequential zero-based ordinals.
- `MigrationAttempt` items are ordered strictly by transition log sequence.
- Ordering is independent of application order, thread scheduling, and batch submission order.

### 3. Scope Safety and State Isolation
- Pagination queries are purely read-only and never mutate tracker state, attempt counters, or migration flags.
- Conflicting, rejected, or invalid cursor requests return deterministic error values without mutating tracker state or leaving partial records.

---

## Cursor Design and Serialization

### `MigrationCursor`
A typed composite identity cursor encoding the last observed record of a page:

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MigrationCursor {
    pub checksum: String,
    pub version: u32,
}
```

### Wire Format
- **Format**: `mc:v1:<version>:<checksum_hex>`
- **Prefix**: `mc:v1:` (Versioned token prefix preventing cross-version or malformed format interpretation).
- **Validation Rules**:
  1. Must start with exact prefix `mc:v1:`.
  2. Must contain valid UTF-8 digits for schema `version`.
  3. `checksum` must be non-empty and composed strictly of ASCII hexadecimal characters (`[0-9a-fA-F]`).
  4. Leading/trailing whitespace is trimmed gracefully.
  5. Any malformed or unsupported string returns [`MigrationError::InvalidCursor(String)`].

---

## Page Limit Normalization

All paginated endpoints enforce standard limit bounds via [`clamp_migration_limit`]:

```rust
pub const DEFAULT_MIGRATION_PAGE_LIMIT: usize = 20;
pub const MAX_MIGRATION_PAGE_LIMIT: usize = 100;

pub fn clamp_migration_limit(limit: usize) -> usize {
    match limit {
        0 => DEFAULT_MIGRATION_PAGE_LIMIT,
        n if n > MAX_MIGRATION_PAGE_LIMIT => MAX_MIGRATION_PAGE_LIMIT,
        n => n,
    }
}
```

- `limit = 0` defaults to `20` records.
- `limit > 100` is clamped to `100` records.
- `1 <= limit <= 100` is passed through unchanged.
- Clamping is idempotent: `clamp_migration_limit(clamp_migration_limit(n)) == clamp_migration_limit(n)`.

---

## API Surface

### 1. `MigrationTracker` & `SharedMigrationTracker`

```rust
impl MigrationTracker {
    pub fn imported_records_page(
        &self,
        cursor: Option<&MigrationCursor>,
        limit: usize,
    ) -> Result<ImportRecordPage, MigrationError>;

    pub fn attempt_history_page(
        &self,
        cursor: Option<usize>,
        limit: usize,
    ) -> Result<MigrationAttemptPage, MigrationError>;
}
```

`SharedMigrationTracker` exposes the same methods over thread-safe locks with poisoned-lock recovery.

### 2. `ExportSnapshot`

```rust
impl ExportSnapshot {
    pub fn reconciliation_page(
        &self,
        cursor: Option<usize>,
        limit: usize,
    ) -> Result<SnapshotReconciliationPage, MigrationError>;
}
```

---

## Response Shapes

### `ImportRecordPage`
```rust
pub struct ImportRecordPage {
    pub items: Vec<ImportRecord>,
    pub next_cursor: Option<MigrationCursor>,
    pub total_count: usize,
    pub is_last_page: bool,
}
```

### `SnapshotReconciliationPage`
```rust
pub struct SnapshotReconciliationPage {
    pub snapshot_checksum: String,
    pub version: u32,
    pub payload_type: String,
    pub total_records: usize,
    pub items: Vec<SnapshotRecordRef>,
    pub next_cursor: Option<usize>,
    pub is_last_page: bool,
    pub gap_free: bool,
}
```

### `MigrationAttemptPage`
```rust
pub struct MigrationAttemptPage {
    pub items: Vec<MigrationAttempt>,
    pub next_cursor: Option<usize>,
    pub total_count: usize,
    pub is_last_page: bool,
}
```

---

## Concurrency & Retry Behavior

1. **Concurrent Reads**: Multiple indexers or operator threads reading pages concurrently observe consistent, deterministic snapshots without contention artifacts or mutex deadlocks.
2. **Concurrent Writes**: `SharedMigrationTracker` serializes imports atomically. A page query executed against a shared tracker always observes a valid, sorted prefix of committed records.
3. **Poisoned-Lock Recovery**: If an import worker panics while holding the lock, the recovered tracker remains coherent and continues serving valid pages.

---

## Compatibility & Migration

- **Public API Backward Compatibility**: The existing unbounded `imported_records()` and `reconciliation_report()` APIs remain unchanged and fully functional.
- **Storage Serialization**: `MigrationTracker` utilizes `BTreeMap` serialization for byte-level determinism. Bincode and serde formats deserialize transparently.
