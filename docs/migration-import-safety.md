# Migration Import Safety Contract

This document describes the validation guarantees enforced on every import path
in the `data_migration` crate, addressing
[issue #655](https://github.com/Remitwise-Org/Remitwise-Contracts/issues/655).

## Import entry points

| Function | Replay tracking |
|---|---|
| `import_from_json` | Yes — requires a `MigrationTracker` |
| `import_from_binary` | Yes — requires a `MigrationTracker` |
| `import_from_json_untracked` | No — uses a throwaway tracker |
| `import_from_binary_untracked` | No — uses a throwaway tracker |

All four functions enforce the **same** validation pipeline. The "untracked"
variants delegate directly to their tracked counterparts; they do **not** skip
any guard.

## Validation pipeline (applied to every import)

### 1. Size guard (pre-deserialisation)

The raw byte slice is checked against `MAX_MIGRATION_SNAPSHOT_BYTES` before any
deserialisation occurs. This prevents denial-of-service from oversized inputs.

```
if bytes.len() > MAX_MIGRATION_SNAPSHOT_BYTES → MigrationError::SnapshotTooLarge
```

### 2. Deserialisation

The bytes are decoded into an `ExportSnapshot` (JSON via `serde_json` or binary
via `bincode`). A malformed envelope returns `MigrationError::DeserializeError`.

### 3. Version compatibility (`validate_for_import`)

```
MIN_SUPPORTED_VERSION ≤ header.version ≤ SCHEMA_VERSION
```

- `header.version < MIN_SUPPORTED_VERSION` → `MigrationError::IncompatibleVersion`
- `header.version > SCHEMA_VERSION` → `MigrationError::IncompatibleVersion`

Current values: `MIN_SUPPORTED_VERSION = 1`, `SCHEMA_VERSION = 1`.

### 4. Payload bounds

- Record count > `MAX_MIGRATION_RECORDS` → `MigrationError::TooManyRecords`
- Canonical payload JSON > `MAX_MIGRATION_PAYLOAD_BYTES` → `MigrationError::PayloadTooLarge`

### 5. Checksum verification

The stored `header.checksum` is recomputed from the live payload and compared.

**SHA-256 algorithm** (default for new snapshots):

```
SHA-256(version_le_bytes || format_utf8_bytes || canonical_payload_json)
```

**Simple algorithm** (legacy snapshots):

```
wrapping_sum(version_le_bytes || format_utf8_bytes || canonical_payload_json)
```

A mismatch returns `MigrationError::ChecksumMismatch`. This detects tampered
payloads, bit-flips, and format-substitution attacks.

### 6. Replay protection (tracked imports only)

`import_from_json` and `import_from_binary` call `MigrationTracker::mark_imported`,
which records the `(checksum, version)` identity. A second import of the same
snapshot returns `MigrationError::DuplicateImport`.

The untracked helpers use a fresh, ephemeral tracker per call, so they do not
persist replay state across invocations. Use the tracked variants whenever
persistent replay protection is required.

## Security notes

- **Fail-closed**: every error path returns a typed `MigrationError`; there is
  no silent success on invalid input.
- **Version-downgrade protection**: the checksum binds the schema version, so a
  snapshot cannot be re-labelled with a lower version to bypass future guards.
- **Format-substitution protection**: the checksum also binds the format string
  (`"json"`, `"binary"`, etc.), preventing a JSON snapshot from being accepted
  as binary or vice versa.
- **No cryptographic authentication**: the checksum provides integrity, not
  authenticity. Callers that require authenticated imports should wrap the
  snapshot bytes in an authenticated encryption scheme before transport and
  verify the MAC before calling any import function.

## References

- `data_migration/src/lib.rs` — `import_from_json`, `import_from_binary`,
  `import_from_json_untracked`, `import_from_binary_untracked`,
  `validate_for_import`, `verify_checksum`, `is_version_compatible`,
  `MigrationTracker`, `MigrationError`
- `SCHEMA_VERSION`, `MIN_SUPPORTED_VERSION`, `MAX_MIGRATION_SNAPSHOT_BYTES`,
  `MAX_MIGRATION_PAYLOAD_BYTES`, `MAX_MIGRATION_RECORDS`
