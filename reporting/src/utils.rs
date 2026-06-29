use soroban_sdk::contracterror;

/// Error type for overflow when converting u64 → u32.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ConversionError {
    /// Value does not fit into a u32.
    Overflow = 1,
}

/// Safely convert a u64 to u32, returning an error on overflow.
pub fn u64_to_u32(val: u64) -> Result<u32, ConversionError> {
    if val <= u32::MAX as u64 {
        Ok(val as u32)
    } else {
        Err(ConversionError::Overflow)
    }
}
