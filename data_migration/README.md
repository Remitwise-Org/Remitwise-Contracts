# data_migration

Off-chain import/export utilities for Remitwise contract snapshots.

Supports JSON, binary (bincode), CSV, and encrypted formats. Every snapshot carries a SHA-256 checksum that binds the schema version, format label, and payload together — making any single-field tampering detectable.

## Security model

### What the checksum protects

The checksum is computed as:

```
SHA-256( version_le_bytes(4) || format_utf8_bytes || canonical_payload_json )
```

Binding all three inputs closes attack surfaces that a **payload-only** hash leaves open:

| Attack | Payload-only hash | This implementation |
|--------|:-----------------:|:-------------------:|
| Mutate a goal's `current_amount` | Detected ✓ | Detected ✓ |
| Change `header.version` to trigger a downgrade | **Not detected ✗** | Detected ✓ |
| Relabel `header.format` from `json` → `binary` | **Not detected ✗** | Detected ✓ |

### What the checksum does NOT protect

The checksum provides **integrity** (tamper detection), not **authentication**. An attacker who can create a snapshot from scratch can produce a valid checksum. Callers that require end-to-end authenticity should sign the serialised snapshot bytes with an asymmetric key (e.g. Ed25519) before transmission and verify the signature before calling `import_from_*`.

### Hash algorithm field

Every `SnapshotHeader` carries a `hash_algorithm: ChecksumAlgorithm` field. New exports produce `ChecksumAlgorithm::Sha256`, while legacy snapshots without an explicit algorithm field or with `ChecksumAlgorithm::Simple` continue to import successfully. The field is `#[non_exhaustive]` so future algorithm upgrades can be added as new variants without breaking existing importers — which must reject any algorithm they do not recognise rather than silently skipping verification.

## API reference

### Building a snapshot

```rust
use data_migration::{ExportSnapshot, ExportFormat, SnapshotPayload, RemittanceSplitExport};

let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
    owner: "GABC...".into(),
    spending_percent: 50,
    savings_percent: 30,
    bills_percent: 15,
    insurance_percent: 5,
});

// Checksum is computed automatically.
let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
assert!(snapshot.verify_checksum());
```

### Exporting

```rust
// JSON (human-readable)
let json_bytes = data_migration::export_to_json(&snapshot)?;

// Binary (compact, bincode)
let bin_bytes = data_migration::export_to_binary(&snapshot)?;

// CSV (goals list only)
let csv_bytes = data_migration::export_to_csv(&goals_export)?;

// Encrypted passthrough (caller encrypts first, then base64-wraps)
let b64 = data_migration::export_to_encrypted_payload(&ciphertext_bytes);
```

### Importing

All import functions validate version compatibility and SHA-256 checksum before returning. An `Err` is returned if either check fails — the caller must not use the snapshot data if validation fails.

```rust
// JSON
let snapshot = data_migration::import_from_json(&json_bytes)?;

// Binary
let snapshot = data_migration::import_from_binary(&bin_bytes)?;

// CSV (goals only; no header checksum)
let goals = data_migration::import_goals_from_csv(&csv_bytes)?;

// Encrypted passthrough (caller decrypts after)
let plain_bytes = data_migration::import_from_encrypted_payload(&b64)?;
```

### Manual validation

```rust
// Check version only
data_migration::check_version_compatibility(snapshot.header.version)?;

// Full validation (version + checksum)
snapshot.validate_for_import()?;
```

## Data structures

### `SnapshotHeader`

| Field | Type | Description |
|-------|------|-------------|
| `version` | `u32` | Schema version (bound into checksum) |
| `checksum` | `String` | 64-char lowercase hex SHA-256 |
| `hash_algorithm` | `ChecksumAlgorithm` | Algorithm used (`Sha256`) |
| `format` | `String` | Format label — `"json"`, `"binary"`, `"csv"`, `"encrypted"` (bound into checksum) |
| `created_at_ms` | `Option<u64>` | Optional UNIX timestamp in milliseconds |

### `SnapshotPayload` variants

| Variant | Inner type | Description |
|---------|------------|-------------|
| `RemittanceSplit` | `RemittanceSplitExport` | Remittance allocation config |
| `SavingsGoals` | `SavingsGoalsExport` | Goals list + next ID |
| `Generic` | `HashMap<String, Value>` | Arbitrary JSON map for future use |

## Error types

| Variant | When raised |
|---------|-------------|
| `IncompatibleVersion` | `header.version` outside `[MIN_SUPPORTED_VERSION, SCHEMA_VERSION]` |
| `ChecksumMismatch` | Recomputed hash does not match stored `header.checksum` |
| `UnknownHashAlgorithm` | `header.hash_algorithm` is not `Sha256` |
| `InvalidFormat` | CSV or serialisation format error |
| `DeserializeError` | JSON/binary deserialisation failure |
| `ValidationFailed` | General validation failure |

## Security assumptions

1. `serde_json::to_vec` produces deterministic output for the same Rust value across serialise→deserialise roundtrips (true for all types used here).
2. SHA-256 is collision-resistant under current cryptographic assumptions.
3. The `hex` module in this crate produces lowercase hex consistent with common verifiers.
4. Callers are responsible for transport-layer authenticity (signing/verification) if the threat model includes a fully active attacker who can forge entire snapshots.
