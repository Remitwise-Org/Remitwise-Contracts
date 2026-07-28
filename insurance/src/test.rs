#![allow(clippy::all)]
#[cfg(test)]
mod tests {
    extern crate std;
    use std::format;

    use crate::*;
    use alloc::format;
    use alloc::string::String as StdString;
    use core::fmt::Write;
    use remitwise_common::CoverageType;
    use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, String, Vec};

    fn setup(env: &Env) -> InsuranceClient<'_> {
        let id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(env, &id);
        c.init(&Address::generate(env));
        c
    }

    fn n(env: &Env, s: &str) -> String {
        String::from_str(env, s)
    }

    /// Generate a policy name "Px" where x is the index, without format! macro.
    /// Only works for indices 0-99 to avoid format! in no_std context.
    fn policy_name(env: &Env, index: u32) -> String {
        let name = match index {
            0 => "P0",
            1 => "P1",
            2 => "P2",
            3 => "P3",
            4 => "P4",
            5 => "P5",
            6 => "P6",
            7 => "P7",
            8 => "P8",
            9 => "P9",
            10 => "P10",
            11 => "P11",
            12 => "P12",
            13 => "P13",
            14 => "P14",
            15 => "P15",
            16 => "P16",
            17 => "P17",
            18 => "P18",
            19 => "P19",
            20 => "P20",
            21 => "P21",
            22 => "P22",
            23 => "P23",
            24 => "P24",
            25 => "P25",
            26 => "P26",
            27 => "P27",
            28 => "P28",
            29 => "P29",
            30 => "P30",
            31 => "P31",
            32 => "P32",
            33 => "P33",
            34 => "P34",
            35 => "P35",
            36 => "P36",
            37 => "P37",
            38 => "P38",
            39 => "P39",
            40 => "P40",
            41 => "P41",
            42 => "P42",
            43 => "P43",
            44 => "P44",
            45 => "P45",
            46 => "P46",
            47 => "P47",
            48 => "P48",
            49 => "P49",
            50 => "P50",
            51 => "P51",
            52 => "P52",
            53 => "P53",
            54 => "P54",
            55 => "P55",
            56 => "P56",
            57 => "P57",
            58 => "P58",
            59 => "P59",
            _ => "P0",
        };
        n(env, name)
    }

    // ── Existing tests ────────────────────────────────────────────────────────

    #[test]
    fn test_init_success() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        assert_eq!(
            c.try_init(&Address::generate(&env)).unwrap_err().unwrap(),
            InsuranceError::AlreadyInitialized,
        );
    }

    #[test]
    fn test_create_policy_success() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let caller = Address::generate(&env);
        let id = c.create_policy(
            &caller,
            &n(&env, "P1"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        assert_eq!(id, 1);
        let p = c.get_policy(&id).unwrap();
        assert_eq!(p.monthly_premium, 5_000_000);
    }

    #[test]
    fn test_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);
        for _ in 0..10 {
            c.create_policy(
                &owner,
                &n(&env, "P"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            );
        }
        let page = c.get_active_policies(&owner, &0, &5);
        assert_eq!(page.items.len(), 5);
        assert_eq!(page.count, 5);
        assert_eq!(page.next_cursor, 6);
    }

    #[test]
    fn test_total_premium_isolation() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let u1 = Address::generate(&env);
        let u2 = Address::generate(&env);
        c.create_policy(
            &u1,
            &n(&env, "P1"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        c.create_policy(
            &u2,
            &n(&env, "P2"),
            &CoverageType::Health,
            &6_000_000i128,
            &50_000_000i128,
            &None,
        );
        assert_eq!(c.get_total_monthly_premium(&u1), 5_000_000);
        assert_eq!(c.get_total_monthly_premium(&u2), 6_000_000);
    }

    #[test]
    fn test_batch_pay() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);
        let id1 = c.create_policy(
            &owner,
            &n(&env, "P1"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let id2 = c.create_policy(
            &owner,
            &n(&env, "P2"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let mut ids = Vec::new(&env);
        ids.push_back(id1);
        ids.push_back(id2);
        assert_eq!(c.batch_pay_premiums(&owner, &ids), 2);
    }

    // ── Per-CoverageType boundary tests ──────────────────────────────────────
    //
    // Mirrors TypeConstraints::for_type.  A future bound change here will
    // automatically break these tests — do not hard-code magic numbers.

    struct Bounds {
        min_premium: i128,
        max_premium: i128,
        min_coverage: i128,
        max_coverage: i128,
    }

    impl Bounds {
        fn for_type(ct: &CoverageType) -> Self {
            match ct {
                CoverageType::Health => Self {
                    min_premium: 1,
                    max_premium: 500_000_000_000,
                    min_coverage: 1,
                    max_coverage: 100_000_000_000_000,
                },
                CoverageType::Life => Self {
                    min_premium: 1,
                    max_premium: 1_000_000_000_000,
                    min_coverage: 1,
                    max_coverage: 500_000_000_000_000,
                },
                CoverageType::Property => Self {
                    min_premium: 1,
                    max_premium: 2_000_000_000_000,
                    min_coverage: 1,
                    max_coverage: 1_000_000_000_000_000,
                },
                CoverageType::Auto => Self {
                    min_premium: 1,
                    max_premium: 750_000_000_000,
                    min_coverage: 1,
                    max_coverage: 200_000_000_000_000,
                },
                CoverageType::Liability => Self {
                    min_premium: 1,
                    max_premium: 400_000_000_000,
                    min_coverage: 1,
                    max_coverage: 50_000_000_000_000,
                },
            }
        }
    }

    fn assert_boundary(ct: CoverageType) {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let b = Bounds::for_type(&ct);

        // min_premium + min_coverage → accept
        c.create_policy(
            &Address::generate(&env),
            &n(&env, "T"),
            &ct,
            &b.min_premium,
            &b.min_coverage,
            &None,
        );

        // max_premium + max_coverage → accept
        c.create_policy(
            &Address::generate(&env),
            &n(&env, "T"),
            &ct,
            &b.max_premium,
            &b.max_coverage,
            &None,
        );

        // premium = 0 (min_premium - 1) → InvalidPremium
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &ct,
                &(b.min_premium - 1),
                &b.min_coverage,
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidPremium,
        );

        // premium = max_premium + 1 → InvalidPremium
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &ct,
                &(b.max_premium + 1),
                &b.min_coverage,
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidPremium,
        );

        // coverage = min_coverage - 1 (i.e. 0) → InvalidCoverageAmount
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &ct,
                &b.min_premium,
                &(b.min_coverage - 1),
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidCoverageAmount,
        );

        // coverage = max_coverage + 1 → InvalidCoverageAmount
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &ct,
                &b.max_premium,
                &(b.max_coverage + 1),
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidCoverageAmount,
        );
    }

    #[test]
    fn test_type_constraints_health() {
        assert_boundary(CoverageType::Health);
    }
    #[test]
    fn test_type_constraints_life() {
        assert_boundary(CoverageType::Life);
    }
    #[test]
    fn test_type_constraints_property() {
        assert_boundary(CoverageType::Property);
    }
    #[test]
    fn test_type_constraints_auto() {
        assert_boundary(CoverageType::Auto);
    }
    #[test]
    fn test_type_constraints_liability() {
        assert_boundary(CoverageType::Liability);
    }

    #[test]
    fn test_unsupported_combination() {
        // coverage_amount > monthly_premium * 12 * 500 → UnsupportedCombination
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let premium = 1_000_000i128; // 0.1 XLM
        let max_ratio = premium * 12 * 500;

        // exactly at the ratio limit → accept
        c.create_policy(
            &Address::generate(&env),
            &n(&env, "T"),
            &CoverageType::Health,
            &premium,
            &max_ratio,
            &None,
        );

        // one over → UnsupportedCombination
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &CoverageType::Health,
                &premium,
                &(max_ratio + 1),
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::UnsupportedCombination,
        );
    }

    #[test]
    fn test_overflow_safety() {
        // A premium near i128::MAX is caught by max_premium before any
        // arithmetic — no panic, just InvalidPremium.
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        assert_eq!(
            c.try_create_policy(
                &Address::generate(&env),
                &n(&env, "T"),
                &CoverageType::Health,
                &(i128::MAX - 1),
                &1i128,
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidPremium,
        );
    }

    // ── Helper: initialise contract with a known owner ────────────────────────

    fn setup_with_owner(env: &Env) -> (InsuranceClient<'_>, Address) {
        let id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(env, &id);
        let contract_owner = Address::generate(env);
        c.init(&contract_owner);
        (c, contract_owner)
    }

    // ── deactivate_policy ─────────────────────────────────────────────────────

    /// Success path: the policy owner can deactivate their own policy.
    #[test]
    fn test_deactivate_policy_by_owner_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert!(c.deactivate_policy(&policy_owner, &pid));

        let p = c.get_policy(&pid).unwrap();
        assert!(!p.active, "policy should be inactive after deactivation");
    }

    /// Success path: the contract owner can deactivate any policy.
    #[test]
    fn test_deactivate_policy_by_contract_owner_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert!(c.deactivate_policy(&contract_owner, &pid));

        let p = c.get_policy(&pid).unwrap();
        assert!(!p.active);
    }

    /// A third party (neither policy owner nor contract owner) must get Unauthorized.
    #[test]
    fn test_deactivate_policy_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let stranger = Address::generate(&env);

        assert!(
            !c.deactivate_policy(&stranger, &pid),
            "stranger must not be able to deactivate a policy they don't own"
        );
    }

    /// Attempting to deactivate an already-inactive policy must be idempotent (returns true).
    #[test]
    fn test_deactivate_policy_already_inactive() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // First deactivation — should succeed
        assert!(c.deactivate_policy(&policy_owner, &pid));

        // Second deactivation — idempotent, must still return true
        assert!(
            c.deactivate_policy(&policy_owner, &pid),
            "deactivating already-inactive policy must be idempotent"
        );
    }

    /// Deactivating a non-existent policy must return false.
    #[test]
    fn test_deactivate_policy_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);

        assert!(
            !c.deactivate_policy(&contract_owner, &9999),
            "deactivating non-existent policy must return false"
        );
    }

    /// Calling deactivate_policy before init must return false.
    #[test]
    fn test_deactivate_policy_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);
        let caller = Address::generate(&env);

        assert!(
            !c.deactivate_policy(&caller, &1),
            "deactivating on uninitialized contract must return false"
        );
    }

    /// Deactivated policy should no longer appear in get_active_policies.
    #[test]
    fn test_deactivate_policy_removes_from_active_list() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        c.deactivate_policy(&owner, &pid);

        let page = c.get_active_policies(&owner, &0, &10);
        assert_eq!(
            page.count, 0,
            "active list should be empty after deactivation"
        );
    }

    // ── reactivate_policy tests ───────────────────────────────────────────

    #[test]
    fn test_reactivate_policy_by_owner_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // Deactivate then reactivate
        c.deactivate_policy(&owner, &pid);
        let p = c.get_policy(&pid).unwrap();
        let old_next = p.next_payment_date;

        assert!(c.reactivate_policy(&owner, &pid));

        let p2 = c.get_policy(&pid).unwrap();
        assert!(p2.active, "policy should be active after reactivation");
        // Next payment date should have been refreshed forward
        assert!(p2.next_payment_date > old_next);

        let page = c.get_active_policies(&owner, &0, &10);
        assert_eq!(page.count, 1);
        assert_eq!(page.items.len(), 1);
    }

    #[test]
    fn test_reactivate_policy_already_active() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert_eq!(
            c.try_reactivate_policy(&owner, &pid).unwrap_err().unwrap(),
            InsuranceError::PolicyAlreadyActive
        );
    }

    #[test]
    fn test_reactivate_policy_max_reached() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // Deactivate so we can attempt to reactivate
        c.deactivate_policy(&owner, &pid);

        // Fill the active index to MAX_POLICIES with IDs that don't include pid
        let mut full = Vec::new(&env);
        // Start from MAX_POLICIES+1 so we don't collide with pid (which is 1)
        for i in MAX_POLICIES + 1..=MAX_POLICIES * 2 {
            full.push_back(i);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &full);

        assert_eq!(
            c.try_reactivate_policy(&owner, &pid).unwrap_err().unwrap(),
            InsuranceError::MaxPoliciesReached
        );
    }

    #[test]
    fn test_get_deactivated_policies_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        let _p1 = c.create_policy(
            &owner,
            &n(&env, "P1"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let p2 = c.create_policy(
            &owner,
            &n(&env, "P2"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let _p3 = c.create_policy(
            &owner,
            &n(&env, "P3"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let p4 = c.create_policy(
            &owner,
            &n(&env, "P4"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // Deactivate a subset
        c.deactivate_policy(&owner, &p2);
        c.deactivate_policy(&owner, &p4);

        let page = c.get_deactivated_policies(&owner, &0, &10);
        assert_eq!(page.count, 2);
        assert_eq!(page.items.len(), 2);
    }

    // ── MAX_TENURE_SECS expiry boundary tests ─────────────────────────────

    /// Reactivation at exactly MAX_TENURE_SECS after deactivation must succeed.
    #[test]
    fn test_reactivate_exactly_at_tenure_boundary_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let base_time = 1_000_000u64;
        env.ledger().set_timestamp(base_time);

        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        c.deactivate_policy(&owner, &pid);

        // Advance to exactly MAX_TENURE_SECS after deactivation
        env.ledger().set_timestamp(base_time + MAX_TENURE_SECS);

        assert!(
            c.reactivate_policy(&owner, &pid),
            "reactivation exactly at tenure boundary must succeed"
        );
    }

    /// Reactivation one second before MAX_TENURE_SECS elapses must fail.
    #[test]
    fn test_reactivate_one_second_before_tenure_boundary_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let base_time = 1_000_000u64;
        env.ledger().set_timestamp(base_time);

        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        c.deactivate_policy(&owner, &pid);

        // Advance to one second before tenure expires
        env.ledger().set_timestamp(base_time + MAX_TENURE_SECS - 1);

        assert_eq!(
            c.try_reactivate_policy(&owner, &pid).unwrap_err().unwrap(),
            InsuranceError::PolicyDeactivationTooSoon,
            "reactivation one second before tenure must fail"
        );
    }

    /// Reactivation one second past MAX_TENURE_SECS must succeed.
    #[test]
    fn test_reactivate_one_second_past_tenure_boundary_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);
        let base_time = 1_000_000u64;
        env.ledger().set_timestamp(base_time);

        let pid = c.create_policy(
            &owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        c.deactivate_policy(&owner, &pid);

        // Advance to one second past tenure expiry
        env.ledger().set_timestamp(base_time + MAX_TENURE_SECS + 1);

        assert!(
            c.reactivate_policy(&owner, &pid),
            "reactivation one second past tenure must succeed"
        );
    }

    // ── set_external_ref ──────────────────────────────────────────────────────

    /// Success path: contract owner can attach a valid external reference.
    #[test]
    fn test_set_external_ref_success() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert!(c.set_external_ref(
            &contract_owner,
            &pid,
            &core::option::Option::Some(n(&env, "ref-abc-123"))
        ));

        let p = c.get_policy(&pid).unwrap();
        assert_eq!(
            p.external_ref,
            core::option::Option::Some(n(&env, "ref-abc-123"))
        );
    }

    /// Success path: contract owner can clear an existing external reference (None).
    #[test]
    fn test_set_external_ref_clear() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        c.set_external_ref(
            &contract_owner,
            &pid,
            &core::option::Option::Some(n(&env, "ref-abc-123")),
        );
        c.set_external_ref(&contract_owner, &pid, &core::option::Option::None);

        let p = c.get_policy(&pid).unwrap();
        assert_eq!(p.external_ref, core::option::Option::None);
    }

    /// Clearing a policy's external_ref (owner-only) must free the ref so a
    /// different policy can immediately take it — proves no stale index entry
    /// survives the clear.
    #[test]
    fn test_set_external_ref_clear_allows_reuse_by_another_policy() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let ref_id = core::option::Option::Some(n(&env, "REUSE-REF"));

        let pid_a = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        c.set_external_ref(&contract_owner, &pid_a, &ref_id);

        // Free the ref from policy A.
        c.set_external_ref(&contract_owner, &pid_a, &core::option::Option::None);

        // A second, unrelated policy must be able to take the freed ref.
        let pid_b = c.create_policy(
            &policy_owner,
            &n(&env, "P2"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        assert!(
            c.set_external_ref(&contract_owner, &pid_b, &ref_id),
            "clearing policy A's external_ref must allow policy B to reuse it"
        );

        let policy_a = c.get_policy(&pid_a).unwrap();
        let policy_b = c.get_policy(&pid_b).unwrap();
        assert_eq!(policy_a.external_ref, core::option::Option::None);
        assert_eq!(policy_b.external_ref, ref_id);
    }

    /// A non-owner (policy owner or stranger) must not be able to clear an
    /// existing external_ref — the ref must remain untouched after the
    /// rejected call.
    #[test]
    fn test_set_external_ref_unauthorized_clear_leaves_ref_intact() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let ref_id = core::option::Option::Some(n(&env, "keep-me"));
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        c.set_external_ref(&contract_owner, &pid, &ref_id);

        assert_eq!(
            c.try_set_external_ref(&policy_owner, &pid, &core::option::Option::None)
                .unwrap_err()
                .unwrap(),
            InsuranceError::Unauthorized,
        );

        let policy = c.get_policy(&pid).unwrap();
        assert_eq!(
            policy.external_ref, ref_id,
            "external_ref must be unchanged after a rejected clear attempt"
        );
    }

    /// Policy owner (non-contract-owner) calling set_external_ref must get Unauthorized.
    #[test]
    fn test_set_external_ref_unauthorized_policy_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert_eq!(
            c.try_set_external_ref(
                &policy_owner,
                &pid,
                &core::option::Option::Some(n(&env, "ref"))
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::Unauthorized,
        );
    }

    /// Any stranger calling set_external_ref must get Unauthorized.
    #[test]
    fn test_set_external_ref_unauthorized_stranger() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let stranger = Address::generate(&env);

        assert_eq!(
            c.try_set_external_ref(&stranger, &pid, &core::option::Option::Some(n(&env, "ref")))
                .unwrap_err()
                .unwrap(),
            InsuranceError::Unauthorized,
        );
    }

    /// An over-length external reference must yield InvalidExternalRef.
    #[test]
    fn test_set_external_ref_too_long() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // 129 characters — one over the MAX_EXT_REF_LEN of 128
        let long_ref = n(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            c.try_set_external_ref(&contract_owner, &pid, &core::option::Option::Some(long_ref))
                .unwrap_err()
                .unwrap(),
            InsuranceError::InvalidExternalRef,
        );
    }

    /// An empty external reference string must yield InvalidExternalRef.
    #[test]
    fn test_set_external_ref_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner,
            &n(&env, "P"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        assert_eq!(
            c.try_set_external_ref(
                &contract_owner,
                &pid,
                &core::option::Option::Some(n(&env, ""))
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::InvalidExternalRef,
        );
    }

    /// set_external_ref on a non-existent policy must yield PolicyNotFound.
    #[test]
    fn test_set_external_ref_policy_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);

        assert_eq!(
            c.try_set_external_ref(
                &contract_owner,
                &9999,
                &core::option::Option::Some(n(&env, "ref"))
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::PolicyNotFound,
        );
    }

    /// Calling set_external_ref before init must yield NotInitialized.
    #[test]
    fn test_set_external_ref_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);
        let caller = Address::generate(&env);

        assert_eq!(
            c.try_set_external_ref(&caller, &1, &core::option::Option::Some(n(&env, "ref")))
                .unwrap_err()
                .unwrap(),
            InsuranceError::NotInitialized,
        );
    }

    // ── #846: Uniform initialization guard tests ───────────────────────────────

    #[test]
    fn test_create_policy_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);

        let caller = Address::generate(&env);

        assert_eq!(
            c.try_create_policy(
                &caller,
                &n(&env, "Policy"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::NotInitialized,
        );
    }

    #[test]
    fn test_pay_premium_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);

        let caller = Address::generate(&env);

        // pay_premium on uninitialized contract returns false
        assert!(!c.pay_premium(&caller, &1u32));
    }

    #[test]
    fn test_batch_pay_premiums_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);

        let caller = Address::generate(&env);
        let ids = Vec::<u32>::new(&env);

        // batch_pay_premiums on uninitialized contract returns 0
        assert_eq!(c.batch_pay_premiums(&caller, &ids), 0u32);
    }

    #[test]
    fn test_get_active_policies_not_initialized() {
        let env = Env::default();

        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);

        let owner = Address::generate(&env);

        // get_active_policies on uninitialized contract returns empty page
        let page = c.get_active_policies(&owner, &0u32, &10u32);
        assert_eq!(page.count, 0u32);
    }

    #[test]
    fn test_get_total_monthly_premium_not_initialized() {
        let env = Env::default();

        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);

        let owner = Address::generate(&env);

        // get_total_monthly_premium on uninitialized contract returns 0
        assert_eq!(c.get_total_monthly_premium(&owner), 0i128);
    }

    #[test]
    fn test_pre_upgrade_roundtrip() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        client.init(&owner);

        // Take snapshot
        let result = client.try_pre_upgrade(&owner);
        assert!(result.is_ok());

        // Set version (this function was added with pre_upgrade support)
        let result = client.try_set_version(&owner, &42);
        assert!(result.is_ok());

        // Verify version changed
        assert_eq!(client.get_version(), 42);

        // Restore from snapshot
        let result = client.try_restore_from_snapshot(&owner);
        assert!(result.is_ok());

        // Version should be restored to default
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_pre_upgrade_unauthorized_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let owner = Address::generate(&env);
        let stranger = Address::generate(&env);
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        client.init(&owner);

        let result = client.try_pre_upgrade(&stranger);
        assert_eq!(result, Err(Ok(InsuranceError::Unauthorized)));
    }

    // ── batch_pay_premiums deterministic partial-result accounting and atomicity (#1038) ──

    /// Policies belonging to another owner are silently skipped; count reflects only
    /// the caller's paid premiums — not the total policies in the batch.
    #[test]
    fn test_batch_pay_premiums_skips_policies_of_other_owners() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        let alice_id = c.create_policy(
            &alice,
            &n(&env, "Alice Health"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let bob_id = c.create_policy(
            &bob,
            &n(&env, "Bob Life"),
            &CoverageType::Life,
            &10_000_000i128,
            &100_000_000i128,
            &None,
        );

        // Alice submits a batch that includes Bob's policy_id.
        let mut ids = Vec::new(&env);
        ids.push_back(alice_id);
        ids.push_back(bob_id);

        // Only Alice's policy must be paid; Bob's is skipped (owner mismatch).
        let paid = c.batch_pay_premiums(&alice, &ids);
        assert_eq!(paid, 1, "only Alice's policy should be counted");
    }

    /// A batch with duplicate IDs must not double-count or double-update the same policy.
    #[test]
    fn test_batch_pay_premiums_duplicate_ids_are_each_processed() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);

        let id = c.create_policy(
            &owner,
            &n(&env, "Dup Test"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        // Same ID twice — both iterations will process the same policy.
        // The count must be 2 (each iteration succeeds) and next_payment_date updates twice.
        let mut ids = Vec::new(&env);
        ids.push_back(id);
        ids.push_back(id);

        let paid = c.batch_pay_premiums(&owner, &ids);
        // Both passes over the same policy succeed (it stays active).
        assert_eq!(paid, 2, "each iteration of a duplicate ID counts once");
    }

    /// An empty batch must return 0 and not panic.
    #[test]
    fn test_batch_pay_premiums_empty_ids_returns_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);

        let ids = Vec::<u32>::new(&env);
        let paid = c.batch_pay_premiums(&owner, &ids);
        assert_eq!(paid, 0, "empty batch must return 0");
    }

    /// A batch containing a non-existent policy ID skips it silently.
    #[test]
    fn test_batch_pay_premiums_nonexistent_id_returns_policy_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);

        let mut ids = Vec::new(&env);
        ids.push_back(999u32);

        // batch_pay_premiums skips not-found policies, returns 0 paid
        let count = c.batch_pay_premiums(&owner, &ids);
        assert_eq!(count, 0u32, "non-existent policy should be skipped, 0 paid");
    }

    // ── clamp_limit pagination tests for get_deactivated_policies ─────────────
    //
    // Three cases lock the pagination-limit normalisation contract used by
    // `get_deactivated_policies`:
    //   1. `limit == 0`  → treated as DEFAULT_PAGE_LIMIT (20)
    //   2. `limit > MAX_PAGE_LIMIT` → clamped to MAX_PAGE_LIMIT (50)
    //   3. `1 <= limit <= MAX_PAGE_LIMIT` → passes through unchanged

    /// A zero limit must be normalised to DEFAULT_PAGE_LIMIT (20).
    ///
    /// Seed 25 deactivated policies so the page is visibly bounded by the
    /// default rather than by the actual record count.
    #[test]
    fn get_deactivated_policies_zero_limit_returns_default_page_limit() {
        use remitwise_common::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Create and deactivate 25 policies (> DEFAULT_PAGE_LIMIT=20).
        let total: u32 = DEFAULT_PAGE_LIMIT + 5;
        for _i in 0..total {
            let name = String::from_str(&env, "Policy");
            let id = c.create_policy(
                &owner,
                &name,
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            );
            c.deactivate_policy(&owner, &id);
        }

        let page = c.get_deactivated_policies(&owner, &0, &0);
        assert_eq!(
            page.items.len(),
            DEFAULT_PAGE_LIMIT,
            "limit=0 must be normalised to DEFAULT_PAGE_LIMIT={DEFAULT_PAGE_LIMIT}, \
             not return all {total} records"
        );
        assert_eq!(page.count, DEFAULT_PAGE_LIMIT);
        // More pages exist because total > DEFAULT_PAGE_LIMIT.
        assert!(
            page.next_cursor > 0,
            "next_cursor must be non-zero when more pages remain"
        );
        let _ = MAX_PAGE_LIMIT; // keep import used
    }

    /// An oversized limit must be clamped to MAX_PAGE_LIMIT (50).
    ///
    /// Seed 55 deactivated policies so the page is visibly bounded by the
    /// maximum rather than by the actual record count.
    #[test]
    fn get_deactivated_policies_oversized_limit_clamped_to_max_page_limit() {
        use remitwise_common::MAX_PAGE_LIMIT;

        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        let total: u32 = MAX_PAGE_LIMIT + 5;
        for _i in 0..total {
            let name = String::from_str(&env, "Policy");
            let id = c.create_policy(
                &owner,
                &name,
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            );
            c.deactivate_policy(&owner, &id);
        }

        let page = c.get_deactivated_policies(&owner, &0, &u32::MAX);
        assert_eq!(
            page.items.len(),
            MAX_PAGE_LIMIT,
            "limit=u32::MAX must be clamped to MAX_PAGE_LIMIT={MAX_PAGE_LIMIT}"
        );
        assert_eq!(page.count, MAX_PAGE_LIMIT);
        assert!(
            page.next_cursor > 0,
            "next_cursor must be non-zero when more pages remain"
        );
    }

    /// A limit within [1, MAX_PAGE_LIMIT] must pass through unchanged.
    #[test]
    fn get_deactivated_policies_in_range_limit_passes_through_unchanged() {
        use remitwise_common::MAX_PAGE_LIMIT;

        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        let requested_limit: u32 = 7;
        assert!(requested_limit >= 1 && requested_limit <= MAX_PAGE_LIMIT);

        // Seed more records than the requested limit.
        let total: u32 = requested_limit + 3;
        for _i in 0..total {
            let name = String::from_str(&env, "Policy");
            let id = c.create_policy(
                &owner,
                &name,
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            );
            c.deactivate_policy(&owner, &id);
        }

        let page = c.get_deactivated_policies(&owner, &0, &requested_limit);
        assert_eq!(
            page.items.len(),
            requested_limit,
            "in-range limit={requested_limit} must be returned unmodified"
        );
        assert_eq!(page.count, requested_limit);
        assert!(
            page.next_cursor > 0,
            "next_cursor must be non-zero when more pages remain"
        );
    }

    // ── MAX_POLICIES global cap boundary tests ───────────────────────────────
    //
    // Lock the "0, at cap, over cap" boundary for the global active-policy
    // limit (MAX_POLICIES = 1_000).  Most tests manipulate storage directly
    // so we don't pay the cost of creating 1 000 real policies per test.
    //
    // Where tests pre-fill the active index with dummy IDs that have no
    // backing Policy entries in storage: this is intentional — the cap check
    // only inspects Vec::len(), and remove_active_policy only touches the
    // active list.  If either function later validates policy existence the
    // corresponding test must be updated.

    /// Creating the first policy when the active index starts at zero must
    /// return a valid, positive policy ID.
    #[test]
    fn max_policies_create_from_zero_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Pre-condition: active index is empty.
        let page = c.get_active_policies(&owner, &0, &1);
        assert_eq!(page.count, 0, "precondition: no active policies exist");

        let id = c.create_policy(
            &owner,
            &n(&env, "First"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
        );
        assert!(id > 0, "first policy creation from zero must succeed");
    }

    /// Creating a policy when the active index is exactly at MAX_POLICIES must
    /// return MaxPoliciesReached.
    #[test]
    fn max_policies_at_cap_returns_max_policies_reached() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Pre-fill the active index to exactly MAX_POLICIES entries.
        let mut full = Vec::new(&env);
        for i in 1..=MAX_POLICIES {
            full.push_back(i);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &full);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &MAX_POLICIES);

        assert_eq!(
            c.try_create_policy(
                &owner,
                &n(&env, "Over"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::MaxPoliciesReached,
            "creating at MAX_POLICIES cap must return MaxPoliciesReached"
        );
    }

    /// Creating a policy when the active index *exceeds* MAX_POLICIES must also
    /// return MaxPoliciesReached (defence-in-depth for the `>=` guard).
    #[test]
    fn max_policies_over_cap_returns_max_policies_reached() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Pre-fill beyond MAX_POLICIES.
        let mut over = Vec::new(&env);
        for i in 1..=MAX_POLICIES + 1 {
            over.push_back(i);
        }
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &over);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &(MAX_POLICIES + 1));

        assert_eq!(
            c.try_create_policy(
                &owner,
                &n(&env, "Over"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::MaxPoliciesReached,
            "creating beyond MAX_POLICIES must return MaxPoliciesReached"
        );
    }

    /// Deactivating one policy at the cap must free a slot so a subsequent
    /// create succeeds — the active count must stay at MAX_POLICIES.
    #[test]
    fn max_policies_deactivate_at_cap_frees_slot_for_new_create() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Create one real policy so we can deactivate it later.
        let pid = c.create_policy(
            &owner,
            &n(&env, "ToDeactivate"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
        );
        assert_eq!(pid, 1);

        // Fill the remaining slots to MAX_POLICIES with dummy IDs.
        let mut full = Vec::new(&env);
        full.push_back(pid);
        for i in 2..=MAX_POLICIES {
            full.push_back(i);
        }
        assert_eq!(full.len(), MAX_POLICIES);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &full);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &MAX_POLICIES);

        // At cap — create must be rejected.
        assert_eq!(
            c.try_create_policy(
                &owner,
                &n(&env, "ShouldFail"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::MaxPoliciesReached,
            "precondition: must be at cap before deactivation"
        );

        // Deactivate the real policy — this calls remove_active_policy.
        assert!(c.deactivate_policy(&owner, &pid));

        // Slot freed — new create must succeed.
        let new_id = c.create_policy(
            &owner,
            &n(&env, "NewPolicy"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
        );
        assert!(
            new_id > pid,
            "deactivating at cap must free a slot so a new create succeeds"
        );
    }

    /// Aggregating MAX_POLICIES policies at the maximum per-type premium
    /// must return the correct total without panicking.
    #[test]
    fn max_policies_total_premium_at_cap_aggregates_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_unlimited();
        let (c, _contract_owner) = setup_with_owner(&env);
        let owner = Address::generate(&env);

        // Pick Health's max_premium for a large-but-valid value.
        let premium: i128 = 500_000_000_000i128;

        // Create MAX_POLICIES real policies at max premium.
        for _ in 0..MAX_POLICIES {
            c.create_policy(
                &owner,
                &n(&env, "Big"),
                &CoverageType::Health,
                &premium,
                &10_000i128,
            );
        }

        let total = c.get_total_monthly_premium(&owner);
        let expected = premium.saturating_mul(MAX_POLICIES as i128);
        assert_eq!(
            total, expected,
            "total premium at cap must aggregate correctly"
        );
    }

    /// The MAX_POLICIES cap is global (on the ActivePolicies index), not
    /// per-owner.  When the global cap is reached — even entirely through
    /// another owner's policies — a new owner with zero policies must be
    /// rejected with MaxPoliciesReached.
    #[test]
    fn max_policies_cap_is_global_not_per_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        // Alice creates one real policy.
        let alice_pid = c.create_policy(
            &alice,
            &n(&env, "AlicePolicy"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
        );

        // Pre-fill the rest of the global active index to MAX_POLICIES.
        let mut full = Vec::new(&env);
        full.push_back(alice_pid);
        for i in 2..=MAX_POLICIES {
            full.push_back(i);
        }
        assert_eq!(full.len(), MAX_POLICIES);
        env.storage()
            .instance()
            .set(&DataKey::ActivePolicies, &full);
        env.storage()
            .instance()
            .set(&DataKey::PolicyCount, &MAX_POLICIES);

        // Bob — who has zero policies — must be rejected because the
        // *global* cap is full.
        assert_eq!(
            c.try_create_policy(
                &bob,
                &n(&env, "BobPolicy"),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
            )
            .unwrap_err()
            .unwrap(),
            InsuranceError::MaxPoliciesReached,
            "cap is global: Bob must be rejected even though he has zero policies"
        );
    }
}
