import re
import os

files_to_fix = [
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\insurance\src\test.rs',
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\insurance\src\lib.rs',
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\bill_payments\src\test.rs',
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\bill_payments\src\lib.rs'
]

def split_args(args_text):
    args = []
    current_arg = ""
    paren_depth = 0
    for char in args_text:
        if char == ',' and paren_depth == 0:
            args.append(current_arg.strip())
            current_arg = ""
        else:
            if char == '(': paren_depth += 1
            if char == ')': paren_depth -= 1
            current_arg += char
    if current_arg.strip():
        args.append(current_arg.strip())
    return args

def fix_insurance_create_policy(match):
    name = match.group(1)
    args_text = match.group(2)
    suffix = match.group(3)
    args = split_args(args_text)
    
    if len(args) == 5:
        # Add external_ref = &None
        return f"{name}({args_text.rstrip().rstrip(',')},\n                &None){suffix}"
    return match.group(0)

def fix_bill_payments_create_bill(match, is_lib_rs=False):
    name = match.group(1)
    args_text = match.group(2)
    suffix = match.group(3)
    args = split_args(args_text)
    
    # Target signature (8 args for client):
    # (owner, name, amount, due_date, recurring, frequency_days, external_ref, currency)
    
    if len(args) == 7:
        # Assume it's: (owner, name, amount, due_date, recurring, frequency_days, currency)
        # Missing external_ref at index 6
        if is_lib_rs:
            # Inline tests in lib.rs often omit '&' for some args if they are passed directly
            # but usually it's currency at the end.
            new_args = args[:6] + ["&None"] + [args[6]]
        else:
            new_args = args[:6] + ["&None"] + [args[6]]
        return f"{name}({', '.join(new_args)}){suffix}"
    
    if len(args) == 9:
        # Some tests have 9 args (e.g. line 64 in bill_payments/src/test.rs)
        # It looks like: (owner, name, amount, due_date, recurring, frequency_days, currency, external_ref, currency)
        # We should keep only 8.
        new_args = args[:6] + [args[7], args[8]]
        return f"{name}({', '.join(new_args)}){suffix}"
        
    return match.group(0)

def fix_insurance_get_active_policies(match):
    name = match.group(1)
    args_text = match.group(2)
    suffix = match.group(3)
    args = split_args(args_text)
    if len(args) >= 3:
        return f"{name}({args[0]}){suffix}"
    return match.group(0)

for file_path in files_to_fix:
    if not os.path.exists(file_path):
        print(f"Skipping {file_path}")
        continue
    
    with open(file_path, 'r', encoding='utf-8') as f:
        content = f.read()

    is_bill_payments = "bill_payments" in file_path
    is_insurance = "insurance" in file_path
    is_lib_rs = "lib.rs" in file_path

    if is_insurance:
        # Replace string literals with CoverageType enum
        patterns_to_enums = {
            r'&String::from_str\(&env, "(?i)health"\)': '&CoverageType::Health',
            r'&String::from_str\(&env, "(?i)life"\)': '&CoverageType::Life',
            r'&String::from_str\(&env, "(?i)property"\)': '&CoverageType::Property',
            r'&String::from_str\(&env, "(?i)auto"\)': '&CoverageType::Auto',
            r'&String::from_str\(&env, "(?i)liability"\)': '&CoverageType::Liability',
            r'&String::from_str\(&env, "(?i)emergency"\)': '&CoverageType::Health',
            r'&String::from_str\(&env, "T1"\)': '&CoverageType::Health',
            r'&String::from_str\(&env, "T2"\)': '&CoverageType::Life',
            r'&String::from_str\(&env, "T3"\)': '&CoverageType::Auto',
            r'&String::from_str\(&env, "Type 1"\)': '&CoverageType::Health',
            r'&String::from_str\(&env, "Type 2"\)': '&CoverageType::Life',
            r'&String::from_str\(&env, "Type"\)': '&CoverageType::Health',
        }
        for pattern, replacement in patterns_to_enums.items():
            content = re.sub(pattern, replacement, content)
            
        content = re.sub(r'(client\.(?:try_)?create_policy)\s*\((.*?)\)(\s*[;,\)])', fix_insurance_create_policy, content, flags=re.DOTALL)
        content = re.sub(r'(client\.get_active_policies)\s*\((.*?)\)(\s*[;,\)])', fix_insurance_get_active_policies, content, flags=re.DOTALL)
        
        # Cleanup .items patterns
        content = content.replace('.items.len()', '.len()')
        content = content.replace('.items.get(', '.get(')
        content = content.replace('.items.iter()', '.iter()')

    if is_bill_payments:
        content = re.sub(r'(client\.(?:try_)?create_bill)\s*\((.*?)\)(\s*[;,\)])', lambda m: fix_bill_payments_create_bill(m, is_lib_rs), content, flags=re.DOTALL)

    with open(file_path, 'w', encoding='utf-8') as f:
        f.write(content)
    print(f"Successfully updated {file_path}")

print("Done.")
