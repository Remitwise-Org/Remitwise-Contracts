use soroban_sdk::{contractclient, contracterror, Address, Env};

/// Standard error types for reversible (compensation) operations.
///
/// Each contract maps its domain-specific error into these shared variants
/// so that callers like the orchestrator can handle reversals uniformly.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReversibleOpError {
    /// The operation was already reversed or had no effect (idempotent no-op).
    NothingToReverse = 1,
    /// The caller is not authorized to reverse this operation.
    Unauthorized = 2,
    /// The target entity (goal, bill, policy) does not exist.
    NotFound = 3,
    /// The entity is in a state that cannot be reversed.
    InvalidState = 4,
}

/// Interface for reversing savings goal contributions.
///
/// Implemented by `savings_goals` to allow the orchestrator to compensate
/// a previous `add_to_goal` call during rollback.
#[contractclient(name = "SavingsGoalsReversibleClient")]
pub trait SavingsGoalsReversible {
    /// Remove `amount` from the goal identified by `goal_id` on behalf of `user`.
    ///
    /// Returns `true` when funds were actually removed, `false` if there was
    /// nothing to reverse (e.g. the goal had already been cleared).
    fn remove_from_goal(
        env: Env,
        user: Address,
        goal_id: u32,
        amount: i128,
    ) -> Result<bool, ReversibleOpError>;
}

/// Interface for reversing bill payments.
///
/// Implemented by `bill_payments` to allow the orchestrator to compensate
/// a previous `pay_bill` call during rollback.
#[contractclient(name = "BillPaymentsReversibleClient")]
pub trait BillPaymentsReversible {
    /// Reverse a payment for the bill identified by `bill_id` on behalf of `user`.
    ///
    /// Returns `true` when the payment was actually reversed, `false` if there
    /// was nothing to reverse.
    fn reverse_payment(
        env: Env,
        user: Address,
        bill_id: u32,
        amount: i128,
    ) -> Result<bool, ReversibleOpError>;
}

/// Interface for reversing insurance premium payments.
///
/// Implemented by `insurance` to allow the orchestrator to compensate
/// a previous `pay_premium` call during rollback.
#[contractclient(name = "InsuranceReversibleClient")]
pub trait InsuranceReversible {
    /// Reverse a premium payment for the policy identified by `policy_id` on
    /// behalf of `user`.
    ///
    /// Returns `true` when the premium was actually reversed, `false` if there
    /// was nothing to reverse.
    fn reverse_premium(
        env: Env,
        user: Address,
        policy_id: u32,
        amount: i128,
    ) -> Result<bool, ReversibleOpError>;
}
