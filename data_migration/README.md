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

## ⚠️ Encrypted payload: encoding-only (no cryptography)

> **The `export_to_encrypted_payload` / `import_from_encrypted_payload` functions do NOT perform encryption.**
>
> The `enc:v1:<base64>` format is an **encoding/marker only** and provides no
> confidentiality or integrity protection beyond the snapshot checksum.
>
> **Wire format:** ``enc:v1:` + base64(plain_bytes)``
>
> - Prefix constant: `ENCRYPTED_PAYLOAD_PREFIX_V1 = "enc:v1:"` (`lib.rs:31`)
> - Max encoded size: `MAX_ENCRYPTED_PAYLOAD_BYTES` (`lib.rs:52–53`)
>
> ### Why this matters
>
> A developer reading "encrypted" will reasonably assume the payload is
> confidential. This crate does not use any key, cipher, or on-chain
> cryptographic operation. Putting sensitive data through this function
> **leaves it fully visible** to anyone with access to the encoded string.
>
> ### What to do instead
>
> 1. Encrypt sensitive data off-chain (e.g. AES-256-GCM or
>    age/chacha20poly1305) **before** calling this function.
> 2. Decrypt off-chain **after** calling `import_from_encrypted_payload`.
> 3. A future `enc:v2:` format may add on-chain cryptographic operations.
>
> ### Related security context
>
> See [`THREAT_MODEL.md`](../THREAT_MODEL.md) §5.1 (Critical Gaps / Weak
> Checksum) and [`SECURITY_REVIEW_SUMMARY.md`](../SECURITY_REVIEW_SUMMARY.md)
> (Short-Term / SECURITY-004) for the broader data-migration security picture.

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

All import functions validate version compatibility, SHA-256 checksum, **and payload-type semantic invariants** before returning. An `Err` is returned if any check fails — the caller must not use the snapshot data if validation fails.

```rust
// JSON (tracked — provides cross-call duplicate/replay protection)
let mut tracker = MigrationTracker::new();
let snapshot = data_migration::import_from_json(&json_bytes, &mut tracker, timestamp_ms)?;

// Binary (tracked — provides cross-call duplicate/replay protection)
let snapshot = data_migration::import_from_binary(&bin_bytes, &mut tracker, timestamp_ms)?;

// CSV (goals only; no header checksum)
let goals = data_migration::import_goals_from_csv(&csv_bytes)?;

// Encrypted passthrough (caller decrypts after)
let plain_bytes = data_migration::import_from_encrypted_payload(&b64)?;
```

> **⚠️ Untracked variants do NOT provide cross-call duplicate/replay protection.**
>
> [`import_from_json_untracked`] and [`import_from_binary_untracked`] construct a
> throwaway [`MigrationTracker`] on each call. Importing the same payload twice via
> these helpers **succeeds both times** — no error is raised on the second call.
>
> Use these functions only for true one-shot scenarios (e.g. migration scripts
> guaranteed to run exactly once). Prefer the tracked variants in all other contexts.
> See the [Tracked vs Untracked duplicate protection](#tracked-vs-untracked-duplicate-protection)
> section below for the full behavioural matrix.

### Manual validation

```rust
// Check version only
data_migration::check_version_compatibility(snapshot.header.version)?;

// Full validation (version + payload bounds + checksum + semantic invariants)
snapshot.validate_for_import()?;
```

### Semantic invariants enforced at import

`validate_for_import` (and therefore all `import_from_*` helpers) is **fail-closed**: in addition to structural checks it enforces the same business rules the live contracts enforce at write-time.

| Payload type | Invariant | Error on violation |
|---|---|---|
| `RemittanceSplit` | `spending + savings + bills + insurance == 100` | `ValidationFailed` — sum and individual values included in message |
| `SavingsGoals` | `next_id >= max(goal.id)` across all goals | `ValidationFailed` — both ids included in message |
| `SavingsGoals` | `current_amount <= target_amount` for every goal | `ValidationFailed` — goal id and amounts included in message |
| `Generic` | *(none beyond size/count bounds)* | — |

**Why this matters:** migration is where contract invariants are most easily bypassed, because data arrives pre-formed rather than through guarded entry-points. A split config that sums to 73% or 140%, or a savings snapshot with a wound-back `next_id`, would produce corrupt on-chain state that the contract would subsequently refuse to touch — a silent data-integrity bug introduced at the import boundary.

## Tracked vs Untracked duplicate protection

[`MigrationTracker`] is how this crate prevents the same snapshot from being applied
to on-chain state more than once. The tracker records every successfully imported
snapshot by its `(checksum, version)` identity. Passing the **same long-lived tracker**
to every `import_from_json` / `import_from_binary` call means a second attempt with
the same payload is always rejected with `MigrationError::DuplicateImport`.

The `_untracked` variants are **convenience wrappers that construct and immediately
discard a throwaway tracker**. There is no persistent state between calls. Because
of this:

| Scenario | `import_from_json` / `import_from_binary` | `import_from_json_untracked` / `import_from_binary_untracked` |
|---|:---:|:---:|
| First import | ✅ `Ok` | ✅ `Ok` |
| Second import, same payload, same call session | ❌ `Err(DuplicateImport)` | ✅ `Ok` (footgun!) |
| Structural validation (size, version, checksum, semantics) | ✅ enforced | ✅ enforced |

### When `_untracked` is safe

- The import runs exactly once and there is no possibility of a retry or replay
  (e.g. a CI migration script that runs as a one-shot job).
- You manage duplicate detection outside this crate (e.g. idempotency keys at the
  transport layer).

### When `_untracked` is NOT safe

Any scenario where the same snapshot bytes could arrive twice — network retries,
operator restarts, replay attacks — requires a long-lived `MigrationTracker`. Use
[`import_from_json`] / [`import_from_binary`] and persist the tracker.

> **Deprecation note:** The `_untracked` variants are retained for legacy one-shot
> call sites. New code should use the tracked variants and pass an explicit
> `MigrationTracker`. A future breaking release may remove the untracked helpers
> entirely.

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
| `ValidationFailed` | Semantic invariant violated (percent sum ≠ 100, `next_id` wound back, `current_amount > target_amount`) |

## Security assumptions

1. `serde_json::to_vec` produces deterministic output for the same Rust value across serialise→deserialise roundtrips (true for all types used here).
2. SHA-256 is collision-resistant under current cryptographic assumptions.
3. The `hex` module in this crate produces lowercase hex consistent with common verifiers.
4. Callers are responsible for transport-layer authenticity (signing/verification) if the threat model includes a fully active attacker who can forge entire snapshots.
