import os
import re

def fix_file(path, method, target_count, missing_args, insert_pos):
    if not os.path.exists(path):
        return
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()
    
    def split_args(s):
        res = []
        cur = []
        d = 0
        for c in s:
            if c == ',' and d == 0:
                res.append(''.join(cur).strip())
                cur = []
            else:
                if c == '(': d += 1
                elif c == ')': d -= 1
                cur.append(c)
        res.append(''.join(cur).strip())
        return res

    res = []
    pos = 0
    while pos < len(content):
        start = content.find(method + '(', pos)
        if start == -1:
            break
        res.append(content[pos:start])
        p = start + len(method) + 1
        d = 0
        end = -1
        for i in range(p, len(content)):
            if content[i] == '(': d += 1
            elif content[i] == ')':
                if d == 0:
                    end = i
                    break
                d -= 1
        if end != -1:
            inner_content = content[p:end]
            args = split_args(inner_content)
            if len(args) == target_count:
                new_args = list(args)
                for i, arg in enumerate(missing_args):
                    new_args.insert(insert_pos + i, arg)
                res.append(method + '(' + ', '.join(new_args) + ')')
            else:
                res.append(content[start:end+1])
            pos = end + 1
        else:
            res.append(content[start:start+len(method)+1])
            pos = start + len(method) + 1
    res.append(content[pos:])
    with open(path, 'w', encoding='utf-8') as f:
        f.write(''.join(res))

# Repair 7-arg create_bill (missing ext_ref)
target_files = [
    'bill_payments/tests/stress_test_large_amounts.rs', 
    'bill_payments/tests/stress_tests.rs', 
    'scenarios/tests/flow.rs',
    'integration_tests/tests/multi_contract_integration.rs',
    'reporting/src/tests.rs'
]
for f in target_files:
    fix_file(f, 'client.create_bill', 7, ['&None'], 6)
    fix_file(f, 'bills_client.create_bill', 7, ['&None'], 6)

# Repair 5-arg create_policy (missing ext_ref)
ins_files = [
    'insurance/tests/stress_tests.rs', 
    'insurance/tests/gas_bench.rs', 
    'insurance/src/test.rs', 
    'scenarios/tests/flow.rs',
    'integration_tests/tests/multi_contract_integration.rs',
    'reporting/src/tests.rs'
]
for f in ins_files:
    fix_file(f, 'client.create_policy', 5, ['&None'], 5)
    fix_file(f, 'insurance_client.create_policy', 5, ['&None'], 5)

# Fix reporting tests naming
replace_files = ['reporting/src/tests.rs', 'reporting/src/lib.rs']
for f in replace_files:
    if os.path.exists(f):
        with open(f, 'r', encoding='utf-8', errors='ignore') as fr:
            c = fr.read()
        c = c.replace('ReportingClient', 'ReportingContractClient')
        c = c.replace('Reporting', 'ReportingContract')
        # Fix back the Error naming if I broke it
        c = c.replace('ReportingContractError', 'ReportingError')
        with open(f, 'w', encoding='utf-8') as fw:
            fw.write(c)

# Fix setup_test_env! in savings_goals
f = 'savings_goals/src/test.rs'
if os.path.exists(f):
    with open(f, 'r', encoding='utf-8') as fr:
        c = fr.read()
    c = c.replace('setup_test_env!(env, SavingsGoalContract, client, user);', 'setup_test_env!(env, SavingsGoalContract, client, user, SavingsGoalContractClient);')
    c = c.replace('setup_test_env!(env, SavingsGoalContract, client, owner);', 'setup_test_env!(env, SavingsGoalContract, client, owner, SavingsGoalContractClient);')
    with open(f, 'w', encoding='utf-8') as fw:
        fw.write(c)

# Final CoverageType cleanup
for f in ins_files:
    if os.path.exists(f):
        with open(f, 'r', encoding='utf-8', errors='ignore') as fr:
            c = fr.read()
        c = c.replace('CoverageType::Health', 'String::from_str(&env, "health")')
        c = c.replace('CoverageType::Life', 'String::from_str(&env, "life")')
        c = c.replace('CoverageType::Auto', 'String::from_str(&env, "auto")')
        # Also handle SorobanString vs String differences
        c = c.replace('SorobanString::from_str(&env, "health")', 'SorobanString::from_str(&env, "health")') # already good
        with open(f, 'w', encoding='utf-8') as fw:
            fw.write(c)
