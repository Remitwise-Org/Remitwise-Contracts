import os
import re

tests_dir = 'bill_payments/tests'
files = [f for f in os.listdir(tests_dir) if f.endswith('.rs')]

for filename in files:
    filepath = os.path.join(tests_dir, filename)
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    # Need to match lines like mount: &500i128, and replace with mount: 500i128,
    # Also due_date: &due_date, to due_date: due_date, (but not changing 
ame: &name unless it's in CreateBillConfig)
    
    # Just replacing within the CreateBillConfig block is safest
    def fix_config_block(match):
        block = match.group(0)
        block = re.sub(r'amount:\s*&', 'amount: ', block)
        block = re.sub(r'due_date:\s*&', 'due_date: ', block)
        block = re.sub(r'recurring:\s*&', 'recurring: ', block)
        block = re.sub(r'frequency_days:\s*&', 'frequency_days: ', block)
        return block

    new_content = re.sub(r'CreateBillConfig\s*\{[^}]+\}', fix_config_block, content)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(new_content)

print('Fixed reference types in CreateBillConfig.')
