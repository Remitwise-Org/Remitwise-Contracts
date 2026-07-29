#!/usr/bin/env bash
# Generate golden snapshot for binary format testing
cd /workspaces/Remitwise-Contracts

# Run a test that will output the golden vector
# We'll temporarily add a test that prints the golden snapshot

cat > /tmp/gen_golden.rs << 'EOF'
#[cfg(test)]
mod gen_golden {
    use data_migration::*;

    #[test]
    #[ignore]
    fn gen_golden_snapshot() {
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

        let bytes = export_to_binary(&snapshot).expect("export failed");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

        println!("Golden snapshot base64 (copy to data_migration/tests/golden_snapshot.bin.b64):");
        println!("{}", b64);
        println!("\nChecksum: {}", snapshot.header.checksum);
    }
}
EOF

echo "Generated helper: /tmp/gen_golden.rs"
echo "Note: The helper is for reference. We'll generate the golden snapshot directly."
