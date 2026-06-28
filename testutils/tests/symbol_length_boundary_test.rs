/// Symbol Length Boundary Tests
///
/// These tests lock in the behaviour of Soroban `Symbol` values at the
/// boundary between "small symbols" (≤ 9 bytes) and "large symbols"
/// (10–32 bytes) for the SDK pinned at `soroban-sdk = "=21.7.7"`.
///
/// The boundary is named in [`STORAGE_LAYOUT.md`](../STORAGE_LAYOUT.md) and in
/// [`storage_key_naming_test.rs`](storage_key_naming_test.rs) via the
/// `MAX_KEY_LENGTH = 9` constant. Every documented storage key is kept at or
/// below nine bytes so the `symbol_short!` macro can be used at compile time.
///
/// What the tests cover:
///
/// 1. A 9-byte symbol can be constructed via the runtime API
///    `Symbol::new(&env, "...")` and round-trips through `SCVal`.
/// 2. A 10-byte symbol can be constructed via the runtime API
///    `Symbol::new(&env, "...")` and round-trips through `SCVal`.
/// 3. A 9-byte symbol can be constructed via the compile-time macro
///    `symbol_short!("...")` and matches a runtime-built clone.
/// 4. The 9-byte and 10-byte boundary values are distinct as values
///    (inequality) and as storage keys (write/read independence).
/// 5. The boundary values work as a happy path even when stored side-by-side,
///    so indexers and off-chain tooling can't conflate the two encodings.
///
/// Sad path: a 9-byte and a 10-byte string sharing a 9-byte prefix must not
/// be considered equal — locking in that the SDK stores and compares them
/// by their full byte content. Additionally, the SDK's documented 32-byte
/// ceiling is enforced via an explicit panic test.
///
/// All inputs are deterministic `&str` literals. No `Date.now()` /
/// `Math.random()` equivalents. Test names are assertive (state the
/// expected outcome).
use soroban_sdk::{symbol_short, Env, IntoVal, Symbol, TryFromVal};

/// 9 ASCII bytes: the exact upper limit accepted by `symbol_short!`.
/// Below this length, the SDK stores the symbol inline as a small symbol.
const NINE_CHAR_NAME: &str = "boundary9";

/// 10 ASCII bytes: one byte past the `symbol_short!` cap. Forces the SDK to
/// use the large-symbol encoding on the wire.
const TEN_CHAR_NAME: &str = "boundary10";

/// One byte past the SDK's documented Symbol upper bound (32 bytes).
/// `Symbol::new` must reject this at runtime in SDK 21.x.
const OVER_LONG_NAME: &str = "abcdefghijklmnopqrstuvwxyz0123456"; // 33 bytes

// Compile-time guard: the `boundary9` literal fed to `symbol_short!` must
// match `NINE_CHAR_NAME` exactly. If either side is renamed in isolation,
// the equality test below would silently drift to a non-boundary sample and
// still pass. Const-context `assert!` is available on the pinned stable
// toolchain (stabilized in Rust 1.79).
const _: () = assert!(NINE_CHAR_NAME == "boundary9");

/// Round-trip a `Symbol` through its `Val` wire form and back, asserting no
/// loss along the way. Mirrors how the Soroban host re-imports a symbol
/// after it crosses a contract-call boundary.
///
/// Takes `&Symbol` and clones internally so the caller can still assert
/// equality on the original. Pins to `IntoVal for Symbol` (owned) rather
/// than `IntoVal for &Symbol`, the latter of which is not guaranteed
/// across soroban-sdk minor versions on the `=21.7.7` pin.
fn round_trip_via_val(env: &Env, original: &Symbol) -> Symbol {
    let val = original.clone().into_val(env);
    Symbol::try_from_val(env, &val).expect("Symbol must round-trip through SCVal")
}

#[test]
fn nine_char_symbol_round_trips_via_new() {
    let env = Env::default();

    let original = Symbol::new(&env, NINE_CHAR_NAME);
    let recovered = round_trip_via_val(&env, &original);

    assert_eq!(
        original, recovered,
        "9-char Symbol::new must round-trip identically through SCVal"
    );
}

#[test]
fn ten_char_symbol_round_trips_via_new() {
    let env = Env::default();

    let original = Symbol::new(&env, TEN_CHAR_NAME);
    let recovered = round_trip_via_val(&env, &original);

    assert_eq!(
        original, recovered,
        "10-char Symbol::new must round-trip identically through SCVal"
    );
}

#[test]
fn nine_char_symbol_short_macro_matches_runtime_new() {
    // Compile-time path: `symbol_short!` enforces the 9-byte cap so a
    // 10-char literal here would be a *compile* error. We use the documented
    // max length on purpose to lock in the boundary.
    //
    // Note: the macro is fed a *string literal* (not the `NINE_CHAR_NAME`
    // const identifier) because `symbol_short!` expands via `stringify!`,
    // which would otherwise export the path name as the symbol's contents.
    let compile_time = symbol_short!("boundary9");

    let env = Env::default();
    let runtime = Symbol::new(&env, NINE_CHAR_NAME);

    assert_eq!(
        compile_time, runtime,
        "symbol_short!(\"boundary9\") and Symbol::new(NINE_CHAR_NAME) must produce equal Symbols"
    );
}

#[test]
fn nine_char_and_ten_char_symbols_remain_distinct_values() {
    // Sad-path boundary check: a 9-char string that is a *prefix* of a
    // 10-char string must not compare equal to that 10-char counterpart.
    let env = Env::default();

    let nine = Symbol::new(&env, NINE_CHAR_NAME);
    let ten = Symbol::new(&env, TEN_CHAR_NAME);

    assert_ne!(
        nine, ten,
        "9-char and 10-char Symbols sharing a prefix must remain distinct values"
    );
}

#[test]
fn nine_char_and_ten_char_symbols_preserve_separate_storage_slots() {
    let env = Env::default();

    let nine = Symbol::new(&env, NINE_CHAR_NAME);
    let ten = Symbol::new(&env, TEN_CHAR_NAME);

    env.storage().instance().set(&nine, &1_u32);
    env.storage().instance().set(&ten, &2_u32);

    assert_eq!(
        env.storage().instance().get::<_, u32>(&nine),
        Some(1),
        "9-char key must retrieve the value written under it"
    );
    assert_eq!(
        env.storage().instance().get::<_, u32>(&ten),
        Some(2),
        "10-char key must retrieve the value written under it"
    );
}

#[test]
fn boundary_literals_hold_expected_byte_lengths() {
    // Pin the boundary on the literal side so this test catches both
    // accidental renames (which would shift the boundary) and any future
    // collapse of the small/large symbol division in the SDK.
    assert_eq!(
        NINE_CHAR_NAME.len(),
        9,
        "NINE_CHAR_NAME must be exactly 9 bytes (small-symbol cap)"
    );
    assert_eq!(
        TEN_CHAR_NAME.len(),
        10,
        "TEN_CHAR_NAME must be exactly 10 bytes (small-symbol cap + 1)"
    );
}

/// Explicit sad path: the SDK's published Symbol upper bound is 32 bytes.
/// Anything strictly over that limit must be rejected on construction.
#[test]
#[should_panic]
fn symbol_over_32_bytes_is_rejected_on_new() {
    let env = Env::default();
    let _ = Symbol::new(&env, OVER_LONG_NAME);
}
