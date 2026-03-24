import os
import re

tests_dir = 'bill_payments/tests'
files = [f for f in os.listdir(tests_dir) if f.endswith('.rs')]

pattern = re.compile(r'client\.create_bill\(\s*([\w\&]+),\s*(.+?),\s*(.+?),\s*(.+?),\s*(.+?),\s*(.+?),\s*(.+?),\s*\)', re.DOTALL)

def replacer(match):
    owner = match.group(1)
    name = match.group(2).strip()
    amount = match.group(3).strip()
    due_date = match.group(4).strip()
    recurring = match.group(5).strip()
    freq = match.group(6).strip()
    currency = match.group(7).strip()
    
    # Handle name cloning if it's a reference
    name_val = f"{name}.clone()" if name.startswith("&") else name
    
    # Handle currency string creation or cloning
    curr_val = f"{currency}.clone()" if currency.startswith("&") else currency
    if curr_val.startswith("&String::from_str"):
        curr_val = curr_val[1:] # Remove &
        if curr_val.endswith(".clone()"):
            curr_val = curr_val[:-8]
    if curr_val.startswith("&soroban_sdk::String::from_str"):
        curr_val = curr_val[1:]
        if curr_val.endswith(".clone()"):
            curr_val = curr_val[:-8]

    return f'''client.create_bill(
        {owner},
        &remitwise_common::CreateBillConfig {{
            name: {named},
            amount: {amount},
            due_date: {due_date},
            recurring: {recurring},
            frequency_days: {freq},
            external_ref: None,
            currency: {curr_val},
        }}
    )'''

for filename in files:
    filepath = os.path.join(tests_dir, filename)
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Adding CreateBillConfig import if not present
    if 'remitwise_common::CreateBillConfig' not in content:
        content = content.replace('use remitwise_common::{', 'use remitwise_common::{CreateBillConfig, ')
        if 'CreateBillConfig' not in content:
            content = "use remitwise_common::CreateBillConfig;\n" + content
    
    # Just use the fully qualified path in the replacement to avoid import issues
    
    def safe_repl(m):
        owner = m.group(1)
        name = m.group(2).strip()
        amount = m.group(3).strip()
        due_date = m.group(4).strip()
        recurring = m.group(5).strip()
        freq = m.group(6).strip()
        currency = m.group(7).strip()
        
        name_val = f"{name[1:]}.clone()" if name.startswith("&") else f"{name}.clone()"
        curr_val = currency
        if curr_val.startswith("&String::from_str"):
            curr_val = curr_val[1:]
        elif curr_val.startswith("&soroban_sdk::String::from_str"):
            curr_val = curr_val[1:]
        elif curr_val.startswith("&"):
            curr_val = f"{curr_val[1:]}.clone()"
        else:
            curr_val = f"{curr_val}.clone()"

        return f'''client.create_bill(
        {owner},
        &remitwise_common::CreateBillConfig {{
            name: {name_val},
            amount: {amount},
            due_date: {due_date},
            recurring: {recurring},
            frequency_days: {freq},
            external_ref: None,
            currency: {curr_val},
        }}
    )'''

    new_content = pattern.sub(safe_repl, content)
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(new_content)

print('Replaced create_bill usages in tests.')
