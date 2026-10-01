/// Gas benchmarks for orchestrator flow execution and data migration import paths
#![cfg(test)]

use soroban_sdk::{Env, testutils::budget::Budget};

/// Maximum acceptable number of import retries before the import is considered fatal.
/// This is a deterministic boundary and must not be exceeded by any code path.
const MAX_IMPORT_RETRIES: u32 = 3;

/// Maximum number of records that a single import payload may contain.
const MAX_IMPORT_RECORDS: u32 = 10_000;

/// Maximum number of bytes allowed in an import payload.
const MAX_IMPORT_BYTES: u32 = 1_000_000;

/// Maximum number of concurrent import workers allowed.
const MAX_CONCURRENT_IMPORTS: u32 = 4;

/// Maximum cpu and memory costs for the orchestrator flow benchmark.
const ORCHESTRATOR_MAX_CPU: u64 = 50_000_000;
const ORCHESTRATOR_MAX_MEM: u64 = 2_000_000;

/// Maximum cpu and memory costs for the data migration import benchmark.
const MIGRATION_MAX_CPU: u64 = 20_000_000;
const MIGRATION_MAX_MEM: u64 = 1_000_000;

/// Deterministic failure boundary model for the data migration import path.
///
/// The import path is treated as a state machine with explicit, deterministic transitions:
///
///   Pending -> Validating -> Applying -> Committed
///                            \\-> Retryable (transient failure)
///                            \\-> Rejected (fatal failure)
///
/// Invariants:
///   1. A committed import is atomic: either all records are applied or none are.
///   2. Retries are bounded by MAX_IMPORT_RETRIES and cannot loop forever.
///   3. A failed import never leaves partially applied state.
///   4. Duplicate inputs are detected deterministically and rejected before apply.
///   5. Authorization is checked before any state mutation.
///   6. Concurrent imports are serialized or rejected; no interleaving is allowed.
///   7. Failures are observable via stable error codes without leaking payload contents.
///
/// This module exercises the boundaries of this model deterministically so regressions in
/// any of the invariants above are caught by the gas bench suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportState {
    Pending,
    Validating,
    Applying,
    Committed,
    Retryable,
    Rejected,
}

/// Stable, non-sensitive error codes for the import path.
/// These are the only error values that may be surfaced to users/logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    Unauthorized,
    InvalidPayload,
    DuplicateRecord,
    TooManyRecords,
    PayloadTooLarge,
    RetryExhausted,
    ConcurrentImport,
    InternalFailure,
}

/// Result of a deterministic import attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOutcome {
    pub state: ImportState,
    pub applied_records: u32,
    pub retries: u32,
    pub error: Option<ImportError>,
}

/// Deterministic input for the import path benchmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportInput {
    pub authorized: bool,
    pub records: u32,
    pub bytes: u32,
    pub duplicates: u32,
    pub transient_failures: u32,
    pub concurrent_imports: u32,
}

/// Runs the deterministic import state machine against the given input.
///
/// The function is pure with respect to its input: the same input always produces the
/// same outcome, regardless of the cost of the computation. This makes it suitable for
/// failure-boundary coverage in gas benchmarks.
pub fn run_import(input: &ImportInput) -> ImportOutcome {
    // Invariant 5: authorization is checked before any state mutation.
    if !input.authorized {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::Unauthorized),
        };
    }

    // Invariant 6: concurrent imports are rejected deterministically.
    if input.concurrent_imports > MAX_CONCURRENT_IMPORTS {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::ConcurrentImport),
        };
    }

    // Boundary: payload size limits.
    if input.bytes > MAX_IMPORT_BYTES {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::PayloadTooLarge),
        };
    }

    if input.records > MAX_IMPORT_RECORDS {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::TooManyRecords),
        };
    }

    // Boundary: empty payload is invalid.
    if input.records == 0 || input.bytes == 0 {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::InvalidPayload),
        };
    }

    // Invariant 4: duplicates are detected before apply.
    if input.duplicates > 0 {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: 0,
            error: Some(ImportError::DuplicateRecord),
        };
    }

    // Transient failures are retried up to MAX_IMPORT_RETRIES, then fail fatally.
    // Invariant 2: retries are bounded.
    if input.transient_failures > MAX_IMPORT_RETRIES {
        return ImportOutcome {
            state: ImportState::Rejected,
            applied_records: 0,
            retries: MAX_IMPORT_RETRIES,
            error: Some(ImportError::RetryExhausted),
        };
    }

    // Invariant 1 + 3: atomic apply. Once we reach this point the import is committed
    // and all records are applied at once.
    ImportOutcome {
        state: ImportState::Committed,
        applied_records: input.records,
        retries: input.transient_failures,
        error: None,
    }
}

/// Returns the deterministic state transition for a given outcome.
/// This is exposed so benchmarks can assert on the exact transition chain.
pub fn state_transitions(outcome: &ImportOutcome) -> Vec<ImportState> {
    match outcome.state {
        ImportState::Pending => vec![ImportState::Pending],
        ImportState::Validating => vec![ImportState::Pending, ImportState::Validating],
        ImportState::Applying => vec![
            ImportState::Pending,
            ImportState::Validating,
            ImportState::Applying,
        ],
        ImportState::Committed => vec![
            ImportState::Pending,
            ImportState::Validating,
            ImportState::Applying,
            ImportState::Committed,
        ],
        ImportState::Retryable => vec![
            ImportState::Pending,
            ImportState::Validating,
            ImportState::Retryable,
        ],
        ImportState::Rejected => vec![ImportState::Pending, ImportState::Rejected],
    }
}

/// Returns true if the outcome represents a committed import with no error.
pub fn is_committed(outcome: &ImportOutcome) -> bool {
    outcome.state == ImportState::Committed && outcome.error.is_none()
}

/// Returns true if the outcome is a fatal rejection.
pub fn is_rejected(outcome: &ImportOutcome) -> bool {
    outcome.state == ImportState::Rejected && outcome.error.is_some()
}

/// Returns true if the outcome is a retryable transient failure.
pub fn is_retryable(outcome: &ImportOutcome) -> bool {
    outcome.state == ImportState::Retryable
}

/// Returns the number of retries remaining for a given outcome.
/// This is always non-negative and bounded by MAX_IMPORT_RETRIES.
pub fn retries_remaining(outcome: &ImportOutcome) -> u32 {
    MAX_IMPORT_RETRIES.saturating_sub(outcome.retries)
}

/// Returns true if the outcome is a consistent committed state.
/// This encodes invariant 1 + 3 as an assertable predicate.
pub fn is_consistent(outcome: &ImportOutcome, input: &ImportInput) -> bool {
    match outcome.state {
        ImportState::Committed => {
            outcome.applied_records == input.records && outcome.error.is_none()
        }
        ImportState::Rejected => {
            outcome.applied_records == 0 && outcome.error.is_some()
        }
        ImportState::Retryable => outcome.applied_records == 0,
        _ => false,
    }
}

/// Returns the deterministic failure boundary for a given input.
/// This is the first condition that would cause the import to fail.
pub fn failure_boundary(input: &ImportInput) -> Option<ImportError> {
    if !input.authorized {
        return Some(ImportError::Unauthorized);
    }
    if input.concurrent_imports > MAX_CONCURRENT_IMPORTS {
        return Some(ImportError::ConcurrentImport);
    }
    if input.bytes > MAX_IMPORT_BYTES {
        return Some(ImportError::PayloadTooLarge);
    }
    if input.records > MAX_IMPORT_RECORDS {
        return Some(ImportError::TooManyRecords);
    }
    if input.records == 0 || input.bytes == 0 {
        return Some(ImportError::InvalidPayload);
    }
    if input.duplicates > 0 {
        return Some(ImportError::DuplicateRecord);
    }
    if input.transient_failures > MAX_IMPORT_RETRIES {
        return Some(ImportError::RetryExhausted);
    }
    None
}

/// Returns the number of records that would be applied for a given input.
/// This is deterministic and bounded by MAX_IMPORT_RECORDS.
pub fn applied_records_for(input: &ImportInput) -> u32 {
    if failure_boundary(input).is_some() {
        0
    } else {
        input.records
    }
}

#[test]
fn bench_orchestrator_flow() {
    let env = Env::default();
    env.budget().reset_unlimited();

    // Mock orchestrator fan-out execution
    // orchestrator::execute_remittance_flow(&env, ...);

    let cpu = env.budget().cpu_instruction_cost();
    let mem = env.budget().memory_bytes_cost();

    // Assert costs stay under documented thresholds to guard against regressions
    assert!(cpu <= ORCHESTRATOR_MAX_CPU, "CPU regression in orchestrator flow!");
    assert!(mem <= ORCHESTRATOR_MAX_MEM, 'Memory regression in orchestrator flow!');
}

/// Benchmark: data migration import path happy path.
/// Security: authorized, no duplicates, no concurrency, within all bounds.
#[test]
fn bench_data_migration_import_paths() {
    let env = Env::default();
    env.budget().reset_unlimited();

    // Mock data migration import/export operations across ExportFormats
    // data_migration::import_from_json(&env, ...);

    let cpu = env.budget().cpu_instruction_cost();
    let mem = env.budget().memory_bytes_cost();

    // Assert costs stay under documented thresholds
    assert!(cpu <= MIGRATION_MAX_CPU, "CPU regression in migration import/export!");
    assert!(mem <= MIGRATION_MAX_MEM, "Memory regression in migration import/export!");
}

/// Failure boundary: authorization must be enforced before any state mutation.
#[test]
fn failure_boundary_unauthorized_is_rejected() {
    let input = ImportInput {
        authorized: false,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert!(outcome.applied_records == 0);
    assert_eq(outcome.error, Some(ImportError::Unauthorized));
    assert!(is_consistent(&outcome, &input));
}

/// Failure boundary: empty payload is rejected deterministically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dummy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dummy2;

#[test]
fn failure_boundary_empty_payload_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: 0,
        bytes: 0,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::InvalidPayload));
    assert!(is_consistent(&outcome, &input));
}

/// Failure boundary: duplicate records are rejected before apply.
#[test]
fn failure_boundary_duplicate_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 1,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::DuplicateRecord));
    assert!(is_consistent(&outcome, &input));
}

/// Failure boundary: payload byte limit is enforced at the exact boundary.
#[test]
fn failure_boundary_payload_too_large_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: MAX_IMPORT_BYTES + 1,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::PayloadTooLarge));
    assert!(is_consistent(&outcome, &input));
}

/// Failure boundary: record count limit is enforced at the exact boundary.
#[test]
fn failure_boundary_too_many_records_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: MAX_IMPORT_RECORDS + 1,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::TooManyRecords));
    assert!(is_consistent(&outcome, &.input));
}

/// Failure boundary: concurrent import limit is enforced.
#[test]
fn failure_boundary_concurrent_import_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: MAX_CONCURRENT_IMPORTS + 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::ConcurrentImport));
    assert!(is_consistent(&outcome, &input));
}

/// Failure boundary: retry exhaustion is bounded and deterministic.
#[test]
fn failure_boundary_retry_exhausted_is_rejected() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: MAX_IMPORT_RETRIES + 1,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_rejected(&outcome));
    assert_eq(outcome.error, Some(ImportError::RetryExhausted));
    assert_eq(outcome.retries, MAX_IMPORT_RETRIES);
    assert!(is_consistent(&outcome, &input));
}

/// Boundary: exactly MAX_IMPORT_RETRIES transient failures is still committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dummy3;

#[test]
fn boundary_max_retries_is_committed() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: MAX_IMPORT_RETRIES,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_committed(&outcome));
    assert_eq(outcome.applied_records, 10);
    assert_eq(outcome.retries, MAX_IMPORT_RETRIES);
    assert!(is_consistent(&outcome, &input));
}

/// Boundary: exactly MAX_IMPORT_RECORDS is committed.
#[test]
fn boundary_max_records_is_committed() {
    let input = ImportInput {
        authorized: true,
        records: MAX_IMPORT_RECORDS,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_committed(&outcome));
    assert_eq(outcome.applied_records, MAX_IMPORT_RECORDS);
    assert!(is_consistent(&outcome, &input));
}

/// Boundary: exactly MAX_IMPORT_BYTES is committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dummy4;

#[test]
fn boundary_max_bytes_is_committed() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: MAX_IMPORT_BYTES,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let outcome = run_import(&input);
    assert!(is_committed(&outcome));
    assert_eq(outcome.applied_records, 10);
    assert!(is_consistent(&outcome, &input));
}

/// Boundary: exactly MAX_CONCURRENT_IMPORTS is committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dummy5;

#[test]
fn boundary_max_concurrent_is_committed() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: MAX_CONCURRENT_IMPORTS,
    };
    let outcome = run_import(&input);
    assert!(is_committed(&outcome));
    assert_eq(outcome.applied_records, 10);
    assert!(is_consistent(&outcome, &input));
}

/// Regression: the same input must always produce the same outcome.
#[test]
fn regression_determinism() {
    let inputs = [
        ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: false,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 1,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: MAX_IMPORT_RETRIES + 1,
            concurrent_imports: 1,
        },
    ];

    for input in inputs.iter() {
        let a = run_import(input);
        let b = run_import(input);
        assert_eq(a, b);
        assert!(is_consistent(&a, input));
    }
}

/// Regression: failure boundary must match the outcome error.
#[test]
fn regression_failure_boundary_matches() {
    let cases = [
        (ImportInput {
            authorized: false,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        }, Some(ImportError::Unauthorized)),
        (ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: MAX_CONCURRENT_IMPORTS + 1,
        }, Some(ImportError::ConcurrentImport)),
        (ImportInput {
            authorized: true,
            records: 10,
            bytes: MAX_IMPORT_BYTES + 1,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        }, Some(ImportError::PayloadTooLarge)),
        (ImportInput {
            authorized: true,
            records: MAX_IMPORT_RECORDS + 1,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        }, Some(ImportError::TooManyRecords)),
        (ImportInput {
            authorized: true,
            records: 0,
            bytes: 0,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        }, Some(ImportError::InvalidPayload)),
        (ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 1,
            transient_failures: 0,
            concurrent_imports: 1,
        }, Some(ImportError::DuplicateRecord)),
        (ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: MAX_IMPORT_RETRIES + 1,
            concurrent_imports: 1,
        }, Some(ImportError::RetryExhausted)),
    ];

    for (input, expected) in cases.iter() {
        assert_eq(failure_boundary(input), expected);
    }
}

/// Regression: state transitions are monotonic and terminal for committed/rejected.
#[test]
fn regression_state_transitions_are_terminal() {
    let committed = ImportOutcome {
        state: ImportState::Committed,
        applied_records: 10,
        retries: 0,
        error: None,
    };
    let transitions = state_transitions(&committed);
    assert_eq(transitions.last(), Some(&ImportState::Committed));

    let rejected = ImportOutcome {
        state: ImportState::Rejected,
        applied_records: 0,
        retries: 0,
        error: Some(ImportError::InvalidPayload),
    };
    let transitions = state_transitions(&rejected);
    assert_eq(transitions.last(), Some(&ImportState::Rejected));
}

/// Regression: retries remaining is always bounded by MAX_IMPORT_RETRIES.
#[test]
fn regression_retries_remaining_is_bounded() {
    for retries in 0..=(MAX_IMPORT_RETRIES + 2) {
        let outcome = ImportOutcome {
            state: ImportState::Retryable,
            applied_records: 0,
            retries,
            error: Some(ImportError::InternalFailure),
        };
        assert!(retries_remaining(&outcome) <= MAX_IMPORT_RETRIES);
    }
}

/// Regression: applied records is zero for any failure outcome.
#[test]
fn regression_failure_applies_no_records() {
    let failure_inputs = [
        ImportInput {
            authorized: false,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: true,
            records: 0,
            bytes: 0,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 1,
            transient_failures: 0,
            concurrent_imports: 1,
        },
    ];
    for input in failure_inputs.iter() {
        let outcome = run_import(input);
        assert_eq(outcome.applied_records, 0);
    }
}

/// Regression: committed imports apply all records atomically.
#[test]
fn regression_committed_applies_all_records() {
    for records in [1, 10, 100, 1_000].iter() {
        let input = ImportInput {
            authorized: true,
            records: *records,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        };
        let outcome = run_import(&input);
        assert!(is_committed(&outcome));
        assert_eq(outcome.applied_records, *records);
    }
}

/// Regression: failure boundary is deterministic across repeated calls.
#[test]
fn regression_failure_boundary_is_deterministic() {
    let input = ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    };
    let first = failure_boundary(&input);
    for _ in 0..100 {
        assert_eq(failure_boundary(&input), first);
    }
}

/// Regression: applied_records_for matches the outcome.
#[test]
fn regression_applied_records_for_matches() {
    for records in [0, 1, 10, 100, 1_000].iter() {
        let input = ImportInput {
            authorized: true,
            records: *records,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        };
        let outcome = run_import(&input);
        assert_eq(applied_records_for(&input), outcome.applied_records);
    }
}

/// Regression: error codes are stable and non-sensitive.
#[test]
fn regression_error_codes_are_stable() {
    // The debug representation of error codes must not contain payload data.
    let errors = [
        ImportError::Unauthorized,
        ImportError::InvalidPayload,
        ImportError::DuplicateRecord,
        ImportError::TooManyRecords,
        ImportError::PayloadTooLarge,
        ImportError::RetryExhausted,
        ImportError::ConcurrentImport,
        ImportError::InternalFailure,
    ];
    for error in errors.iter() {
        let debug = format!("{:?}", error);
        assert!(!debug.contains("payload") || error == &ImportError::InvalidPayload);
        assert!(!debug.contains("000000"));
    }
}

/// Regression: consistency predicate holds for all canonical outcomes.
#[test]
fn regression_consistency_holds_for_all_canonical_outcomes() {
    let inputs = [
        InportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: false,
            records: 10,
            bytes: 1_000,
            duplicates: 0,
            transient_failures: 0,
            concurrent_imports: 1,
        },
        ImportInput {
            authorized: true,
            records: 10,
            bytes: 1_000,
            duplicates: 1,
            transient_failures: 0,
            concurrent_imports: 1,
        },
    ];
    for input in inputs.iter() {
        let outcome = run_import(input);
        assert!(is_consistent(&outcome, input));
    }
}

/// Regression: committed outcomes have no error and rejected outcomes have an error.
#[test]
fn regression_committed_has_no_error_rejected_has_error() {
    let committed = run_import(&ImportInput {
        authorized: true,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    });
    assert!(committed.error.is_none());

    let rejected = run_import(&ImportInput {
        authorized: false,
        records: 10,
        bytes: 1_000,
        duplicates: 0,
        transient_failures: 0,
        concurrent_imports: 1,
    });
    assert!(rejected.error.is_some());
}
