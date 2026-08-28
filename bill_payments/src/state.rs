use soroban_sdk::{contracttype, Env};

use crate::{Bill, BillPaymentsError};

/// The actual states of a bill in the system.
/// Note: "Cancelled" is not a persisted state - cancelled bills are deleted.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BillState {
    Active,   // Bill exists and is unpaid
    Paid,     // Bill exists and is paid
    Archived, // Bill is in archive storage
}

impl BillState {
    /// Derive state from bill data
    pub fn from_bill(bill: &Bill, is_archived: bool) -> Self {
        if is_archived {
            return BillState::Archived;
        }
        if bill.paid {
            BillState::Paid
        } else {
            BillState::Active
        }
    }

    /// Check if transition is legal
    pub fn can_transition_to(&self, target: BillState) -> bool {
        use BillState::*;
        matches!(
            (self, target),
            (Active, Paid)      // Pay bill
                | (Active, Active)  // No change
                | (Active, Archived) // Archive (only if paid, checked elsewhere)
                | (Paid, Archived)   // Archive paid bill
                | (Archived, Active) // Restore
        )
    }

    /// Validate a state transition
    pub fn validate_transition(
        current: &Bill,
        is_archived: bool,
        target: BillState,
        _operation: &str,
    ) -> Result<(), BillPaymentsError> {
        let current_state = BillState::from_bill(current, is_archived);
        if !current_state.can_transition_to(target) {
            return Err(BillPaymentsError::InvalidStateTransition);
        }
        Ok(())
    }
}

/// Check bill invariants
pub fn check_invariants(
    _env: &Env,
    bill: &Bill,
    _is_archived: bool,
) -> Result<(), BillPaymentsError> {
    // Paid bills must have paid_at
    if bill.paid && bill.paid_at.is_none() {
        return Err(BillPaymentsError::InvariantViolation);
    }
    // Unpaid bills must NOT have paid_at
    if !bill.paid && bill.paid_at.is_some() {
        return Err(BillPaymentsError::InvariantViolation);
    }
    // Recurring bills need frequency > 0
    if bill.recurring && bill.frequency_days == 0 {
        return Err(BillPaymentsError::InvariantViolation);
    }
    // Amount must be positive
    if bill.amount <= 0 {
        return Err(BillPaymentsError::InvalidAmount);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Address, Env, String, Vec};

    #[test]
    fn test_state_transitions() {
        // Legal transitions
        assert!(BillState::Active.can_transition_to(BillState::Paid));
        assert!(BillState::Active.can_transition_to(BillState::Active));
        assert!(BillState::Active.can_transition_to(BillState::Archived));
        assert!(BillState::Paid.can_transition_to(BillState::Archived));
        assert!(BillState::Archived.can_transition_to(BillState::Active));

        // Illegal transitions
        assert!(!BillState::Paid.can_transition_to(BillState::Active));
        assert!(!BillState::Archived.can_transition_to(BillState::Paid));
        assert!(!BillState::Archived.can_transition_to(BillState::Archived));
    }

    #[test]
    fn test_invariant_checks() {
        let env = Env::default();
        let owner = Address::generate(&env);

        let valid_bill = Bill {
            id: 1,
            owner: owner.clone(),
            name: String::from_str(&env, "Test"),
            external_ref: None,
            amount: 1000,
            due_date: env.ledger().timestamp() + 86400,
            recurring: false,
            frequency_days: 0,
            paid: false,
            created_at: env.ledger().timestamp(),
            paid_at: None,
            schedule_id: None,
            tags: Vec::new(&env),
            currency: String::from_str(&env, "XLM"),
        };
        assert!(check_invariants(&env, &valid_bill, false).is_ok());

        let mut bad_bill = valid_bill.clone();
        bad_bill.paid = true;
        bad_bill.paid_at = None;
        assert!(check_invariants(&env, &bad_bill, false).is_err());
    }
}