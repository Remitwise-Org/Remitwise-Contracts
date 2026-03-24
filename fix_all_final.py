import os
import re

def fix_insurance_tests():
    path = 'insurance/src/test.rs'
    if not os.path.exists(path): return
    with open(path, 'r', encoding='utf-8', errors='ignore') as f: content = f.read()

    # 1. Fix get_active_policies calls
    content = content.replace('client.get_active_policies(&owner)', 'client.get_active_policies(&owner, &0, &10)')
    content = content.replace('active.len()', 'active.items.len()')

    # 2. Fix set_ledger_time calls
    content = content.replace('set_ledger_time(&env, 1000)', 'set_ledger_time(&env, 1u32, 1000u64)')

    # 3. Fix create_policy calls (missing &None)
    # This was partially fixed but some patterns might remain
    # Pattern: 5 arguments, needs 6
    # Let's use a regex to find multiline calls that have 5 args
    def repl_policy(m):
        args = [a.strip() for a in m.group(1).split(',')]
        if len([a for a in args if a]) == 5:
            return f'client.create_policy({m.group(1)}, &None)'
        return m.group(0)

    # Simplified replacement for the multiline ones I saw in the view_file
    content = content.replace(
        'let policy_id = client.create_policy(\n        &owner,\n        &String::from_str(&env, "Policy"),\n        &String::from_str(&env, "Type"),\n        &100,\n        &10000,\n    );',
        'let policy_id = client.create_policy(\n        &owner,\n        &String::from_str(&env, "Policy"),\n        &String::from_str(&env, "Type"),\n        &100,\n        &10000,\n        &None,\n    );'
    )

    # General multiline fix for create_policy with 5 args ending with &50000
    content = re.sub(
        r'client\.create_policy\(\s*&owner,\s*&String::from_str\(&env, "Health Insurance"\),\s*&String::from_str\(&env, "health"\),\s*&500,\s*&50000,\s*\)',
        r'client.create_policy(\n        &owner,\n        &String::from_str(&env, "Health Insurance"),\n        &String::from_str(&env, "health"),\n        &500,\n        &50000,\n        &None,\n    )',
        content
    )

    # Fix syntax error from previous bad replacement: , &None);
    content = content.replace('\n    , &None);', '\n        &None\n    );')

    with open(path, 'w', encoding='utf-8') as f: f.write(content)

def fix_examples():
    # insurance_example.rs
    path = 'examples/insurance_example.rs'
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f: c = f.read()
        c = c.replace('policy.next_payment_date', 'policy.unwrap().next_payment_date')
        with open(path, 'w', encoding='utf-8') as f: f.write(c)

    # family_wallet_example.rs
    path = 'examples/family_wallet_example.rs'
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f: c = f.read()
        c = c.replace('use family_wallet::{FamilyWallet, FamilyWalletClient, FamilyRole};', 'use family_wallet::{FamilyWallet, FamilyWalletClient};\nuse remitwise_common::FamilyRole;')
        c = c.replace('spending_limit).unwrap();', 'spending_limit);')
        with open(path, 'w', encoding='utf-8') as f: f.write(c)

    # savings_goals_example.rs
    path = 'examples/savings_goals_example.rs'
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f: c = f.read()
        c = c.replace('target_date).unwrap();', 'target_date);')
        c = c.replace('println!("Creating savings goal: \'{}\'', 'println!("Creating savings goal: \'{:?}\'')
        c = c.replace('println!("  Name: {}", goal.name)', 'println!("  Name: {:?}", goal.name)')
        with open(path, 'w', encoding='utf-8') as f: f.write(c)

    # bill_payments_example.rs
    path = 'examples/bill_payments_example.rs'
    if os.path.exists(path):
        with open(path, 'r', encoding='utf-8') as f: c = f.read()
        c = c.replace('println!("Creating bill: \'{}\'', 'println!("Creating bill: \'{:?}\'')
        c = c.replace('println!("  ID: {}, Name: {}, Amount: {} {}",', 'println!("  ID: {:?}, Name: {:?}, Amount: {} {:?}",')
        with open(path, 'w', encoding='utf-8') as f: f.write(c)

fix_insurance_tests()
fix_examples()
