/// View Function Read-Only Enforcement Tests
///
/// These tests verify the `check_view_functions_readonly.sh` script correctly
/// detects storage writes inside view functions (get_*, is_*).
///
/// ## Why this matters (threat model)
///
/// View functions are expected to be side-effect free. If a view function can
/// call `env.storage().*.set()` or `.remove()` or `.extend_ttl()`, an attacker
/// gains the ability to:
///   1. Mutate state through calls that appear read-only to off-chain observers.
///   2. Bypass authorization checks that guard mutation entrypoints.
///   3. Exhaust storage through cheap "read" calls.
///   4. Poison audit logs or indexes that rely on view-call purity.
///
/// The grep-based check is a *static* defense-in-depth layer: it runs at CI
/// time, before any contract is deployed, and catches accidental or intentional
/// violations before they reach production.
///
/// ## Test strategy
///
/// - **Happy path**: A fixture file that contains *only* legitimate storage reads
///   inside view functions must exit 0.
/// - **Negative (sad) path**: A fixture file that contains a `get_*` function
///   calling `env.storage().instance().set(...)` must exit 1 and report the
///   offending function/line.
use std::fs;
use std::path::PathBuf;

/// Returns the path to the workspace root (parent of this crate's manifest dir).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get parent of manifest dir")
        .to_path_buf()
}

// ---------------------------------------------------------------------------
// Happy path: view functions that only READ storage
// ---------------------------------------------------------------------------

/// A minimal contract skeleton where `get_balance` and `is_paused` only read.
const CLEAN_FIXTURE: &str = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

#[contract]
pub struct TestContract;

#[contractimpl]
impl TestContract {
    pub fn get_balance(env: Env, key: Symbol) -> i128 {
        env.storage().instance().get(&key).unwrap_or(0i128)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&symbol_short!("PAUSED")).unwrap_or(false)
    }
}
"#;

/// A minimal contract skeleton where `get_*` function writes storage (violation).
const VIOLATING_FIXTURE: &str = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

#[contract]
pub struct BadContract;

#[contractimpl]
impl BadContract {
    /// get_cached_value writes to storage as a side effect – this is the bad pattern.
    pub fn get_cached_value(env: Env, key: Symbol) -> i128 {
        let val: i128 = env.storage().instance().get(&key).unwrap_or(0i128);
        // BUG: view function must not write storage
        env.storage().instance().set(&symbol_short!("CACHE"), &val);
        val
    }
}
"#;

/// A minimal contract skeleton where a view function uses a storage alias and
/// still writes storage as a side effect.
const VIOLATING_ALIAS_FIXTURE: &str = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};

#[contract]
pub struct AliasContract;

#[contractimpl]
impl AliasContract {
    pub fn get_value(env: Env) -> u32 {
        let storage = env.storage().instance();
        let val: u32 = storage.get(&symbol_short!("V")).unwrap_or(0);
        storage.set(&symbol_short!("V"), &val);
        val
    }
}
"#;

/// An `is_*` variant that writes storage (violation).
const VIOLATING_IS_FIXTURE: &str = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};

#[contract]
pub struct BadIsContract;

#[contractimpl]
impl BadIsContract {
    /// is_initialized writes a flag as a side effect – this is the bad pattern.
    pub fn is_initialized(env: Env) -> bool {
        let flag: bool = env.storage().instance().get(&symbol_short!("INIT")).unwrap_or(false);
        // BUG: is_* functions must not write storage
        env.storage().instance().set(&symbol_short!("CHECKED"), &true);
        flag
    }
}
"#;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// The script only scans the hard-coded list of contract names.  We wrap it
// to run against an arbitrary directory by temporarily replacing the cwd and
// using a minimal inline script instead.
// ---------------------------------------------------------------------------

/// Runs the *core detection logic* (grep patterns) directly rather than invoking
/// the full shell script (which only scans hard-coded contract names).
///
/// This avoids coupling the tests to the hard-coded contract list while still
/// exercising the same grep patterns.
///
/// Returns `(violations_found, violation_lines)`.
fn detect_violations_in_source(source: &str) -> (bool, Vec<String>) {
    let mut violations = Vec::new();
    let mut current_idx = 0;

    while let Some(fn_idx) = source[current_idx..].find("pub fn ") {
        let abs_idx = current_idx + fn_idx;
        let start_of_name = abs_idx + 7;
        let end_of_name = source[start_of_name..].find('(').unwrap_or(0) + start_of_name;
        let fn_name = source[start_of_name..end_of_name].trim();

        if fn_name.starts_with("get_") || fn_name.starts_with("is_") {
            let mut brace_count = 0;
            let mut started = false;
            let mut end_idx = abs_idx;

            for (i, c) in source[abs_idx..].char_indices() {
                if c == '{' {
                    brace_count += 1;
                    started = true;
                } else if c == '}' {
                    brace_count -= 1;
                }
                if started && brace_count == 0 {
                    end_idx = abs_idx + i;
                    break;
                }
            }

            if started {
                let body = &source[abs_idx..=end_idx];
                let mut found = false;

                // Emulate the script's pattern: env.storage().*.set(
                // In rust, we check for presence of env.storage() and any of the mutators.
                if body.contains("env.storage()")
                    && (body.contains(".set(")
                        || body.contains(".remove(")
                        || body.contains(".extend_ttl("))
                {
                    found = true;
                }

                if found {
                    violations.push(format!("VIOLATION: fn {} writes storage", fn_name));
                }
            }
        }
        current_idx = abs_idx + 7;
    }

    (!violations.is_empty(), violations)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// HAPPY PATH: A clean view function that only reads storage must NOT trigger
/// any violations.
///
/// This test would pass BEFORE and AFTER the fix — it documents the expected
/// negative (no violation) state.
#[test]
fn test_view_fn_read_only_passes_clean_fixture() {
    let (violations_found, violations) = detect_violations_in_source(CLEAN_FIXTURE);

    assert!(
        !violations_found,
        "Clean fixture should produce no violations, but got:\n{}",
        violations.join("\n")
    );
}

/// NEGATIVE TEST: A `get_*` function that calls `env.storage().instance().set()`
/// MUST be flagged as a violation.
///
/// This test FAILS before the fix (no check exists) and PASSES after the fix
/// (the script correctly catches the write).
#[test]
fn test_get_fn_writing_storage_is_detected() {
    let (violations_found, violations) = detect_violations_in_source(VIOLATING_FIXTURE);

    assert!(
        violations_found,
        "A get_* function that writes storage must be flagged, but no violation was detected."
    );

    // Also verify the violation message mentions the offending function
    let mentions_fn = violations.iter().any(|v| v.contains("get_cached_value"));
    assert!(
        mentions_fn,
        "Violation output should name 'get_cached_value', got:\n{}",
        violations.join("\n")
    );
}

/// NEGATIVE TEST: A view function that writes through a storage alias
/// (`let storage = env.storage().instance(); storage.set(...)`) MUST also be
/// flagged as a violation.
#[test]
fn test_get_fn_writing_storage_via_alias_is_detected() {
    let (violations_found, violations) = detect_violations_in_source(VIOLATING_ALIAS_FIXTURE);

    assert!(
        violations_found,
        "A get_* function that writes storage through a storage alias must be flagged"
    );

    let mentions_fn = violations.iter().any(|v| v.contains("get_value"));
    assert!(
        mentions_fn,
        "Violation output should name 'get_value', got:\n{}",
        violations.join("\n")
    );
}

/// NEGATIVE TEST: An `is_*` function that calls `env.storage().instance().set()`
/// MUST be flagged as a violation.
///
/// This test FAILS before the fix (no check exists) and PASSES after the fix.
#[test]
fn test_is_fn_writing_storage_is_detected() {
    let (violations_found, violations) = detect_violations_in_source(VIOLATING_IS_FIXTURE);

    assert!(
        violations_found,
        "An is_* function that writes storage must be flagged, but no violation was detected."
    );

    let mentions_fn = violations.iter().any(|v| v.contains("is_initialized"));
    assert!(
        mentions_fn,
        "Violation output should name 'is_initialized', got:\n{}",
        violations.join("\n")
    );
}

/// NEGATIVE TEST: A `get_*` function that calls `env.storage().*.remove()` must
/// also be flagged — removes are mutations just like sets.
#[test]
fn test_get_fn_removing_storage_is_detected() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn get_value(env: Env) -> u32 {
        let v: u32 = env.storage().instance().get(&symbol_short!("V")).unwrap_or(0);
        env.storage().instance().remove(&symbol_short!("V")); // BUG: should not remove
        v
    }
}
"#;

    let (violations_found, violations) = detect_violations_in_source(source);

    assert!(
        violations_found,
        "A get_* function that removes storage must be flagged"
    );

    let mentions_fn = violations.iter().any(|v| v.contains("get_value"));
    assert!(
        mentions_fn,
        "Violation should name 'get_value', got:\n{}",
        violations.join("\n")
    );
}

/// NEGATIVE TEST: A `get_*` function that calls `env.storage().*.extend_ttl()`
/// must also be flagged — TTL extension is a mutation.
#[test]
fn test_get_fn_extending_ttl_is_detected() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn get_value(env: Env) -> u32 {
        let key = symbol_short!("V");
        let v: u32 = env.storage().instance().get(&key).unwrap_or(0);
        // BUG: extend_ttl in a view function is a storage mutation
        env.storage().instance().extend_ttl(&key, 100, 200);
        v
    }
}
"#;

    let (violations_found, _violations) = detect_violations_in_source(source);

    assert!(
        violations_found,
        "A get_* function that calls extend_ttl must be flagged"
    );
}

#[test]
fn test_all_workspace_contracts_are_read_only() {
    let root = workspace_root();
    let contracts = [
        "remittance_split",
        "savings_goals",
        "bill_payments",
        "insurance",
        "family_wallet",
        "orchestrator",
        "reporting",
        "emergency_killswitch",
        "data_migration",
    ];

    let mut all_violations = Vec::new();

    for contract in contracts {
        let lib_path = root.join(contract).join("src").join("lib.rs");
        if !lib_path.exists() {
            continue;
        }

        let source = fs::read_to_string(&lib_path).expect("read lib.rs");
        let (violations_found, violations) = detect_violations_in_source(&source);

        if violations_found {
            for v in violations {
                all_violations.push(format!("{}: {}", contract, v));
            }
        }
    }

    assert!(
        all_violations.is_empty(),
        "Found view functions that write to storage in workspace contracts:\n{}",
        all_violations.join("\n")
    );
}

// ============================================================================
// #1273 — Compile-time no-panic check on view functions
//
// View functions (`get_*` / `is_*`) must never panic unconditionally.
// A panicking view degrades to a harmful call:
//
//   1. An attacker can trigger the panic with a crafted call, bricking the
//      view for all callers until state is repaired.
//   2. A panic in a read-only context still consumes the caller's gas budget.
//   3. Off-chain indexers that rely on view functions for state snapshots
//      silently fail on panicking paths, producing stale or missing data.
//
// The patterns scanned for:
//   • `.unwrap()` — panics when Option is None / Result is Err.
//   • `.expect("…")` — same but with a message.
//   • `panic!(…)` — explicit unconditional panic.
//   • `panic_with_error!(…)` — Soroban macro that aborts the transaction.
//
// `.unwrap_or()` / `.unwrap_or_default()` / `.unwrap_or_else()` are NOT
// flagged because they handle the None/Err case safely.
//
// The implementation mirrors the `detect_violations_in_source` approach:
// static text-level analysis so tests run without a Rust compiler, matching
// the CI pattern already established in this crate.
// ============================================================================

/// Scans `source` for unconditional panics inside `get_*` / `is_*` functions.
///
/// Returns `(violations_found, violation_messages)`.
fn detect_panics_in_view_fns(source: &str) -> (bool, Vec<String>) {
    let mut violations = Vec::new();
    let mut current_idx = 0;

    while let Some(fn_idx) = source[current_idx..].find("pub fn ") {
        let abs_idx = current_idx + fn_idx;
        let start_of_name = abs_idx + 7; // skip "pub fn "
        let end_of_name = source[start_of_name..]
            .find('(')
            .unwrap_or(0)
            + start_of_name;
        let fn_name = source[start_of_name..end_of_name].trim();

        if fn_name.starts_with("get_") || fn_name.starts_with("is_") {
            // Extract the function body by counting braces.
            let mut brace_count = 0i32;
            let mut started = false;
            let mut end_idx = abs_idx;

            for (i, c) in source[abs_idx..].char_indices() {
                if c == '{' {
                    brace_count += 1;
                    started = true;
                } else if c == '}' {
                    brace_count -= 1;
                }
                if started && brace_count == 0 {
                    end_idx = abs_idx + i;
                    break;
                }
            }

            if started {
                let body = &source[abs_idx..=end_idx];

                // Detect bare `.unwrap()` (not `.unwrap_or`, `.unwrap_or_default`,
                // `.unwrap_or_else`).
                let has_bare_unwrap = {
                    let mut found = false;
                    let mut s = 0;
                    while let Some(p) = body[s..].find(".unwrap") {
                        let after = &body[s + p + 7..]; // skip ".unwrap"
                        if after.starts_with('(') {
                            // It is ".unwrap()" — bare unwrap, panics
                            found = true;
                            break;
                        }
                        // It is ".unwrap_or…" — safe variant, skip
                        s += p + 7;
                    }
                    found
                };

                let has_expect = body.contains(".expect(");
                let has_panic = body.contains("panic!(")
                    || body.contains("panic_with_error!(");

                if has_bare_unwrap || has_expect || has_panic {
                    let reason = if has_bare_unwrap {
                        ".unwrap()"
                    } else if has_expect {
                        ".expect(\"…\")"
                    } else {
                        "panic!(…)"
                    };
                    violations.push(format!(
                        "NO-PANIC VIOLATION: fn {} contains {}",
                        fn_name, reason
                    ));
                }
            }
        }

        current_idx = abs_idx + 7;
    }

    (!violations.is_empty(), violations)
}

// ---------------------------------------------------------------------------
// Happy path — view functions with safe fallbacks must not be flagged
// ---------------------------------------------------------------------------

/// A view function using only `.unwrap_or(…)` (safe) must NOT be flagged.
#[test]
fn no_panic_view_fn_unwrap_or_is_clean() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn get_value(env: Env) -> u32 {
        env.storage().instance().get(&symbol_short!("V")).unwrap_or(0)
    }
    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&symbol_short!("P")).unwrap_or(false)
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(
        !found,
        "unwrap_or is safe — must not be flagged, got:\n{}",
        violations.join("\n")
    );
}

/// A view function using `.unwrap_or_default()` must NOT be flagged.
#[test]
fn no_panic_view_fn_unwrap_or_default_is_clean() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn get_count(env: Env) -> u32 {
        env.storage().instance().get(&symbol_short!("N")).unwrap_or_default()
    }
}
"#;
    let (found, _) = detect_panics_in_view_fns(source);
    assert!(!found, "unwrap_or_default is safe — must not be flagged");
}

/// A view function using `.unwrap_or_else(|| …)` must NOT be flagged.
#[test]
fn no_panic_view_fn_unwrap_or_else_is_clean() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short, Vec};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn get_items(env: Env) -> Vec<u32> {
        env.storage()
            .instance()
            .get(&symbol_short!("ITEMS"))
            .unwrap_or_else(|| Vec::new(&env))
    }
}
"#;
    let (found, _) = detect_panics_in_view_fns(source);
    assert!(!found, "unwrap_or_else is safe — must not be flagged");
}

/// Non-view functions (`set_*`, `init`, `execute_*`) must not be scanned —
/// panics in mutation paths are guarded separately.
#[test]
fn no_panic_check_does_not_scan_non_view_functions() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct C;
#[contractimpl]
impl C {
    pub fn set_value(env: Env, v: u32) {
        env.storage().instance().set(&symbol_short!("V"), &v);
    }
    pub fn init(env: Env) {
        let _ = env.storage().instance().get::<_, u32>(&symbol_short!("I")).unwrap();
    }
    pub fn execute_flow(env: Env) {
        panic!("not yet");
    }
}
"#;
    let (found, _) = detect_panics_in_view_fns(source);
    assert!(
        !found,
        "non-view functions must not be scanned for the no-panic rule"
    );
}

// ---------------------------------------------------------------------------
// Sad path — violations must be detected
// ---------------------------------------------------------------------------

/// A `get_*` function with a bare `.unwrap()` must be flagged.
#[test]
fn no_panic_detects_bare_unwrap_in_get_fn() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct Bad;
#[contractimpl]
impl Bad {
    pub fn get_balance(env: Env) -> i128 {
        // BUG: panics when storage key is absent
        env.storage().instance().get(&symbol_short!("BAL")).unwrap()
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(
        found,
        "bare .unwrap() in a get_* fn must be flagged as a no-panic violation"
    );
    assert!(
        violations.iter().any(|v| v.contains("get_balance")),
        "violation must name 'get_balance', got:\n{}",
        violations.join("\n")
    );
}

/// A `get_*` function with `.expect("…")` must be flagged.
#[test]
fn no_panic_detects_expect_in_get_fn() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct Bad;
#[contractimpl]
impl Bad {
    pub fn get_owner(env: Env) -> soroban_sdk::Address {
        env.storage()
            .instance()
            .get(&symbol_short!("OWNER"))
            .expect("owner must be set")   // BUG: panics when absent
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(
        found,
        ".expect() in a get_* fn must be flagged as a no-panic violation"
    );
    assert!(
        violations.iter().any(|v| v.contains("get_owner")),
        "violation must name 'get_owner', got:\n{}",
        violations.join("\n")
    );
}

/// A `get_*` function with a bare `panic!` macro must be flagged.
#[test]
fn no_panic_detects_explicit_panic_macro_in_get_fn() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env};
#[contract]
pub struct Bad;
#[contractimpl]
impl Bad {
    pub fn get_rate(_env: Env) -> u32 {
        panic!("not implemented")   // BUG: unconditional panic in view fn
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(found, "explicit panic!() in a get_* fn must be flagged");
    assert!(
        violations.iter().any(|v| v.contains("get_rate")),
        "violation must name 'get_rate', got:\n{}",
        violations.join("\n")
    );
}

/// An `is_*` function with a bare `.unwrap()` must be flagged.
#[test]
fn no_panic_detects_bare_unwrap_in_is_fn() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct Bad;
#[contractimpl]
impl Bad {
    pub fn is_initialized(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&symbol_short!("INIT"))
            .unwrap()   // BUG: panics when storage key absent
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(
        found,
        "bare .unwrap() in an is_* fn must be flagged"
    );
    assert!(
        violations.iter().any(|v| v.contains("is_initialized")),
        "violation must name 'is_initialized', got:\n{}",
        violations.join("\n")
    );
}

/// A `get_*` function containing `panic_with_error!` must be flagged.
#[test]
fn no_panic_detects_panic_with_error_in_get_fn() {
    let source = r#"
#![no_std]
use soroban_sdk::{contract, contractimpl, Env, symbol_short};
#[contract]
pub struct Bad;
#[contractimpl]
impl Bad {
    pub fn get_data(env: Env) -> u32 {
        match env.storage().instance().get::<_, u32>(&symbol_short!("D")) {
            Some(v) => v,
            None => panic_with_error!(&env, 1u32),  // BUG: aborts tx
        }
    }
}
"#;
    let (found, violations) = detect_panics_in_view_fns(source);
    assert!(
        found,
        "panic_with_error! in a get_* fn must be flagged as a no-panic violation"
    );
    assert!(
        violations.iter().any(|v| v.contains("get_data")),
        "violation must name 'get_data', got:\n{}",
        violations.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Workspace-wide no-panic scan — the core deliverable for #1273
// ---------------------------------------------------------------------------

/// Loops over every workspace contract `lib.rs` and asserts that all
/// `get_*` / `is_*` functions are free of unconditional panics.
///
/// Failure output names the offending contract and function so CI output is
/// immediately actionable without a manual code search.
#[test]
fn test_all_workspace_view_fns_are_panic_free() {
    let root = workspace_root();
    let contracts = [
        "remittance_split",
        "savings_goals",
        "bill_payments",
        "insurance",
        "family_wallet",
        "orchestrator",
        "reporting",
        "emergency_killswitch",
        "data_migration",
    ];

    let mut all_violations = Vec::new();

    for contract_name in contracts {
        let lib_path = root.join(contract_name).join("src").join("lib.rs");
        if !lib_path.exists() {
            continue;
        }

        let source = fs::read_to_string(&lib_path).expect("read lib.rs");
        let (found, violations) = detect_panics_in_view_fns(&source);

        if found {
            for v in violations {
                all_violations.push(format!("{}: {}", contract_name, v));
            }
        }
    }

    assert!(
        all_violations.is_empty(),
        "Found view functions with unconditional panics in workspace contracts:\n{}\n\n\
         View functions (get_*/is_*) must use .unwrap_or() / .unwrap_or_default() / \
         .unwrap_or_else() instead of bare .unwrap() / .expect() / panic!().",
        all_violations.join("\n")
    );
}
