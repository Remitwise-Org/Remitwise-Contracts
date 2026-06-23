#![cfg(test)]

/// Tests for [`canonicalize_tags`] and [`clamp_limit`].
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
use proptest::prelude::*;
use soroban_sdk::{Env, String, Vec};

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
fn get(env: &Env, v: &Vec<String>, i: u32) -> std::string::String {
    let s = v.get(i).unwrap();
    let mut buf = std::vec![0u8; s.len() as usize];
    s.copy_into_slice(&mut buf);
    std::string::String::from_utf8(buf).unwrap()
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
