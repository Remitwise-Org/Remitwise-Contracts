#!/usr/bin/env bash
set -e

git add Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in workspace root to fix version resolution"

git add bill_payments/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in bill_payments"

git add emergency_killswitch/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in emergency_killswitch"

git add family_wallet/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in family_wallet"

git add insurance/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in insurance"

git add integration_tests/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in integration_tests"

git add orchestrator/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in orchestrator"

git add remittance_split/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in remittance_split"

git add remitwise-common/Cargo.toml
git commit -m "chore: align remitwise-common soroban-sdk to =21.7.7 to fix mixed-version conflict"

git add reporting/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in reporting"

git add savings_goals/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in savings_goals"

git add scenarios/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in scenarios"

git add testutils/Cargo.toml
git commit -m "chore: pin soroban-sdk to =21.7.7 in testutils"

git add reporting/src/lib.rs
git commit -m "feat: add get_trend_analysis_multi for windowed trend analysis; fix duplicate PolicyPage"

git add reporting/src/tests.rs
git commit -m "test: add 21 deterministic trend analysis tests covering sparse, dense, and boundary windows"

git add reporting/README.md
git commit -m "docs: add reporting contract README with API reference and trend analysis notes"

git add reporting/test_snapshots/tests/test_archive_empty_when_no_old_reports.1.json \
        reporting/test_snapshots/tests/test_archive_old_reports.1.json \
        reporting/test_snapshots/tests/test_archive_ttl_extended_on_archive_reports.1.json \
        reporting/test_snapshots/tests/test_archive_unauthorized.1.json
git commit -m "test: update snapshots for archive lifecycle tests"

git add reporting/test_snapshots/tests/test_calculate_health_score.1.json \
        reporting/test_snapshots/tests/test_health_score_no_goals.1.json
git commit -m "test: update snapshots for health score tests"

git add reporting/test_snapshots/tests/test_cleanup_old_reports.1.json \
        reporting/test_snapshots/tests/test_cleanup_unauthorized.1.json
git commit -m "test: update snapshots for cleanup tests"

git add reporting/test_snapshots/tests/test_configure_addresses_unauthorized.1.json
git commit -m "test: update snapshot for configure_addresses_unauthorized"

git add reporting/test_snapshots/tests/test_get_bill_compliance_report.1.json \
        reporting/test_snapshots/tests/test_get_financial_health_report.1.json \
        reporting/test_snapshots/tests/test_get_insurance_report.1.json \
        reporting/test_snapshots/tests/test_get_remittance_summary.1.json \
        reporting/test_snapshots/tests/test_get_savings_report.1.json
git commit -m "test: update snapshots for report generation tests"

git add reporting/test_snapshots/tests/test_get_trend_analysis.1.json \
        reporting/test_snapshots/tests/test_get_trend_analysis_decrease.1.json
git commit -m "test: update snapshots for trend analysis tests"

git add reporting/test_snapshots/tests/test_init_twice_fails.1.json
git commit -m "test: update snapshot for init_twice_fails"

git add reporting/test_snapshots/tests/test_instance_ttl_refreshed_on_store_report.1.json \
        reporting/test_snapshots/tests/test_report_data_persists_across_ledger_advancements.1.json
git commit -m "test: update snapshots for TTL and data persistence tests"

git add reporting/test_snapshots/tests/test_retrieve_nonexistent_report.1.json \
        reporting/test_snapshots/tests/test_store_and_retrieve_report.1.json \
        reporting/test_snapshots/tests/test_storage_stats.1.json
git commit -m "test: update snapshots for store, retrieve, and storage stats tests"

echo ""
echo "All commits done. Run: git log --oneline to review."
