/// Error type for overflow when converting u64 → u32.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversionError {
    /// Value does not fit into a u32.
    Overflow,
}

/// Safely convert a u64 to u32, returning an error on overflow.
pub fn u64_to_u32(val: u64) -> Result<u32, ConversionError> {
    if val <= u32::MAX as u64 {
        Ok(val as u32)
    } else {
        Err(ConversionError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::{u64_to_u32, ConversionError};

    // ─── less than u32::MAX ──────────────────────────────────────────────────

    /// A value strictly less than u32::MAX must convert without error.
    #[test]
    fn converts_value_less_than_u32_max() {
        let val: u64 = (u32::MAX as u64) - 1;
        assert_eq!(u64_to_u32(val), Ok(u32::MAX - 1));
    }

    // ─── equal to u32::MAX (inclusive boundary) ──────────────────────────────

    /// u32::MAX itself sits on the inclusive boundary and must succeed.
    #[test]
    fn converts_value_equal_to_u32_max() {
        let val: u64 = u32::MAX as u64;
        assert_eq!(u64_to_u32(val), Ok(u32::MAX));
    }

    // ─── greater than u32::MAX (exclusive boundary) ──────────────────────────

    /// One step above u32::MAX must return Overflow.
    #[test]
    fn returns_overflow_for_value_greater_than_u32_max() {
        let val: u64 = (u32::MAX as u64) + 1;
        assert_eq!(u64_to_u32(val), Err(ConversionError::Overflow));
    }

    // ─── u64::MAX ────────────────────────────────────────────────────────────

    /// The largest possible u64 value must return Overflow, not panic.
    #[test]
    fn returns_overflow_for_u64_max() {
        assert_eq!(u64_to_u32(u64::MAX), Err(ConversionError::Overflow));
    }
}
