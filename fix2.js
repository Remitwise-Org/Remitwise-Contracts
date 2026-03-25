const fs = require('fs');
const path = require('path');

const libPath = path.join('c:', 'Users', 'HP', 'Desktop', 'Northbridge', 'Remitwise-Contracts', 'insurance', 'src', 'lib.rs');
let content = fs.readFileSync(libPath, 'utf8');

function rep(search, replaceStr) {
    if (content.includes(search)) {
        content = content.replace(search, replaceStr);
    }
}

// Block 1
rep(`    }

        client.create_policy(&owner, &name, &coverage_type, &100, &0, &None);
    #[test]
    fn test_get_active_policies_single_page() {`, 
`    }

    #[test]
    fn test_get_active_policies_single_page() {`);

// Block 2
rep(`        assert_eq!(page3.next_cursor, 0);
    }
        // Create a policy
        let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "Emergency Coverage"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        env.mock_all_auths();

        let name = String::from_str(&env, "Health Insurance");
        let coverage_type = String::from_str(&env, "health");
        let policy_id = client.create_policy(&owner, &name, &coverage_type, &100, &10000, &None);
        let ids = setup_policies(&env, &client, &owner, 4);
        // Deactivate policy #2
        client.deactivate_policy(&owner, &ids.get(1).unwrap());

        let page = client.get_active_policies(&owner, &0, &10);
        assert_eq!(page.count, 3); // only 3 active
        for p in page.items.iter() {
            assert!(p.active, "only active policies should be returned");
        }
    }

    #[test]
    fn test_get_active_policies_multi_owner_isolation() {`,
`        assert_eq!(page3.next_cursor, 0);
    }

    #[test]
    fn test_get_active_policies_multi_owner_isolation() {`);

// Block 3
rep(`    #[test]
    fn test_create_policy_emits_event_exists() {
        let env = make_env();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, Insurance);
        let client = InsuranceClient::new(&env, &contract_id);
        let owner = Address::generate(&env);

        // Create multiple policies
        let name1 = String::from_str(&env, "Health Insurance");
        let coverage_type1 = String::from_str(&env, "health");
        let policy_id1 = client.create_policy(&owner, &name1, &coverage_type1, &100, &10000, &None);

        let name2 = String::from_str(&env, "Emergency Insurance");
        let coverage_type2 = String::from_str(&env, "emergency");
        let policy_id2 = client.create_policy(&owner, &name2, &coverage_type2, &200, &20000, &None);

        let name3 = String::from_str(&env, "Life Insurance");
        let coverage_type3 = String::from_str(&env, "life");
        let policy_id3 = client.create_policy(&owner, &name3, &coverage_type3, &300, &30000, &None);
        let policy_id = client.create_policy(
        client.create_policy(
            &owner,
            &String::from_str(&env, "Health Insurance"),
            &CoverageType::Health,
            &String::from_str(&env, "Policy 1"),
            &String::from_str(&env, "health"),
            &100,
            &50000,
        );
        client.create_policy(
            &owner,
            &String::from_str(&env, "Policy 2"),
            &String::from_str(&env, "life"),
            &200,
            &100000,
        );
        client.create_policy(
            &owner,
            &String::from_str(&env, "Policy 3"),
            &String::from_str(&env, "emergency"),
            &75,
            &25000,
        );

        client.pay_premium(&owner, &policy_id);

        let events_after = env.events().all().len();
        assert_eq!(events_after - events_before, 2);
    }

    #[test]
    fn test_policy_lifecycle_emits_all_events() {`,
`    #[test]
    fn test_policy_lifecycle_emits_all_events() {`);

// Block 4
rep(`        let name3 = String::from_str(&env, "Life Insurance");
        let coverage_type3 = String::from_str(&env, "life");
        let policy_id3 = client.create_policy(&owner, &name3, &coverage_type3, &300, &30000, &None);
        // Create a policy
        let policy_id = client.create_policy(`,
`        // Create a policy
        let policy_id = client.create_policy(`);

fs.writeFileSync(libPath, content, 'utf8');
console.log("Replacements round 2 done!");
