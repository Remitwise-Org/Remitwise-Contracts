use data_migration::*;
use base64::engine::general_purpose;
use base64::Engine;

fn main() {
    let snapshot = build_savings_snapshot(
        data_migration::SavingsGoalsExport {
            next_id: 2,
            goals: vec![data_migration::SavingsGoalExport {
                id: 1,
                owner: "GOWNER".into(),
                name: "Emergency Fund".into(),
                target_amount: 5_000,
                current_amount: 1_000,
                target_date: 2_000_000_000,
                locked: false,
            }],
        },
        data_migration::ExportFormat::Binary,
    );

    let bytes = export_to_binary(&snapshot).expect("serialize");
    println!("{}", general_purpose::STANDARD.encode(&bytes));
}
