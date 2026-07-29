//! Issue #1516 – regression tests for the persistent-storage TTL bucket.
//!
//! Every `persistent().extend_ttl` call in this crate previously used the
//! *instance* bucket's constants (1-day threshold / 30-day bump) under
//! misleading `INSTANCE_*` names, so persistent user data — savings goals,
//! archives, schedules — was archived on half its intended lifetime. These
//! tests pin the bump to the shared persistent bucket so the buckets can
//! never silently swap again.

#[cfg(test)]
mod ttl_bucket_tests {
    use soroban_sdk::testutils::storage::Persistent;
    use soroban_sdk::testutils::{Address as AddressTrait, Ledger};
    use soroban_sdk::{Address, Env, String};

    use crate::{DataKey, SavingsGoalContract, SavingsGoalContractClient};

    fn env_with_ttl_headroom() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        // Raise the ledger's max entry TTL so the persistent bump amount
        // (60 days of ledgers) is representable in the test host.
        env.ledger().with_mut(|li| {
            li.max_entry_ttl = 3_000_000;
            li.min_persistent_entry_ttl = 16;
            li.timestamp = 1_700_000_000;
        });
        env
    }

    /// A freshly created goal's persistent entry must carry the persistent
    /// bucket's bump amount — not the instance bucket's 518_400 ledgers the
    /// old constants applied.
    #[test]
    fn test_goal_entry_bumped_with_persistent_bucket() {
        let env = env_with_ttl_headroom();
        let contract_id = env.register_contract(None, SavingsGoalContract);
        let client = SavingsGoalContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "vacation");
        let goal_id = client.create_goal(&owner, &name, &1000, &1_800_000_000, &false);

        let ttl = env.as_contract(&contract_id, || {
            env.storage().persistent().get_ttl(&DataKey::Goal(goal_id))
        });

        assert_eq!(
            ttl,
            remitwise_common::PERSISTENT_BUMP_AMOUNT,
            "persistent goal entry must be bumped with the persistent bucket"
        );
        assert_ne!(
            ttl, 518_400,
            "regression guard: 518_400 is the old instance-bucket bump"
        );
    }

    /// The crate's constants must stay bound to the shared persistent bucket.
    #[test]
    fn test_constants_match_shared_persistent_bucket() {
        assert_eq!(
            crate::PERSISTENT_LIFETIME_THRESHOLD,
            remitwise_common::PERSISTENT_LIFETIME_THRESHOLD
        );
        assert_eq!(
            crate::PERSISTENT_BUMP_AMOUNT,
            remitwise_common::PERSISTENT_BUMP_AMOUNT
        );
        // And the two buckets must actually differ, or the distinction is dead.
        assert!(
            remitwise_common::PERSISTENT_BUMP_AMOUNT > remitwise_common::INSTANCE_BUMP_AMOUNT,
            "persistent bump must exceed instance bump"
        );
    }
}
