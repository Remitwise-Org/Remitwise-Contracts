import re
import os

path = r'C:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\bill_payments\src\test.rs'

import os

path = r'C:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\bill_payments\src\test.rs'

with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

def fix_args(args_str):
    # Split args by comma, but respect nested parentheses/brackets
    args = []
    bracket_level = 0
    current_arg = ""
    for char in args_str:
        if char == ',' and bracket_level == 0:
            args.append(current_arg.strip())
            current_arg = ""
        else:
            if char in '([{': bracket_level += 1
            if char in ')]}': bracket_level -= 1
            current_arg += char
    if current_arg:
        args.append(current_arg.strip())
    
    new_args = args[:]
    
    # Standardize to 8 arguments
    if len(new_args) == 7:
        if "XLM" in new_args[-1]:
            new_args.insert(6, "&None")
        else:
            new_args.append('&String::from_str(&env, "XLM")')
    elif len(new_args) == 6:
        new_args.append("&None")
        new_args.append('&String::from_str(&env, "XLM")')
    elif len(new_args) == 9:
        if "XLM" in new_args[6] and "XLM" in new_args[8]:
             new_args.pop(6)
        elif "XLM" in new_args[6] and "None" in new_args[7] and "XLM" in new_args[8]:
             new_args.pop(6)

    # Force to 8
    if len(new_args) > 8:
        new_args = new_args[:8]
    elif len(new_args) < 8:
        while len(new_args) < 8:
            new_args.append("&None")

    # Swap if needed
    if len(new_args) == 8:
        if "XLM" in new_args[6] and "None" in new_args[7]:
             new_args[6], new_args[7] = new_args[7], new_args[6]

    return ", ".join(new_args)

# Manual scan instead of re.sub to handle nested parens perfectly
new_content = ""
pos = 0
while pos < len(content):
    # Look for the next call
    match_cb = content.find("client.create_bill(", pos)
    match_tcb = content.find("client.try_create_bill(", pos)
    
    if match_cb == -1 and match_tcb == -1:
        new_content += content[pos:]
        break
        
    start_idx = match_cb if (match_tcb == -1 or (match_cb != -1 and match_cb < match_tcb)) else match_tcb
    call_str = "client.create_bill(" if start_idx == match_cb else "client.try_create_bill("
    
    new_content += content[pos:start_idx]
    new_content += call_str
    
    # Find the matching closing paren
    paren_pos = start_idx + len(call_str)
    bracket_level = 1
    args_start = paren_pos
    while bracket_level > 0 and paren_pos < len(content):
        if content[paren_pos] == '(': bracket_level += 1
        elif content[paren_pos] == ')': bracket_level -= 1
        paren_pos += 1
    
    args_str = content[args_start:paren_pos-1]
    new_content += fix_args(args_str)
    new_content += ")"
    
    pos = paren_pos

with open(path, 'w', encoding='utf-8') as f:
    f.write(new_content)

print("Done fixing bill_payments/src/test.rs")
