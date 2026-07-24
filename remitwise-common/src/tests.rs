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
use proptest::prelude::*;
use soroban_sdk::{Bytes, Env, IntoVal, String, Symbol, Vec};

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

// ─── verify_signature tests ──────────────────────────────────────────────────

#[test]
fn test_verify_signature_valid() {
    let env = Env::default();
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    // Sign the prefixed message
    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(domain);
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    let result = verify_signature(&env, domain, message, &signature, &pk);
    assert_eq!(result, Ok(()));
}

#[test]
#[should_panic]
fn test_verify_signature_invalid_signature() {
    let env = Env::default();
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let invalid_signature = [0u8; 64];

    let _ = verify_signature(&env, domain, message, &invalid_signature, &pk);
}

#[test]
fn test_verify_signature_invalid_signature_length() {
    let env = Env::default();
    let domain = b"test-domain";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();
    let short_signature = [0u8; 32];

    let result = verify_signature(&env, domain, message, &short_signature, &pk);
    assert_eq!(result, Err(SignatureError::InvalidSignatureLength));
}

#[test]
fn test_verify_signature_invalid_public_key_length() {
    let env = Env::default();
    let domain = b"test-domain";
    let message = b"hello world";

    let short_pk = [0u8; 16];
    let signature = [0u8; 64];

    let result = verify_signature(&env, domain, message, &signature, &short_pk);
    assert_eq!(result, Err(SignatureError::InvalidPublicKeyLength));
}

#[test]
#[should_panic]
fn test_verify_signature_wrong_domain() {
    let env = Env::default();
    let domain1 = b"domain1";
    let domain2 = b"domain2";
    let message = b"hello world";

    let sk = ed25519_dalek::SigningKey::generate(&mut rand::rngs::OsRng);
    let pk = sk.verifying_key().to_bytes();

    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(domain1);
    prefixed.extend_from_slice(message);
    let signature = sk.sign(&prefixed).to_bytes();

    let _ = verify_signature(&env, domain2, message, &signature, &pk);
}

#[test]
fn test_verify_slash_signature_valid() {
    let env = Env::default();
    let message = b"slash payload";

    // Generate a keypair
    let (sk, pk) = soroban_sdk::testutils::ed25519::generate(&env);

    // Sign the prefixed message
    let mut prefixed = std::vec::Vec::new();
    prefixed.extend_from_slice(b"slash-auth");
    prefixed.extend_from_slice(message);
    let signature = soroban_sdk::testutils::ed25519::sign(&env, &sk, &prefixed);

    // Verify the slash signature
    let result = verify_slash_signature(&env, message, Some(&signature), &pk);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_verify_slash_signature_optional_none() {
    let env = Env::default();
    let message = b"slash payload";
    let pk = [0u8; 32];

    // Verify when signature is None (should succeed as it's optional)
    let result = verify_slash_signature(&env, message, None, &pk);
    assert_eq!(result, Ok(()));
}

#[test]
fn test_verify_slash_signature_invalid() {
    let env = Env::default();
    let message = b"slash payload";

    // Generate a keypair
    let (_, pk) = soroban_sdk::testutils::ed25519::generate(&env);
    let invalid_signature = [0u8; 64]; // Invalid

    // Verify the invalid slash signature
    let result = verify_slash_signature(&env, message, Some(&invalid_signature), &pk);
    assert_eq!(result, Err(SlashError::InvalidSignature));
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
    let tags = soroban_sdk::vec![&env,
        soroban_sdk::String::from_str(&env, "payments"),
        soroban_sdk::String::from_str(&env, "SAVINGS"),
    ];
    let result = canonicalize_tags_checked(&env, &tags);
    assert!(result.is_ok());
    let out = result.unwrap();
    assert_eq!(out.len(), 2);
    assert_eq!(out.get(1).unwrap(), soroban_sdk::String::from_str(&env, "savings"));
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
    assert!(matches!(result, Err(crate::TagError::InvalidChar { position: 0 })));
}
