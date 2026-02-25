//! Data migration, import/export utilities for Remitwise contracts.
//!
//! Supports multiple formats (JSON, binary, CSV), checksum validation,
//! version compatibility checks, and data integrity verification.

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Current schema version for migration compatibility.
pub const SCHEMA_VERSION: u32 = 1;

/// Minimum supported schema version for import.
pub const MIN_SUPPORTED_VERSION: u32 = 1;

/// Versioned migration event payload meant for indexing and historical tracking.
///
/// # Indexer Migration Guidance
/// - **v1**: Indexers should match on `MigrationEvent::V1`. This is the fundamental schema containing baseline metadata (contract, type, version, timestamp).
/// - **v2+**: Future schemas will add new variants (e.g., `MigrationEvent::V2`) potentially mapping to new data structures.
/// Indexers must be prepared to handle unknown variants gracefully (e.g., by logging a warning/alert) rather than crashing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MigrationEvent {
    V1(MigrationEventV1),
    // V2(MigrationEventV2), // Add in the future when schema changes and update indexers
}

/// Base migration event containing metadata about the migration operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationEventV1 {
    pub contract_id: String,
    pub migration_type: String, // e.g., "export", "import", "upgrade"
    pub version: u32,
    pub timestamp_ms: u64,
}

/// Export format for snapshot data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Human-readable JSON.
    Json,
    /// Compact binary (bincode).
    Binary,
    /// CSV for spreadsheet compatibility (tabular exports).
    Csv,
    /// Opaque encrypted payload (caller handles encryption/decryption).
    Encrypted,
}

/// Snapshot header with version and checksum for integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    pub version: u32,
    pub checksum: String,
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

/// Exportable remittance split config (mirrors contract SplitConfig).
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
    /// Compute SHA256 checksum of the payload (canonical JSON).
    pub fn compute_checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&self.payload).expect("payload must be serializable"));
        hex::encode(hasher.finalize().as_ref())
    }

    /// Verify stored checksum matches payload.
    pub fn verify_checksum(&self) -> bool {
        self.header.checksum == self.compute_checksum()
    }

    /// Check if snapshot version is supported for import.
    pub fn is_version_compatible(&self) -> bool {
        self.header.version >= MIN_SUPPORTED_VERSION && self.header.version <= SCHEMA_VERSION
    }

    /// Validate snapshot for import: version and checksum.
    pub fn validate_for_import(&self) -> Result<(), MigrationError> {
        if !self.is_version_compatible() {
            return Err(MigrationError::IncompatibleVersion {
                found: self.header.version,
                min: MIN_SUPPORTED_VERSION,
                max: SCHEMA_VERSION,
            });
        }
        if !self.verify_checksum() {
            return Err(MigrationError::ChecksumMismatch);
        }
        Ok(())
    }

    /// Build a new snapshot with correct version and checksum.
    pub fn new(payload: SnapshotPayload, format: ExportFormat) -> Self {
        let mut snapshot = Self {
            header: SnapshotHeader {
                version: SCHEMA_VERSION,
                checksum: String::new(),
                format: format_label(format),
                created_at_ms: None,
            },
            payload,
        };
        snapshot.header.checksum = snapshot.compute_checksum();
        snapshot
    }
}

fn format_label(f: ExportFormat) -> String {
    match f {
        ExportFormat::Json => "json".into(),
        ExportFormat::Binary => "binary".into(),
        ExportFormat::Csv => "csv".into(),
        ExportFormat::Encrypted => "encrypted".into(),
    }
}

/// Migration/import errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationError {
    IncompatibleVersion { found: u32, min: u32, max: u32 },
    ChecksumMismatch,
    InvalidFormat(String),
    ValidationFailed(String),
    DeserializeError(String),
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
            MigrationError::ChecksumMismatch => write!(f, "checksum mismatch"),
            MigrationError::InvalidFormat(s) => write!(f, "invalid format: {}", s),
            MigrationError::ValidationFailed(s) => write!(f, "validation failed: {}", s),
            MigrationError::DeserializeError(s) => write!(f, "deserialize error: {}", s),
        }
    }
}

impl std::error::Error for MigrationError {}

/// Export snapshot to JSON bytes.
pub fn export_to_json(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError> {
    serde_json::to_vec_pretty(snapshot).map_err(|e| MigrationError::DeserializeError(e.to_string()))
}

/// Export snapshot to binary bytes (bincode).
pub fn export_to_binary(snapshot: &ExportSnapshot) -> Result<Vec<u8>, MigrationError> {
    bincode::serialize(snapshot).map_err(|e| MigrationError::DeserializeError(e.to_string()))
}

/// Export to CSV (for tabular payloads only; e.g. goals list).
pub fn export_to_csv(payload: &SavingsGoalsExport) -> Result<Vec<u8>, MigrationError> {
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
    for g in &payload.goals {
        wtr.write_record(&[
            g.id.to_string(),
            g.owner.clone(),
            g.name.clone(),
            g.target_amount.to_string(),
            g.current_amount.to_string(),
            g.target_date.to_string(),
            g.locked.to_string(),
        ])
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;
    }
    wtr.flush()
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))?;
    wtr.into_inner()
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))
}

/// Encrypted format: store base64-encoded payload (caller encrypts before passing).
pub fn export_to_encrypted_payload(plain_bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(plain_bytes)
}

/// Decode encrypted payload from base64 (caller decrypts after).
pub fn import_from_encrypted_payload(encoded: &str) -> Result<Vec<u8>, MigrationError> {
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| MigrationError::InvalidFormat(e.to_string()))
}

/// Import snapshot from JSON bytes with validation.
pub fn import_from_json(bytes: &[u8]) -> Result<ExportSnapshot, MigrationError> {
    let snapshot: ExportSnapshot = serde_json::from_slice(bytes)
        .map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    snapshot.validate_for_import()?;
    Ok(snapshot)
}

/// Import snapshot from binary bytes with validation.
pub fn import_from_binary(bytes: &[u8]) -> Result<ExportSnapshot, MigrationError> {
    let snapshot: ExportSnapshot =
        bincode::deserialize(bytes).map_err(|e| MigrationError::DeserializeError(e.to_string()))?;
    snapshot.validate_for_import()?;
    Ok(snapshot)
}

/// Import goals from CSV into SavingsGoalsExport (no header checksum; use for merge/import).
pub fn import_goals_from_csv(bytes: &[u8]) -> Result<Vec<SavingsGoalExport>, MigrationError> {
    let mut rdr = csv::Reader::from_reader(bytes);
    let mut goals = Vec::new();
    for result in rdr.deserialize() {
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

/// Rollback metadata (for migration scripts to record last good state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackMetadata {
    pub previous_version: u32,
    pub previous_checksum: String,
    pub timestamp_ms: u64,
}

// Re-export hex for checksum display if needed; use hex crate for encoding in compute_checksum.
mod hex {
    const HEX: &[u8] = b"0123456789abcdef";
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            s.push(HEX[(b >> 4) as usize] as char);
            s.push(HEX[(b & 0xf) as usize] as char);
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_checksum_roundtrip() {
        let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
            owner: "GABC".into(),
            spending_percent: 50,
            savings_percent: 30,
            bills_percent: 15,
            insurance_percent: 5,
        });
        let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
        assert!(snapshot.verify_checksum());
        assert!(snapshot.is_version_compatible());
        assert!(snapshot.validate_for_import().is_ok());
    }

    #[test]
    fn export_import_json() {
        let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
            owner: "GXYZ".into(),
            spending_percent: 40,
            savings_percent: 40,
            bills_percent: 10,
            insurance_percent: 10,
        });
        let snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
        let bytes = export_to_json(&snapshot).unwrap();
        let loaded = import_from_json(&bytes).unwrap();
        assert_eq!(loaded.header.version, SCHEMA_VERSION);
        assert!(loaded.verify_checksum());
    }

    #[test]
    fn export_import_binary() {
        let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
            owner: "GBIN".into(),
            spending_percent: 25,
            savings_percent: 25,
            bills_percent: 25,
            insurance_percent: 25,
        });
        let snapshot = ExportSnapshot::new(payload, ExportFormat::Binary);
        let bytes = export_to_binary(&snapshot).unwrap();
        let loaded = import_from_binary(&bytes).unwrap();
        assert!(loaded.verify_checksum());
    }

    #[test]
    fn checksum_mismatch_fails_import() {
        let payload = SnapshotPayload::RemittanceSplit(RemittanceSplitExport {
            owner: "GX".into(),
            spending_percent: 100,
            savings_percent: 0,
            bills_percent: 0,
            insurance_percent: 0,
        });
        let mut snapshot = ExportSnapshot::new(payload, ExportFormat::Json);
        snapshot.header.checksum = "wrong".into();
        assert!(!snapshot.verify_checksum());
        assert!(snapshot.validate_for_import().is_err());
    }

    #[test]
    fn version_compatibility() {
        assert!(check_version_compatibility(1).is_ok());
        assert!(check_version_compatibility(SCHEMA_VERSION).is_ok());
        assert!(check_version_compatibility(0).is_err());
        assert!(check_version_compatibility(SCHEMA_VERSION + 1).is_err());
    }

    #[test]
    fn csv_export_import_goals() {
        let export = SavingsGoalsExport {
            next_id: 2,
            goals: vec![SavingsGoalExport {
                id: 1,
                owner: "G1".into(),
                name: "Emergency".into(),
                target_amount: 1000,
                current_amount: 500,
                target_date: 2000000000,
                locked: true,
            }],
        };
        let csv_bytes = export_to_csv(&export).unwrap();
        let goals = import_goals_from_csv(&csv_bytes).unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].name, "Emergency");
        assert_eq!(goals[0].target_amount, 1000);
    }

    #[test]
    fn migration_event_serialization() {
        let event = MigrationEvent::V1(MigrationEventV1 {
            contract_id: "CABCD".into(),
            migration_type: "export".into(),
            version: SCHEMA_VERSION,
            timestamp_ms: 123456789,
        });

        // Ensure we can serialize cleanly for indexers.
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""V1":{"#));
        assert!(json.contains(r#""contract_id":"CABCD""#));
        assert!(json.contains(r#""version":1"#));

        let loaded: MigrationEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, loaded);

        if let MigrationEvent::V1(v1) = loaded {
            assert_eq!(v1.version, SCHEMA_VERSION);
        } else {
            panic!("Expected V1 event");
        }
    }
}
