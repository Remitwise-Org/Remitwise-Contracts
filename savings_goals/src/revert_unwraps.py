import re
import os

file_path = r'c:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\savings_goals\src\test.rs'

with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

# Remove .unwrap() from the regular calls
methods = ['create_goal', 'add_to_goal', 'withdraw_from_goal', 'create_savings_schedule', 'modify_savings_schedule', 'cancel_savings_schedule', 'unlock_goal', 'lock_goal', 'set_time_lock']

fixed = content
for method in methods:
    # Match client.method(...) followed by .unwrap()
    # We use the paren counter logic to be safe.
    
    new_content = ""
    pos = 0
    while pos < len(fixed):
        search_str = f"client.{method}("
        start_idx = fixed.find(search_str, pos)
        if start_idx == -1:
            new_content += fixed[pos:]
            break
        
        # Check if it's a try_ call (keep unwraps for try_ if we added them, though usually try_ needs manual handling)
        is_try = False
        if start_idx >= 4 and fixed[start_idx-4:start_idx] == "try_":
            is_try = True
        
        new_content += fixed[pos:start_idx + len(search_str)]
        
        # Find closing paren
        paren_count = 1
        inner_pos = start_idx + len(search_str)
        end_found = False
        while inner_pos < len(fixed):
            char = fixed[inner_pos]
            if char == '(':
                paren_count += 1
            elif char == ')':
                paren_count -= 1
            
            if paren_count == 0:
                # Found end of call
                # Check for .unwrap()
                tail = fixed[inner_pos+1:inner_pos+10]
                if tail.startswith(".unwrap()"):
                    if not is_try:
                        # Remove it!
                        new_content += ")"
                        pos = inner_pos + 10 # skip ).unwrap()
                        end_found = True
                        break
            
            new_content += char
            inner_pos += 1
        
        if not end_found:
            pos = inner_pos

    fixed = new_content

with open(file_path, 'w', encoding='utf-8', newline='') as f:
    f.write(fixed)

print("Reverted unnecessary unwraps in test.rs.")
