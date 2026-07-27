// =========================================================================
// Issue #1272 — require_no_panic_in_view_fn() compile-time check
//
// View functions (`get_*`, `is_*`) must never call `unwrap()`, `expect()`,
// `panic!()`, or `unreachable!()` directly. Panics in view functions:
//
//   1. Abort the Soroban transaction unconditionally — there is no try/catch.
//   2. Expose internal state through panic messages that are visible on-chain.
//   3. Make "read-only" RPC calls unpredictably expensive (failed WASM
//      execution still consumes resources).
//
// This module implements a `require_no_panic_in_view_fn` static analyser that
// parses Rust source text and flags any `get_*` / `is_*` function body that
// contains a banned panic expression. It deliberately mirrors the style of
// `view_fn_readonly_test.rs` so both checks compose as a unified static layer.
//
// The analyser is intentionally simple (text-based, not AST-based) so that it
// has no build-time dependencies beyond `std`. For production-grade enforcement
// integrate with a `rustc` lint or a `cargo-geiger`-style tool; this check is
// the lightweight CI tripwire.
// =========================================================================

// ---- public API -----------------------------------------------------------

/// Banned patterns that must not appear inside a view-function body.
/// Extend this list if new panic-producing macros are added to the codebase.
pub const BANNED_PANIC_PATTERNS: &[&str] = &[
    ".unwrap()",
    ".expect(",
    "panic!(",
    "unreachable!(",
    "todo!(",
    "unimplemented!(",
];

/// Describes a single violation found by `require_no_panic_in_view_fn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanicViolation {
    /// Name of the view function that contains the banned pattern.
    pub fn_name: String,
    /// The banned pattern that was detected.
    pub pattern: String,
}

impl std::fmt::Display for PanicViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PANIC VIOLATION in view fn `{}`: contains banned pattern `{}`",
            self.fn_name, self.pattern
        )
    }
}

/// Scan `source` for view functions (`pub fn get_*` / `pub fn is_*`) that
/// contain any of [`BANNED_PANIC_PATTERNS`]. Returns a list of violations.
///
/// A function is considered a view function if its name starts with `get_` or
/// `is_` (matching the convention used across all contracts in this workspace).
///
/// The body scan is brace-balanced so inner functions, closures, and impl
/// blocks are handled correctly.
///
/// # Compile-time enforcement
///
/// Wrap a call to this function in a `#[test]` and `assert!(violations.is_empty())`
/// to get a compile-time-equivalent CI failure whenever a banned pattern is
/// introduced into a view function.
pub fn require_no_panic_in_view_fn(source: &str) -> Vec<PanicViolation> {
    let mut violations = Vec::new();
    let mut cursor = 0usize;

    while cursor < source.len() {
        // Find the next `pub fn` declaration.
        let Some(rel_fn_idx) = source[cursor..].find("pub fn ") else {
            break;
        };
        let fn_kw_start = cursor + rel_fn_idx;
        let name_start = fn_kw_start + 7; // skip "pub fn "

        // Extract the function name up to the first `(`.
        let Some(rel_paren) = source[name_start..].find('(') else {
            cursor = fn_kw_start + 7;
            continue;
        };
        let fn_name = source[name_start..name_start + rel_paren].trim().to_string();

        // Only inspect view functions.
        if !fn_name.starts_with("get_") && !fn_name.starts_with("is_") {
            cursor = fn_kw_start + 7;
            continue;
        }

        // Extract the body by counting braces.
        let (body, body_end) = match extract_fn_body(source, fn_kw_start) {
            Some(result) => result,
            None => {
                cursor = fn_kw_start + 7;
                continue;
            }
        };

        // Scan the body for banned patterns.
        for &pattern in BANNED_PANIC_PATTERNS {
            if body.contains(pattern) {
                violations.push(PanicViolation {
                    fn_name: fn_name.clone(),
                    pattern: pattern.to_string(),
                });
            }
        }

        cursor = body_end + 1;
    }

    violations
}

/// Extract the text of the brace-delimited body of the function starting at
/// `fn_start`. Returns `(body_text, end_index)` where `end_index` is the
/// position of the closing `}`.
fn extract_fn_body(source: &str, fn_start: usize) -> Option<(String, usize)> {
    let slice = &source[fn_start..];
    let mut brace_count = 0i32;
    let mut started = false;
    let mut body_start = 0usize;

    for (i, c) in slice.char_indices() {
        match c {
            '{' => {
                brace_count += 1;
                if !started {
                    started = true;
                    body_start = i;
                }
            }
            '}' => {
                brace_count -= 1;
                if started && brace_count == 0 {
                    let body = slice[body_start..=i].to_string();
                    return Some((body, fn_start + i));
                }
            }
            _ => {}
        }
    }
    None
}

// ---- test fixtures --------------------------------------------------------

/// A clean view function that only reads — no banned patterns.
const CLEAN_VIEW_FN: &str = r#"
pub fn get_balance(env: Env, key: Symbol) -> i128 {
    env.storage().instance().get(&key).unwrap_or(0i128)
}

pub fn is_paused(env: Env) -> bool {
    env.storage().instance().get(&symbol_short!("PAUSED")).unwrap_or(false)
}
"#;

/// A `get_*` function that calls `.unwrap()` — violation.
const UNWRAP_VIOLATION: &str = r#"
pub fn get_value(env: Env, key: Symbol) -> i128 {
    env.storage().instance().get(&key).unwrap()
}
"#;

/// A `get_*` function that calls `.expect(...)` — violation.
const EXPECT_VIOLATION: &str = r#"
pub fn get_config(env: Env) -> Config {
    env.storage().instance().get(&symbol_short!("CFG")).expect("config must exist")
}
"#;

/// A `get_*` function that calls `panic!(...)` — violation.
const PANIC_VIOLATION: &str = r#"
pub fn get_owner(env: Env) -> Address {
    let opt: Option<Address> = env.storage().instance().get(&symbol_short!("OWN"));
    if opt.is_none() {
        panic!("owner not set");
    }
    opt.unwrap()
}
"#;

/// An `is_*` function that calls `unreachable!(...)` — violation.
const UNREACHABLE_VIOLATION: &str = r#"
pub fn is_valid_state(env: Env) -> bool {
    match env.storage().instance().get(&symbol_short!("S")).unwrap_or(0u32) {
        0 => false,
        1 => true,
        _ => unreachable!("unexpected state value"),
    }
}
"#;

/// A non-view function that calls `.unwrap()` — must NOT be flagged.
const NON_VIEW_WITH_UNWRAP: &str = r#"
pub fn set_owner(env: Env, new_owner: Address) {
    let old: Option<Address> = env.storage().instance().get(&symbol_short!("OWN"));
    let _ = old.unwrap(); // allowed: not a view function
    env.storage().instance().set(&symbol_short!("OWN"), &new_owner);
}
"#;

/// A `get_*` function that uses `unwrap_or` (not banned) — must NOT be flagged.
const CLEAN_UNWRAP_OR: &str = r#"
pub fn get_counter(env: Env) -> u32 {
    env.storage().instance().get(&symbol_short!("CNT")).unwrap_or(0u32)
}
"#;

/// A `get_*` function that calls `todo!()` — violation.
const TODO_VIOLATION: &str = r#"
pub fn get_unimplemented(env: Env) -> u32 {
    todo!("implement this")
}
"#;

// ---- tests ----------------------------------------------------------------

/// HAPPY PATH: A clean view function with no banned patterns must produce no violations.
#[test]
fn clean_view_fn_produces_no_violations() {
    let violations = require_no_panic_in_view_fn(CLEAN_VIEW_FN);
    assert!(
        violations.is_empty(),
        "clean view functions must produce no violations, got:\n{}",
        violations
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// NEGATIVE TEST: A `get_*` function calling `.unwrap()` must be flagged.
///
/// This test verifies that the checker fires on the most common panic pattern.
#[test]
fn get_fn_with_unwrap_is_detected() {
    let violations = require_no_panic_in_view_fn(UNWRAP_VIOLATION);

    assert!(
        !violations.is_empty(),
        "get_* function calling .unwrap() must be flagged as a panic violation"
    );
    assert!(
        violations.iter().any(|v| v.fn_name == "get_value"),
        "violation must name the offending function 'get_value', got: {:?}",
        violations
    );
    assert!(
        violations.iter().any(|v| v.pattern == ".unwrap()"),
        "violation must name the banned pattern '.unwrap()', got: {:?}",
        violations
    );
}

/// NEGATIVE TEST: A `get_*` function calling `.expect(...)` must be flagged.
#[test]
fn get_fn_with_expect_is_detected() {
    let violations = require_no_panic_in_view_fn(EXPECT_VIOLATION);

    assert!(
        !violations.is_empty(),
        "get_* function calling .expect() must be flagged"
    );
    assert!(
        violations.iter().any(|v| v.fn_name == "get_config"),
        "violation must name 'get_config'"
    );
    assert!(
        violations.iter().any(|v| v.pattern.contains(".expect(")),
        "violation must name '.expect(' as the pattern"
    );
}

/// NEGATIVE TEST: A `get_*` function calling `panic!(...)` must be flagged.
#[test]
fn get_fn_with_panic_macro_is_detected() {
    let violations = require_no_panic_in_view_fn(PANIC_VIOLATION);

    assert!(
        !violations.is_empty(),
        "get_* function calling panic!() must be flagged"
    );
    let names: Vec<_> = violations.iter().map(|v| &v.fn_name).collect();
    assert!(
        names.contains(&&"get_owner".to_string()),
        "violation must name 'get_owner', got: {:?}",
        names
    );
}

/// NEGATIVE TEST: An `is_*` function calling `unreachable!(...)` must be flagged.
#[test]
fn is_fn_with_unreachable_is_detected() {
    let violations = require_no_panic_in_view_fn(UNREACHABLE_VIOLATION);

    assert!(
        !violations.is_empty(),
        "is_* function calling unreachable!() must be flagged"
    );
    assert!(
        violations.iter().any(|v| v.fn_name == "is_valid_state"),
        "violation must name 'is_valid_state'"
    );
}

/// NON-VIEW functions calling `.unwrap()` must NOT be flagged.
///
/// The checker is scoped to `get_*` and `is_*` only.
#[test]
fn non_view_fn_with_unwrap_is_not_flagged() {
    let violations = require_no_panic_in_view_fn(NON_VIEW_WITH_UNWRAP);
    assert!(
        violations.is_empty(),
        "non-view functions must not be flagged, got:\n{}",
        violations
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// `unwrap_or(...)` is NOT banned — it does not panic.
#[test]
fn get_fn_with_unwrap_or_is_not_flagged() {
    let violations = require_no_panic_in_view_fn(CLEAN_UNWRAP_OR);
    assert!(
        violations.is_empty(),
        "unwrap_or is not a panic pattern and must not be flagged, got: {:?}",
        violations
    );
}

/// NEGATIVE TEST: A `get_*` function calling `todo!()` must be flagged.
#[test]
fn get_fn_with_todo_is_detected() {
    let violations = require_no_panic_in_view_fn(TODO_VIOLATION);

    assert!(
        !violations.is_empty(),
        "get_* function calling todo!() must be flagged"
    );
    assert!(
        violations.iter().any(|v| v.fn_name == "get_unimplemented"),
        "violation must name 'get_unimplemented'"
    );
}

/// Multiple violations in a single source are all reported (not just the first).
#[test]
fn multiple_violations_are_all_reported() {
    let source = format!("{}\n{}", UNWRAP_VIOLATION, EXPECT_VIOLATION);
    let violations = require_no_panic_in_view_fn(&source);

    // At least two different function names must appear.
    let names: std::collections::HashSet<_> = violations.iter().map(|v| &v.fn_name).collect();
    assert!(
        names.contains(&"get_value".to_string()),
        "get_value must be reported"
    );
    assert!(
        names.contains(&"get_config".to_string()),
        "get_config must be reported"
    );
}

/// PanicViolation Display message is human-readable.
#[test]
fn violation_display_message_is_descriptive() {
    let v = PanicViolation {
        fn_name: "get_balance".to_string(),
        pattern: ".unwrap()".to_string(),
    };
    let msg = v.to_string();
    assert!(
        msg.contains("get_balance"),
        "display must mention the function name: {msg}"
    );
    assert!(
        msg.contains(".unwrap()"),
        "display must mention the pattern: {msg}"
    );
    assert!(
        msg.contains("PANIC VIOLATION"),
        "display must include severity label: {msg}"
    );
}
