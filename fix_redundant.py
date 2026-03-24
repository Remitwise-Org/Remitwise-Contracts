import os
import re

files = [
    'bill_payments/tests/stress_tests.rs',
    'bill_payments/tests/stress_test_large_amounts.rs'
]

for filepath in files:
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Replace 'amount: amount,' with 'amount,'
    # Replace 'due_date: due_date,' with 'due_date,'
    
    new_content = re.sub(r'amount:\s*amount,', 'amount,', content)
    new_content = re.sub(r'due_date:\s*due_date,', 'due_date,', new_content)
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(new_content)

print('Fixed redundant field names.')
