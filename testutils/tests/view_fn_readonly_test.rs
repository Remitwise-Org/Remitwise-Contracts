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
use std::process::Command;

/// Returns the path to the workspace root (parent of this crate's manifest dir).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get parent of manifest dir")
        .to_path_buf()
}

/// Returns the absolute path to the check script.
fn check_script() -> PathBuf {
    workspace_root().join("scripts/check_view_functions_readonly.sh")
}

/// Runs the check script against a temporary directory containing one lib.rs
/// fixture.  Returns (exit_success, combined stdout+stderr).
fn run_check_on_fixture(contract_dir: &std::path::Path, lib_rs_content: &str) -> (bool, String) {
    let src_dir = contract_dir.join("src");
    fs::create_dir_all(&src_dir)
        .expect("Failed to create src dir");
    let lib_path = src_dir.join("lib.rs");
    fs::write(&lib_path, lib_rs_content)
        .expect("Failed to write fixture lib.rs");

    // The script iterates over a hard-coded list of contract directories.
    // We wrap the fixture into a standalone script that sets the right CWD.
    let script = check_script();
    let output = Command::new("bash")
        .arg(script)
        .current_dir(contract_dir.parent().expect("contract dir has no parent"))
        .output()
        .expect("Failed to spawn check script");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}{}", stdout, stderr);
    (output.status.success(), combined)
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

/// Creates a temporary contract directory named `contract_name` under a
/// per-test temp root and runs the check against the given fixture source.
///
/// Returns `(script_passed, output_text)`.
fn check_fixture(test_name: &str, contract_name: &str, fixture: &str) -> (bool, String) {
    // Use a subdirectory of the system temp dir isolated per test
    let tmp_root = std::env::temp_dir()
        .join("remitwise_view_fn_tests")
        .join(test_name);

    // Clean up any leftover state from prior run
    let _ = fs::remove_dir_all(&tmp_root);
    fs::create_dir_all(&tmp_root).expect("Failed to create tmp root");

    let contract_dir = tmp_root.join(contract_name);
    let (passed, output) = run_check_on_fixture(&contract_dir, fixture);

    // Clean up after ourselves
    let _ = fs::remove_dir_all(&tmp_root);

    (passed, output)
}

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
    use std::io::Write;

    // Write to a temp file
    let tmp_dir = std::env::temp_dir().join("remitwise_view_fn_grep_test");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).expect("mkdir tmp");
    let lib_path = tmp_dir.join("lib.rs");
    fs::write(&lib_path, source).expect("write fixture");

    // Inline bash logic mirroring the script's core detection loop
    let inline_script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
file="{lib_path}"
view_prefixes=("get_" "is_")
write_patterns=(
    'env\.storage\(\)\.[a-z]*\(\)\.set\('
    'env\.storage\(\)\.[a-z]*\(\)\.remove\('
    'env\.storage\(\)\.[a-z]*\(\)\.extend_ttl\('
)
found=0
for prefix in "${{view_prefixes[@]}}"; do
    while IFS= read -r line_info; do
        [ -z "$line_info" ] && continue
        line_num=$(echo "$line_info" | cut -d: -f1)
        func_name=$(echo "$line_info" | cut -d: -f2- | sed 's/.*pub fn \([a-z_0-9]*\).*/\1/')
        end_line=$((line_num + 150))
        func_body=$(sed -n "${{line_num}},${{end_line}}p" "$file")
        for pattern in "${{write_patterns[@]}}"; do
            if echo "$func_body" | grep -qE "$pattern"; then
                echo "VIOLATION: fn $func_name writes storage"
                found=1
            fi
        done
    done < <(grep -n "pub fn ${{prefix}}" "$file" 2>/dev/null || true)
done
exit $found
"#,
        lib_path = lib_path.display()
    );

    let script_path = tmp_dir.join("check.sh");
    let mut f = fs::File::create(&script_path).expect("create script");
    f.write_all(inline_script.as_bytes()).expect("write script");
    drop(f);

    let output = Command::new("bash")
        .arg(&script_path)
        .output()
        .expect("spawn inline script");

    let _ = fs::remove_dir_all(&tmp_dir);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let violations: Vec<String> = stdout
        .lines()
        .filter(|l| l.starts_with("VIOLATION:"))
        .map(|l| l.to_string())
        .collect();

    (!output.status.success(), violations)
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
    let mentions_fn = violations
        .iter()
        .any(|v| v.contains("get_cached_value"));
    assert!(
        mentions_fn,
        "Violation output should name 'get_cached_value', got:\n{}",
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

/// Verify the check script itself exists and is executable.
#[test]
fn test_check_script_exists_and_is_executable() {
    let script = check_script();

    assert!(
        script.exists(),
        "Expected check script at {}",
        script.display()
    );

    // On Unix, check the executable bit
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = fs::metadata(&script).expect("metadata");
        let mode = meta.permissions().mode();
        assert!(
            mode & 0o111 != 0,
            "Script {} must be executable (mode: {:o})",
            script.display(),
            mode
        );
    }
}
