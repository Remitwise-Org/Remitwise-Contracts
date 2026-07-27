//! Shared period-key validation helpers.
//!
//! Period keys identify the accounting period to which an entity belongs.
//! Combining entities with different keys can cause state from one period to
//! be interpreted as state from another, so callers must validate keys before
//! performing a multi-entity operation.

use soroban_sdk::contracterror;

/// Errors returned by period-key validation.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PeriodKeyError {
    /// The entities belong to different periods.
    MismatchedPeriodKey = 1,
}

/// Require two entities to belong to the same period.
pub fn require_matching_period_key(
    a_pk: u64,
    b_pk: u64,
) -> Result<(), PeriodKeyError> {
    if a_pk == b_pk {
        Ok(())
    } else {
        Err(PeriodKeyError::MismatchedPeriodKey)
    }
}

#[cfg(test)]
mod tests {
    use super::{require_matching_period_key, PeriodKeyError};

    #[test]
    fn rejects_entities_from_different_periods() {
        let result = require_matching_period_key(202401, 202402);

        assert_eq!(result, Err(PeriodKeyError::MismatchedPeriodKey));
    }

    #[test]
    fn accepts_entities_from_the_same_period() {
        assert_eq!(require_matching_period_key(202401, 202401), Ok(()));
    }
}
