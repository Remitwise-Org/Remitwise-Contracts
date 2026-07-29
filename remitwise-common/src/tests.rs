#![allow(unused_imports, dead_code)]
// remitwise-common tests

/// Tests for [`canonicalize_tags`], [`canonicalize_tags_checked`], and [`clamp_limit`].
///
/// # Canonicalization contract (pinned here)
/// - ASCII uppercase letters are silently folded to lowercase.
/// - Allowed charset after folding: `[a-z0-9\-_]`.
/// - Any other byte causes the `on_invalid_char` closure to be invoked
///   (typically a panic or `panic_with_error!` at the call site).
/// - Tag length must be in `1..=TAG_MAX_LEN` (32) bytes; 0 or >32 panics.
/// - An empty tag batch (zero tags) panics.
/// - Output order matches input order; the function does **not** deduplicate.
///   If two input tags canonicalize to the same string (e.g. "Travel" and
///   "travel" both become "travel"), both copies appear in the output. Callers
///   that need uniqueness must deduplicate the result themselves.
extern crate std;

use super::*;
use crate::distribute_pro_rata;
use crate::PeriodKind::{Day, Month, Week};
use ed25519_dalek::Signer;
use proptest::prelude::*;
use soroban_sdk::testutils::{Ledger, LedgerInfo};
use soroban_sdk::{contract, contractimpl, Bytes, Env, IntoVal, String, Symbol, Vec};

#[allow(dead_code)]
fn set_ledger(env: &Env, sequence_number: u32) {
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 3_000_000,
    });
}

/// Sets only `network_id` on the ledger, preserving the rest of its current
/// state. Used to simulate the same contract instance storage being read
/// under a different Stellar network.
fn set_network(env: &Env, network_id: [u8; 32]) {
    let proto = env.ledger().protocol_version();
    let sequence_number = env.ledger().sequence();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number,
        timestamp: 1_700_000_000,
        network_id,
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 3_000_000,
    });
}

// helper: build a single-element tag Vec
fn single(env: &Env, tag: &str) -> Vec<String> {
    let mut v = Vec::new(env);
    v.push_back(String::from_str(env, tag));
    v
}

// helper: build a multi-element tag Vec from a slice of &str
fn tags(env: &Env, items: &[&str]) -> Vec<String> {
    let mut v = Vec::new(env);
    for &s in items {
        v.push_back(String::from_str(env, s));
    }
    v
}

// helper: extract the nth tag as a std::String for assertions
fn get(_env: &Env, v: &Vec<String>, i: u32) -> std::string::String {
    let s = v.get(i).unwrap();
    let mut buf = std::vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut buf);
    std::string::String::from_utf8(buf).unwrap()
}

fn prefixed_message(domain_separator: &[u8], message: &[u8]) -> std::vec::Vec<u8> {
    let mut bytes = std::vec::Vec::new();
    bytes.extend_from_slice(&(domain_separator.len() as u64).to_le_bytes());
    bytes.extend_from_slice(domain_separator);
    bytes.extend_from_slice(&(message.len() as u64).to_le_bytes());
    bytes.extend_from_slice(message);
    bytes
}

// ─── canonicalize_tags: lowercasing ──────────────────────────────────────────

/// Uppercase letters are folded to lowercase.
#[test]
fn test_canonicalize_uppercase_folded_to_lowercase() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "Travel"), || panic!("invalid"));
    assert_eq!(out.len(), 1);
    assert_eq!(get(&env, &out, 0), "travel");
}

/// ALL-CAPS tag is fully lowercased.
#[test]
fn test_canonicalize_all_caps_tag() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "FIRE"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "fire");
}

/// Mixed-case tag is fully lowercased.
#[test]
fn test_canonicalize_mixed_case_tag() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "MyGoal"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "mygoal");
}

/// Already-lowercase tag passes through unchanged.
#[test]
fn test_canonicalize_lowercase_passthrough() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "travel"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "travel");
}

// ─── canonicalize_tags: valid charset ────────────────────────────────────────

/// Digits are allowed.
#[test]
fn test_canonicalize_digits_allowed() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "goal2025"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "goal2025");
}

/// Hyphens are allowed.
#[test]
fn test_canonicalize_hyphen_allowed() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "long-term"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "long-term");
}

/// Underscores are allowed.
#[test]
fn test_canonicalize_underscore_allowed() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "my_goal"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "my_goal");
}

/// A tag using all allowed character classes together passes.
#[test]
fn test_canonicalize_mixed_valid_chars() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "my-tag_01"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "my-tag_01");
}

/// Single-character tag is valid.
#[test]
fn test_canonicalize_single_char_tag() {
    let env = Env::default();
    let out = canonicalize_tags(&env, &single(&env, "a"), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "a");
}

// ─── canonicalize_tags: invalid charset ──────────────────────────────────────

/// Space character triggers on_invalid_char.
#[test]
#[should_panic(expected = "invalid char: space")]
fn test_canonicalize_space_triggers_callback() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, "my goal"), || {
        panic!("invalid char: space")
    });
}

/// `@` symbol triggers on_invalid_char.
#[test]
#[should_panic(expected = "invalid char: at")]
fn test_canonicalize_at_symbol_triggers_callback() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, "user@domain"), || {
        panic!("invalid char: at")
    });
}

/// Dot (`.`) triggers on_invalid_char — common mistake.
#[test]
#[should_panic(expected = "invalid char: dot")]
fn test_canonicalize_dot_triggers_callback() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, "goal.2025"), || {
        panic!("invalid char: dot")
    });
}

/// Exclamation mark triggers on_invalid_char.
#[test]
#[should_panic(expected = "invalid char")]
fn test_canonicalize_exclamation_triggers_callback() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, "urgent!"), || panic!("invalid char"));
}

/// Hash (`#`) triggers on_invalid_char.
#[test]
#[should_panic(expected = "invalid char")]
fn test_canonicalize_hash_triggers_callback() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, "#savings"), || panic!("invalid char"));
}

// ─── canonicalize_tags: length boundaries ────────────────────────────────────

/// A 32-character tag (TAG_MAX_LEN) passes without error.
#[test]
fn test_canonicalize_tag_exactly_32_chars_passes() {
    let env = Env::default();
    // Exactly 32 lowercase ASCII letters.
    let tag = "abcdefghijklmnopqrstuvwxyzabcdef"; // 32 chars
    assert_eq!(tag.len(), 32);
    let out = canonicalize_tags(&env, &single(&env, tag), || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), tag);
}

/// A 33-character tag (one over TAG_MAX_LEN) panics with the length message.
#[test]
#[should_panic(expected = "Tag must be between 1 and 32 characters")]
fn test_canonicalize_tag_33_chars_panics() {
    let env = Env::default();
    let tag = "abcdefghijklmnopqrstuvwxyzabcdefg"; // 33 chars
    assert_eq!(tag.len(), 33);
    canonicalize_tags(&env, &single(&env, tag), || panic!("invalid"));
}

/// An empty string tag (len = 0) panics with the length message.
#[test]
#[should_panic(expected = "Tag must be between 1 and 32 characters")]
fn test_canonicalize_empty_string_tag_panics() {
    let env = Env::default();
    canonicalize_tags(&env, &single(&env, ""), || panic!("invalid"));
}

// ─── canonicalize_tags: empty batch ──────────────────────────────────────────

/// Passing an empty Vec panics with the empty-batch message.
#[test]
#[should_panic(expected = "Tags cannot be empty")]
fn test_canonicalize_empty_batch_panics() {
    let env = Env::default();
    let empty: Vec<String> = Vec::new(&env);
    canonicalize_tags(&env, &empty, || panic!("invalid"));
}

// ─── canonicalize_tags: batch behaviour ──────────────────────────────────────

/// Multiple tags in one batch are all individually normalized.
#[test]
fn test_canonicalize_multiple_tags_all_normalized() {
    let env = Env::default();
    let input = tags(&env, &["Travel", "FIRE", "long-term"]);
    let out = canonicalize_tags(&env, &input, || panic!("invalid"));
    assert_eq!(out.len(), 3);
    assert_eq!(get(&env, &out, 0), "travel");
    assert_eq!(get(&env, &out, 1), "fire");
    assert_eq!(get(&env, &out, 2), "long-term");
}

/// Output order matches input order.
#[test]
fn test_canonicalize_order_preserved() {
    let env = Env::default();
    let input = tags(&env, &["zebra", "apple", "mango"]);
    let out = canonicalize_tags(&env, &input, || panic!("invalid"));
    assert_eq!(get(&env, &out, 0), "zebra");
    assert_eq!(get(&env, &out, 1), "apple");
    assert_eq!(get(&env, &out, 2), "mango");
}

/// canonicalize_tags does NOT deduplicate: "Travel" and "travel" both become
/// "travel" and both appear in the output (len == 2, not 1).
/// Callers that need unique tags must deduplicate the result themselves.
#[test]
fn test_canonicalize_does_not_deduplicate() {
    let env = Env::default();
    let input = tags(&env, &["Travel", "travel"]);
    let out = canonicalize_tags(&env, &input, || panic!("invalid"));
    assert_eq!(
        out.len(),
        2,
        "canonicalize_tags must not deduplicate — deduplication is the caller's responsibility"
    );
    assert_eq!(get(&env, &out, 0), "travel");
    assert_eq!(get(&env, &out, 1), "travel");
}

/// One invalid tag in a batch causes on_invalid_char to fire even when
/// preceding tags in the same batch were valid.
#[test]
#[should_panic(expected = "invalid char")]
fn test_canonicalize_invalid_tag_in_batch_fires_callback() {
    let env = Env::default();
    // First tag is valid; second has a space.
    let input = tags(&env, &["valid", "bad tag"]);
    canonicalize_tags(&env, &input, || panic!("invalid char"));
}

// ─── canonicalize_tags_checked: success paths ──────────────────────────────

#[test]
fn test_checked_normalizes_valid_tags() {
    let env = Env::default();
    let input = tags(&env, &["Travel", "FIRE", "long-term"]);
    let out = canonicalize_tags_checked(&env, &input).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(get(&env, &out, 0), "travel");
    assert_eq!(get(&env, &out, 1), "fire");
    assert_eq!(get(&env, &out, 2), "long-term");
}

#[test]
fn test_checked_tag_exactly_32_chars_passes() {
    let env = Env::default();
    let tag = "abcdefghijklmnopqrstuvwxyzabcdef";
    assert_eq!(tag.len(), 32);
    let out = canonicalize_tags_checked(&env, &single(&env, tag)).unwrap();
    assert_eq!(get(&env, &out, 0), tag);
}

#[test]
fn test_checked_does_not_deduplicate() {
    let env = Env::default();
    let input = tags(&env, &["Travel", "travel"]);
    let out = canonicalize_tags_checked(&env, &input).unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(get(&env, &out, 0), "travel");
    assert_eq!(get(&env, &out, 1), "travel");
}

// ─── canonicalize_tags_checked: error paths ──────────────────────────────────

#[test]
fn test_checked_empty_batch_returns_empty() {
    let env = Env::default();
    let empty: Vec<String> = Vec::new(&env);
    assert_eq!(
        canonicalize_tags_checked(&env, &empty),
        Err(TagError::Empty)
    );
}

#[test]
fn test_checked_empty_string_tag_returns_empty() {
    let env = Env::default();
    assert_eq!(
        canonicalize_tags_checked(&env, &single(&env, "")),
        Err(TagError::Empty)
    );
}

#[test]
fn test_checked_tag_33_chars_returns_too_long() {
    let env = Env::default();
    let tag = "abcdefghijklmnopqrstuvwxyzabcdefg";
    assert_eq!(tag.len(), 33);
    assert_eq!(
        canonicalize_tags_checked(&env, &single(&env, tag)),
        Err(TagError::TooLong)
    );
}

#[test]
fn test_checked_invalid_char_at_position_zero() {
    let env = Env::default();
    assert_eq!(
        canonicalize_tags_checked(&env, &single(&env, "#savings")),
        Err(TagError::InvalidChar { position: 0 })
    );
}

#[test]
fn test_checked_invalid_char_at_last_position() {
    let env = Env::default();
    let tag = "valid!";
    let last = (tag.len() - 1) as u32;
    assert_eq!(
        canonicalize_tags_checked(&env, &single(&env, tag)),
        Err(TagError::InvalidChar { position: last })
    );
}

#[test]
fn test_checked_short_circuits_on_first_invalid_char() {
    let env = Env::default();
    // '!' is at position 3; a later space at position 4 must not be reported.
    assert_eq!(
        canonicalize_tags_checked(&env, &single(&env, "bad! tag")),
        Err(TagError::InvalidChar { position: 3 })
    );
}

#[test]
fn test_checked_invalid_tag_in_batch_short_circuits() {
    let env = Env::default();
    let input = tags(&env, &["valid", "bad tag"]);
    assert_eq!(
        canonicalize_tags_checked(&env, &input),
        Err(TagError::InvalidChar { position: 3 })
    );
}

#[test]
fn test_checked_empty_batch_before_length_check() {
    let env = Env::default();
    let empty: Vec<String> = Vec::new(&env);
    let err = canonicalize_tags_checked(&env, &empty).unwrap_err();
    assert_eq!(err, TagError::Empty);
}

// ─── canonicalise_symbol ──────────────────────────────────────────────────────

/// A deterministic helper: get string content from a Symbol for assertions.
/// Available only in non-WASM (test) builds via `ToString`.
fn symbol_str(sym: &Symbol) -> std::string::String {
    use std::string::ToString;
    sym.to_string()
}

/// Lowercase-only input passes through unchanged.
#[test]
fn test_canonicalise_symbol_lowercase_passthrough() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "hello");
}

/// Uppercase letters are folded to lowercase.
#[test]
fn test_canonicalise_symbol_uppercase_folded() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "HELLO");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "hello");
}

/// Mixed case is fully lowercased.
#[test]
fn test_canonicalise_symbol_mixed_case() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "HelloWorld");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "helloworld");
}

/// Leading whitespace is stripped.
#[test]
fn test_canonicalise_symbol_leading_whitespace_stripped() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "  hello");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "hello");
}

/// Trailing whitespace is stripped.
#[test]
fn test_canonicalise_symbol_trailing_whitespace_stripped() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello  ");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "hello");
}

/// Whitespace on both sides is stripped.
#[test]
fn test_canonicalise_symbol_surrounding_whitespace_stripped() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "  hello_World  ");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "hello_world");
}

/// Underscore is a valid Symbol character.
#[test]
fn test_canonicalise_symbol_underscore_allowed() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "my_symbol");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "my_symbol");
}

/// Digits are valid Symbol characters.
#[test]
fn test_canonicalise_symbol_digits_allowed() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "goal2025");
    let out = canonicalise_symbol(&env, &input);
    assert_eq!(symbol_str(&out), "goal2025");
}

/// Function is idempotent: applying twice yields the same Symbol.
#[test]
fn test_canonicalise_symbol_idempotent() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "  HeLLo_WORLD  ");
    let once = canonicalise_symbol(&env, &input);
    let once_str = symbol_str(&once);
    let twice_input = soroban_sdk::String::from_str(&env, &once_str);
    let twice = canonicalise_symbol(&env, &twice_input);
    assert_eq!(symbol_str(&once), symbol_str(&twice));
}

/// Space-only input panics.
#[test]
#[should_panic(expected = "non-whitespace character")]
fn test_canonicalise_symbol_whitespace_only_panics() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "   ");
    canonicalise_symbol(&env, &input);
}

/// Empty input panics.
#[test]
#[should_panic(expected = "symbol input must contain between 1 and 32 characters")]
fn test_canonicalise_symbol_empty_panics() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "");
    canonicalise_symbol(&env, &input);
}

/// Input with invalid Symbol character (hyphen) panics.
#[test]
#[should_panic]
fn test_canonicalise_symbol_hyphen_invalid() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello-world");
    canonicalise_symbol(&env, &input);
}

/// Input with invalid Symbol character (space inside) panics.
#[test]
#[should_panic]
fn test_canonicalise_symbol_internal_space_invalid() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello world");
    canonicalise_symbol(&env, &input);
}

proptest! {
    /// Property test for `canonicalise_symbol`.
    ///
    /// Pins the canonicalization contract:
    /// 1. **Idempotence** — applying the function twice yields the same Symbol.
    /// 2. **Whitespace stripping** — leading/trailing whitespace is removed.
    /// 3. **Case folding** — ASCII uppercase letters are lowered.
    /// 4. **Length discipline** — input must be 1..=32 bytes after trimming.
    #[test]
    fn proptest_canonicalise_symbol_contract(
        s in proptest::string::string_regex(" *[a-zA-Z0-9_]+ *").unwrap(),
    ) {
        let trimmed = s.trim();
        let trimmed_len = trimmed.len();
        // Skip strings longer than 32 bytes after trimming (Symbol max length).
        if trimmed_len > 32 {
            return Ok(());
        }

        let env = Env::default();
        let input = soroban_sdk::String::from_str(&env, &s);
        let once = canonicalise_symbol(&env, &input);
        let once_str = symbol_str(&once);

        // Property 1: idempotence — second application yields the same Symbol.
        let twice_input = soroban_sdk::String::from_str(&env, &once_str);
        let twice = canonicalise_symbol(&env, &twice_input);
        prop_assert_eq!(once.clone(), twice);

        // Property 2: whitespace was stripped — output equals
        // canonicalising the trimmed form directly.
        let trimmed_input = soroban_sdk::String::from_str(&env, trimmed);
        let from_trimmed = canonicalise_symbol(&env, &trimmed_input);
        prop_assert_eq!(once, from_trimmed);

        // Property 3: output contains only lowercase ASCII letters, digits, underscores.
        prop_assert!(
            once_str.len() == trimmed_len,
            "output length must equal trimmed input length (no chars added/removed)"
        );
        for b in once_str.bytes() {
            prop_assert!(
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_',
                "canonical output char {:?} is not in [a-z0-9_]",
                b as char
            );
        }
    }
}

// ─── canonicalise_symbol_checked ─────────────────────────────────────────────

/// Lowercase–only input is returned unchanged.
#[test]
fn test_checked_symbol_lowercase_passthrough() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "hello");
}

/// Uppercase letters are folded to lowercase.
#[test]
fn test_checked_symbol_uppercase_folded() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "HELLO");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "hello");
}

/// Mixed-case input is fully lowercased.
#[test]
fn test_checked_symbol_mixed_case_folded() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "HelloWorld");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "helloworld");
}

/// Leading whitespace is stripped.
#[test]
fn test_checked_symbol_leading_whitespace_stripped() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "  hello");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "hello");
}

/// Trailing whitespace is stripped.
#[test]
fn test_checked_symbol_trailing_whitespace_stripped() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello  ");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "hello");
}

/// Underscore is valid.
#[test]
fn test_checked_symbol_underscore_allowed() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "my_symbol");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "my_symbol");
}

/// Digits are valid.
#[test]
fn test_checked_symbol_digits_allowed() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "goal2025");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "goal2025");
}

/// Empty string returns Err(Empty).
#[test]
fn test_checked_symbol_empty_returns_err() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::Empty),
    );
}

/// Whitespace-only string returns Err(Empty).
#[test]
fn test_checked_symbol_whitespace_only_returns_err() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "   ");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::Empty),
    );
}

/// Trimmed input longer than 32 bytes returns Err(TooLong).
#[test]
fn test_checked_symbol_too_long_returns_err() {
    let env = Env::default();
    // 33 lowercase letters — one over the limit
    let input = soroban_sdk::String::from_str(&env, "abcdefghijklmnopqrstuvwxyzabcdefg");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::TooLong),
    );
}

/// Exactly 32 bytes (the boundary) succeeds.
#[test]
fn test_checked_symbol_exactly_32_bytes_passes() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "abcdefghijklmnopqrstuvwxyzabcdef");
    let out = canonicalise_symbol_checked(&env, &input).unwrap();
    assert_eq!(symbol_str(&out), "abcdefghijklmnopqrstuvwxyzabcdef");
}

/// Hyphen at position 5 returns Err(InvalidChar { position: 5 }).
#[test]
fn test_checked_symbol_hyphen_returns_invalid_char() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "hello-world");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::InvalidChar { position: 5 }),
    );
}

/// Internal space (after trimming) returns Err(InvalidChar) at the correct byte position.
#[test]
fn test_checked_symbol_internal_space_returns_invalid_char() {
    let env = Env::default();
    // leading space is trimmed; remaining "hello world" has space at position 5
    let input = soroban_sdk::String::from_str(&env, " hello world");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::InvalidChar { position: 5 }),
    );
}

/// `@` at position 0 returns Err(InvalidChar { position: 0 }).
#[test]
fn test_checked_symbol_at_sign_returns_invalid_char() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "@admin");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::InvalidChar { position: 0 }),
    );
}

/// Dot (`.`) at position 4 returns Err(InvalidChar { position: 4 }).
#[test]
fn test_checked_symbol_dot_returns_invalid_char() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "v1_0.1");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::InvalidChar { position: 4 }),
    );
}

/// Short-circuits on the FIRST bad byte, not the last.
#[test]
fn test_checked_symbol_short_circuits_on_first_invalid_char() {
    let env = Env::default();
    // '!' at index 3; '@' at index 5 — only position 3 should be reported
    let input = soroban_sdk::String::from_str(&env, "bad!xy@z");
    assert_eq!(
        canonicalise_symbol_checked(&env, &input),
        Err(SymbolValidationError::InvalidChar { position: 3 }),
    );
}

/// Idempotence: applying checked twice yields the same Symbol.
#[test]
fn test_checked_symbol_idempotent() {
    let env = Env::default();
    let input = soroban_sdk::String::from_str(&env, "  HeLLo_WORLD  ");
    let once = canonicalise_symbol_checked(&env, &input).unwrap();
    let once_str = symbol_str(&once);
    let twice_input = soroban_sdk::String::from_str(&env, &once_str);
    let twice = canonicalise_symbol_checked(&env, &twice_input).unwrap();
    assert_eq!(symbol_str(&once), symbol_str(&twice));
}

// ─── canonicalise_symbols (batch) ────────────────────────────────────────────

/// Empty batch returns Err(Empty).
#[test]
fn test_canonicalise_symbols_empty_batch_returns_err() {
    let env = Env::default();
    let empty: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    assert_eq!(
        canonicalise_symbols(&env, &empty),
        Err(SymbolValidationError::Empty),
    );
}

/// Valid batch — all symbols canonicalised in order.
#[test]
fn test_canonicalise_symbols_valid_batch() {
    let env = Env::default();
    let mut inputs: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    inputs.push_back(soroban_sdk::String::from_str(&env, "Hello"));
    inputs.push_back(soroban_sdk::String::from_str(&env, "WORLD"));
    inputs.push_back(soroban_sdk::String::from_str(&env, "my_key"));

    let out = canonicalise_symbols(&env, &inputs).unwrap();
    assert_eq!(out.len(), 3);
    assert_eq!(symbol_str(&out.get(0).unwrap()), "hello");
    assert_eq!(symbol_str(&out.get(1).unwrap()), "world");
    assert_eq!(symbol_str(&out.get(2).unwrap()), "my_key");
}

/// Order is preserved in the batch output.
#[test]
fn test_canonicalise_symbols_order_preserved() {
    let env = Env::default();
    let mut inputs: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    inputs.push_back(soroban_sdk::String::from_str(&env, "zebra"));
    inputs.push_back(soroban_sdk::String::from_str(&env, "apple"));
    inputs.push_back(soroban_sdk::String::from_str(&env, "mango"));

    let out = canonicalise_symbols(&env, &inputs).unwrap();
    assert_eq!(symbol_str(&out.get(0).unwrap()), "zebra");
    assert_eq!(symbol_str(&out.get(1).unwrap()), "apple");
    assert_eq!(symbol_str(&out.get(2).unwrap()), "mango");
}

/// First invalid element short-circuits the batch.
#[test]
fn test_canonicalise_symbols_invalid_element_short_circuits() {
    let env = Env::default();
    let mut inputs: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    inputs.push_back(soroban_sdk::String::from_str(&env, "valid_one"));
    inputs.push_back(soroban_sdk::String::from_str(&env, "bad-char")); // hyphen at pos 3
    inputs.push_back(soroban_sdk::String::from_str(&env, "valid_two"));

    assert_eq!(
        canonicalise_symbols(&env, &inputs),
        Err(SymbolValidationError::InvalidChar { position: 3 }),
    );
}

/// Too-long element in a batch returns Err(TooLong).
#[test]
fn test_canonicalise_symbols_too_long_element() {
    let env = Env::default();
    let mut inputs: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    inputs.push_back(soroban_sdk::String::from_str(&env, "ok"));
    inputs.push_back(soroban_sdk::String::from_str(
        &env,
        "abcdefghijklmnopqrstuvwxyzabcdefg",
    )); // 33 chars

    assert_eq!(
        canonicalise_symbols(&env, &inputs),
        Err(SymbolValidationError::TooLong),
    );
}

/// Single-element batch works end-to-end.
#[test]
fn test_canonicalise_symbols_single_element() {
    let env = Env::default();
    let mut inputs: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    inputs.push_back(soroban_sdk::String::from_str(&env, "  MyKey  "));
    let out = canonicalise_symbols(&env, &inputs).unwrap();
    assert_eq!(out.len(), 1);
    assert_eq!(symbol_str(&out.get(0).unwrap()), "mykey");
}

// ─── require_page_limit_within_bounds ───────────────────────────────────────

#[test]
fn test_require_page_limit_within_bounds_valid() {
    assert_eq!(require_page_limit_within_bounds(0), Ok(()));
    assert_eq!(require_page_limit_within_bounds(1), Ok(()));
    assert_eq!(require_page_limit_within_bounds(DEFAULT_PAGE_LIMIT), Ok(()));
    assert_eq!(require_page_limit_within_bounds(MAX_PAGE_LIMIT), Ok(()));
}

#[test]
fn test_require_page_limit_within_bounds_exceeded_negative() {
    assert_eq!(
        require_page_limit_within_bounds(MAX_PAGE_LIMIT + 1),
        Err(PageLimitError::LimitExceedsMax)
    );
    assert_eq!(
        require_page_limit_within_bounds(100),
        Err(PageLimitError::LimitExceedsMax)
    );
    assert_eq!(
        require_page_limit_within_bounds(u32::MAX),
        Err(PageLimitError::LimitExceedsMax)
    );
}

// ─── clamp_limit ─────────────────────────────────────────────────────────────

/// 0 is treated as "use default" and returns DEFAULT_PAGE_LIMIT.
#[test]
fn test_clamp_limit_zero_returns_default() {
    assert_eq!(clamp_limit(0), DEFAULT_PAGE_LIMIT);
}

/// 1 is within range and passes through.
#[test]
fn test_clamp_limit_one_passthrough() {
    assert_eq!(clamp_limit(1), 1);
}

/// A mid-range value passes through unchanged.
#[test]
fn test_clamp_limit_mid_range_passthrough() {
    assert_eq!(clamp_limit(25), 25);
}

/// MAX_PAGE_LIMIT itself passes through (inclusive upper bound).
#[test]
fn test_clamp_limit_max_page_limit_passthrough() {
    assert_eq!(clamp_limit(MAX_PAGE_LIMIT), MAX_PAGE_LIMIT);
}

/// One above MAX_PAGE_LIMIT is capped at MAX_PAGE_LIMIT.
#[test]
fn test_clamp_limit_one_above_max_clamped() {
    assert_eq!(clamp_limit(MAX_PAGE_LIMIT + 1), MAX_PAGE_LIMIT);
}

/// u32::MAX is capped at MAX_PAGE_LIMIT.
#[test]
fn test_clamp_limit_u32_max_clamped() {
    assert_eq!(clamp_limit(u32::MAX), MAX_PAGE_LIMIT);
}

proptest! {
    /// Property test for the shared pagination limit normalizer.
    ///
    /// This pins the full contract consumed by paginated reads across contracts:
    /// zero selects the default, oversized limits clamp to the maximum, in-range
    /// values pass through, output remains bounded, and normalization is idempotent.
    #[test]
    fn proptest_clamp_limit_contract(limit in any::<u32>()) {
        let clamped = clamp_limit(limit);

        if limit == 0 {
            prop_assert_eq!(clamped, DEFAULT_PAGE_LIMIT);
        } else if limit > MAX_PAGE_LIMIT {
            prop_assert_eq!(clamped, MAX_PAGE_LIMIT);
        } else {
            prop_assert_eq!(clamped, limit);
        }

        prop_assert!((1..=MAX_PAGE_LIMIT).contains(&clamped));
        prop_assert_eq!(clamp_limit(clamped), clamped);
    }

    /// Property test for cross-domain signature replay protection.
    ///
    /// Sign for domain A, replay against domain B — must fail.
    #[test]
    fn proptest_signature_signed_for_domain_a_replayed_against_domain_b_fails(
        domain_a in proptest::collection::vec(any::<u8>(), 8),
        domain_b in proptest::collection::vec(any::<u8>(), 8),
        msg in proptest::collection::vec(any::<u8>(), 1..64),
    ) {
        prop_assume!(domain_a != domain_b);

        let env = Env::default();
        let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
        let pk = sk.verifying_key().to_bytes();

        let mut prefixed = std::vec::Vec::new();
        prefixed.extend_from_slice(&domain_a);
        prefixed.extend_from_slice(&msg);
        let signature = sk.sign(&prefixed).to_bytes();

        let valid_res = verify_signature(&env, &domain_a, &msg, &signature, &pk);
        prop_assert_eq!(valid_res, Ok(()));

        let replay_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = verify_signature(&env, &domain_b, &msg, &signature, &pk);
        }));
        prop_assert!(replay_res.is_err(), "Signature signed for domain A replayed against domain B must fail");
    }
}

/// Explicit regression pin for the largest u32 input: it must clamp without
/// overflow or special-case caller handling.
#[test]
fn test_clamp_limit_u32_max_contract_regression() {
    let clamped = clamp_limit(u32::MAX);

    assert_eq!(clamped, MAX_PAGE_LIMIT);
    assert!((1..=MAX_PAGE_LIMIT).contains(&clamped));
    assert_eq!(clamp_limit(clamped), clamped);
}

// ─── Timestamp::to_period_key ────────────────────────────────────────────────

#[test]
fn test_period_key_day_and_week_buckets() {
    // Jan 1, 1970 UTC (epoch)
    let ts = 0u64;
    assert_eq!(Timestamp::to_period_key(ts, PeriodKind::Day), 0);
    assert_eq!(Timestamp::to_period_key(ts, PeriodKind::Week), 0);
    assert_eq!(Timestamp::to_period_key(ts + 86399, PeriodKind::Day), 0);
    assert_eq!(Timestamp::to_period_key(ts + 86400, PeriodKind::Day), 1);
    assert_eq!(Timestamp::to_period_key(ts + 604799, PeriodKind::Week), 0);
    assert_eq!(Timestamp::to_period_key(ts + 604800, PeriodKind::Week), 1);
}

#[test]
fn test_period_key_month_bucket_epoch_and_dec_2023() {
    // Epoch (Jan 1970)
    assert_eq!(Timestamp::to_period_key(0, PeriodKind::Month), 197001);
    // Dec 31, 2023 23:59:59 UTC
    let ts = 1704067199;
    assert_eq!(Timestamp::to_period_key(ts, PeriodKind::Month), 202312);
    // Jan 1, 2024 00:00:00 UTC
    let jan1_2024 = 1704067200;
    assert_eq!(
        Timestamp::to_period_key(jan1_2024, PeriodKind::Month),
        202401
    );
}

#[test]
fn test_period_key_exact_rollover_edges() {
    // Midnight UTC at 2021-02-28 to Mar 1st transition, and leap-year
    let feb_28_2020 = 1582848000; // 2020-02-28 00:00:00 UTC
    let feb_29_2020 = 1582934400; // 2020-02-29 00:00:00 UTC (leap)
    let mar_1_2020 = 1583020800; // 2020-03-01 00:00:00 UTC
    assert_eq!(
        Timestamp::to_period_key(feb_28_2020, PeriodKind::Month),
        202002
    );
    assert_eq!(
        Timestamp::to_period_key(feb_29_2020, PeriodKind::Month),
        202002
    );
    assert_eq!(
        Timestamp::to_period_key(mar_1_2020, PeriodKind::Month),
        202003
    );
}

#[test]
fn test_period_key_idempotent_and_monotonic_within_bucket() {
    // For any t in the same day/week/month, key is identical and monotonic within that bucket
    for period in [PeriodKind::Day, PeriodKind::Week, PeriodKind::Month] {
        let mut prev = None;
        for t in (1_700_000_000..1_700_010_000).step_by(101) {
            let key = Timestamp::to_period_key(t, period);
            if let Some(pkey) = prev {
                // Monotonic: never decreases
                assert!(key >= pkey);
                // Idempotent: next t in same bucket, key stays same or bumps by 1
                assert!(key == pkey || key == pkey + 1 || key == pkey); // Week/month boundaries are much further
            }
            prev = Some(key);
        }
    }
}

proptest! {
    #[test]
    fn proptest_period_key_roundtrips(t in 0u64..4660000000) { // up to year 2117
        use crate::PeriodKind::*;
        let day = Timestamp::to_period_key(t, Day);
        let week = Timestamp::to_period_key(t, Week);
        let month = Timestamp::to_period_key(t, Month);
        // Day bucket reconstructs the day-start timestamp
        let day_start = day * 86400;
        assert!(t >= day_start);
        assert!(t < day_start + 86400);
        // Week bucket reconstructs week-start timestamp
        let week_start = week * 604800;
        assert!(t >= week_start);
        assert!(t < week_start + 604800);
        // Month bucket is always >= 197001, never < 197001
        assert!(month >= 197001);
        let y = month / 100;
        let m = month % 100;
        assert!(y >= 1970 && m >= 1 && m <= 12);
    }
}

// Pins the "no Result, no panic" contract of `Timestamp::to_period_key`
// at the practical `u64::MAX` corner of the input domain.
//
// Day and Week are simple floor divisions and round-trip exactly. For Month
// we only assert well-formed YYYYMM (month component in `1..=12`); the
// numeric magnitude of the year component is implementation-defined and is
// not pinned here.
#[test]
fn test_period_key_u64_max_does_not_panic() {
    let ts = u64::MAX;

    // Day: timestamp / SECONDS_PER_DAY (floor).
    assert_eq!(
        Timestamp::to_period_key(ts, PeriodKind::Day),
        ts / SECONDS_PER_DAY
    );

    // Week: timestamp / SECONDS_PER_WEEK (floor).
    assert_eq!(
        Timestamp::to_period_key(ts, PeriodKind::Week),
        ts / SECONDS_PER_WEEK
    );

    // Month: YYYYMM must contain a valid calendar month component.
    let key = Timestamp::to_period_key(ts, PeriodKind::Month);
    let month = key % 100;
    assert!(
        (1..=12).contains(&month),
        "month component out of range ({}) for key={} at ts=u64::MAX",
        month,
        key
    );
}

// ─── Timestamp::seconds_until ────────────────────────────────────────────────

/// A future target returns the exact distance in seconds.
#[test]
fn test_timestamp_seconds_until_future_target() {
    assert_eq!(Timestamp::seconds_until(1_700_000_000, 1_700_000_300), 300);
}

/// A target equal to now has no remaining distance.
#[test]
fn test_timestamp_seconds_until_equal_target() {
    assert_eq!(Timestamp::seconds_until(1_700_000_000, 1_700_000_000), 0);
}

/// A past target saturates at zero instead of underflowing.
#[test]
fn test_timestamp_seconds_until_past_target_saturates() {
    assert_eq!(Timestamp::seconds_until(1_700_000_300, 1_700_000_000), 0);
}

/// The helper remains overflow-safe at the upper `u64` boundary.
#[test]
fn test_timestamp_seconds_until_u64_max_boundary() {
    assert_eq!(Timestamp::seconds_until(u64::MAX - 1, u64::MAX), 1);
}

/// Maximum possible distance: now at 0, target at `u64::MAX`.
/// Pins that the return value can represent the full `u64` range.
#[test]
fn test_timestamp_seconds_until_max_distance_zero_to_u64_max() {
    assert_eq!(Timestamp::seconds_until(0, u64::MAX), u64::MAX);
}

/// Max past: now at `u64::MAX`, target at 0 saturates at zero.
#[test]
fn test_timestamp_seconds_until_max_past_u64_max_to_zero_saturates() {
    assert_eq!(Timestamp::seconds_until(u64::MAX, 0), 0);
}

/// Equal timestamps at the maximum boundary return zero.
#[test]
fn test_timestamp_seconds_until_equal_at_u64_max_returns_zero() {
    assert_eq!(Timestamp::seconds_until(u64::MAX, u64::MAX), 0);
}

/// Base case: both now and target at epoch zero return zero.
#[test]
fn test_timestamp_seconds_until_zero_now_zero_target_returns_zero() {
    assert_eq!(Timestamp::seconds_until(0, 0), 0);
}

/// Near-max distance: now at 1, target at `u64::MAX`.
#[test]
fn test_timestamp_seconds_until_near_max_distance_from_one_to_u64_max() {
    assert_eq!(Timestamp::seconds_until(1, u64::MAX), u64::MAX - 1);
}

// ─── Current, Future, Past Temporal Tests (Timestamp & validate_period) ──────

/// validate_period returns Ok when start equals end (current/same timestamp).
#[test]
fn test_validate_period_returns_ok_for_current_timestamp_equal_start_end() {
    assert_eq!(validate_period(1_700_000_000, 1_700_000_000), Ok(()));
}

/// validate_period returns Ok when start is before end (future end timestamp).
#[test]
fn test_validate_period_returns_ok_for_future_end_timestamp() {
    assert_eq!(validate_period(1_700_000_000, 1_700_000_500), Ok(()));
}

/// validate_period returns Err(TimeError::InvalidPeriod) when start is after end (past end timestamp).
#[test]
fn test_validate_period_returns_err_invalid_period_for_past_end_timestamp() {
    assert_eq!(
        validate_period(1_700_000_500, 1_700_000_000),
        Err(TimeError::InvalidPeriod)
    );
}

/// validate_period boundary checks for 1 second difference (future vs past).
#[test]
fn test_validate_period_boundary_one_second_future_and_past() {
    let now = 1_700_000_000;
    // 1 second in the future is valid
    assert_eq!(validate_period(now, now + 1), Ok(()));
    // 1 second in the past relative to start is invalid
    assert_eq!(validate_period(now + 1, now), Err(TimeError::InvalidPeriod));
}

/// Timestamp::seconds_until explicit classification across current, future, and past.
#[test]
fn test_timestamp_seconds_until_current_future_past_boundaries() {
    let now = 1_700_000_000;
    // Current (now == target)
    assert_eq!(Timestamp::seconds_until(now, now), 0);
    // Future (now < target)
    assert_eq!(Timestamp::seconds_until(now, now + 100), 100);
    // Past (now > target)
    assert_eq!(Timestamp::seconds_until(now + 100, now), 0);
}

proptest! {
    /// Property test pinning `Timestamp::seconds_until` behavior across current, future, and past targets.
    #[test]
    fn proptest_timestamp_seconds_until_current_future_past(
        now in any::<u64>(),
        target in any::<u64>(),
    ) {
        let result = Timestamp::seconds_until(now, target);
        if target > now {
            // Future target: returns exact positive distance
            prop_assert_eq!(result, target - now);
            prop_assert!(result > 0);
        } else if target == now {
            // Current target: returns zero
            prop_assert_eq!(result, 0);
        } else {
            // Past target: saturates at zero
            prop_assert_eq!(result, 0);
        }
    }

    /// Property test pinning `validate_period` behavior across current, future, and past ordering.
    #[test]
    fn proptest_validate_period_current_future_past(
        start in any::<u64>(),
        end in any::<u64>(),
    ) {
        let res = validate_period(start, end);
        if start <= end {
            // Current (start == end) or Future (start < end) range: valid
            prop_assert_eq!(res, Ok(()));
        } else {
            // Past (start > end) range: invalid period error
            prop_assert_eq!(res, Err(TimeError::InvalidPeriod));
        }
    }
}

// ─── verify_signature tests ──────────────────────────────────────────────────

// `register_verifier`/`require_registered_verifier` read and write instance
// storage, which the Soroban host only allows inside a contract's execution
// context. Tests exercising them run their storage-touching calls inside
// `env.as_contract(&contract_id, || { .. })` against this no-op contract,
// mirroring the pattern already used in `orchestrator/src/test.rs`.
#[contract]
struct VerifierTestContract;

#[contractimpl]
impl VerifierTestContract {}

#[test]
fn test_verify_signature_valid() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let prefixed = prefixed_message(domain, message);
    let signature = sk.sign(&prefixed).to_bytes();

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        let result = verify_signature(&env, domain, message, &signature, &pk);
        assert_eq!(result, Ok(()));
    });
}

#[test]
fn test_verify_signature_rejects_unregistered_verifier() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(domain);
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    env.as_contract(&contract_id, || {
        let result = verify_signature(&env, domain, message, &signature, &pk);
        assert_eq!(result, Err(SignatureError::UnregisteredVerifier));
    });
}

#[test]
#[should_panic]
fn test_verify_signature_invalid_signature() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let invalid_signature = [0u8; 64];

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        let _ = verify_signature(&env, domain, message, &invalid_signature, &pk);
    });
}

#[test]
fn test_verify_signature_invalid_signature_length() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let short_signature = [0u8; 32];

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        let result = verify_signature(&env, domain, message, &short_signature, &pk);
        assert_eq!(result, Err(SignatureError::InvalidSignatureLength));
    });
}

#[test]
fn test_verify_signature_invalid_public_key_length() {
    let env = Env::default();
    let domain = b"test-domain";
    let message = b"hello world";

    let short_pk = [0u8; 16];
    let signature = [0u8; 64];

    // Invalid key length is rejected before any storage access, so this
    // does not need a contract context.
    let result = verify_signature(&env, domain, message, &signature, &short_pk);
    assert_eq!(result, Err(SignatureError::InvalidPublicKeyLength));
}

#[test]
#[should_panic]
fn test_verify_signature_wrong_domain() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain1 = b"domain1";
    let domain2 = b"domain2";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let prefixed = prefixed_message(domain1, message);
    let signature = sk.sign(&prefixed).to_bytes();

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        let _ = verify_signature(&env, domain2, message, &signature, &pk);
    });
}

/// Regression test for the "test signer on prod" gap: a verifier public key
/// registered while the contract instance observed one network (e.g. Testnet)
/// must NOT be accepted once the same storage is read under a different
/// network (e.g. Public/Mainnet), even though the key itself is unchanged.
///
/// This simulates a verifier registry entry that ended up on the wrong
/// deployment (e.g. via a copy-pasted `REMITWISE_ACTIVE_VERIFIERS` config, or
/// a snapshot import) by registering under one `network_id` and then mutating
/// the ledger's `network_id` before verifying, all against the same
/// underlying instance storage.
///
/// Before the fix: `require_registered_verifier` only tracked `bool`
/// membership, so this passed regardless of network. This test fails against
/// that behavior and passes once registration is bound to `network_id`.
#[test]
fn test_verify_signature_rejects_verifier_from_different_network() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(domain);
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    env.as_contract(&contract_id, || {
        // Register the verifier while the contract instance observes "testnet".
        set_network(&env, [7u8; 32]);
        register_verifier(&env, &pk).unwrap();

        // Same storage, but the instance is now running on a different
        // network ("mainnet") — the registration above must no longer be
        // honored.
        set_network(&env, [9u8; 32]);
        let result = require_registered_verifier(&env, &pk);
        assert_eq!(result, Err(SignatureError::VerifierNetworkMismatch));

        let result = verify_signature(&env, domain, message, &signature, &pk);
        assert_eq!(result, Err(SignatureError::VerifierNetworkMismatch));
    });
}

/// Sign for domain A, replay against domain B — must fail.
#[test]
#[should_panic]
fn test_sign_for_domain_a_replay_against_domain_b_fails() {
    let env = Env::default();
    let domain_a = b"domain-A-auth-v1";
    let domain_b = b"domain-B-auth-v1";
    let message = b"transfer 1000 USDC to account X";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(domain_a);
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    assert_eq!(
        verify_signature(&env, domain_a, message, &signature, &pk),
        Ok(())
    );
    let _ = verify_signature(&env, domain_b, message, &signature, &pk);
}

#[test]
fn test_verify_signature_rejects_adjacent_domain_message_collision() {
    let env = Env::default();
    let domain1 = b"abc";
    let message1 = b"def";
    let domain2 = b"ab";
    let message2 = b"cdef";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let signature = sk.sign(&prefixed_message(domain1, message1)).to_bytes();

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = verify_signature(&env, domain2, message2, &signature, &pk);
    }));

    assert!(outcome.is_err(), "adjacent payload bytes must not verify");
}

#[test]
fn test_verify_slash_signature_valid() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let message = b"slash payload";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(b"slash-auth");
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        // Verify the slash signature
        let result = verify_slash_signature(&env, message, Some(&signature), &pk);
        assert_eq!(result, Ok(()));
    });
}

#[test]
fn test_verify_slash_signature_optional_none() {
    let env = Env::default();
    let message = b"slash payload";
    let pk = [0u8; 32];

    // Optional-signature short-circuit never touches storage, so this does
    // not need a contract context.
    let result = verify_slash_signature(&env, message, None, &pk);
    assert_eq!(result, Ok(()));
}

#[test]
#[should_panic]
fn test_verify_slash_signature_invalid() {
    let env = Env::default();
    let contract_id = env.register_contract(None, VerifierTestContract);
    let message = b"slash payload";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let invalid_signature = [0u8; 64]; // Invalid

    env.as_contract(&contract_id, || {
        register_verifier(&env, &pk).unwrap();

        // Verify the invalid slash signature
        let result = verify_slash_signature(&env, message, Some(&invalid_signature), &pk);
        assert_eq!(result, Err(SlashError::InvalidSignature));
    });
}

// ─── cross_contract_epoch tests ──────────────────────────────────────────

#[test]
fn test_require_matching_cross_contract_epoch() {
    let env = Env::default();

    // Default is 0, so epoch 0 matches
    assert_eq!(require_matching_cross_contract_epoch(&env, 0), Ok(()));

    // Set epoch to 5
    env.storage()
        .instance()
        .set(&STORAGE_CROSS_CONTRACT_EPOCH, &5u64);

    // Exact match is accepted
    assert_eq!(require_matching_cross_contract_epoch(&env, 5), Ok(()));

    // Stale epoch (less than current) is rejected
    assert_eq!(
        require_matching_cross_contract_epoch(&env, 4),
        Err(CrossContractEpochError::EpochMismatch)
    );

    // Future epoch (greater than current) is rejected
    assert_eq!(
        require_matching_cross_contract_epoch(&env, 6),
        Err(CrossContractEpochError::EpochMismatch)
    );
}

// ─── distribute_pro_rata tests (#1085) ───────────────────────────────────────

/// Distributes an indivisible total across unequal weights — remainder benefits smallest recipient.
#[test]
fn distributes_indivisible_total_with_remainder_to_last_bucket() {
    let mut out = [0i128; 4];
    distribute_pro_rata(100, &[50, 30, 15, 5], 100, &mut out);

    // First three buckets receive their floor share
    assert_eq!(out[0], 50); // 100 * 50 / 100 = 50
    assert_eq!(out[1], 30); // 100 * 30 / 100 = 30
    assert_eq!(out[2], 15); // 100 * 15 / 100 = 15
    assert_eq!(out[3], 5); // 100 * 5 / 100 = 5, plus remainder 0

    // Conservation: sum equals input total
    assert_eq!(out.iter().sum::<i128>(), 100);
}

/// Distributes an indivisible amount where rounding creates a remainder.
#[test]
fn distributes_amount_with_non_zero_remainder_to_last_bucket() {
    let mut out = [0i128; 3];
    // 10 * 3333 / 10000 = 3.333 → floor = 3 (per bucket)
    // Allocated: 3 + 3 = 6, remainder: 10 - 6 = 4 goes to last bucket
    distribute_pro_rata(10, &[3333, 3333, 3334], 10_000, &mut out);

    assert_eq!(out[0], 3); // floor(10 * 3333 / 10000) = 3
    assert_eq!(out[1], 3); // floor(10 * 3333 / 10000) = 3
    assert_eq!(out[2], 4); // 10 - 3 - 3 = 4 (includes remainder)

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 10);
}

/// Last bucket receives extra units when total does not divide evenly.
#[test]
fn last_bucket_receives_rounding_remainder() {
    let mut out = [0i128; 4];
    // 1_000_007 * 2500 / 10000 = 250001.75 → floor = 250001
    // Four buckets, each gets floor share, last gets remainder
    distribute_pro_rata(1_000_007, &[2500, 2500, 2500, 2500], 10_000, &mut out);

    // First three buckets
    assert_eq!(out[0], 250001);
    assert_eq!(out[1], 250001);
    assert_eq!(out[2], 250001);

    // Last bucket gets remainder: 1_000_007 - 3*250001 = 1_000_007 - 750_003 = 250_004
    assert_eq!(out[3], 250_004);

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 1_000_007);
}

/// Distributes a perfectly divisible total with no remainder.
#[test]
fn distributes_evenly_divisible_total_exactly() {
    let mut out = [0i128; 4];
    distribute_pro_rata(1_000_000, &[5000, 3000, 1500, 500], 10_000, &mut out);

    assert_eq!(out[0], 500_000); // 1M * 5000 / 10000
    assert_eq!(out[1], 300_000); // 1M * 3000 / 10000
    assert_eq!(out[2], 150_000); // 1M * 1500 / 10000
    assert_eq!(out[3], 50_000); // 1M * 500 / 10000

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 1_000_000);
}

/// Distributes entire amount to a single recipient.
#[test]
fn distributes_full_amount_to_single_recipient() {
    let mut out = [0i128; 1];
    distribute_pro_rata(999_999, &[100], 100, &mut out);

    assert_eq!(out[0], 999_999);

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 999_999);
}

/// Distributes zero total — all recipients receive zero.
#[test]
fn distributes_zero_total_without_panic() {
    let mut out = [0i128; 4];
    distribute_pro_rata(0, &[25, 25, 25, 25], 100, &mut out);

    assert_eq!(out[0], 0);
    assert_eq!(out[1], 0);
    assert_eq!(out[2], 0);
    assert_eq!(out[3], 0);

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 0);
}

/// Distributes when one weight is zero — zero-weight recipient receives nothing.
#[test]
fn distributes_with_zero_weight_recipient() {
    let mut out = [0i128; 4];
    distribute_pro_rata(100, &[50, 30, 20, 0], 100, &mut out);

    assert_eq!(out[0], 50);
    assert_eq!(out[1], 30);
    assert_eq!(out[2], 20);
    assert_eq!(out[3], 0); // zero weight → receives only remainder (0 in this case)

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 100);
}

/// Distributes when last weight is zero and remainder exists.
#[test]
fn zero_weight_last_bucket_receives_remainder() {
    let mut out = [0i128; 3];
    // 10 * 4000 / 10000 = 4, twice = 8
    // Remainder: 10 - 8 = 2 goes to last bucket (even though its weight is 0)
    distribute_pro_rata(10, &[4000, 4000, 0], 10_000, &mut out);

    assert_eq!(out[0], 4);
    assert_eq!(out[1], 4);
    assert_eq!(out[2], 2); // weight is 0, but receives remainder 2

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 10);
}

/// Distributes using basis points (10_000 = 100%).
#[test]
fn distributes_using_basis_points_denomination() {
    let mut out = [0i128; 4];
    // 5% = 500 bps, 3% = 300 bps, 1.5% = 150 bps, 0.5% = 50 bps
    distribute_pro_rata(1_000_000, &[500, 300, 150, 50], 10_000, &mut out);

    assert_eq!(out[0], 50_000); // 5%
    assert_eq!(out[1], 30_000); // 3%
    assert_eq!(out[2], 15_000); // 1.5%
    assert_eq!(out[3], 5_000); // 0.5%

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), 100_000); // only 10% of total distributed
}

/// Conservation invariant holds for large total near i128 upper range.
#[test]
fn conservation_holds_for_large_total() {
    let mut out = [0i128; 4];
    let large_total = i128::MAX / 1_000_000; // Large but won't overflow in multiplication

    distribute_pro_rata(large_total, &[2500, 2500, 2500, 2500], 10_000, &mut out);

    // Conservation
    assert_eq!(out.iter().sum::<i128>(), large_total);

    // Each bucket gets approximately 1/4
    let expected_floor = large_total / 4;
    assert!(out[0] >= expected_floor);
    assert!(out[1] >= expected_floor);
    assert!(out[2] >= expected_floor);
    // Last bucket absorbs remainder
    assert!(out[3] >= expected_floor);
}

proptest! {
    /// Property test: conservation and smallest-benefits-from-rounding invariants.
    ///
    /// For any valid input (non-negative total, positive total_weight, non-empty weights),
    /// the following properties must hold:
    /// 1. **Conservation**: sum(out) == total
    /// 2. **Non-negative outputs**: every output >= 0
    /// 3. **Last bucket absorbs remainder**: out[last] >= floor(total * weight[last] / total_weight)
    /// 4. **Bounded outputs**: each out[i] <= total
    #[test]
    fn proptest_distribute_pro_rata_conservation_and_rounding(
        total in 0i128..=i128::MAX / 1_000_000, // Avoid overflow in intermediate products
        weights in proptest::collection::vec(1u32..=1000u32, 1..=10),
    ) {
        let total_weight: u32 = weights.iter().copied().sum();
        if total_weight == 0 {
            return Ok(()); // Skip invalid input
        }

        let mut out = std::vec![0i128; weights.len()];
        distribute_pro_rata(total, &weights, total_weight, &mut out);

        // Property 1: Conservation — sum equals input
        prop_assert_eq!(out.iter().sum::<i128>(), total);

        // Property 2: Non-negative outputs
        for &amount in &out {
            prop_assert!(amount >= 0, "output must be non-negative");
        }

        // Property 3: Last bucket receives at least its floor share (absorbs remainder)
        let last_idx = weights.len() - 1;
        let last_floor = total
            .saturating_mul(weights[last_idx] as i128)
            .saturating_div(total_weight as i128);
        prop_assert!(
            out[last_idx] >= last_floor,
            "last bucket must receive at least floor share (absorbs rounding remainder)"
        );

        // Property 4: No output exceeds total
        for &amount in &out {
            prop_assert!(amount <= total, "no bucket can exceed total");
        }
    }
}

// ─── require_supported_rate_unit tests (#1188) ───────────────────────────────

#[test]
fn test_require_supported_rate_unit_accepts_basis_points() {
    assert_eq!(require_supported_rate_unit(1), Ok(RateUnit::BasisPoints));
    assert_eq!(Rate::try_from_input(500, 1), Ok(Rate::from_bps(500)));
}

#[test]
fn test_require_supported_rate_unit_rejects_unsupported_unit() {
    assert_eq!(
        require_supported_rate_unit(2),
        Err(RateUnitError::UnsupportedRateUnit)
    );
    assert_eq!(
        Rate::try_from_input(500, 2),
        Err(RateUnitError::UnsupportedRateUnit)
    );
}

// ─── require_valid_symbol_length tests (#1078) ───────────────────────────────

/// A 9-char short symbol (at the boundary) passes validation.
#[test]
fn test_require_valid_symbol_length_9_chars_passes() {
    let env = Env::default();
    let sym = Symbol::new(&env, "boundary9"); // exactly 9 chars
    assert_eq!(require_valid_symbol_length(&env, &sym), Ok(()));
}

/// A 1-char symbol passes validation.
#[test]
fn test_require_valid_symbol_length_single_char_passes() {
    let env = Env::default();
    let sym = Symbol::new(&env, "a");
    assert_eq!(require_valid_symbol_length(&env, &sym), Ok(()));
}

/// An empty symbol (0 bytes) passes validation (it's under 9).
#[test]
fn test_require_valid_symbol_length_empty_passes() {
    let env = Env::default();
    let sym = Symbol::new(&env, "");
    assert_eq!(require_valid_symbol_length(&env, &sym), Ok(()));
}

/// A 10-char symbol (one past the short-symbol boundary) is rejected.
#[test]
fn test_require_valid_symbol_length_10_chars_rejected() {
    let env = Env::default();
    let sym = Symbol::new(&env, "boundary10"); // exactly 10 chars
    assert_eq!(
        require_valid_symbol_length(&env, &sym),
        Err(SymbolError::SymbolTooLong)
    );
}

/// A 32-char symbol (at the SDK's hard limit) is rejected because it's
/// still past the 9-char short-symbol boundary.
#[test]
fn test_require_valid_symbol_length_32_chars_rejected() {
    let env = Env::default();
    let sym = Symbol::new(&env, "abcdefghijklmnopqrstuvwxyzabcd"); // 32 chars
    assert_eq!(
        require_valid_symbol_length(&env, &sym),
        Err(SymbolError::SymbolTooLong)
    );
}

/// A short 4-char symbol passes validation.
#[test]
fn test_require_valid_symbol_length_short_passes() {
    let env = Env::default();
    let sym = Symbol::new(&env, "test"); // 4 chars, well within limit
    assert_eq!(require_valid_symbol_length(&env, &sym), Ok(()));
}

/// A typical storage-key style symbol (≤ 9 chars) passes.
#[test]
fn test_require_valid_symbol_length_storage_key_passes() {
    let env = Env::default();
    let sym = Symbol::new(&env, "ADMIN"); // 5 chars
    assert_eq!(require_valid_symbol_length(&env, &sym), Ok(()));
}

/// The error discriminant is stable (pinned to 1) via `#[contracterror]`.
/// This test checks that the enum round-trips through Val encoding.
#[test]
fn test_symbol_error_encoding_stability() {
    use soroban_sdk::TryFromVal;
    let env = Env::default();
    let val: soroban_sdk::Val = soroban_sdk::IntoVal::into_val(&SymbolError::SymbolTooLong, &env);
    let err: SymbolError =
        <SymbolError as TryFromVal<Env, soroban_sdk::Val>>::try_from_val(&env, &val)
            .expect("SymbolError must round-trip through Val");
    assert_eq!(err, SymbolError::SymbolTooLong);
}

// ============================================================================
// canonicalize_tags_checked — untrusted caller tests (#1034)
// ============================================================================

#[test]
fn test_canonicalize_tags_checked_returns_ok_for_valid_tags() {
    let env = Env::default();
    let tags = soroban_sdk::vec![
        &env,
        soroban_sdk::String::from_str(&env, "payments"),
        soroban_sdk::String::from_str(&env, "SAVINGS"),
    ];
    let result = canonicalize_tags_checked(&env, &tags);
    assert!(result.is_ok());
    let out = result.unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(
        out.get(1).unwrap(),
        soroban_sdk::String::from_str(&env, "savings")
    );
}

#[test]
fn test_canonicalize_tags_checked_returns_err_for_empty_batch() {
    let env = Env::default();
    let tags: soroban_sdk::Vec<soroban_sdk::String> = soroban_sdk::Vec::new(&env);
    let result = canonicalize_tags_checked(&env, &tags);
    assert_eq!(result, Err(crate::TagError::Empty));
}

#[test]
fn test_canonicalize_tags_checked_returns_err_for_empty_tag_string() {
    let env = Env::default();
    let tags = soroban_sdk::vec![&env, soroban_sdk::String::from_str(&env, "")];
    let result = canonicalize_tags_checked(&env, &tags);
    assert_eq!(result, Err(crate::TagError::Empty));
}

#[test]
fn test_canonicalize_tags_checked_returns_err_for_tag_too_long() {
    let env = Env::default();
    let long_tag = "a".repeat((crate::TAG_MAX_LEN + 1) as usize);
    let tags = soroban_sdk::vec![&env, soroban_sdk::String::from_str(&env, &long_tag)];
    let result = canonicalize_tags_checked(&env, &tags);
    assert_eq!(result, Err(crate::TagError::TooLong));
}

#[test]
fn test_canonicalize_tags_checked_returns_invalid_char_for_untrusted_input() {
    let env = Env::default();
    // Space is not in the allowed charset — should return InvalidChar, not panic.
    let tags = soroban_sdk::vec![&env, soroban_sdk::String::from_str(&env, "bad tag")];
    let result = canonicalize_tags_checked(&env, &tags);
    assert!(matches!(result, Err(crate::TagError::InvalidChar { .. })));
}

#[test]
fn test_canonicalize_tags_checked_does_not_panic_on_injected_special_chars() {
    let env = Env::default();
    // Callers from untrusted sources (e.g., indexer input) must get Result not panic.
    let tags = soroban_sdk::vec![&env, soroban_sdk::String::from_str(&env, "=formula")];
    let result = canonicalize_tags_checked(&env, &tags);
    // '=' is not in [a-z0-9-_], so it must return InvalidChar.
    assert!(matches!(
        result,
        Err(crate::TagError::InvalidChar { position: 0 })
    ));
}

// ============================================================================
// require_active_pause_channel tests
// ============================================================================

#[test]
fn test_require_active_pause_channel_uninitialized() {
    let env = Env::default();
    // Map doesn't exist yet, should not panic
    crate::require_active_pause_channel(&env, symbol_short!("PAYMENTS"));
}

#[test]
fn test_require_active_pause_channel_active() {
    let env = Env::default();
    let mut map = soroban_sdk::Map::<soroban_sdk::Symbol, bool>::new(&env);
    map.set(symbol_short!("PAYMENTS"), false);
    env.storage().instance().set(
        &soroban_sdk::Symbol::new(&env, crate::STORAGE_PAUSE_CHANNELS),
        &map,
    );

    // Channel is active (false), should not panic
    crate::require_active_pause_channel(&env, symbol_short!("PAYMENTS"));
}

#[test]
#[should_panic(expected = "Pause channel is inactive")]
fn test_require_active_pause_channel_paused() {
    let env = Env::default();
    let mut map = soroban_sdk::Map::<soroban_sdk::Symbol, bool>::new(&env);
    map.set(symbol_short!("PAYMENTS"), true);
    env.storage().instance().set(
        &soroban_sdk::Symbol::new(&env, crate::STORAGE_PAUSE_CHANNELS),
        &map,
    );

    // Channel is paused (true), should panic
    crate::require_active_pause_channel(&env, symbol_short!("PAYMENTS"));
}

// ============================================================================
// Type-safe Percent -> Basis Points conversion tests
// ============================================================================

#[test]
fn test_bps_per_percent_constants() {
    assert_eq!(BPS_PER_PERCENT, 100);
    assert_eq!(BASIS_POINTS_PER_PERCENT, 100);
    assert_eq!(BASIS_POINTS, 10_000);
    assert_eq!(BASIS_POINTS / BPS_PER_PERCENT, 100);
}

#[test]
fn test_rate_from_percent() {
    assert_eq!(Rate::from_percent(0), Ok(Rate::from_bps(0)));
    assert_eq!(Rate::from_percent(1), Ok(Rate::from_bps(100)));
    assert_eq!(Rate::from_percent(5), Ok(Rate::from_bps(500)));
    assert_eq!(Rate::from_percent(50), Ok(Rate::from_bps(5_000)));
    assert_eq!(Rate::from_percent(100), Ok(Rate::from_bps(10_000)));
    assert_eq!(Rate::from_percent(500), Ok(Rate::from_bps(50_000)));
    assert_eq!(
        Rate::from_percent(u32::MAX / 100),
        Ok(Rate::from_bps((u32::MAX / 100) * 100))
    );
    assert_eq!(
        Rate::from_percent((u32::MAX / 100) + 1),
        Err(RateError::Overflow)
    );
    assert_eq!(Rate::from_percent(u32::MAX), Err(RateError::Overflow));
}

#[test]
fn test_rate_from_percent_boundaries() {
    // 0%
    assert_eq!(Rate::from_percent(0), Ok(Rate::from_bps(0)));

    // 0.01% is 1 basis point. `from_percent` only takes whole percentages,
    // so fractional percentages must be constructed via `from_bps`.
    let point_zero_one = Rate::from_bps(1);
    assert!(point_zero_one.has_fractional_percent());
    assert_eq!(point_zero_one.to_percent(), 0);

    // 100%
    assert_eq!(Rate::from_percent(100), Ok(Rate::from_bps(10_000)));

    // 100.01% is 10,001 basis points.
    let hundred_point_zero_one = Rate::from_bps(10_001);
    assert!(hundred_point_zero_one.has_fractional_percent());
    assert_eq!(hundred_point_zero_one.to_percent(), 100);
}

#[test]
fn test_rate_to_percent_and_fractional() {
    let rate_0 = Rate::from_bps(0);
    assert_eq!(rate_0.to_percent(), 0);
    assert!(!rate_0.has_fractional_percent());

    let rate_500 = Rate::from_bps(500); // 5%
    assert_eq!(rate_500.to_percent(), 5);
    assert!(!rate_500.has_fractional_percent());

    let rate_550 = Rate::from_bps(550); // 5.5%
    assert_eq!(rate_550.to_percent(), 5); // truncated
    assert!(rate_550.has_fractional_percent());

    let rate_1 = Rate::from_bps(1); // 0.01%
    assert_eq!(rate_1.to_percent(), 0);
    assert!(rate_1.has_fractional_percent());
}

#[test]
fn test_percent_type_conversions() {
    let p0 = Percent::ZERO;
    assert_eq!(p0.to_percentage(), 0);
    assert_eq!(p0.to_rate(), Ok(Rate::ZERO));
    assert_eq!(p0.to_bps(), Ok(0));

    let p5 = Percent::from_percentage(5);
    assert_eq!(p5.to_percentage(), 5);
    assert_eq!(p5.to_rate(), Ok(Rate::from_bps(500)));
    assert_eq!(p5.to_bps(), Ok(500));

    let p100 = Percent::HUNDRED;
    assert_eq!(p100.to_percentage(), 100);
    assert_eq!(p100.to_rate(), Ok(Rate::from_bps(10_000)));
    assert_eq!(p100.to_bps(), Ok(10_000));

    let rate_from_p: Result<Rate, RateError> = p5.try_into();
    assert_eq!(rate_from_p, Ok(Rate::from_bps(500)));

    let rate_from_type = Rate::from_percent_type(p5);
    assert_eq!(rate_from_type, Ok(Rate::from_bps(500)));

    let p_overflow = Percent::from_percentage(u32::MAX);
    assert_eq!(p_overflow.to_rate(), Err(RateError::Overflow));
    assert_eq!(p_overflow.to_bps(), Err(RateError::Overflow));
}

#[test]
fn test_verify_config_migration() {
    use super::{verify_config_migration, MigrationError, CONTRACT_VERSION};
    // Current version and newer versions must pass
    assert_eq!(verify_config_migration(CONTRACT_VERSION), Ok(()));
    assert_eq!(verify_config_migration(CONTRACT_VERSION + 1), Ok(()));

    // Older versions must return an error (negative test)
    if CONTRACT_VERSION > 0 {
        assert_eq!(
            verify_config_migration(CONTRACT_VERSION - 1),
            Err(MigrationError::OutdatedVersion)
        );
    }
}

proptest! {
    #[test]
    fn proptest_percent_rate_roundtrip(pct in 0u32..=(u32::MAX / 100)) {
        let rate = Rate::from_percent(pct).unwrap();
        prop_assert_eq!(rate.to_percent(), pct);
        prop_assert_eq!(rate.to_bps(), pct * 100);
        prop_assert!(!rate.has_fractional_percent());

        let p = Percent::from_percentage(pct);
        prop_assert_eq!(p.to_rate(), Ok(rate));
        prop_assert_eq!(p.to_bps(), Ok(pct * 100));
    }
}

// ─── require_valid_symbol_name_length ─────────────────────────────────────────
//
// These tests lock in the boundary contract for [`require_valid_symbol_name_length`]:
//
// - Empty input (0 bytes)  → Err(SymbolLengthError::Empty)
// - 1-byte input           → Ok(())   (lower inclusive boundary)
// - 9-byte input           → Ok(())   (upper inclusive boundary for symbol_short!)
// - 10-byte input          → Err(SymbolLengthError::TooLong)  (one past the cap)
//
// The 9-byte cap matches the Soroban SDK `symbol_short!` macro constraint that
// is enforced across all storage keys in this workspace (see STORAGE_LAYOUT.md
// and `testutils/tests/storage_key_naming_test.rs`).  These tests are purely
// concerned with the project-level validation function, not with the SDK macro
// itself (which has its own coverage in symbol_length_boundary_test.rs).

/// Empty byte slice is rejected with Empty.
#[test]
fn require_valid_symbol_length_empty_input_returns_empty_error() {
    assert_eq!(
        require_valid_symbol_length_bytes(b""),
        Err(SymbolLengthError::Empty),
        "empty name must be rejected with SymbolLengthError::Empty"
    );
}

/// A single-byte name is the smallest valid symbol and must be accepted.
#[test]
fn require_valid_symbol_length_one_char_returns_ok() {
    assert_eq!(
        require_valid_symbol_length_bytes(b"A"),
        Ok(()),
        "1-byte name is the lower boundary and must be accepted"
    );
}

/// A 9-byte name is the upper boundary accepted by `symbol_short!` and must pass.
#[test]
fn require_valid_symbol_length_nine_chars_returns_ok() {
    // Exactly SYMBOL_SHORT_MAX_LEN bytes.
    const NAME: &[u8] = b"NINE_BYTE"; // 9 bytes
    const _: () = assert!(NAME.len() == 9);
    assert_eq!(
        require_valid_symbol_length_bytes(NAME),
        Ok(()),
        "9-byte name is exactly at the symbol_short! cap and must be accepted"
    );
}

/// A 10-byte name is one past the `symbol_short!` cap and must be rejected.
#[test]
fn require_valid_symbol_length_ten_chars_returns_too_long_error() {
    const NAME: &[u8] = b"TEN_BYTES_"; // 10 bytes
    const _: () = assert!(NAME.len() == 10);
    assert_eq!(
        require_valid_symbol_length_bytes(NAME),
        Err(SymbolLengthError::TooLong),
        "10-byte name exceeds the symbol_short! cap and must be rejected with SymbolLengthError::TooLong"
    );
}

/// Additional boundary: names much longer than the cap are also rejected.
#[test]
fn require_valid_symbol_length_very_long_input_returns_too_long_error() {
    let name = b"TOOLONGKEYNAME"; // 14 bytes
    assert_eq!(
        require_valid_symbol_length_bytes(name),
        Err(SymbolLengthError::TooLong),
        "names well above the cap must also be rejected with SymbolLengthError::TooLong"
    );
}

// ─── same_address tests (#1141) ───────────────────────────────────────────────

/// Two references to the same address value return `true`.
#[test]
fn test_same_address_equal_returns_true() {
    use soroban_sdk::testutils::Address as _;
    let env = Env::default();
    let a = soroban_sdk::Address::generate(&env);
    // b is a clone of a — they must compare equal.
    let b = a.clone();
    assert!(crate::same_address(&a, &b));
}

/// Two different address values return `false`.
#[test]
fn test_same_address_different_returns_false() {
    use soroban_sdk::testutils::Address as _;
    let env = Env::default();
    let a = soroban_sdk::Address::generate(&env);
    let b = soroban_sdk::Address::generate(&env);
    // generate produces unique addresses each call.
    assert!(!crate::same_address(&a, &b));
}

/// `same_address` does not consume either address — both remain usable after the call.
#[test]
fn test_same_address_does_not_consume_arguments() {
    use soroban_sdk::testutils::Address as _;
    let env = Env::default();
    let owner = soroban_sdk::Address::generate(&env);
    let caller = owner.clone();
    // Call same_address — neither address should be moved.
    let result = crate::same_address(&owner, &caller);
    // Both addresses are still accessible here (no clone required by same_address itself).
    assert!(result);
    // Reuse the addresses to prove they were not consumed.
    let _ = &owner;
    let _ = &caller;
}

/// A single address is equal to itself (reflexivity).
#[test]
fn test_same_address_reflexive() {
    use soroban_sdk::testutils::Address as _;
    let env = Env::default();
    let a = soroban_sdk::Address::generate(&env);
    assert!(crate::same_address(&a, &a));
}

/// Symmetry: `same_address(a, b) == same_address(b, a)`.
#[test]
fn test_same_address_symmetric() {
    use soroban_sdk::testutils::Address as _;
    let env = Env::default();
    let a = soroban_sdk::Address::generate(&env);
    let b = soroban_sdk::Address::generate(&env);
    assert_eq!(crate::same_address(&a, &b), crate::same_address(&b, &a));
}

// ============================================================================
// require_registered_operator tests (#1182)
// ============================================================================

#[test]
fn test_require_registered_operator_success() {
    let env = Env::default();
    let caller = Address::generate(&env);
    
    // Register the operator
    env.storage().instance().set(&symbol_short!("OPERATOR"), &true);
    
    let result = require_registered_operator(&env, &caller);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_require_registered_operator_fails_if_missing() {
    let env = Env::default();
    let caller = Address::generate(&env);
    
    // Missing operator registration
    let result = require_registered_operator(&env, &caller);
    assert_eq!(result, Err(OperatorError::NotRegistered));
}

// ============================================================================
// Kill-switch guard comprehensive tests (#1290)
// ============================================================================
//
// These tests lock in every observable behaviour of the kill-switch guard:
//  - Default state (no flag set → inactive, writes allowed)
//  - Activation makes `is_kill_switch_active` return true
//  - `require_no_active_kill_switch` returns WriteBlocked when active
//  - Deactivation clears the flag and restores write permission
//  - Idempotent activation (double-activate remains blocked)
//  - Idempotent deactivation (double-deactivate remains clear)
//  - Multiple activate/deactivate cycles (toggle durability)
//  - Storage isolation: independent envs do not share kill-switch state
//  - Error discriminant is stable at 1 (ABI contract pinned)
//  - Error round-trips through Val encoding
//  - `require_no_active_kill_switch` is the single canonical guard call-sites
//    must use — callers may not bypass it by checking `is_kill_switch_active`
//    directly and inverting the result (both are tested to be equivalent)

#[cfg(test)]
mod kill_switch_guard_comprehensive_tests {
    use crate::{
        activate_kill_switch, deactivate_kill_switch, is_kill_switch_active,
        require_no_active_kill_switch, KillSwitchError,
    };
    use soroban_sdk::Env;

    // ── Happy-path (inactive) ─────────────────────────────────────────────

    /// The kill switch is inactive by default: no storage has been set.
    /// `is_kill_switch_active` must return false and `require_no_active_kill_switch`
    /// must return Ok(()).
    #[test]
    fn inactive_by_default_allows_writes() {
        let env = Env::default();
        assert!(
            !is_kill_switch_active(&env),
            "kill switch must be inactive when no flag is stored"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Ok(()),
            "guard must pass when kill switch has never been activated"
        );
    }

    /// After deactivation on a fresh environment (never activated), the state
    /// remains inactive — deactivate is a safe no-op.
    #[test]
    fn deactivate_on_pristine_env_is_safe_noop() {
        let env = Env::default();
        deactivate_kill_switch(&env);
        assert!(
            !is_kill_switch_active(&env),
            "deactivating a never-activated kill switch must leave it inactive"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Ok(()),
            "guard must pass after deactivating a never-activated kill switch"
        );
    }

    // ── Sad-path (active) ─────────────────────────────────────────────────

    /// Activating the kill switch must make `is_kill_switch_active` return true
    /// and `require_no_active_kill_switch` return `Err(WriteBlocked)`.
    #[test]
    fn activate_blocks_writes_with_typed_error() {
        let env = Env::default();
        activate_kill_switch(&env);

        assert!(
            is_kill_switch_active(&env),
            "is_kill_switch_active must return true immediately after activation"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Err(KillSwitchError::WriteBlocked),
            "guard must return WriteBlocked after activation"
        );
    }

    /// The typed error discriminant must be 1 (pinned for ABI stability across
    /// contract versions and downstream integrators).
    #[test]
    fn write_blocked_error_discriminant_is_one() {
        assert_eq!(
            KillSwitchError::WriteBlocked as u32,
            1u32,
            "KillSwitchError::WriteBlocked discriminant must be 1 (ABI contract)"
        );
    }

    /// The error round-trips through Val encoding (encoding stability guard).
    #[test]
    fn write_blocked_error_round_trips_through_val_encoding() {
        use soroban_sdk::{IntoVal, TryFromVal};
        let env = Env::default();
        let val: soroban_sdk::Val = KillSwitchError::WriteBlocked.into_val(&env);
        let decoded: KillSwitchError =
            KillSwitchError::try_from_val(&env, &val).expect("KillSwitchError must round-trip");
        assert_eq!(
            decoded,
            KillSwitchError::WriteBlocked,
            "round-trip must preserve WriteBlocked"
        );
    }

    // ── Recovery (deactivation) ───────────────────────────────────────────

    /// After activation and then deactivation, writes are allowed again.
    #[test]
    fn deactivate_after_activate_allows_writes_again() {
        let env = Env::default();
        activate_kill_switch(&env);
        assert!(is_kill_switch_active(&env));

        deactivate_kill_switch(&env);

        assert!(
            !is_kill_switch_active(&env),
            "is_kill_switch_active must return false after deactivation"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Ok(()),
            "guard must pass after deactivation"
        );
    }

    // ── Idempotency ───────────────────────────────────────────────────────

    /// Double-activation: activating twice in a row must leave the kill switch active.
    #[test]
    fn double_activate_remains_blocked() {
        let env = Env::default();
        activate_kill_switch(&env);
        activate_kill_switch(&env); // second call must not clear the flag

        assert!(
            is_kill_switch_active(&env),
            "double-activate must keep kill switch active"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Err(KillSwitchError::WriteBlocked),
            "guard must remain blocked after double-activate"
        );
    }

    /// Double-deactivation: deactivating twice must keep the kill switch inactive.
    #[test]
    fn double_deactivate_remains_clear() {
        let env = Env::default();
        activate_kill_switch(&env);
        deactivate_kill_switch(&env);
        deactivate_kill_switch(&env); // second deactivate must not re-arm

        assert!(
            !is_kill_switch_active(&env),
            "double-deactivate must keep kill switch inactive"
        );
        assert_eq!(
            require_no_active_kill_switch(&env),
            Ok(()),
            "guard must pass after double-deactivate"
        );
    }

    // ── Toggle durability ─────────────────────────────────────────────────

    /// Three full activate/deactivate cycles must each independently produce
    /// the expected state, proving the flag is written and removed correctly
    /// across repeated operations.
    #[test]
    fn three_full_toggle_cycles_produce_correct_state() {
        let env = Env::default();

        for cycle in 1u32..=3 {
            // Activate
            activate_kill_switch(&env);
            assert!(
                is_kill_switch_active(&env),
                "cycle {cycle}: expected active after activate"
            );
            assert_eq!(
                require_no_active_kill_switch(&env),
                Err(KillSwitchError::WriteBlocked),
                "cycle {cycle}: guard must block after activate"
            );

            // Deactivate
            deactivate_kill_switch(&env);
            assert!(
                !is_kill_switch_active(&env),
                "cycle {cycle}: expected inactive after deactivate"
            );
            assert_eq!(
                require_no_active_kill_switch(&env),
                Ok(()),
                "cycle {cycle}: guard must pass after deactivate"
            );
        }
    }

    // ── Storage isolation ─────────────────────────────────────────────────

    /// Two independent `Env` instances must not share kill-switch state.
    /// Activating in one must not affect the other.
    #[test]
    fn kill_switch_state_is_isolated_per_env() {
        let env_a = Env::default();
        let env_b = Env::default();

        activate_kill_switch(&env_a);

        assert!(
            is_kill_switch_active(&env_a),
            "env_a kill switch must be active"
        );
        assert!(
            !is_kill_switch_active(&env_b),
            "env_b kill switch must remain inactive when only env_a was activated"
        );
        assert_eq!(
            require_no_active_kill_switch(&env_b),
            Ok(()),
            "guard on env_b must pass when env_a was activated"
        );
    }

    // ── Guard equivalence ─────────────────────────────────────────────────

    /// `require_no_active_kill_switch` and `!is_kill_switch_active` must agree
    /// in every state. This pins the contract that the guard is the canonical
    /// call-site and is not just a thin wrapper that could silently diverge.
    #[test]
    fn guard_result_matches_negated_is_active_in_all_states() {
        let env = Env::default();

        // Inactive
        assert_eq!(
            require_no_active_kill_switch(&env).is_ok(),
            !is_kill_switch_active(&env),
            "inactive: guard result must equal !is_kill_switch_active"
        );

        // Active
        activate_kill_switch(&env);
        assert_eq!(
            require_no_active_kill_switch(&env).is_ok(),
            !is_kill_switch_active(&env),
            "active: guard result must equal !is_kill_switch_active"
        );

        // Deactivated again
        deactivate_kill_switch(&env);
        assert_eq!(
            require_no_active_kill_switch(&env).is_ok(),
            !is_kill_switch_active(&env),
            "deactivated: guard result must equal !is_kill_switch_active"
        );
    }

    // ── Boundary: simulate a write entry-point being guarded ─────────────

    /// Simulates the pattern used in every write entry point:
    /// `require_no_active_kill_switch` is called first; if it returns Err the
    /// entry point must propagate the error without executing any mutations.
    ///
    /// This test pins both the "before kill-switch" and "after kill-switch"
    /// paths of a typical guarded write entry point.
    #[test]
    fn guarded_write_entrypoint_simulation_passes_and_fails_correctly() {
        let env = Env::default();
        let mut mutated = false;

        // Happy path: guard passes, mutation proceeds.
        let result: Result<(), KillSwitchError> = (|| {
            require_no_active_kill_switch(&env)?;
            mutated = true;
            Ok(())
        })();
        assert_eq!(result, Ok(()));
        assert!(mutated, "mutation must occur when kill switch is inactive");

        // Sad path: guard blocks, mutation must NOT occur.
        mutated = false;
        activate_kill_switch(&env);

        let result: Result<(), KillSwitchError> = (|| {
            require_no_active_kill_switch(&env)?;
            mutated = true; // must not be reached
            Ok(())
        })();
        assert_eq!(result, Err(KillSwitchError::WriteBlocked));
        assert!(
            !mutated,
            "mutation must NOT occur when kill switch is active"
        );

        // Recovery: deactivate and verify mutation proceeds again.
        mutated = false;
        deactivate_kill_switch(&env);

        let result: Result<(), KillSwitchError> = (|| {
            require_no_active_kill_switch(&env)?;
            mutated = true;
            Ok(())
        })();
        assert_eq!(result, Ok(()));
        assert!(
            mutated,
            "mutation must resume after kill switch is deactivated"
        );
    }
}

// ============================================================================
// Investigation epoch guard comprehensive tests (#1293 — common-lib side)
// ============================================================================
//
// `investigation_epoch` is the time-bounded sibling of the binary kill switch.
// These tests lock in:
//  - Default state: no epoch active → writes allowed
//  - Starting an epoch blocks writes for its duration
//  - After the epoch expires, writes are allowed again
//  - Clearing an active epoch immediately restores writes
//  - Clearing an already-expired/nonexistent epoch is a safe no-op
//  - Off-by-one: ledger timestamp exactly at epoch_end is NOT active
//    (the guard uses strict >, so epoch_end is the first allowed timestamp)
//  - Back-to-back epochs (new epoch started before old one expires)
//  - Epoch error discriminant is stable

#[cfg(test)]
mod investigation_epoch_guard_comprehensive_tests {
    use crate::{
        clear_investigation_epoch, is_investigation_epoch_active, require_no_investigation_epoch,
        start_investigation_epoch, InvestigationEpochError,
    };
    use soroban_sdk::testutils::{Ledger, LedgerInfo};
    use soroban_sdk::Env;

    /// Set the ledger timestamp, preserving all other ledger state.
    fn set_ts(env: &Env, timestamp: u64) {
        let proto = env.ledger().protocol_version();
        let seq = env.ledger().sequence();
        env.ledger().set(LedgerInfo {
            protocol_version: proto,
            sequence_number: seq,
            timestamp,
            network_id: [0; 32],
            base_reserve: 10,
            min_temp_entry_ttl: 1,
            min_persistent_entry_ttl: 1,
            max_entry_ttl: 3_000_000,
        });
    }

    const T0: u64 = 1_000_000; // baseline timestamp
    const DURATION: u64 = 3_600; // 1 hour

    // ── Happy path (no epoch active) ──────────────────────────────────────

    /// No epoch stored: writes are allowed by default.
    #[test]
    fn no_epoch_set_allows_writes() {
        let env = Env::default();
        set_ts(&env, T0);

        assert!(
            !is_investigation_epoch_active(&env),
            "no epoch active by default"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must pass when no epoch is set"
        );
    }

    // ── Sad path (epoch active) ───────────────────────────────────────────

    /// Starting an epoch immediately blocks writes.
    #[test]
    fn active_epoch_blocks_writes() {
        let env = Env::default();
        set_ts(&env, T0);

        start_investigation_epoch(&env, DURATION);

        assert!(
            is_investigation_epoch_active(&env),
            "epoch must be active immediately after start"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Err(InvestigationEpochError::WriteBlocked),
            "guard must return WriteBlocked during active epoch"
        );
    }

    // ── Expiry ────────────────────────────────────────────────────────────

    /// After the epoch duration has elapsed, writes are allowed again without
    /// any explicit `clear` call — expiry is automatic.
    #[test]
    fn epoch_expires_automatically_after_duration() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        // Advance past the epoch end
        set_ts(&env, T0 + DURATION + 1);

        assert!(
            !is_investigation_epoch_active(&env),
            "epoch must be inactive after its duration has elapsed"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must pass after epoch expires"
        );
    }

    // ── Off-by-one: strict boundary ───────────────────────────────────────

    /// At exactly `epoch_end` the epoch is NOT active (the guard uses `>`, so
    /// `epoch_end == timestamp` means the epoch has just expired).
    #[test]
    fn epoch_inactive_at_exact_end_timestamp() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        let epoch_end = T0 + DURATION;
        set_ts(&env, epoch_end);

        assert!(
            !is_investigation_epoch_active(&env),
            "epoch must be inactive at the exact epoch_end timestamp (guard uses strict >)"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must pass at exact epoch_end"
        );
    }

    /// One second BEFORE epoch_end the epoch is still active.
    #[test]
    fn epoch_still_active_one_second_before_end() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        let one_before_end = T0 + DURATION - 1;
        set_ts(&env, one_before_end);

        assert!(
            is_investigation_epoch_active(&env),
            "epoch must still be active one second before epoch_end"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Err(InvestigationEpochError::WriteBlocked),
            "guard must block one second before epoch_end"
        );
    }

    // ── Immediate clear ───────────────────────────────────────────────────

    /// Clearing an active epoch immediately restores writes (before expiry).
    #[test]
    fn clear_active_epoch_immediately_allows_writes() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        assert!(is_investigation_epoch_active(&env));

        clear_investigation_epoch(&env);

        assert!(
            !is_investigation_epoch_active(&env),
            "epoch must be inactive immediately after clear"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must pass immediately after clear"
        );
    }

    /// Clearing when no epoch is active is a safe no-op.
    #[test]
    fn clear_nonexistent_epoch_is_safe_noop() {
        let env = Env::default();
        set_ts(&env, T0);

        // Must not panic
        clear_investigation_epoch(&env);

        assert!(
            !is_investigation_epoch_active(&env),
            "state must remain inactive after no-op clear"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must still pass after no-op clear"
        );
    }

    /// Clearing an already-expired epoch is also a safe no-op.
    #[test]
    fn clear_expired_epoch_is_safe_noop() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        // Let it expire
        set_ts(&env, T0 + DURATION + 1);
        assert!(!is_investigation_epoch_active(&env));

        // Explicitly clear — must not panic or re-arm the epoch
        clear_investigation_epoch(&env);

        assert!(
            !is_investigation_epoch_active(&env),
            "epoch must remain inactive after clearing an expired epoch"
        );
    }

    // ── Back-to-back epochs ───────────────────────────────────────────────

    /// Starting a new epoch while one is still active replaces (extends) it.
    #[test]
    fn new_epoch_while_active_extends_block() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        // Advance to 1 second before the first epoch ends, then start a new one
        let mid = T0 + DURATION - 1;
        set_ts(&env, mid);
        start_investigation_epoch(&env, DURATION); // new end = mid + DURATION

        // Original epoch_end (T0 + DURATION) has passed but new one is active
        let new_epoch_end = mid + DURATION;
        set_ts(&env, T0 + DURATION + 1); // past original, still in new

        assert!(
            is_investigation_epoch_active(&env),
            "new epoch must still be active after original epoch_end"
        );

        // Jump to exactly new_epoch_end — must be inactive
        set_ts(&env, new_epoch_end);
        assert!(
            !is_investigation_epoch_active(&env),
            "new epoch must expire at its own epoch_end"
        );
    }

    /// Starting an epoch after a previous one expired resets the guard.
    #[test]
    fn new_epoch_after_expiry_reinstates_block() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, DURATION);

        // Let the first epoch expire
        set_ts(&env, T0 + DURATION + 1);
        assert!(!is_investigation_epoch_active(&env));

        // Start a fresh epoch at the current time
        let t1 = T0 + DURATION + 1;
        set_ts(&env, t1);
        start_investigation_epoch(&env, DURATION);

        // Immediately: must be active
        assert!(
            is_investigation_epoch_active(&env),
            "fresh epoch must block immediately after starting"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Err(InvestigationEpochError::WriteBlocked),
            "guard must block after fresh epoch starts"
        );

        // Expire the second epoch
        set_ts(&env, t1 + DURATION + 1);
        assert!(
            !is_investigation_epoch_active(&env),
            "second epoch must expire after its duration"
        );
    }

    // ── Error discriminant stability ──────────────────────────────────────

    /// The `WriteBlocked` discriminant must be 1 (ABI contract — pinned for
    /// encoding stability across contract upgrades and downstream integrators).
    #[test]
    fn write_blocked_discriminant_is_one() {
        assert_eq!(
            InvestigationEpochError::WriteBlocked as u32,
            1u32,
            "InvestigationEpochError::WriteBlocked discriminant must be 1 (ABI contract)"
        );
    }

    /// The error round-trips through Val encoding.
    #[test]
    fn write_blocked_error_round_trips_through_val() {
        use soroban_sdk::{IntoVal, TryFromVal};
        let env = Env::default();
        let val: soroban_sdk::Val = InvestigationEpochError::WriteBlocked.into_val(&env);
        let decoded = InvestigationEpochError::try_from_val(&env, &val)
            .expect("InvestigationEpochError must round-trip through Val");
        assert_eq!(decoded, InvestigationEpochError::WriteBlocked);
    }

    // ── Zero-duration epoch ───────────────────────────────────────────────

    /// A zero-duration epoch expires immediately (epoch_end == T0 and the
    /// guard checks `epoch_end > timestamp`, so it is never active).
    #[test]
    fn zero_duration_epoch_is_never_active() {
        let env = Env::default();
        set_ts(&env, T0);
        start_investigation_epoch(&env, 0);

        assert!(
            !is_investigation_epoch_active(&env),
            "zero-duration epoch must never be active (epoch_end == now, guard uses strict >)"
        );
        assert_eq!(
            require_no_investigation_epoch(&env),
            Ok(()),
            "guard must pass for zero-duration epoch"
        );
    }

    // ── u64 saturation boundary ───────────────────────────────────────────

    /// `start_investigation_epoch` uses `saturating_add` for the end time.
    /// At `u64::MAX` ledger time + any duration, the epoch_end saturates at
    /// `u64::MAX`. The guard compares `epoch_end > timestamp`; at exactly
    /// `u64::MAX` they are equal, so the guard passes (epoch not active).
    #[test]
    fn saturation_at_u64_max_does_not_panic() {
        let env = Env::default();
        set_ts(&env, u64::MAX);
        // Must not panic even though saturating_add overflows conceptually
        start_investigation_epoch(&env, u64::MAX);

        // epoch_end == u64::MAX::saturating_add(u64::MAX) == u64::MAX
        // timestamp == u64::MAX → epoch_end == timestamp → NOT active
        assert!(
            !is_investigation_epoch_active(&env),
            "saturated epoch_end == timestamp must not be considered active"
        );
    }
}
