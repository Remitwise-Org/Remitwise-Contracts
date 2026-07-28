#![cfg(test)]

/// Tests for [`NonZeroU128`] and [`ZeroNotAllowed`].
///
/// # Contract
/// - Zero is rejected with `Err(ZeroNotAllowed)`.
/// - Any non-zero value (including 1 and `u128::MAX`) is accepted.
/// - `get()` returns the original value unchanged.
extern crate std;
use std::format;
use super::*;

// ─── new: happy path ──────────────────────────────────────────────────────

/// The smallest non-zero value (1) is accepted.
#[test]
fn new_accepts_one() {
    let nz = NonZeroU128::new(1).unwrap();
    assert_eq!(nz.get(), 1);
}

/// The largest possible value (`u128::MAX`) is accepted.
#[test]
fn new_accepts_u128_max() {
    let nz = NonZeroU128::new(u128::MAX).unwrap();
    assert_eq!(nz.get(), u128::MAX);
}

/// A typical mid-range value is accepted and preserved.
#[test]
fn new_accepts_typical_value() {
    let nz = NonZeroU128::new(42).unwrap();
    assert_eq!(nz.get(), 42);
}

// ─── new: sad path ────────────────────────────────────────────────────────

/// Zero is explicitly rejected with `ZeroNotAllowed`.
#[test]
fn new_rejects_zero() {
    assert_eq!(NonZeroU128::new(0), Err(ZeroNotAllowed));
}

// ─── get ──────────────────────────────────────────────────────────────────

/// The internal value is returned unchanged through `get()`.
#[test]
fn get_returns_original_value() {
    let values = [1u128, 42, u128::MAX >> 1, u128::MAX];
    for &v in &values {
        let nz = NonZeroU128::new(v).unwrap();
        assert_eq!(nz.get(), v);
    }
}

// ─── trait impls ──────────────────────────────────────────────────────────

/// Clone produces an independent copy with the same value.
#[test]
fn clone_preserves_value() {
    let a = NonZeroU128::new(99).unwrap();
    let b = a;
    assert_eq!(a.get(), b.get());
}

/// Copy produces an independent copy with the same value.
#[test]
fn copy_preserves_value() {
    let a = NonZeroU128::new(99).unwrap();
    let b = a;
    assert_eq!(a.get(), b.get());
}

/// Equality compares the inner values.
#[test]
fn eq_compares_inner_value() {
    let a = NonZeroU128::new(100).unwrap();
    let b = NonZeroU128::new(100).unwrap();
    let c = NonZeroU128::new(200).unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
}

/// Ordering compares the inner values.
#[test]
fn ord_compares_inner_value() {
    let small = NonZeroU128::new(10).unwrap();
    let large = NonZeroU128::new(20).unwrap();
    assert!(small < large);
    assert!(large > small);
}

/// Debug formatting shows the inner value.
#[test]
fn debug_shows_inner_value() {
    let nz = NonZeroU128::new(7).unwrap();
    assert_eq!(format!("{:?}", nz), "NonZeroU128(7)");
}

/// ZeroNotAllowed debug formatting is descriptive.
#[test]
fn zero_not_allowed_debug() {
    assert_eq!(format!("{:?}", ZeroNotAllowed), "ZeroNotAllowed");
}
