//! Reserved storage key enforcement.
//!
//! `docs/RESERVED_STORAGE_KEYS.md` documents storage-key prefixes that are
//! set aside for roadmap features (yield generation, staking, the V2
//! migration path, etc.) and tells contributors not to reuse them. Until
//! now that document was aspirational: nothing checked that a contract
//! never actually stores data under one of those keys, so the "reviewer
//! verification" checklist at the bottom of that doc relied entirely on a
//! human noticing during code review.
//!
//! This module closes that gap the same way `storage_key_source_scan_test.rs`
//! closes the drift gap for naming conventions: it parses the reserved-key
//! table straight out of the markdown doc (so the doc stays the single
//! source of truth) and cross-checks it against every storage-key literal
//! actually used in each contract's `src/lib.rs`. A reserved key finding its
//! way into source now fails CI instead of waiting on a reviewer to catch it.
//!
//! Reference: `docs/RESERVED_STORAGE_KEYS.md`,
//! `docs/storage-key-naming-conventions.md`,
//! `testutils/tests/storage_key_source_scan_test.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Crates scanned for storage keys. Mirrors
/// `storage_key_source_scan_test.rs`; duplicated intentionally so this file
/// does not depend on another test binary's internals (each `tests/*.rs`
/// file compiles as its own crate).
const SCANNED_CRATES: &[&str] = &[
    "remittance_split",
    "savings_goals",
    "bill_payments",
    "insurance",
    "family_wallet",
    "reporting",
    "orchestrator",
    "emergency_killswitch",
    "remitwise-common",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testutils has a parent directory")
        .to_path_buf()
}

/// Parses the `| \`KEY\` | ... |` rows out of the "List of Reserved Keys"
/// table in `docs/RESERVED_STORAGE_KEYS.md`. Only rows whose first cell is a
/// backtick-quoted key are matched, which naturally skips the header row
/// (`| Reserved Key | ... |`), the separator row (`|---|---|`), and the
/// fenced code blocks in the "Concrete Examples" section (those lines don't
/// start with `|`).
fn parse_reserved_keys_from_doc() -> BTreeSet<String> {
    let path = workspace_root()
        .join("docs")
        .join("RESERVED_STORAGE_KEYS.md");
    let src = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    let mut keys = BTreeSet::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let Some(first_backtick) = trimmed.find('`') else {
            continue;
        };
        let rest = &trimmed[first_backtick + 1..];
        let Some(end) = rest.find('`') else {
            continue;
        };
        let key = rest[..end].trim();
        if !key.is_empty() {
            keys.insert(key.to_string());
        }
    }
    keys
}

/// Finds every `const NAME: Symbol = symbol_short!("KEY");` (with or
/// without a `pub` prefix) in `src` and returns a map of `NAME -> KEY`.
fn find_local_symbol_consts(src: &str) -> BTreeMap<String, String> {
    let mut consts = BTreeMap::new();
    let marker = "const ";
    let mut cursor = 0usize;

    while let Some(rel) = src[cursor..].find(marker) {
        let name_start = cursor + rel + marker.len();
        let rest = &src[name_start..];

        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        cursor = name_start + name_end.max(1);

        if name.is_empty() {
            continue;
        }

        let after_name = rest[name_end..].trim_start();
        let Some(after_colon) = after_name.strip_prefix(':') else {
            continue;
        };
        let after_type = after_colon.trim_start();
        let Some(after_symbol_ty) = after_type.strip_prefix("Symbol") else {
            continue;
        };
        let after_eq = after_symbol_ty.trim_start();
        let Some(after_eq) = after_eq.strip_prefix('=') else {
            continue;
        };
        let after_eq = after_eq.trim_start();
        let Some(after_macro) = after_eq.strip_prefix("symbol_short!(\"") else {
            continue;
        };
        if let Some(quote_end) = after_macro.find('"') {
            consts.insert(name.to_string(), after_macro[..quote_end].to_string());
        }
    }

    consts
}

/// Returns byte offsets of the opening `(` for every call to `.{accessor}(`
/// or `.{accessor}::<...>(` in `src`.
fn find_call_open_parens(src: &str, accessor: &str) -> Vec<usize> {
    let dotted = format!(".{accessor}");
    let mut sites = Vec::new();
    let mut cursor = 0usize;

    while let Some(rel) = src[cursor..].find(&dotted) {
        let idx = cursor + rel;
        let after = idx + dotted.len();
        match src[after..].chars().next() {
            Some('(') => {
                sites.push(after);
                cursor = after + 1;
            }
            Some(':') => {
                if let Some(paren_rel) = src[after..].find('(') {
                    sites.push(after + paren_rel);
                    cursor = after + paren_rel + 1;
                } else {
                    cursor = after;
                }
            }
            _ => cursor = after,
        }
    }

    sites
}

/// Given the byte offset of a call's opening `(`, returns the source text of
/// its first top-level argument (i.e. up to the first depth-1 comma, or the
/// closing `)` if there is only one argument). Handles nested parens such as
/// `symbol_short!("X")` or `DataKey::Goal(id)`.
fn extract_first_arg(src: &str, open_idx: usize) -> Option<&str> {
    let bytes = src.as_bytes();
    let arg_start = open_idx + 1;
    let mut depth = 1i32;
    let mut i = arg_start;

    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[arg_start..i]);
                }
            }
            b',' if depth == 1 => return Some(&src[arg_start..i]),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Extracts every storage key literal used in `src` that is passed to a
/// Soroban storage accessor (`.get()`, `.set()`, `.has()`, `.remove()`),
/// either as an inline `symbol_short!("KEY")` literal or via a local
/// `const NAME: Symbol = symbol_short!("KEY");` referenced by name.
fn extract_storage_keys(src: &str) -> BTreeSet<String> {
    let local_consts = find_local_symbol_consts(src);
    let mut keys = BTreeSet::new();

    for accessor in ["get", "set", "has", "remove"] {
        for open_idx in find_call_open_parens(src, accessor) {
            let Some(raw_arg) = extract_first_arg(src, open_idx) else {
                continue;
            };
            let arg = raw_arg.trim().trim_start_matches('&').trim();

            if let Some(rest) = arg.strip_prefix("symbol_short!(\"") {
                if let Some(quote_end) = rest.find('"') {
                    keys.insert(rest[..quote_end].to_string());
                }
                continue;
            }

            if !arg.is_empty() && arg.chars().all(|c| c.is_alphanumeric() || c == '_') {
                if let Some(key) = local_consts.get(arg) {
                    keys.insert(key.clone());
                }
            }
        }
    }

    keys
}

/// Reads `<crate>/src/lib.rs` for every crate in `SCANNED_CRATES` and
/// returns `(crate_name, storage_key)` pairs for every key found.
fn scan_all_crates() -> Vec<(&'static str, String)> {
    let root = workspace_root();
    let mut found = Vec::new();

    for crate_name in SCANNED_CRATES {
        let path = root.join(crate_name).join("src").join("lib.rs");
        let src = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

        for key in extract_storage_keys(&src) {
            found.push((*crate_name, key));
        }
    }

    found
}

/// Sanity check on the doc parser itself: if `RESERVED_STORAGE_KEYS.md`'s
/// table format ever changes in a way the parser can't follow, this fails
/// loudly instead of the enforcement test below silently checking against
/// an empty set.
#[test]
fn reserved_keys_doc_parses_expected_entries() {
    let reserved = parse_reserved_keys_from_doc();
    let expected = ["YIELD_CFG", "STAKE_POL", "REWARD_CF", "V2_MIGR", "TMP_LOCK"];

    for key in expected {
        assert!(
            reserved.contains(key),
            "\n\nExpected reserved key '{key}' not found by the doc parser.\n\
             Either docs/RESERVED_STORAGE_KEYS.md's table format changed \
             (update parse_reserved_keys_from_doc to match) or the key was \
             genuinely removed from the table (update this test's expectations).\n"
        );
    }
}

/// The main enforcement check (happy path): no contract in the workspace may
/// currently store data under a key reserved for a future feature.
#[test]
fn no_scanned_crate_uses_a_reserved_storage_key() {
    let reserved = parse_reserved_keys_from_doc();
    assert!(
        !reserved.is_empty(),
        "reserved-key doc parser returned no keys; see reserved_keys_doc_parses_expected_entries"
    );

    let violations: Vec<String> = scan_all_crates()
        .into_iter()
        .filter(|(_, key)| reserved.contains(key))
        .map(|(krate, key)| {
            format!("❌ {krate}: uses '{key}', which is reserved for a future feature")
        })
        .collect();

    assert!(
        violations.is_empty(),
        "\n\nReserved storage key violations found in live source:\n{}\n\n\
         A contract stores data under a key reserved for a future roadmap \
         feature. See docs/RESERVED_STORAGE_KEYS.md for the reserved list \
         and why it exists, and pick a different, non-reserved key.\n",
        violations.join("\n")
    );
}

/// Explicit failure mode: proves the detector actually flags reuse of a
/// reserved key. Runs the same extraction + reserved-set check used above
/// against a synthetic `lib.rs`-shaped snippet rather than mutating real
/// contract source, so this test documents the failure behavior without
/// requiring a second, throwaway crate to intentionally break the build.
#[test]
fn detector_flags_reserved_key_reuse_in_synthetic_source() {
    let reserved = parse_reserved_keys_from_doc();
    assert!(reserved.contains("REWARD_CF"));

    let synthetic_src = r#"
        use soroban_sdk::{symbol_short, Env, Symbol};

        const KEY_REWARD: Symbol = symbol_short!("REWARD_CF");

        pub fn set_notification(env: Env, enabled: bool) {
            env.storage().instance().set(&KEY_REWARD, &enabled);
        }
    "#;

    let used_keys = extract_storage_keys(synthetic_src);
    assert!(
        used_keys.contains("REWARD_CF"),
        "extractor failed to find the synthetic REWARD_CF usage at all"
    );

    let violations: Vec<&String> = used_keys.iter().filter(|k| reserved.contains(*k)).collect();
    assert!(
        !violations.is_empty(),
        "detector failed to flag a reserved key ('REWARD_CF') reused in synthetic source"
    );
}
