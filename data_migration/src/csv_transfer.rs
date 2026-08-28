//! CSV import/export for tabular payloads (savings goals).
//!
//! Split out of `lib.rs` (issue #1625) as a behavior-preserving move: every
//! item here is unchanged, just relocated. Re-exported from the crate root
//! so existing call sites (`data_migration::export_to_csv`, `::import_goals_from_csv`)
//! and `use super::*;` test imports keep working unmodified.

use super::*;

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

        if record.target_amount < 0 || record.current_amount < 0 {
            return Err(MigrationError::ValidationFailed(
                "negative amounts are not allowed".into(),
            ));
        }

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

