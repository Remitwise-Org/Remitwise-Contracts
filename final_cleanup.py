import os
import re

def fix_syntax_final(path):
    if not os.path.exists(path): return
    with open(path, 'r', encoding='utf-8', errors='ignore') as f: content = f.read()
    # Correct the nested paren error
    content = content.replace('    )\n    );', '    );')
    content = content.replace('    )\n);', '    );')
    # Use regex to find &None) followed by arbitrary space and then extra closing paren/semicolon
    content = re.sub(r'&None\)\s*\n\s*\);', '&None);', content)
    with open(path, 'w', encoding='utf-8') as f: f.write(content)

for f in ['insurance/src/test.rs', 'insurance/tests/stress_tests.rs', 'insurance/tests/gas_bench.rs', 'integration_tests/tests/multi_contract_integration.rs']:
    fix_syntax_final(f)

# Fix scenarios/tests/flow.rs
f = 'scenarios/tests/flow.rs'
if os.path.exists(f):
    with open(f, 'r', encoding='utf-8') as fr: c = fr.read()
    # Replace multiline create_bill
    pattern = r'bills_client\.create_bill\(\s+&user,\s+&bill_name,\s+&300,\s+&due_date,\s+&true,\s+&30,\s+&String::from_str\(&env, "USDC"\),\s+\)'
    replacement = 'bills_client.create_bill(\n        &user,\n        &bill_name,\n        &300,\n        &due_date,\n        &true,\n        &30,\n        &None,\n        &String::from_str(&env, "USDC"),\n    )'
    c = re.sub(pattern, replacement, c, flags=re.DOTALL)
    with open(f, 'w', encoding='utf-8') as fw: fw.write(c)

# Fix examples/bill_payments_example.rs
f = 'examples/bill_payments_example.rs'
if os.path.exists(f):
    with open(f, 'r', encoding='utf-8') as fr: c = fr.read()
    # Remove unwrap on u32 and ()
    c = c.replace('.unwrap()', '')
    # Fix println formatting for Soroban strings
    c = c.replace('\"{}\"', '\"{:?}\"')
    # Fix create_bill args (missing &None)
    c = c.replace('&currency)', '&None, &currency)')
    with open(f, 'w', encoding='utf-8') as fw: fw.write(c)

# Fix examples/reporting_example.rs
f = 'examples/reporting_example.rs'
if os.path.exists(f):
    with open(f, 'r', encoding='utf-8') as fr: c = fr.read()
    c = c.replace('.unwrap()', '')
    # Fix imports
    c = c.replace('reporting::{ReportingContractClient, Category}', 'reporting::{ReportingContractClient, remitwise_common::Category}')
    c = c.replace('use reporting::{ReportingContractClient, Category};', 'use reporting::{ReportingContractClient};\nuse remitwise_common::Category;')
    with open(f, 'w', encoding='utf-8') as fw: fw.write(c)
