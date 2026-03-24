import os

def fix_remittance_split():
    path = 'remittance_split/tests/stress_test_large_amounts.rs'
    if not os.path.exists(path): return
    with open(path, 'r', encoding='utf-8') as f: lines = f.readlines()
    
    new_lines = []
    for line in lines:
        if 'let overflow_amount = i128::MAX / 50 + 1;' in line:
            new_lines.append('// ' + line)
        else:
            new_lines.append(line)
            
    with open(path, 'w', encoding='utf-8') as f: f.writelines(new_lines)

def fix_savings_goals():
    path = 'savings_goals/src/test.rs'
    if not os.path.exists(path): return
    with open(path, 'r', encoding='utf-8') as f: content = f.read()
    
    # 1. Add IntoVal and set_time helper
    if 'use soroban_sdk::IntoVal;' not in content:
        content = content.replace('use soroban_sdk::{', 'use soroban_sdk::IntoVal;\nuse soroban_sdk::{')
    
    if 'fn set_time' not in content:
        helper = '\nfn set_time(env: &Env, timestamp: u64) {\n    set_ledger_time(env, 1u32, timestamp);\n}\n'
        content = content.replace('use testutils::{set_ledger_time, setup_test_env};', 'use testutils::{set_ledger_time, setup_test_env};' + helper)

    # 2. Add extern crate std; for format! (or replace format!)
    if 'extern crate std;' not in content:
        content = 'extern crate std;\n' + content

    with open(path, 'w', encoding='utf-8') as f: f.write(content)

def fix_insurance_src_test():
    path = 'insurance/src/test.rs'
    if not os.path.exists(path): return
    with open(path, 'r', encoding='utf-8') as f: content = f.read()

    # 1. Add IntoVal and set_time helper
    if 'use soroban_sdk::IntoVal;' not in content:
        content = content.replace('use soroban_sdk::{', 'use soroban_sdk::IntoVal;\nuse soroban_sdk::{')
    
    if 'fn set_time' not in content:
        helper = '\nfn set_time(env: &Env, timestamp: u64) {\n    use testutils::set_ledger_time;\n    set_ledger_time(env, 1u32, timestamp);\n}\n'
        content = content.replace('use super::*;', 'use super::*;' + helper)

    # 2. Fix remaining create_policy calls
    # Searching for patterns like create_policy(..., &100000, );
    content = content.replace('&100000,\n    );', '&100000,\n        &None,\n    );')
    content = content.replace('&100000, );', '&100000, &None);')
    content = content.replace('&50000,\n    );', '&50000,\n        &None,\n    );')
    content = content.replace('&-1, \n// negative coverage\n    );', '&-1, // negative coverage\n        &None,\n    );')

    with open(path, 'w', encoding='utf-8') as f: f.write(content)

fix_remittance_split()
fix_savings_goals()
fix_insurance_src_test()
