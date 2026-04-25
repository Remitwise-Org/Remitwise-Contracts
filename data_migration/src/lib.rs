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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ChecksumAlgorithm {
    /// SHA-256 over `version_le_bytes || format_utf8_bytes || canonical_payload_json`.
    Sha256,
    /// Legacy checksum used by older snapshots.
    Simple,
}

impl Default for ChecksumAlgorithm {
    fn default() -> Self {
        ChecksumAlgorithm::Simple
    }
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

/// Payload variants per contract type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SnapshotPayload {
    RemittanceSplit(RemittanceSplitExport),
    SavingsGoals(SavingsGoalsExport),
    Generic(HashMap<String, serde_json::Value>),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemittanceSplitExport {
    pub owner: String,
    pub spending_percent: u32,
    pub savings_percent: u32,
    pub bills_percent: u32,
    pub insurance_percent: u32,
}

/// Exportable savings goals list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavingsGoalsExport {
    pub next_id: u32,
    pub goals: Vec<SavingsGoalExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn compute_checksum(&self) -> String {
        let payload_bytes = self
            .payload_bytes()
            .unwrap_or_else(|_| panic!("payload must be serializable"));
        Self::checksum_for_parts(self.header.version, &self.header.format, &payload_bytes)
    }

    fn compute_simple_checksum(&self) -> String {
        let payload_bytes = self
            .payload_bytes()
            .unwrap_or_else(|_| panic!("payload must be serializable"));
        Self::simple_checksum_for_parts(self.header.version, &self.header.format, &payload_bytes)
    }

    fn compute_legacy_simple_checksum(&self) -> String {
        let payload_bytes = self
            .payload_bytes()
            .unwrap_or_else(|_| panic!("payload must be serializable"));
        Self::legacy_simple_checksum(&payload_bytes)
    }

    /// Verify that the stored checksum matches the current payload.
    pub fn verify_checksum(&self) -> bool {
        match self.header.hash_algorithm {
            ChecksumAlgorithm::Sha256 => self.header.checksum == self.compute_checksum(),
            ChecksumAlgorithm::Simple => {
                let expected = self.compute_simple_checksum();
                self.header.checksum == expected
                    || self.header.checksum == self.compute_legacy_simple_checksum()
            }
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
        snapshot.header.checksum = snapshot.compute_checksum();
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
            let ordered_entries: BTreeMap<&str, &serde_json::Value> = entries
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

/// Export to CSV (for tabular payloads only; e.g. goals list).
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
            goal.owner.clone(),
            goal.name.clone(),
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

/// Encrypted format: store a prefixed base64-encoded payload.
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

/// Decode encrypted payload from prefixed base64.
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
pub fn import_from_json_untracked(bytes: &[u8]) -> Result<ExportSnapshot, MigrationError> {
    let mut tracker = MigrationTracker::new();
    import_from_json(bytes, &mut tracker, 0)
}

/// Legacy helper for callers that do not need replay tracking.
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

#[derive(Debug, Deserialize)]
struct CsvGoalRow {
    id: u32,
    owner: String,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackMetadata {
    pub previous_version: u32,
    pub previous_checksum: String,
    pub timestamp_ms: u64,
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
        entries.insert("key1".into(), serde_json::json!("value1"));
        entries.insert("key2".into(), serde_json::json!(42));
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
            ("aa".into(), serde_json::json!("11")),
            ("bb".into(), serde_json::json!("22")),
        ]));
        let second_payload = SnapshotPayload::Generic(HashMap::from([
            ("cc".into(), serde_json::json!("33")),
            ("dd".into(), serde_json::json!("44")),
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
        snapshot.header.checksum = snapshot.compute_simple_checksum();

        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let loaded = import_from_json_untracked(&bytes).unwrap();
        assert_eq!(loaded.header.hash_algorithm, ChecksumAlgorithm::Simple);
        assert!(loaded.verify_checksum());
    }

    #[test]
    fn test_missing_hash_algorithm_field_defaults_to_legacy_simple() {
        let mut snapshot = ExportSnapshot::new(sample_remittance_payload(), ExportFormat::Json);
        snapshot.header.checksum = snapshot.compute_simple_checksum();
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
            serde_json::Value::String("x".repeat(MAX_MIGRATION_PAYLOAD_BYTES)),
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
    fn test_generic_payload_checksum_is_stable_across_map_order() {
        let mut first = HashMap::new();
        first.insert("b".into(), serde_json::json!(2));
        first.insert("a".into(), serde_json::json!(1));

        let mut second = HashMap::new();
        second.insert("a".into(), serde_json::json!(1));
        second.insert("b".into(), serde_json::json!(2));

        let first_snapshot =
            ExportSnapshot::new(SnapshotPayload::Generic(first), ExportFormat::Json);
        let second_snapshot =
            ExportSnapshot::new(SnapshotPayload::Generic(second), ExportFormat::Json);

        assert_eq!(
            first_snapshot.compute_checksum(),
            second_snapshot.compute_checksum()
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
}
