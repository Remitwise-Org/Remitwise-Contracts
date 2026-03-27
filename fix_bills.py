import os
import re

def fix_file(path):
    print(f"Fixing {path}...")
    with open(path, 'r', encoding='utf-8') as f:
        content = f.read()

    def replacer(match):
        prefix = match.group(1) 
        args = match.group(2)
        # Simple split by comma might fail if there are nested commas, but Soroban calls usually don't have them in simple tests
        arg_list = [a.strip() for a in args.split(',')]
        if len(arg_list) == 6:
            return f"{prefix}{args}, &None, &String::from_str(&env, \"XLM\"))"
        elif len(arg_list) == 7:
             return f"{prefix}{args}, &String::from_str(&env, \"XLM\"))"
        return match.group(0)

    # Simple single-line regex.
    new_content = re.sub(r'(client\.(?:try_)?create_bill\()([^)]+)\)', replacer, content)
    
    with open(path, 'w', encoding='utf-8') as f:
        f.write(new_content)

paths = [
    r"c:\Users\USER\Desktop\wek wek wek wek\whizness\looking-for-guiding-money\Remitwise-Contracts\bill_payments\tests\stress_tests.rs",
    r"c:\Users\USER\Desktop\wek wek wek wek\whizness\looking-for-guiding-money\Remitwise-Contracts\bill_payments\tests\stress_test_large_amounts.rs",
    r"c:\Users\USER\Desktop\wek wek wek wek\whizness\looking-for-guiding-money\Remitwise-Contracts\bill_payments\src\test.rs",
    r"c:\Users\USER\Desktop\wek wek wek wek\whizness\looking-for-guiding-money\Remitwise-Contracts\bill_payments\tests\test_notifications.rs",
]

for p in paths:
    if os.path.exists(p):
        fix_file(p)
    else:
        print(f"Skipping {p}")
