// Helper program to generate golden snapshot binary vector
// This is a one-time utility to create the golden_snapshot.bin.b64 file.

use data_migration::{
    export_to_binary, ExportSnapshot, SavingsGoalsExport, SavingsGoalExport,
    SnapshotPayload, ExportFormat,
};

fn main() {
    // Create a representative savings goals snapshot (matching sample_savings_payload in tests)
    let goals_export = SavingsGoalsExport {
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
    };

    let snapshot = ExportSnapshot::new(
        SnapshotPayload::SavingsGoals(goals_export),
        ExportFormat::Binary,
    );

    // Export to binary
    let bytes = export_to_binary(&snapshot).expect("Failed to export to binary");

    // Encode as base64
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    // Print for copy-paste
    println!("Golden snapshot base64:");
    println!("{}", b64);
    println!("\n// To use: save to data_migration/tests/golden_snapshot.bin.b64");
}
