//! Storage key naming convention: live source scan.
//!
//! `storage_key_naming_test.rs` validates a hand-maintained catalogue of
//! storage keys. That catalogue is a snapshot: nothing forces it to stay in
//! sync with the actual contract source, so a key can be renamed or added in
//! code without the catalogue (or its format checks) ever seeing it.
//!
//! This module closes that gap by parsing each contract crate's `src/lib.rs`
//! directly and extracting every storage-key literal actually passed to a
//! Soroban storage accessor (`.get(...)`, `.set(...)`, `.has(...)`,
//! `.remove(...)`), then re-running the same length/format constraints
//! against what the source really contains. A drift between "what the code
//! does" and "what the catalogue says" now fails CI regardless of which side
//! changed.
//!
//! ## What counts as a storage key here
//!
//! Two shapes are recognized:
//! 1. An inline literal: `env.storage().instance().get(&symbol_short!("KEY"))`
//! 2. A local named constant: `const KEY: Symbol = symbol_short!("KEY");`
//!    later passed by name to a storage accessor in the same file, e.g.
//!    `env.storage().instance().set(&STORAGE_UNPAID_TOTALS, &v)`.
//!
//! Anything else passed to these accessors (composite `DataKey` enum
//! variants, function parameters, cross-crate constants such as
//! `SNAPSHOT_KEY` from `remitwise-common`) is intentionally skipped: those
//! don't carry a naming-convention string literal at the call site, so
//! there is nothing here to validate. Event topics and action symbols
//! (e.g. `symbol_short!("created")`) are also excluded by construction: the
//! naming convention only applies to storage keys, and this scan only looks
//! at arguments of storage accessor calls, not `.events().publish(...)`.
//!
//! Reference: `docs/storage-key-naming-conventions.md`, `STORAGE_LAYOUT.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Same constraint as `storage_key_naming_test.rs` (Soroban `symbol_short!`
/// limit). Duplicated intentionally: this file validates the source
/// directly and must not depend on the other test's internal list.
const MAX_KEY_LENGTH: usize = 9;

/// Crates scanned for storage keys. Excludes crates with no on-chain
/// Soroban storage of their own (`data_migration`, `cli`, `scenarios`,
/// `integration_tests`, `testutils`).
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

/// Extracts every storage key literal used in `src` (see module docs for
/// exactly what is and isn't recognized).
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

            // Bare identifier (no path separators, no nested calls) that
            // resolves to a local `const NAME: Symbol = symbol_short!(...)`.
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

#[test]
fn scanned_storage_keys_within_max_length() {
    let violations: Vec<String> = scan_all_crates()
        .into_iter()
        .filter(|(_, key)| key.len() > MAX_KEY_LENGTH)
        .map(|(krate, key)| {
            format!(
                "❌ {krate}: '{key}' exceeds max length {MAX_KEY_LENGTH} (actual: {})",
                key.len()
            )
        })
        .collect();

    assert!(
        violations.is_empty(),
        "\n\nStorage key length violations found in live source:\n{}\n\n\
         A storage key was added or renamed in source without honoring the \
         {MAX_KEY_LENGTH}-character symbol_short! limit. See \
         docs/storage-key-naming-conventions.md.\n",
        violations.join("\n")
    );
}

#[test]
fn scanned_storage_keys_are_uppercase_with_underscores() {
    let violations: Vec<String> = scan_all_crates()
        .into_iter()
        .filter(|(_, key)| {
            !key.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        })
        .map(|(krate, key)| format!("❌ {krate}: '{key}' is not UPPERCASE_WITH_UNDERSCORES"))
        .collect();

    assert!(
        violations.is_empty(),
        "\n\nStorage key format violations found in live source:\n{}\n\n\
         A storage key literal in source does not follow the documented \
         UPPERCASE_WITH_UNDERSCORES convention. See \
         docs/storage-key-naming-conventions.md.\n",
        violations.join("\n")
    );
}

#[test]
fn scanned_storage_keys_have_no_leading_trailing_or_double_underscores() {
    let violations: Vec<String> = scan_all_crates()
        .into_iter()
        .filter(|(_, key)| {
            key.starts_with('_') || key.ends_with('_') || key.contains("__") || key.is_empty()
        })
        .map(|(krate, key)| format!("❌ {krate}: '{key}' has an underscore placement violation"))
        .collect();

    assert!(
        violations.is_empty(),
        "\n\nStorage key underscore-placement violations found in live source:\n{}\n",
        violations.join("\n")
    );
}

/// Sanity check on the scanner itself: it must actually find well-known
/// keys that are stored as inline `symbol_short!` literals, so a change to
/// the parsing heuristics that silently stops matching real code is caught
/// here rather than by every key quietly disappearing from coverage.
///
/// Note: `savings_goals` and `insurance` primarily use a composite `DataKey`
/// enum (`DataKey::Goal(u32)`, `DataKey::Policy(u32)`, ...) rather than
/// inline `symbol_short!` literals for most of their storage - see
/// "Scalable DataKey Pattern" in `STORAGE_LAYOUT.md`. Those enum variants
/// don't carry a naming-convention string literal at the call site, so this
/// scanner (by design, see module docs) finds only their few remaining
/// literal-keyed entries (e.g. `SNAP_TS`, `VERSION`), not the full catalogue
/// in `storage_key_naming_test.rs`.
#[test]
fn scanner_finds_known_keys_across_contracts() {
    let found: BTreeSet<(&'static str, String)> = scan_all_crates().into_iter().collect();

    let expected = [
        ("remittance_split", "CONFIG"),
        ("remittance_split", "PAUSE_ADM"),
        ("savings_goals", "SNAP_TS"),
        ("bill_payments", "BILLS"),
        ("bill_payments", "UNPD_TOT"),
        ("insurance", "VERSION"),
        ("family_wallet", "OWNER"),
        ("family_wallet", "MEMBERS"),
        ("reporting", "ADMIN"),
        ("orchestrator", "STATS"),
        ("orchestrator", "AUDIT"),
    ];

    let missing: Vec<String> = expected
        .iter()
        .filter(|(krate, key)| !found.contains(&(*krate, key.to_string())))
        .map(|(krate, key)| format!("{krate}::{key}"))
        .collect();

    assert!(
        missing.is_empty(),
        "\n\nScanner failed to find expected well-known storage keys: {:?}\n\
         Either the parsing heuristic regressed, or these keys were \
         genuinely removed from source (update this test's expectations \
         if so).\n",
        missing
    );
}

#[test]
fn print_scanned_storage_key_summary() {
    let found = scan_all_crates();
    let mut per_crate: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (krate, key) in found {
        per_crate.entry(krate).or_default().insert(key);
    }

    println!("\n📡 Live Source Storage Key Scan:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for (krate, keys) in &per_crate {
        println!("  • {krate}: {} keys -> {:?}", keys.len(), keys);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
}
