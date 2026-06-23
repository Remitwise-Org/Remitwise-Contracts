//! Tests for GoalCompleted event-emission at exact target-amount boundary.
//!
//! # Single-emission guarantee
//!
//! The contract emits `SavingsEvent::GoalCompleted` (carrying a `GoalCompletedEvent`)
//! exactly once per goal, on the contribution that first brings
//! `current_amount >= target_amount`. Subsequent contributions to the same
//! goal must NOT re-emit the event, ensuring downstream indexers and
//! notification services are not double-triggered.

#[cfg(test)]
mod goal_completed_event_tests {
    use soroban_sdk::{
        testutils::{Address as AddressTrait, Events},
        vec, Env, Symbol, TryFromVal,
    };

    use crate::{SavingsGoalContract, SavingsGoalContractClient};

    fn setup() -> (
        Env,
        SavingsGoalContractClient<'static>,
        soroban_sdk::Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);
        let owner = soroban_sdk::Address::generate(&env);
        (env, client, owner)
    }

    fn count_completed_events(env: &Env) -> usize {
        env.events()
            .all()
            .iter()
            .filter(|(_, topics, _)| {
                topics.iter().any(|t| {
                    Symbol::try_from_val(env, &t).ok().as_ref()
                        == Some(&Symbol::new(env, "completed"))
                })
            })
            .count()
    }

    // Test 1 — Exact-target contribution emits GoalCompleted exactly once

    /// When a single `add_to_goal` call brings `current_amount` to exactly
    /// `target_amount`, one `GoalCompleted` event must be emitted.
    #[test]
    fn test_exact_target_emits_goal_completed_once() {
        let (env, client, owner) = setup();

        // Create a goal with target = 1_000
        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "School Fees"),
            &1_000_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        // Add exactly the target amount in one contribution
        client.add_to_goal(&owner, &goal_id, &1_000_i128);

        // Exactly one completion → 2 events (legacy + RemitwiseEvents)
        assert_eq!(
            count_completed_events(&env),
            2,
            "Expected 2 GoalCompleted events (legacy + RemitwiseEvents) when contribution lands on target"
        );

        assert!(
            client.is_goal_completed(&goal_id),
            "is_goal_completed should return true after reaching target"
        );
    }

    // Test 2 — Overshoot emits GoalCompleted events (legacy + RemitwiseEvents)

    /// When `add_to_goal` pushes `current_amount` above `target_amount`,
    /// exactly two `GoalCompleted` events must be emitted — the legacy direct
    /// event and the standardized RemitwiseEvents event.
    #[test]
    fn test_overshoot_emits_goal_completed_once() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Vacation Fund"),
            &500_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        // Contribute MORE than the target in a single call
        client.add_to_goal(&owner, &goal_id, &750_i128);

        assert_eq!(
            count_completed_events(&env),
            2,
            "Expected 2 GoalCompleted events (legacy + RemitwiseEvents) on overshoot contribution"
        );

        assert!(client.is_goal_completed(&goal_id));
    }

    // Test 3 — Multi-step contribution: partial then completing contribution

    /// Two separate `add_to_goal` calls where the first is partial and the
    /// second crosses the target — only the second call should emit the event.
    #[test]
    fn test_partial_then_completing_contribution_emits_once() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "School Fees"),
            &1_000_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        // First contribution: partial (no completion yet)
        client.add_to_goal(&owner, &goal_id, &400_i128);
        assert_eq!(
            count_completed_events(&env),
            0,
            "No GoalCompleted event expected after partial contribution"
        );
        assert!(!client.is_goal_completed(&goal_id));

        // Second contribution: crosses the target
        client.add_to_goal(&owner, &goal_id, &600_i128);
        assert_eq!(
            count_completed_events(&env),
            2,
            "Expected 2 GoalCompleted events (legacy + RemitwiseEvents) after crossing target"
        );
        assert!(client.is_goal_completed(&goal_id));
    }

    // Test 4 — Post-completion add does NOT re-emit GoalCompleted

    /// Once a goal is completed, subsequent `add_to_goal` calls must not
    /// emit additional `GoalCompleted` events. This prevents double-triggering
    /// downstream indexers and notification services.
    #[test]
    fn test_post_completion_add_does_not_re_emit() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Medical Fund"),
            &1_000_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        // Complete the goal → 2 events (legacy + RemitwiseEvents)
        client.add_to_goal(&owner, &goal_id, &1_000_i128);
        assert_eq!(count_completed_events(&env), 2);

        // Add more funds after completion — must NOT emit additional events
        client.add_to_goal(&owner, &goal_id, &500_i128);
        assert_eq!(
            count_completed_events(&env),
            2,
            "GoalCompleted must NOT be re-emitted after goal is already complete"
        );
    }

    // Test 5 — Repeated post-completion adds still do not re-emit

    #[test]
    fn test_multiple_post_completion_adds_never_re_emit() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Home Deposit"),
            &2_000_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        client.add_to_goal(&owner, &goal_id, &2_000_i128);
        assert_eq!(count_completed_events(&env), 2);

        // Three additional contributions after completion
        for _ in 0..3 {
            client.add_to_goal(&owner, &goal_id, &100_i128);
        }

        assert_eq!(
            count_completed_events(&env),
            2,
            "Still exactly 2 GoalCompleted events after multiple post-completion contributions"
        );
    }

    // Test 6 — batch_add_to_goals: completing one goal emits once

    /// Using `batch_add_to_goals`, completing one goal in a batch emits
    /// exactly one `GoalCompleted` event for that goal.
    #[test]
    fn test_batch_add_completes_goal_emits_once() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Business Capital"),
            &1_000_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        client.batch_add_to_goals(
            &owner,
            &vec![
                &env,
                crate::ContributionItem {
                    goal_id: goal_id,
                    amount: 1_000_i128,
                },
            ],
        );

        assert_eq!(
            count_completed_events(&env),
            2,
            "batch_add_to_goals completing a goal should emit 2 GoalCompleted events (legacy + RemitwiseEvents)"
        );
        assert!(client.is_goal_completed(&goal_id));
    }

    // Test 7 — batch_add_to_goals: completing multiple goals emits two per goal

    #[test]
    fn test_batch_add_completes_two_goals_emits_two_events() {
        let (env, client, owner) = setup();

        let goal_a = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Goal A"),
            &500_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );
        let goal_b = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Goal B"),
            &800_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        client.batch_add_to_goals(
            &owner,
            &vec![
                &env,
                crate::ContributionItem {
                    goal_id: goal_a,
                    amount: 500_i128,
                },
                crate::ContributionItem {
                    goal_id: goal_b,
                    amount: 800_i128,
                },
            ],
        );

        assert_eq!(
            count_completed_events(&env),
            4,
            "Two completed goals → 4 events (2 legacy + 2 RemitwiseEvents)"
        );
        assert!(client.is_goal_completed(&goal_a));
        assert!(client.is_goal_completed(&goal_b));
    }

    // -----------------------------------------------------------------------
    // Test 8 — batch_add_to_goals: already-completed goal in batch does not re-emit
    // -----------------------------------------------------------------------

    #[test]
    fn test_batch_add_already_completed_goal_does_not_re_emit() {
        let (env, client, owner) = setup();

        let goal_id = client.create_goal(
            &owner,
            &soroban_sdk::String::from_str(&env, "Already Done"),
            &300_i128,
            &(env.ledger().timestamp() + 86_400),
            &false,
        );

        // Complete via single add first → 2 events
        client.add_to_goal(&owner, &goal_id, &300_i128);
        assert_eq!(count_completed_events(&env), 2);

        // Now include the same completed goal in a batch
        client.batch_add_to_goals(
            &owner,
            &vec![
                &env,
                crate::ContributionItem {
                    goal_id: goal_id,
                    amount: 100_i128,
                },
            ],
        );

        assert_eq!(
            count_completed_events(&env),
            2,
            "batch_add_to_goals must not re-emit GoalCompleted for an already-completed goal"
        );
    }
}
