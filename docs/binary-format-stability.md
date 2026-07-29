# Binary Serialization Format Stability

## Overview

The `data_migration` crate provides binary serialization for `ExportSnapshot` via `export_to_binary()` and `import_from_binary()`. Binary snapshots are the **canonical backup/restore format** and must maintain byte-for-byte stability across releases.

## Stability Guarantee

### Determinism

**Requirement:** Exporting the same `ExportSnapshot` twice must yield **byte-identical output**.

- `export_to_binary(snapshot) == export_to_binary(snapshot)` (always)
- Non-deterministic serialization (e.g., map ordering drift, timestamp inclusion) breaks checksum stability and makes backups unverifiable.

**Implementation:**
- `ExportSnapshot` fields are serialized in a fixed order.
- `SnapshotPayload::Generic` uses `BTreeMap<String, JsonValue>` to ensure stable ordering.
- Metadata fields (e.g., `created_at_ms`) are never included in the checksum computation.

### Checksum Binding

The SHA-256 checksum is computed over:
```
SHA-256(version_le_bytes || format_utf8_bytes || canonical_payload_json)
```

This binds three critical inputs:
1. **Version** – schema version (prevents version-downgrade attacks)
2. **Format** – export format label (prevents format-substitution attacks)
3. **Payload** – canonical JSON representation (ensures integrity)

**Key insight:** The checksum is computed over the *canonical JSON* representation of the payload, not the binary bytes. This ensures:
- The checksum is **stable across binary format versions** (as long as canonical JSON stays the same).
- Re-exporting a snapshot after round-trip (import → export) yields the **same checksum**.

### Round-Trip Stability

**Requirement:** `export_to_binary(import_from_binary(b)) == b`

Round-trip tests verify:
1. Export → Import → Export yields byte-identical output.
2. Checksums are preserved across round-trips.
3. All payload types (RemittanceSplit, SavingsGoals, Generic) round-trip cleanly.

## Format Migration Path

### Backward Compatibility

If the binary format must change:

1. **Bump `SCHEMA_VERSION`** in `src/lib.rs` (e.g., from 1 to 2).
2. **Implement a migration layer** in `import_from_binary()`:
   - Detect version from deserialized header.
   - If `version < SCHEMA_VERSION`, apply backward-compatible migration logic.
3. **Update canonical payload JSON** if needed (e.g., to normalize fields).
4. **Regenerate the golden snapshot** with the new version.
5. **Test both old and new snapshots** in the suite.

### Breaking Changes

A **breaking change** occurs when:
- The binary serialization format changes **without** a version bump.
- The canonical payload JSON representation changes (field names, ordering).
- The checksum algorithm changes.

**Impact:** All existing backups become unverifiable and unrestorable.

## Golden Vector Test

The `tests/golden_snapshot.bin.b64` file contains a frozen binary vector that must **always** import to the expected `SnapshotPayload`. This is checked by:

```rust
#[test]
fn test_binary_golden_vector_imports_to_expected_payload() { ... }
```

If this test fails, it signals a **breaking change** in the binary format.

## Size Constraints

- **`MAX_MIGRATION_SNAPSHOT_BYTES`** – Maximum serialized snapshot envelope size (96 KiB).
- **`MAX_MIGRATION_PAYLOAD_BYTES`** – Maximum canonical payload size (64 KiB).
- **`MAX_MIGRATION_RECORDS`** – Maximum logical records per payload (1,024).

Pre-deserialization size checks prevent DoS attacks from oversized snapshots.

## Security Notes

1. **Checksum is NOT authentication** – It provides integrity, not authenticity. Callers must verify the snapshot source.
2. **Binary format is NOT encrypted** – Sensitive data must be encrypted off-chain before export.
3. **Deterministic format enables verification** – The same snapshot always produces the same binary and checksum, allowing backups to be independently verified.

## Testing Coverage

The binary format is tested for:

- **Round-trip stability** (≥95% coverage)
- **Determinism** (same snapshot → same bytes)
- **Golden vector** (frozen snapshot imports correctly)
- **Size-bound rejections** (oversized, truncated blobs)
- **Checksum verification** (post-round-trip validation)
- **Edge cases** (empty payloads, max records, malformed input)

Run tests with:
```bash
cargo test -p data_migration binary -- --nocapture
```

## References

- [data_migration/src/lib.rs](../data_migration/src/lib.rs) – Implementation and tests.
- [THREAT_MODEL.md](./THREAT_MODEL.md) – Security context for data-migration operations.
- [SECURITY_REVIEW_SUMMARY.md](./SECURITY_REVIEW_SUMMARY.md) – Data-migration security recommendations.
