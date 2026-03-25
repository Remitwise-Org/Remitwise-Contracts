import re

with open('insurance/src/lib.rs', 'r', encoding='utf-8') as f:
    code = f.read()

# Fix vec! import
code = code.replace('use soroban_sdk::{Env, String};', 'use soroban_sdk::{Env, String, vec};')

# Fix result assertion on pay_premium
code = code.replace('        let result = client.pay_premium(&owner, &policy_id);\n        assert!(result);', '        client.pay_premium(&owner, &policy_id);')

# Fix create_test_env
code = code.replace('let env = create_test_env();', 'let env = make_env();\n        env.mock_all_auths();')

# Fix create_policy missing &None
pattern = r'let policy_id = client\.create_policy\(\s*&owner,\s*&String::from_str\(&env, "([^"]+)"\),\s*&String::from_str\(&env, "([^"]+)"\),\s*&(\d+),\s*&(\d+),\s*\);'
repl = r'''let policy_id = client.create_policy(
            &owner,
            &String::from_str(&env, "\1"),
            &String::from_str(&env, "\2"),
            &\3,
            &\4,
            &None,
        );'''
code = re.sub(pattern, repl, code)

pattern2 = r'let policy_id = client\.create_policy\(\s*&owner,\s*&String::from_str\(&env, "Policy"\),\s*&String::from_str\(&env, "health"\),\s*&100,\s*&10000,\s*\);'
repl2 = r'''let policy_id = client.create_policy(
                &owner,
                &String::from_str(&env, "Policy"),
                &String::from_str(&env, "health"),
                &100,
                &10000,
                &None,
            );'''
code = re.sub(pattern2, repl2, code)

with open('insurance/src/lib.rs', 'w', encoding='utf-8') as f:
    f.write(code)
