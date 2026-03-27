//! Comprehensive test suite for the Insurance contract.
//!
//! ## Coverage goals (≥ 95 %)
//!
//! Every public function is exercised across:
//!   - Happy paths (valid inputs → expected state / return value)
//!   - Boundary conditions (min/max values, off-by-one)
//!   - Negative paths (invalid inputs → expected panic)
//!   - Security assertions (unauthorized callers)
//!   - Edge cases (zero values, overflow candidates, empty/long strings)

#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    use crate::{CoverageType, InsuranceContract, InsuranceContractClient};

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Spins up a fresh Env, registers the contract, and initializes it with a
    /// freshly-generated owner address.  Returns `(env, client, owner)`.
    fn setup() -> (Env, InsuranceContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, InsuranceContract);
        let client = InsuranceContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        client.init(&owner);
        (env, client, owner)
    }

    /// Returns a valid short name suitable for most tests.
    fn short_name(env: &Env) -> String {
        String::from_str(env, "Health Policy Alpha")
    }

    // -----------------------------------------------------------------------
    // 1. Initialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_success() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, InsuranceContract);
        let client = InsuranceContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        // Should not panic
        client.init(&owner);
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_init_double_init_panics() {
        let (_, client, owner) = setup();
        // Second init must panic
        client.init(&owner);
    }

    // -----------------------------------------------------------------------
    // 2. create_policy — happy paths
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_health_policy_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,   // 0.5 XLM / month — within [1M, 500M]
            &50_000_000i128,  // 5 XLM coverage — within [10M, 100B]
            &None,
        );
        assert_eq!(id, 1u32);
    }

    #[test]
    fn test_create_life_policy_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "Life Plan"),
            &CoverageType::Life,
            &1_000_000i128,    // within [500K, 1B]
            &60_000_000i128,   // within [50M, 500B]
            &None,
        );
        assert_eq!(id, 1u32);
    }

    #[test]
    fn test_create_property_policy_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "Home Cover"),
            &CoverageType::Property,
            &5_000_000i128,      // within [2M, 2B]
            &200_000_000i128,    // within [100M, 1T]
            &None,
        );
        assert_eq!(id, 1u32);
    }

    #[test]
    fn test_create_auto_policy_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "Car Insurance"),
            &CoverageType::Auto,
            &3_000_000i128,    // within [1.5M, 750M]
            &50_000_000i128,   // within [20M, 200B]
            &None,
        );
        assert_eq!(id, 1u32);
    }

    #[test]
    fn test_create_liability_policy_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "Liability Cover"),
            &CoverageType::Liability,
            &2_000_000i128,    // within [800K, 400M]
            &10_000_000i128,   // within [5M, 50B]
            &None,
        );
        assert_eq!(id, 1u32);
    }

    #[test]
    fn test_create_policy_with_external_ref() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let ext_ref = String::from_str(&env, "PROVIDER-12345");
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(ext_ref),
        );
        let policy = client.get_policy(&id);
        assert!(policy.external_ref.is_some());
    }

    #[test]
    fn test_create_multiple_policies_increment_ids() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        for expected_id in 1u32..=5u32 {
            let id = client.create_policy(
                &caller,
                &short_name(&env),
                &CoverageType::Health,
                &5_000_000i128,
                &50_000_000i128,
                &None,
            );
            assert_eq!(id, expected_id);
        }
    }

    // -----------------------------------------------------------------------
    // 3. create_policy — boundary conditions
    // -----------------------------------------------------------------------

    // --- Health min/max boundaries ---

    #[test]
    fn test_health_premium_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // min_premium for Health = 1_000_000
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &1_000_000i128,
            &10_000_000i128, // min coverage
            &None,
        );
    }

    #[test]
    fn test_health_premium_at_maximum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // max_premium = 500_000_000; need coverage ≤ 500M * 12 * 500 = 3T (within 100B limit)
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_000i128,
            &100_000_000_000i128, // max coverage for Health
            &None,
        );
    }

    #[test]
    fn test_health_coverage_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &10_000_000i128, // exactly min_coverage
            &None,
        );
    }

    #[test]
    fn test_health_coverage_at_maximum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // max_coverage = 100_000_000_000; need premium ≥ 100B / (12*500) ≈ 16_666_667
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_000i128,       // max premium to allow max coverage via ratio
            &100_000_000_000i128,   // exactly max_coverage
            &None,
        );
    }

    // --- Life boundaries ---

    #[test]
    fn test_life_premium_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life Min"),
            &CoverageType::Life,
            &500_000i128,     // min_premium
            &50_000_000i128,  // min_coverage
            &None,
        );
    }

    #[test]
    fn test_liability_premium_at_minimum_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability Min"),
            &CoverageType::Liability,
            &800_000i128,     // min_premium
            &5_000_000i128,   // min_coverage
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 4. create_policy — name validation
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "name cannot be empty")]
    fn test_create_policy_empty_name_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, ""),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "name too long")]
    fn test_create_policy_name_exceeds_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // 65 character name — exceeds MAX_NAME_LEN (64)
        let long_name = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.create_policy(
            &caller,
            &long_name,
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    fn test_create_policy_name_at_max_length_succeeds() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Exactly 64 characters
        let max_name = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        client.create_policy(
            &caller,
            &max_name,
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 5. create_policy — premium validation failures
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "monthly_premium must be positive")]
    fn test_create_policy_zero_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &0i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium must be positive")]
    fn test_create_policy_negative_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &-1i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_health_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health min_premium = 1_000_000; supply 999_999
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &999_999i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_health_policy_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health max_premium = 500_000_000; supply 500_000_001
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_001i128,
            &10_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_life_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Life min_premium = 500_000; supply 499_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life"),
            &CoverageType::Life,
            &499_999i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_property_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Property min_premium = 2_000_000; supply 1_999_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Property"),
            &CoverageType::Property,
            &1_999_999i128,
            &100_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_auto_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto min_premium = 1_500_000; supply 1_499_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &1_499_999i128,
            &20_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_create_liability_policy_premium_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability min_premium = 800_000; supply 799_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &799_999i128,
            &5_000_000i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 6. create_policy — coverage amount validation failures
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "coverage_amount must be positive")]
    fn test_create_policy_zero_coverage_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &0i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount must be positive")]
    fn test_create_policy_negative_coverage_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &-1i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_health_policy_coverage_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health min_coverage = 10_000_000; supply 9_999_999
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &9_999_999i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_health_policy_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Health max_coverage = 100_000_000_000; supply 100_000_000_001
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &500_000_000i128,
            &100_000_000_001i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_life_policy_coverage_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Life min_coverage = 50_000_000; supply 49_999_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life"),
            &CoverageType::Life,
            &1_000_000i128,
            &49_999_999i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_create_property_policy_coverage_below_min_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Property min_coverage = 100_000_000; supply 99_999_999
        client.create_policy(
            &caller,
            &String::from_str(&env, "Property"),
            &CoverageType::Property,
            &5_000_000i128,
            &99_999_999i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 7. create_policy — ratio guard (unsupported combination)
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "unsupported combination: coverage_amount too high relative to premium")]
    fn test_create_policy_coverage_too_high_for_premium_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // premium = 1_000_000 → annual = 12_000_000 → max_coverage = 6_000_000_000
        // supply coverage = 6_000_000_001 (just over the ratio limit, but within Health's hard max)
        // Need premium high enough so health range isn't hit, but ratio is
        // Health max_coverage = 100_000_000_000
        // Use premium = 1_000_000, coverage = 7_000_000_000 → over ratio (6B), under hard cap (100B)
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &1_000_000i128,
            &7_000_000_000i128,
            &None,
        );
    }

    #[test]
    fn test_create_policy_coverage_exactly_at_ratio_limit_succeeds() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // premium = 1_000_000 → ratio limit = 1M * 12 * 500 = 6_000_000_000
        // Health max_coverage = 100B, so 6B is fine
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &1_000_000i128,
            &6_000_000_000i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // 8. External ref validation
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "external_ref length out of range")]
    fn test_create_policy_ext_ref_too_long_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // 129 character external ref — exceeds MAX_EXT_REF_LEN (128)
        let long_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(long_ref),
        );
    }

    #[test]
    fn test_create_policy_ext_ref_at_max_length_succeeds() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Exactly 128 characters
        let max_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        );
        client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(max_ref),
        );
    }

    // -----------------------------------------------------------------------
    // 9. pay_premium — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_pay_premium_success() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let result = client.pay_premium(&caller, &id, &5_000_000i128);
        assert!(result);
    }

    #[test]
    fn test_pay_premium_updates_next_payment_date() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        env.ledger().set_timestamp(1_000_000u64);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        
        let policy_before = client.get_policy(&id);
        let initial_due = policy_before.next_payment_due;
        
        env.ledger().set_timestamp(2_000_000u64);
        client.pay_premium(&caller, &id, &5_000_000i128);
        let policy = client.get_policy(&id);
        
        // next_payment_due should advance from initial due date, not from payment time
        // initial_due = 1_000_000 + 30 * 24 * 60 * 60 = 3_592_000
        // new_due = initial_due + 30 * 24 * 60 * 60 = 6_184_000
        assert_eq!(policy.next_payment_due, initial_due + 30 * 24 * 60 * 60);
        assert_eq!(policy.last_payment_at, 2_000_000u64);
    }

    // -----------------------------------------------------------------------
    // 10. pay_premium — failure cases
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_pay_premium_nonexistent_policy_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        client.pay_premium(&caller, &999u32, &5_000_000i128);
    }

    #[test]
    #[should_panic(expected = "amount must equal monthly_premium")]
    fn test_pay_premium_wrong_amount_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.pay_premium(&caller, &id, &4_999_999i128);
    }

    #[test]
    #[should_panic(expected = "policy inactive")]
    fn test_pay_premium_on_inactive_policy_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.deactivate_policy(&owner, &id);
        client.pay_premium(&caller, &id, &5_000_000i128);
    }

    // -----------------------------------------------------------------------
    // 11. deactivate_policy — happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_deactivate_policy_success() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let result = client.deactivate_policy(&owner, &id);
        assert!(result);

        let policy = client.get_policy(&id);
        assert!(!policy.active);
    }

    #[test]
    fn test_deactivate_removes_from_active_list() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        assert_eq!(client.get_active_policies().len(), 1);
        client.deactivate_policy(&owner, &id);
        assert_eq!(client.get_active_policies().len(), 0);
    }

    // -----------------------------------------------------------------------
    // 12. deactivate_policy — failure cases
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_deactivate_policy_non_owner_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let non_owner = Address::generate(&env);
        client.deactivate_policy(&non_owner, &id);
    }

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_deactivate_nonexistent_policy_panics() {
        let (_env, client, owner) = setup();
        client.deactivate_policy(&owner, &999u32);
    }

    #[test]
    #[should_panic(expected = "policy already inactive")]
    fn test_deactivate_already_inactive_policy_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.deactivate_policy(&owner, &id);
        // Second deactivation must panic
        client.deactivate_policy(&owner, &id);
    }

    // -----------------------------------------------------------------------
    // 13. set_external_ref
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_external_ref_success() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let new_ref = String::from_str(&env, "NEW-REF-001");
        client.set_external_ref(&owner, &id, &Some(new_ref));
        let policy = client.get_policy(&id);
        assert!(policy.external_ref.is_some());
    }

    #[test]
    fn test_set_external_ref_clear() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let ext_ref = String::from_str(&env, "INITIAL-REF");
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &Some(ext_ref),
        );
        // Clear the ref
        client.set_external_ref(&owner, &id, &None);
        let policy = client.get_policy(&id);
        assert!(policy.external_ref.is_none());
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_external_ref_non_owner_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let non_owner = Address::generate(&env);
        let new_ref = String::from_str(&env, "HACK");
        client.set_external_ref(&non_owner, &id, &Some(new_ref));
    }

    #[test]
    #[should_panic(expected = "external_ref length out of range")]
    fn test_set_external_ref_too_long_panics() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        let long_ref = String::from_str(
            &env,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1",
        );
        client.set_external_ref(&owner, &id, &Some(long_ref));
    }

    // -----------------------------------------------------------------------
    // 14. Queries
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_active_policies_empty_initially() {
        let (_env, client, _owner) = setup();
        assert_eq!(client.get_active_policies().len(), 0);
    }

    #[test]
    fn test_get_active_policies_reflects_creates_and_deactivations() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id1 = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.create_policy(
            &caller,
            &String::from_str(&env, "Second Policy"),
            &CoverageType::Life,
            &1_000_000i128,
            &60_000_000i128,
            &None,
        );
        assert_eq!(client.get_active_policies().len(), 2);
        client.deactivate_policy(&owner, &id1);
        assert_eq!(client.get_active_policies().len(), 1);
    }

    #[test]
    fn test_get_total_monthly_premium_sums_active_only() {
        let (env, client, owner) = setup();
        let caller = Address::generate(&env);
        let id1 = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
        client.create_policy(
            &caller,
            &String::from_str(&env, "Second"),
            &CoverageType::Life,
            &1_000_000i128,
            &60_000_000i128,
            &None,
        );
        assert_eq!(client.get_total_monthly_premium(), 6_000_000i128);
        client.deactivate_policy(&owner, &id1);
        assert_eq!(client.get_total_monthly_premium(), 1_000_000i128);
    }

    #[test]
    fn test_get_total_monthly_premium_zero_when_no_policies() {
        let (_env, client, _owner) = setup();
        assert_eq!(client.get_total_monthly_premium(), 0i128);
    }

    #[test]
    #[should_panic(expected = "policy not found")]
    fn test_get_policy_nonexistent_panics() {
        let (_env, client, _owner) = setup();
        client.get_policy(&999u32);
    }

    // -----------------------------------------------------------------------
    // 15. Uninitialized contract guard
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "not initialized")]
    fn test_create_policy_without_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, InsuranceContract);
        let client = InsuranceContractClient::new(&env, &contract_id);
        let caller = Address::generate(&env);
        client.create_policy(
            &caller,
            &String::from_str(&env, "Test"),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "not initialized")]
    fn test_get_active_policies_without_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, InsuranceContract);
        let client = InsuranceContractClient::new(&env, &contract_id);
        client.get_active_policies();
    }

    // -----------------------------------------------------------------------
    // 16. Policy data integrity
    // -----------------------------------------------------------------------

    #[test]
    fn test_policy_fields_stored_correctly() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        env.ledger().set_timestamp(1_700_000_000u64);
        let id = client.create_policy(
            &caller,
            &String::from_str(&env, "My Health Plan"),
            &CoverageType::Health,
            &10_000_000i128,
            &100_000_000i128,
            &Some(String::from_str(&env, "EXT-001")),
        );
        let policy = client.get_policy(&id);
        assert_eq!(policy.id, 1u32);
        assert_eq!(policy.monthly_premium, 10_000_000i128);
        assert_eq!(policy.coverage_amount, 100_000_000i128);
        assert!(policy.active);
        assert_eq!(policy.last_payment_at, 0u64);
        assert_eq!(policy.created_at, 1_700_000_000u64);
        assert_eq!(
            policy.next_payment_due,
            1_700_000_000u64 + 30 * 24 * 60 * 60
        );
        assert!(policy.external_ref.is_some());
    }

    // -----------------------------------------------------------------------
    // 17. Cross-coverage-type boundary checks
    // -----------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_property_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Property max_premium = 2_000_000_000; supply 2_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Property"),
            &CoverageType::Property,
            &2_000_000_001i128,
            &100_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_auto_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto max_premium = 750_000_000; supply 750_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &750_000_001i128,
            &20_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "monthly_premium out of range for coverage type")]
    fn test_liability_premium_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability max_premium = 400_000_000; supply 400_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &400_000_001i128,
            &5_000_000i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_life_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Life max_coverage = 500_000_000_000; supply 500_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Life"),
            &CoverageType::Life,
            &1_000_000_000i128, // max premium for Life
            &500_000_000_001i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_auto_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Auto max_coverage = 200_000_000_000; supply 200_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Auto"),
            &CoverageType::Auto,
            &750_000_000i128,
            &200_000_000_001i128,
            &None,
        );
    }

    #[test]
    #[should_panic(expected = "coverage_amount out of range for coverage type")]
    fn test_liability_coverage_above_max_panics() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        // Liability max_coverage = 50_000_000_000; supply 50_000_000_001
        client.create_policy(
            &caller,
            &String::from_str(&env, "Liability"),
            &CoverageType::Liability,
            &400_000_000i128,
            &50_000_000_001i128,
            &None,
        );
    }

    // -----------------------------------------------------------------------
    // Premium Payment Date Progression Tests
    // -----------------------------------------------------------------------

    const THIRTY_DAYS: u64 = 30 * 24 * 60 * 60;

    #[test]
    fn test_early_payment_no_drift() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;
        assert_eq!(first_due, start_time + THIRTY_DAYS);

        // Pay 10 days early
        env.ledger().set_timestamp(start_time + 20 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // Next due should be first_due + 30 days, not payment_time + 30 days
        assert_eq!(policy.next_payment_due, first_due + THIRTY_DAYS);
    }

    #[test]
    fn test_on_time_payment_no_drift() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Pay exactly on due date
        env.ledger().set_timestamp(first_due);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        assert_eq!(policy.next_payment_due, first_due + THIRTY_DAYS);
    }

    #[test]
    fn test_late_payment_single_period() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Pay 10 days late (still within next period)
        env.ledger().set_timestamp(first_due + 10 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // Next due advances from first_due, landing in the future
        assert_eq!(policy.next_payment_due, first_due + THIRTY_DAYS);
    }

    #[test]
    fn test_very_late_payment_skips_to_future() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Pay 45 days late (past what would be the next due date)
        let payment_time = first_due + 45 * 24 * 60 * 60;
        env.ledger().set_timestamp(payment_time);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // next_due should be advanced until it's in the future
        // first_due + 30 days = in the past, so advance again
        // first_due + 60 days = in the future
        assert_eq!(policy.next_payment_due, first_due + 2 * THIRTY_DAYS);
        assert!(policy.next_payment_due > payment_time);
    }

    #[test]
    fn test_multiple_periods_missed_advances_correctly() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Pay 95 days late (more than 3 periods)
        let payment_time = first_due + 95 * 24 * 60 * 60;
        env.ledger().set_timestamp(payment_time);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // Must advance until next_due > payment_time
        // first_due + 30 = past, + 60 = past, + 90 = past, + 120 = future
        assert_eq!(policy.next_payment_due, first_due + 4 * THIRTY_DAYS);
        assert!(policy.next_payment_due > payment_time);
    }

    #[test]
    fn test_consecutive_early_payments_maintain_schedule() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // First payment: 5 days early
        env.ledger().set_timestamp(first_due - 5 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        let second_due = policy.next_payment_due;
        assert_eq!(second_due, first_due + THIRTY_DAYS);

        // Second payment: 5 days early again
        env.ledger().set_timestamp(second_due - 5 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        let third_due = policy.next_payment_due;
        assert_eq!(third_due, first_due + 2 * THIRTY_DAYS);

        // Third payment: 5 days early again
        env.ledger().set_timestamp(third_due - 5 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // Schedule should stay consistent: first_due + 90 days
        assert_eq!(policy.next_payment_due, first_due + 3 * THIRTY_DAYS);
    }

    #[test]
    fn test_payment_exactly_at_next_due_boundary() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Pay exactly at first_due + THIRTY_DAYS (boundary case)
        env.ledger().set_timestamp(first_due + THIRTY_DAYS);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // When payment_time == next_due after one advance, we need to advance again
        // because next_due <= now triggers another advance
        assert_eq!(policy.next_payment_due, first_due + 2 * THIRTY_DAYS);
    }

    #[test]
    fn test_mixed_early_and_late_payments() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // First payment: on time
        env.ledger().set_timestamp(first_due);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        assert_eq!(policy.next_payment_due, first_due + THIRTY_DAYS);

        // Second payment: 10 days late
        let second_due = policy.next_payment_due;
        env.ledger().set_timestamp(second_due + 10 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        assert_eq!(policy.next_payment_due, first_due + 2 * THIRTY_DAYS);

        // Third payment: 15 days early
        let third_due = policy.next_payment_due;
        env.ledger().set_timestamp(third_due - 15 * 24 * 60 * 60);
        client.pay_premium(&caller, &id, &5_000_000i128);

        let policy = client.get_policy(&id);
        // Schedule remains deterministic
        assert_eq!(policy.next_payment_due, first_due + 3 * THIRTY_DAYS);
    }

    #[test]
    fn test_deterministic_schedule_after_12_months() {
        let (env, client, _owner) = setup();
        let caller = Address::generate(&env);
        let start_time = 1_000_000u64;
        env.ledger().set_timestamp(start_time);

        let id = client.create_policy(
            &caller,
            &short_name(&env),
            &CoverageType::Health,
            &5_000_000i128,
            &50_000_000i128,
            &None,
        );

        let policy = client.get_policy(&id);
        let first_due = policy.next_payment_due;

        // Simulate 12 monthly payments with varying timing
        for month in 0..12 {
            let expected_due = first_due + month * THIRTY_DAYS;
            let policy = client.get_policy(&id);
            assert_eq!(policy.next_payment_due, expected_due);

            // Alternate between early (-5 days) and late (+5 days)
            let offset: i64 = if month % 2 == 0 { -5 } else { 5 };
            let payment_time = (expected_due as i64 + offset * 24 * 60 * 60) as u64;
            env.ledger().set_timestamp(payment_time);
            client.pay_premium(&caller, &id, &5_000_000i128);
        }

        let policy = client.get_policy(&id);
        // After 12 payments, schedule should be exactly 12 periods advanced
        assert_eq!(policy.next_payment_due, first_due + 12 * THIRTY_DAYS);
    }
}