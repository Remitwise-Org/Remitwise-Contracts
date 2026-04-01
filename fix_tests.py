import re
import os

files_to_fix = [
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\insurance\src\test.rs',
    r'c:\Users\Muyideen.Jsx\Desktop\WorkSpace\Wave Proj\Remitwise-Contracts\insurance\src\lib.rs'
]

patterns_to_enums = {
    r'&String::from_str\(&env, "(?i)health"\)': '&CoverageType::Health',
    r'&String::from_str\(&env, "(?i)life"\)': '&CoverageType::Life',
    r'&String::from_str\(&env, "(?i)property"\)': '&CoverageType::Property',
    r'&String::from_str\(&env, "(?i)auto"\)': '&CoverageType::Auto',
    r'&String::from_str\(&env, "(?i)liability"\)': '&CoverageType::Liability',
    r'&String::from_str\(&env, "(?i)emergency"\)': '&CoverageType::Health', # mapping emergency to health as fallback
    r'&String::from_str\(&env, "T1"\)': '&CoverageType::Health',
    r'&String::from_str\(&env, "T2"\)': '&CoverageType::Life',
    r'&String::from_str\(&env, "T3"\)': '&CoverageType::Auto',
    r'&String::from_str\(&env, "Type 1"\)': '&CoverageType::Health',
    r'&String::from_str\(&env, "Type 2"\)': '&CoverageType::Life',
    r'&String::from_str\(&env, "Type"\)': '&CoverageType::Health', 
}

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

def fix_create_policy(match):
    name = match.group(1) # client.(try_)?create_policy
    args_text = match.group(2)
    suffix = match.group(3) # ) or );
    
    args = split_args(args_text)
    
    if len(args) == 5:
        # Add &None
        new_args_text = args_text.rstrip().rstrip(',') + ',\n                &None'
        return f"{name}({new_args_text}){suffix}"
    return match.group(0)

def fix_get_active_policies(match):
    name = match.group(1) # client.get_active_policies
    args_text = match.group(2)
    suffix = match.group(3)
    
    args = split_args(args_text)
    # New signature: (env, owner) -> Vec
    # Old signature: (env, owner, cursor, limit) -> Page
    if len(args) >= 3:
        # Keep only the first two args (usually &env is omitted in client calls, so it's &owner, &cursor, &limit)
        # In client calls, args are typically (&owner, &cursor, &limit)
        new_args_text = args[0]
        return f"{name}({new_args_text}){suffix}"
    return match.group(0)

def fix_get_all_policies_for_owner(match):
    name = match.group(1)
    args_text = match.group(2)
    suffix = match.group(3)
    
    args = split_args(args_text)
    # Old pagination style: .items.len(), .count
    # New style: returns Vec directly (Wait, let me check get_all_policies_for_owner in lib.rs)
    return match.group(0)

for file_path in files_to_fix:
    if not os.path.exists(file_path):
        print(f"Skipping {file_path}")
        continue
        
    with open(file_path, 'r') as f:
        content = f.read()
    
    # Enum replacements
    for pattern, replacement in patterns_to_enums.items():
        content = re.sub(pattern, replacement, content)
        
    # create_policy (5 -> 6 args)
    content = re.sub(r'(client\.(?:try_)?create_policy)\s*\((.*?)\)(\s*[;,\)])', fix_create_policy, content, flags=re.DOTALL)
    
    # get_active_policies (3 -> 1 args in client call)
    content = re.sub(r'(client\.get_active_policies)\s*\((.*?)\)(\s*[;,\)])', fix_get_active_policies, content, flags=re.DOTALL)
    
    # Pagination result cleanup: .items.len() -> .len(), .items.get(0) -> .get(0)
    content = content.replace('.items.len()', '.len()')
    content = content.replace('.items.get(', '.get(')
    content = content.replace('.items.iter()', '.iter()')
    
    with open(file_path, 'w') as f:
        f.write(content)
    print(f"Successfully updated {file_path}")

print("Done.")
