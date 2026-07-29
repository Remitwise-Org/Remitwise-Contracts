use data_migration::{
    export_to_binary, ExportSnapshot, SavingsGoalsExport, SavingsGoalExport,
    SnapshotPayload, ExportFormat,
};
use base64::Engine;

fn main() {
    // Create a representative savings goals snapshot matching sample_savings_payload in tests
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

    let bytes = export_to_binary(&snapshot).expect("Failed to export to binary");
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    println!("Golden snapshot base64:");
    println!("{}", b64);
    println!("\nChecksum: {}", snapshot.header.checksum);
    println!("Version: {}", snapshot.header.version);
    println!("Format: {}", snapshot.header.format);
}
