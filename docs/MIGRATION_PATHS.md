# Migration Paths: N-2 → N

> **Audience:** contributors modifying `data_migration`, adding a new schema
> version, or writing upgrade tests for snapshot-carrying contracts.
>
> This document explains how the `data_migration` crate handles snapshots from
> older schema versions, which invariants each payload type must satisfy at
> import time, and which tests prove each path works. Read
> [docs/MIGRATIONS.md](MIGRATIONS.md) first for the on-chain struct-upgrade
> rules; this document covers the off-chain snapshot migration layer.

---

## 1. Schema Version Model

Every export snapshot carries a `SnapshotHeader` with a `version: u32` field.
Two constants in `data_migration/src/lib.rs` define the supported window:

```rust
pub const SCHEMA_VERSION: u32 = 1;      // current (N)
pub const MIN_SUPPORTED_VERSION: u32 = 1; // oldest accepted (N-2 when the window grows)
```

The import boundary enforces:

```
MIN_SUPPORTED_VERSION ≤ header.version ≤ SCHEMA_VERSION
```

A snapshot outside this window is rejected with
`MigrationError::IncompatibleVersion { found, min, max }` before any payload is
read. When `SCHEMA_VERSION` is bumped to `N`, `MIN_SUPPORTED_VERSION` is set to
`N-2` so operators can migrate two major versions in a single step.

**Rule of thumb:** when bumping `SCHEMA_VERSION` from 2 to 3, set
`MIN_SUPPORTED_VERSION` to 1. When bumping from 3 to 4, set it to 2. Never set
`MIN_SUPPORTED_VERSION` higher than `SCHEMA_VERSION - 2`—doing so forces
operators to run a two-step migration that passes through an intermediate
version.

---

## 2. N-2 → N Migration Paths (per payload type)

### 2.1 RemittanceSplit

**Payload struct:**

```rust
pub struct RemittanceSplitExport {
    pub owner: String,
    pub spending_percent: u32,  // basis points (0–10000)
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
}
```

**Import path for a snapshot at version N-2:**

1. Size guard: raw bytes ≤ `MAX_MIGRATION_SNAPSHOT_BYTES` (96 KiB).
2. Deserialise to `ExportSnapshot`.
3. Version check: `MIN_SUPPORTED_VERSION ≤ N-2 ≤ SCHEMA_VERSION`.
4. Payload bounds: 1 record, canonical JSON ≤ `MAX_MIGRATION_PAYLOAD_BYTES` (64 KiB).
5. Checksum verification (SHA-256 or legacy Simple — the algorithm is stored in
   the header; see §3 for legacy handling).
6. **Semantic invariant:** each field ≤ 10 000 basis points **and** the four
   fields sum to exactly 10 000.
7. Replay guard (`MigrationTracker`): `(checksum, version)` pair must not have
   been seen before.

**Worked example:**

```rust
use data_migration::{
    ExportSnapshot, ExportFormat, SnapshotPayload, RemittanceSplitExport,
    MigrationTracker, import_from_json, export_to_json,
};

// Simulate an N-2 snapshot produced before the version bump.
let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
    owner: "GABC123…".into(),
    spending_percent: 5_000,   // 50 %
    savings_percent:  3_000,   // 30 %
    bills_percent:    1_500,   // 15 %
    insurance_percent:  500,   // 5 %
});
let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
let bytes = export_to_json(&snapshot).unwrap();

// Import into the current version.
let mut tracker = MigrationTracker::new();
let loaded = import_from_json(&bytes, &mut tracker, /*timestamp_ms=*/ 1_700_000_000_000)
    .expect("import must succeed");

// Invariants still hold after the round-trip.
assert!(loaded.verify_checksum());
if let SnapshotPayload::RemittanceSplit(split) = &loaded.payload {
    let total = split.spending_percent
        + split.savings_percent
        + split.bills_percent
        + split.insurance_percent;
    assert_eq!(total, 10_000);
}
```

**Per-path tests (see `data_migration/src/lib.rs`):**

| Test name | What it proves |
|-----------|---------------|
| `test_snapshot_checksum_roundtrip_succeeds` | Valid v1 RemittanceSplit snapshot passes validation |
| `test_export_import_json_succeeds` | JSON round-trip returns version-compatible snapshot |
| `test_export_import_binary_succeeds` | Binary round-trip returns version-compatible snapshot |
| `test_import_from_json_rejects_incompatible_version_too_low` | Version below `MIN_SUPPORTED_VERSION` is rejected |
| `test_import_from_json_rejects_incompatible_version_too_high` | Version above `SCHEMA_VERSION` is rejected |
| `test_semantic_remittance_split_valid_sum_10000_accepted` | Percentages summing to exactly 10 000 are accepted |
| `test_semantic_remittance_split_sum_9999_rejected` | Percentages summing to 9 999 trigger `ValidationFailed` |
| `test_semantic_remittance_split_sum_10001_rejected` | Percentages summing to 10 001 trigger `ValidationFailed` |
| `test_semantic_remittance_split_single_value_out_of_range_rejected` | Any field > 10 000 triggers `ValidationFailed` |
| `test_semantic_remittance_split_all_zero_rejected` | All-zero percentages trigger `ValidationFailed` |
| `test_semantic_remittance_split_invalid_rejected_via_json_untracked` | Untracked JSON path enforces same validation |
| `test_semantic_remittance_split_invalid_rejected_via_binary_untracked` | Untracked binary path enforces same validation |
| `test_semantic_remittance_split_invalid_rejected_via_tracked_json` | Tracked JSON path enforces same validation |
| `test_semantic_remittance_split_invalid_rejected_via_tracked_binary` | Tracked binary path enforces same validation |
| `test_semantic_remittance_split_error_message_contains_sum` | Error message reports both actual sum and expected sum |
| `proptest_percent_rate_roundtrip` (in `remitwise-common`) | Property: any valid basis-points value round-trips through `Percent`/`Rate` |

### 2.2 SavingsGoals

**Payload structs:**

```rust
pub struct SavingsGoalsExport {
    pub next_id: u32,
    pub goals: Vec<SavingsGoalExport>,
}

pub struct SavingsGoalExport {
    pub id: u32,
    pub owner: String,
    pub name: String,
    pub target_amount: i64,
    pub current_amount: i64,
    pub target_date: u64,
    pub locked: bool,
}
```

**Import path for a snapshot at version N-2:**

Steps 1–5 are the same as §2.1. The semantic invariants differ:

6. **Invariant A — ID counter:** `next_id ≥ max(goal.id)` for all goals in the
   payload. A `next_id` lower than the highest goal ID indicates a truncated or
   forged snapshot.
7. **Invariant B — amount ceiling:** `current_amount ≤ target_amount` for every
   individual goal.
8. Replay guard (same as §2.1).

**Worked example:**

```rust
use data_migration::{
    ExportSnapshot, ExportFormat, SnapshotPayload, SavingsGoalsExport, SavingsGoalExport,
    MigrationTracker, import_from_binary, export_to_binary,
};

let payload = SnapshotPayload::SavingsGoals(SavingsGoalsExport {
    next_id: 3,   // one above the highest ID in the list
    goals: vec![
        SavingsGoalExport {
            id: 1,
            owner: "GABC123…".into(),
            name: "Emergency Fund".into(),
            target_amount: 5_000_0000000,   // 5 000 USDC (7-decimal)
            current_amount: 1_000_0000000,  // 1 000 USDC saved so far
            target_date: 1_800_000_000,
            locked: false,
        },
        SavingsGoalExport {
            id: 2,
            owner: "GABC123…".into(),
            name: "School Fees".into(),
            target_amount: 2_000_0000000,
            current_amount: 2_000_0000000,  // fully funded ← current == target is valid
            target_date: 1_750_000_000,
            locked: true,
        },
    ],
});
let snapshot = ExportSnapshot::new(payload, ExportFormat::Binary);
let bytes = export_to_binary(&snapshot).unwrap();

let mut tracker = MigrationTracker::new();
let loaded = import_from_binary(&bytes, &mut tracker, 1_700_000_000_000).unwrap();
assert!(loaded.verify_checksum());
```

**Per-path tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_semantic_savings_goals_valid_next_id_above_max_accepted` | `next_id > max(id)` is accepted |
| `test_semantic_savings_goals_next_id_equals_max_id_accepted` | `next_id == max(id)` is the minimum valid state |
| `test_semantic_savings_goals_next_id_below_max_id_rejected` | Counter wound back triggers `ValidationFailed` |
| `test_semantic_savings_goals_next_id_zero_with_goals_rejected` | `next_id=0` with goals triggers `ValidationFailed` |
| `test_semantic_savings_goals_empty_goals_list_accepted` | Empty list → no ID constraint → accepted |
| `test_semantic_savings_goals_current_amount_below_target_accepted` | `current < target` is valid |
| `test_semantic_savings_goals_current_amount_equals_target_accepted` | `current == target` (fully funded) is valid |
| `test_semantic_savings_goals_current_exceeds_target_rejected` | `current > target` triggers `ValidationFailed` |
| `test_semantic_savings_goals_large_current_amount_rejected` | `current_amount = i64::MAX` with smaller target is rejected |
| `test_semantic_savings_goals_next_id_invalid_rejected_via_json_untracked` | Untracked JSON path enforces invariant A |
| `test_semantic_savings_goals_amount_invalid_rejected_via_binary_untracked` | Untracked binary path enforces invariant B |
| `test_semantic_savings_goals_invalid_rejected_via_tracked_json` | Tracked JSON path enforces invariant B |
| `test_semantic_savings_goals_invalid_rejected_via_tracked_binary` | Tracked binary path enforces invariant A |
| `test_semantic_savings_goals_next_id_error_message_contains_ids` | Error message reports both `next_id` and `max(id)` |
| `test_roundtrip_json_savings_goals_payload` | JSON round-trip preserves all fields |
| `test_roundtrip_binary_savings_goals_payload` | Binary round-trip preserves all fields |
| `test_roundtrip_csv_savings_goals` | CSV round-trip preserves all fields |
| `test_roundtrip_csv_with_unicode_names` | Unicode owner/name strings survive CSV cycle |
| `test_csv_roundtrip_with_commas_in_names` | Commas in names are RFC 4180-escaped |
| `test_csv_roundtrip_with_quotes_in_names` | Double-quotes in names are RFC 4180-escaped |
| `test_csv_roundtrip_with_newlines_in_names` | Embedded newlines are RFC 4180-escaped |
| `test_csv_roundtrip_with_large_numbers` | `i64::MAX` and `u64::MAX` survive CSV cycle |

### 2.3 Generic

**Payload type:**

```rust
pub enum SnapshotPayload {
    Generic(BTreeMap<String, JsonValue>),
    …
}
```

No business invariants beyond the common size and record-count bounds. The
`BTreeMap` ensures deterministic key ordering, which is required for stable
checksums across re-exports.

**Per-path tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_roundtrip_json_generic_payload` | JSON round-trip preserves payload |
| `test_roundtrip_binary_generic_payload` | Binary round-trip preserves payload |
| `test_generic_payload_checksum_is_stable_across_map_order` | Insertion order does not change the checksum |
| `test_replay_protection_generic_payload_json_duplicate_rejected` | Replay guard fires for Generic JSON |
| `test_replay_protection_generic_payload_binary_duplicate_rejected` | Replay guard fires for Generic binary |
| `test_binary_determinism_large_generic_payload` | 50-field `Generic` snapshot is deterministic |

---

## 3. Checksum and Legacy Snapshot Handling

New snapshots use SHA-256:

```
SHA-256(version_le_bytes || format_utf8_bytes || canonical_payload_json)
```

Snapshots produced before SHA-256 was introduced carry
`hash_algorithm: ChecksumAlgorithm::Simple` (or a missing field that defaults
to `Simple`). Both variants are still accepted at import time; the algorithm
stored in the header determines which verifier is used:

```rust
ChecksumAlgorithm::Sha256  → SHA-256 over version || format || canonical_payload
ChecksumAlgorithm::Simple  → wrapping byte sum over version || format || canonical_payload
                             (falls back to payload-only sum for the oldest snapshot format)
```

**Legacy-compatibility tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_legacy_simple_checksum_import_succeeds` | `Simple`-algorithm snapshot imports correctly |
| `test_missing_hash_algorithm_field_defaults_to_legacy_simple` | Absent `hash_algorithm` JSON field defaults to `Simple` |
| `test_algorithm_field_roundtrips_json` | `Sha256` field survives JSON round-trip |
| `test_algorithm_field_roundtrips_binary` | `Sha256` field survives binary round-trip |

**When adding a new algorithm:** add a variant to `ChecksumAlgorithm`, add a
branch to `ExportSnapshot::verify_checksum`, keep the `Simple` branch to stay
backward-compatible, and add a golden-vector test (see §5) for every algorithm
variant.

---

## 4. Replay Protection

`MigrationTracker` tracks every successfully imported snapshot by its
`(checksum, version)` identity. A second import of the same payload via the
**same** tracker returns `MigrationError::DuplicateImport`.

The untracked helpers (`import_from_json_untracked`,
`import_from_binary_untracked`) use a throwaway tracker per call. They enforce
the full validation pipeline but provide no cross-call duplicate detection.
**Use the tracked variants in production.** Use the untracked variants only for
true one-shot scenarios and document why.

**Replay-protection tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_import_replay_protection_prevents_duplicates` | Tracked JSON: second import → `DuplicateImport` |
| `test_replay_protection_savings_goals_json_duplicate_rejected` | Tracked JSON: SavingsGoals duplicate detected |
| `test_replay_protection_generic_payload_json_duplicate_rejected` | Tracked JSON: Generic duplicate detected |
| `test_replay_protection_cross_payload_types_independent` | Importing all three types then re-importing each triggers duplicates |
| `test_replay_protection_savings_goals_binary_duplicate_rejected` | Tracked binary: duplicate detected |
| `test_same_payload_type_different_content_no_collision` | Different content → different checksum → no false positive |
| `test_different_payload_same_size_no_collision` | Different data with same encoded size → no collision |
| `test_tracker_is_imported_reflects_state_across_types` | `is_imported` reflects state for all payload types |
| `test_tracker_mark_imported_rejects_exact_duplicate` | `mark_imported` itself rejects duplicates |
| `test_tracker_mark_imported_allows_different_version_same_checksum` | Same checksum, different version → not a duplicate |
| `test_tracked_json_double_import_second_returns_duplicate_import` | Behavioural proof: tracked JSON double-import → `DuplicateImport` |
| `test_tracked_binary_double_import_second_returns_duplicate_import` | Behavioural proof: tracked binary double-import → `DuplicateImport` |
| `test_untracked_json_double_import_both_succeed` | Behavioural proof: untracked double-import **succeeds** (documented footgun) |
| `test_untracked_binary_double_import_both_succeed` | Behavioural proof: untracked binary double-import **succeeds** |
| `test_tracked_first_then_untracked_same_payload_untracked_succeeds` | Untracked call is independent of long-lived tracker |
| `test_untracked_first_then_tracked_same_payload_tracked_first_call_succeeds` | Untracked leaves no trace; tracked call still succeeds |

---

## 5. Golden-Vector Discipline

A golden vector is a **frozen binary snapshot** checked into the repository that
must always import to the expected payload. It proves that the binary
serialisation format has not silently changed between releases.

Current golden file: `data_migration/tests/golden_snapshot.bin.b64`

The file contains a base64-encoded binary snapshot of a `SavingsGoals` payload.
Any change to the bincode layout, `SnapshotHeader` field order, or the binary
representation of `JsonValue` will cause the golden test to fail.

**Golden-vector tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_binary_golden_vector_imports_to_expected_payload` | Frozen snapshot imports to the expected `SnapshotPayload` |
| `test_binary_golden_vector_checksum_stable` | Checksum is stable across re-exports of the golden snapshot |

### Regenerating the golden vector

Run after a **deliberate, reviewed** format change. Do not regenerate to silence
a failing test; a failing golden test means a breaking change was introduced
unintentionally.

```bash
cd data_migration
bash gen_golden.sh
```

After regenerating:

1. Commit the new `tests/golden_snapshot.bin.b64`.
2. Update the `SCHEMA_VERSION` constant if the format change is backward-incompatible.
3. Add a comment in the PR body explaining why the golden snapshot changed.

---

## 6. Rollback

Every import that might fail partway through should capture a `RollbackMetadata`
before touching any on-chain state:

```rust
use data_migration::{RollbackMetadata, MigrationTracker, ExportSnapshot};

fn apply_migration(
    current: Option<&ExportSnapshot>,
    incoming: &ExportSnapshot,
    tracker: &mut MigrationTracker,
    now_ms: u64,
) -> Result<ExportSnapshot, data_migration::MigrationError> {
    // 1. Validate the incoming snapshot and capture rollback info atomically.
    let rollback = incoming.validate_and_capture(current, now_ms)?;

    // 2. Attempt the import (this records the snapshot in the tracker).
    let bytes = data_migration::export_to_binary(incoming)?;
    let applied = data_migration::import_from_binary(&bytes, tracker, now_ms)?;

    // 3. If further contract-level checks fail, restore:
    //    rollback.restore(&mut state, tracker)?;

    Ok(applied)
}
```

`RollbackMetadata::restore` is idempotent: calling it multiple times is safe.
It:

- Reinstates the previous on-chain snapshot (or clears to `None` if there was
  no prior state).
- Removes the replay-tracking entry for the attempted import so the operator can
  safely retry.

**Rollback tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_failed_import_restores_state_exactly` | State reverts to previous snapshot; attempted import marker removed |
| `test_double_rollback_is_idempotent` | Calling `restore` twice is a no-op after the first |
| `test_rollback_from_empty_state_clears_to_none` | No prior state → `restore` leaves state as `None` |
| `test_rollback_describe_includes_checksums_and_versions` | Audit description includes timestamp and identity fields |
| `test_successful_import_discards_rollback_metadata` | After success, state equals the applied snapshot |

---

## 7. Encrypted Payload Paths

The `enc:vN:` prefix transports opaque, pre-encrypted bytes without performing
cryptography on-chain. Version negotiation is layered on top of the same import
boundary:

| Version | Max decoded size | Notes |
|---------|-----------------|-------|
| `enc:v1:` | `MAX_MIGRATION_PAYLOAD_BYTES` (64 KiB) | Current production version |
| `enc:v2:` | `MAX_MIGRATION_PAYLOAD_BYTES / 2` (32 KiB) | Stricter cap; planned for on-chain crypto extensions |

Versions outside `[MIN_SUPPORTED_ENCRYPTED_VERSION, MAX_SUPPORTED_ENCRYPTED_VERSION]`
(currently 1–2) are rejected with
`MigrationError::UnsupportedEncryptedVersion { found, max }`.

**Encrypted payload tests:**

| Test name | What it proves |
|-----------|---------------|
| `test_encrypted_payload_roundtrip_at_size_limit_succeeds` | v1 at exact size limit round-trips |
| `test_encrypted_payload_missing_marker_fails` | Missing `enc:v1:` prefix → `InvalidFormat` |
| `test_encrypted_payload_unsupported_version_marker_fails` | v3 marker → `UnsupportedEncryptedVersion` |
| `test_enc_v2_roundtrip_succeeds_for_small_payload` | v2 accepts small payloads |
| `test_enc_v2_rejects_payload_exceeding_v2_size_cap` | v2 enforces 32 KiB cap |
| `test_enc_v1_accepted_under_v2_size_cap_does_not_use_v2_rules` | v1 is unaffected by v2 cap |
| `test_encrypted_payload_empty_ciphertext_fails` | `enc:v1:` with empty body → `InvalidFormat` |
| `test_encrypted_payload_invalid_base64_fails` | Non-base64 body → `InvalidFormat` |
| `test_encrypted_payload_version_zero_rejected` | v0 → `UnsupportedEncryptedVersion` |
| `test_encrypted_payload_invalid_version_format_fails` | `enc:vabc:` → `InvalidFormat` |
| `test_enc_marker_fault_condition_exploration` | Proptest: arbitrary non-prefixed strings → `InvalidFormat` |

---

## 8. Bumping the Schema Version (contributor checklist)

When a payload type gains a new field or a validation rule changes:

1. **Update the version constant** in `data_migration/src/lib.rs`:

   ```rust
   pub const SCHEMA_VERSION: u32 = 2;        // was 1
   pub const MIN_SUPPORTED_VERSION: u32 = 1; // keep N-2 compatibility
   ```

2. **Add migration logic** in `validate_payload_semantics` (and
   `import_from_binary` if the struct layout changed):

   ```rust
   // Example: v1 snapshots do not have the `region` field; default to "global".
   if snapshot.header.version == 1 {
       export.region = Some("global".into());
   }
   ```

3. **Regenerate the golden snapshot** (see §5).

4. **Add a per-version import test** that loads a real v_old snapshot and
   asserts the new invariants hold after migration. Follow the pattern in
   `docs/UPGRADE_TESTING.md`.

5. **Run the full suite:**

   ```bash
   cargo test -p data_migration
   cargo build --target wasm32-unknown-unknown --release
   cargo clippy --workspace --all-targets -- -D warnings
   ```

6. **Link the PR to this document** and update the version table in
   [docs/MIGRATIONS.md](MIGRATIONS.md) if the contract-level struct also changed.

---

## 9. Running the Tests

```bash
# Run all data_migration tests
cargo test -p data_migration

# Run a specific path category
cargo test -p data_migration semantic          # semantic payload invariants
cargo test -p data_migration replay            # replay-protection paths
cargo test -p data_migration golden            # golden-vector tests
cargo test -p data_migration roundtrip         # round-trip tests
cargo test -p data_migration binary_determinism # determinism tests

# Run with output for debugging
cargo test -p data_migration -- --nocapture
```

---

## Related Documents

- [docs/MIGRATIONS.md](MIGRATIONS.md) — On-chain struct upgrade rules (storage key stability, optional field discipline).
- [docs/UPGRADE_TESTING.md](UPGRADE_TESTING.md) — How to load a previous snapshot and verify upgrade invariants.
- [docs/migration-formats.md](migration-formats.md) — JSON, binary, and CSV format specifications.
- [docs/migration-import-safety.md](migration-import-safety.md) — Complete import validation pipeline.
- [docs/binary-format-stability.md](binary-format-stability.md) — Binary determinism and golden-vector contract.
- [docs/data-migration-rollback.md](data-migration-rollback.md) — Rollback mechanics (shorter summary).
- [docs/UPGRADE_RUNBOOK.md](UPGRADE_RUNBOOK.md) — Operator-facing step-by-step upgrade runbook.
