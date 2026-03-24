import os
import re

filepath = 'bill_payments/tests/stress_test_large_amounts.rs'
with open(filepath, 'r', encoding='utf-8') as f:
    content = f.read()

# Pattern to match the mangled CreateBillConfig blocks
# They all follow a similar pattern:
# &remitwise_common::CreateBillConfig {
#     name: String::from_str(&env.clone(),
#     amount: "NAME"),
#     due_date: &VARIABLE,
#     recurring: &TIMESTAMP,
#     frequency_days: &BOOL,
#     external_ref: None,
#     currency: VAL,
# &None,
# &String::from_str(&env, "XLM").clone(),
# }

# We need to extract:
# NAME, VARIABLE (amount), TIMESTAMP (due_date), BOOL (recurring), VAL (frequency_days)

pattern = re.compile(
    r'&remitwise_common::CreateBillConfig\s*\{\s*'
    r'name:\s*String::from_str\(&env\.clone\(\),\s*'
    r'amount:\s*"(.+?)"\),\s*'
    r'due_date:\s*&(.+?),\s*'
    r'recurring:\s*&(.+?),\s*'
    r'frequency_days:\s*&(.+?),\s*'
    r'external_ref:\s*None,\s*'
    r'currency:\s*(.+?),\s*'
    r'&None,\s*'
    r'&String::from_str\(&env,\s*"XLM"\)\.clone\(\),\s*\}',
    re.DOTALL
)

def fix(m):
    name = m.group(1)
    amount_var = m.group(2)
    due_date_val = m.group(3)
    recurring_val = m.group(4)
    freq_val = m.group(5)
    
    # Correct positions:
    # name: ...
    # amount: amount_var
    # due_date: due_date_val
    # recurring: recurring_val
    # frequency_days: freq_val
    
    return f'''&remitwise_common::CreateBillConfig {{
            name: String::from_str(&env, "{name}"),
            amount: {amount_var},
            due_date: {due_date_val},
            recurring: {recurring_val},
            frequency_days: {freq_val},
            external_ref: None,
            currency: String::from_str(&env, "XLM"),
        }}'''

# Apply the fix
new_content = pattern.sub(fix, content)

# There are also some in a loop that look slightly different
# name: String::from_str(&env.clone(),
# amount: &format!("Bill{}",
# due_date: i)),
# recurring: &amount,
# frequency_days: &1000000,
# external_ref: None,
# currency: false,
# &0,
# &None,
# &String::from_str(&env, "XLM").clone(),

loop_pattern = re.compile(
    r'&remitwise_common::CreateBillConfig\s*\{\s*'
    r'name:\s*String::from_str\(&env\.clone\(\),\s*'
    r'amount:\s*&format!\("(.+?)",\s*due_date:\s*(.+?)\)\),\s*'
    r'recurring:\s*&(.+?),\s*'
    r'frequency_days:\s*&(.+?),\s*'
    r'external_ref:\s*None,\s*'
    r'currency:\s*(.+?),\s*'
    r'&0,\s*'
    r'&None,\s*'
    r'&String::from_str\(&env,\s*"XLM"\)\.clone\(\),\s*\}',
    re.DOTALL
)

def fix_loop(m):
    name_fmt = m.group(1)
    name_arg = m.group(2)
    amount_var = m.group(3)
    due_date_val = m.group(4)
    recurring_val = m.group(5) # This was false in original?
    
    return f'''&remitwise_common::CreateBillConfig {{
            name: String::from_str(&env, &format!("{name_fmt}", {name_arg})),
            amount: {amount_var},
            due_date: {due_date_val},
            recurring: {recurring_val},
            frequency_days: 0,
            external_ref: None,
            currency: String::from_str(&env, "XLM"),
        }}'''

# new_content = loop_pattern.sub(fix_loop, new_content)
# Let's just do a more general replace if it's still weird.
# Actually, I'll just write a script that has the FINISHED content if I can read it all.
# But it's 473 lines.

# Let's try one more general regex that catches everything and I'll inspect them.

with open(filepath, 'w', encoding='utf-8') as f:
    f.write(new_content)

print('Updated stress_test_large_amounts.rs')
