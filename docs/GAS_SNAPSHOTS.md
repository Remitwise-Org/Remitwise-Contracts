# Gas Snapshots

This document describes Remitwise's gas benchmarking workflow: how to run it, how to update baselines, and how to read the results. It's aimed at contributors submitting or reviewing PRs that touch contract logic.

For the full configuration reference — `baseline.json`/`thresholds.json` schema, writing new benchmark tests, troubleshooting variance — see [`benchmarks/README.md`](../benchmarks/README.md). This doc covers the workflow; that one covers the configuration.

## What a gas snapshot is

A gas snapshot is a JSON record of the CPU instruction count and memory (byte) cost of a single contract method under a specific test scenario — e.g. `remittance_split::create_remittance_schedule::single_recurring_schedule`. Snapshots are captured by dedicated `gas_bench.rs` test files per contract and compared against a checked-in baseline (`benchmarks/baseline.json`) to catch performance regressions before merge.

## How to run

Run every benchmarked contract and regenerate `gas_results.json`:

```bash
./scripts/run_gas_benchmarks.sh
```

This currently benchmarks `bill_payments`, `savings_goals`, `family_wallet`, and `remittance_split` (see the `CONTRACTS` array in the script). `insurance`'s gas_bench is disabled (`insurance/tests/gas_bench.rs.disabled`).

To run just one contract's benchmarks:

```bash
RUST_TEST_THREADS=1 cargo test -p remittance_split --test gas_bench -- --nocapture
```

`RUST_TEST_THREADS=1` keeps runs single-threaded for consistent measurements.

Each benchmark test prints one JSON line in the form:

```json
{"contract": "remittance_split", "method": "create_remittance_schedule", "scenario": "single_recurring_schedule", "cpu": 12345, "mem": 6789}
```

`run_gas_benchmarks.sh` scrapes these lines from `--nocapture` output and assembles them into `gas_results.json`.

`orchestrator`'s `execute_remittance_flow` and `data_migration`'s import/export paths are measured by a separate harness (`benchmarks/src/orchestrator_migration_benches.rs`) that checks against fixed CPU/memory budgets rather than the baseline/threshold comparison described below — 50M CPU / 2M Mem for orchestrator, 20M CPU / 1M Mem for data_migration.

## How to update the baseline

After an intentional performance change, regenerate results and update the baseline:

```bash
./scripts/run_gas_benchmarks.sh
./scripts/update_baseline.sh
```

`update_baseline.sh` backs up the existing `benchmarks/baseline.json` to `benchmarks/history/baseline_<timestamp>.json`, then prompts for confirmation before overwriting it with the new `gas_results.json`. Pass `--force` to skip the prompt. Commit the updated baseline with a message describing what changed and why:

```bash
git add benchmarks/baseline.json
git commit -m "Update gas baseline after <describe changes>"
```

## How to read the output

Compare current results against baseline directly:

```bash
./scripts/compare_gas_results.sh benchmarks/baseline.json gas_results.json
```

For each `contract:method:scenario` key, this prints the percentage change in CPU and memory versus baseline, and the threshold it's checked against. Thresholds are looked up from `benchmarks/thresholds.json` in this order: method-specific → contract-specific → default (10%). Passing a third argument overrides all thresholds with a single flat percentage.

Output markers to look for:

- `⚠️ CPU/MEMORY REGRESSION DETECTED` — the increase exceeded threshold; the script exits non-zero.
- `✨ Improvement detected` — cost dropped more than 5%.
- `BASELINE NOT SET (skipping)` — baseline entry is `0`, not yet measured.
- A trailing `New benchmarks (not in baseline)` section lists scenarios present in the current run but missing from baseline — expected right after adding a new benchmark, before its first baseline is committed.

## CI integration

The `gas-benchmarks` job in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs on every PR:

1. Runs `./scripts/run_gas_benchmarks.sh`.
2. Runs `./scripts/compare_gas_results.sh benchmarks/baseline.json gas_results.json` against the committed baseline — the job fails if a regression is detected.
3. Uploads `gas_results.json` as a build artifact (30-day retention).
4. Posts a PR comment with a results table and baseline comparison.

If a PR intentionally changes gas costs, update and commit the baseline locally (see above) as part of that PR so the CI comparison passes.

## See also

- [`benchmarks/README.md`](../benchmarks/README.md) — configuration reference, benchmark test template, troubleshooting.
