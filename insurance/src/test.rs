#[cfg(test)]
mod tests {
    use crate::*;
    use remitwise_common::CoverageType;
    use soroban_sdk::{
        testutils::Address as _,
        Address, Env, String, Vec,
    };

    fn setup(env: &Env) -> InsuranceClient {
        let id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(env, &id);
        c.init(&Address::generate(env));
        c
    }

    fn n(env: &Env, s: &str) -> String { String::from_str(env, s) }

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
        let id = c.create_policy(&caller, &n(&env, "P1"), &CoverageType::Health, &5_000_000i128, &50_000_000i128);
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
            c.create_policy(&owner, &n(&env, "P"), &CoverageType::Health, &5_000_000i128, &50_000_000i128);
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
        c.create_policy(&u1, &n(&env, "P1"), &CoverageType::Health, &5_000_000i128, &50_000_000i128);
        c.create_policy(&u2, &n(&env, "P2"), &CoverageType::Health, &6_000_000i128, &50_000_000i128);
        assert_eq!(c.get_total_monthly_premium(&u1), 5_000_000);
        assert_eq!(c.get_total_monthly_premium(&u2), 6_000_000);
    }

    #[test]
    fn test_batch_pay() {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let owner = Address::generate(&env);
        let id1 = c.create_policy(&owner, &n(&env, "P1"), &CoverageType::Health, &5_000_000i128, &50_000_000i128);
        let id2 = c.create_policy(&owner, &n(&env, "P2"), &CoverageType::Health, &5_000_000i128, &50_000_000i128);
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
                CoverageType::Health    => Self { min_premium: 1, max_premium: 500_000_000_000,   min_coverage: 1, max_coverage: 100_000_000_000_000   },
                CoverageType::Life      => Self { min_premium: 1, max_premium: 1_000_000_000_000, min_coverage: 1, max_coverage: 500_000_000_000_000   },
                CoverageType::Property  => Self { min_premium: 1, max_premium: 2_000_000_000_000, min_coverage: 1, max_coverage: 1_000_000_000_000_000 },
                CoverageType::Auto      => Self { min_premium: 1, max_premium: 750_000_000_000,   min_coverage: 1, max_coverage: 200_000_000_000_000   },
                CoverageType::Liability => Self { min_premium: 1, max_premium: 400_000_000_000,   min_coverage: 1, max_coverage: 50_000_000_000_000    },
            }
        }
    }

    fn assert_boundary(ct: CoverageType) {
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let b = Bounds::for_type(&ct);

        // min_premium + min_coverage → accept
        c.create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &b.min_premium, &b.min_coverage);

        // max_premium + max_coverage → accept
        c.create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &b.max_premium, &b.max_coverage);

        // premium = 0 (min_premium - 1) → InvalidPremium
        assert_eq!(
            c.try_create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &(b.min_premium - 1), &b.min_coverage)
                .unwrap_err().unwrap(),
            InsuranceError::InvalidPremium,
        );

        // premium = max_premium + 1 → InvalidPremium
        assert_eq!(
            c.try_create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &(b.max_premium + 1), &b.min_coverage)
                .unwrap_err().unwrap(),
            InsuranceError::InvalidPremium,
        );

        // coverage = min_coverage - 1 (i.e. 0) → InvalidCoverageAmount
        assert_eq!(
            c.try_create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &b.min_premium, &(b.min_coverage - 1))
                .unwrap_err().unwrap(),
            InsuranceError::InvalidCoverageAmount,
        );

        // coverage = max_coverage + 1 → InvalidCoverageAmount
        assert_eq!(
            c.try_create_policy(&Address::generate(&env), &n(&env, "T"), &ct, &b.max_premium, &(b.max_coverage + 1))
                .unwrap_err().unwrap(),
            InsuranceError::InvalidCoverageAmount,
        );
    }

    #[test] fn test_type_constraints_health()    { assert_boundary(CoverageType::Health); }
    #[test] fn test_type_constraints_life()      { assert_boundary(CoverageType::Life); }
    #[test] fn test_type_constraints_property()  { assert_boundary(CoverageType::Property); }
    #[test] fn test_type_constraints_auto()      { assert_boundary(CoverageType::Auto); }
    #[test] fn test_type_constraints_liability() { assert_boundary(CoverageType::Liability); }

    #[test]
    fn test_unsupported_combination() {
        // coverage_amount > monthly_premium * 12 * 500 → UnsupportedCombination
        let env = Env::default();
        env.mock_all_auths();
        let c = setup(&env);
        let premium = 1_000_000i128; // 0.1 XLM
        let max_ratio = premium * 12 * 500;

        // exactly at the ratio limit → accept
        c.create_policy(&Address::generate(&env), &n(&env, "T"), &CoverageType::Health, &premium, &max_ratio);

        // one over → UnsupportedCombination
        assert_eq!(
            c.try_create_policy(&Address::generate(&env), &n(&env, "T"), &CoverageType::Health, &premium, &(max_ratio + 1))
                .unwrap_err().unwrap(),
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
                &Address::generate(&env), &n(&env, "T"),
                &CoverageType::Health,
                &(i128::MAX - 1), &1i128,
            ).unwrap_err().unwrap(),
            InsuranceError::InvalidPremium,
        );
    }

    // ── Helper: initialise contract with a known owner ────────────────────────

    fn setup_with_owner(env: &Env) -> (InsuranceClient, Address) {
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );
        let stranger = Address::generate(&env);

        assert_eq!(
            c.try_deactivate_policy(&stranger, &pid)
                .unwrap_err().unwrap(),
            InsuranceError::Unauthorized,
        );
    }

    /// Attempting to deactivate an already-inactive policy must yield PolicyAlreadyInactive.
    #[test]
    fn test_deactivate_policy_already_inactive() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        // First deactivation — should succeed
        c.deactivate_policy(&policy_owner, &pid);

        // Second deactivation — must return PolicyAlreadyInactive
        assert_eq!(
            c.try_deactivate_policy(&policy_owner, &pid)
                .unwrap_err().unwrap(),
            InsuranceError::PolicyAlreadyInactive,
        );
    }

    /// Deactivating a non-existent policy must yield PolicyNotFound.
    #[test]
    fn test_deactivate_policy_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);

        assert_eq!(
            c.try_deactivate_policy(&contract_owner, &9999)
                .unwrap_err().unwrap(),
            InsuranceError::PolicyNotFound,
        );
    }

    /// Calling deactivate_policy before init must yield NotInitialized.
    #[test]
    fn test_deactivate_policy_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let c = InsuranceClient::new(&env, &contract_id);
        let caller = Address::generate(&env);

        assert_eq!(
            c.try_deactivate_policy(&caller, &1)
                .unwrap_err().unwrap(),
            InsuranceError::NotInitialized,
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
            &owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        c.deactivate_policy(&owner, &pid);

        let page = c.get_active_policies(&owner, &0, &10);
        assert_eq!(page.count, 0, "active list should be empty after deactivation");
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        assert!(c.set_external_ref(&contract_owner, &pid, &core::option::Option::Some(n(&env, "ref-abc-123"))));

        let p = c.get_policy(&pid).unwrap();
        assert_eq!(p.external_ref, core::option::Option::Some(n(&env, "ref-abc-123")));
    }

    /// Success path: contract owner can clear an existing external reference (None).
    #[test]
    fn test_set_external_ref_clear() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        c.set_external_ref(&contract_owner, &pid, &core::option::Option::Some(n(&env, "ref-abc-123")));
        c.set_external_ref(&contract_owner, &pid, &core::option::Option::None);

        let p = c.get_policy(&pid).unwrap();
        assert_eq!(p.external_ref, core::option::Option::None);
    }

    /// Policy owner (non-contract-owner) calling set_external_ref must get Unauthorized.
    #[test]
    fn test_set_external_ref_unauthorized_policy_owner() {
        let env = Env::default();
        env.mock_all_auths();
        let (c, _contract_owner) = setup_with_owner(&env);
        let policy_owner = Address::generate(&env);
        let pid = c.create_policy(
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        assert_eq!(
            c.try_set_external_ref(&policy_owner, &pid, &core::option::Option::Some(n(&env, "ref")))
                .unwrap_err().unwrap(),
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );
        let stranger = Address::generate(&env);

        assert_eq!(
            c.try_set_external_ref(&stranger, &pid, &core::option::Option::Some(n(&env, "ref")))
                .unwrap_err().unwrap(),
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        // 129 characters — one over the MAX_EXT_REF_LEN of 128
        let long_ref = n(&env, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(
            c.try_set_external_ref(&contract_owner, &pid, &core::option::Option::Some(long_ref))
                .unwrap_err().unwrap(),
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
            &policy_owner, &n(&env, "P"), &CoverageType::Health,
            &5_000_000i128, &50_000_000i128,
        );

        assert_eq!(
            c.try_set_external_ref(&contract_owner, &pid, &core::option::Option::Some(n(&env, "")))
                .unwrap_err().unwrap(),
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
            c.try_set_external_ref(&contract_owner, &9999, &core::option::Option::Some(n(&env, "ref")))
                .unwrap_err().unwrap(),
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
                .unwrap_err().unwrap(),
            InsuranceError::NotInitialized,
        );
    }
}
