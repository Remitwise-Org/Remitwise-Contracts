import re
import os

file_path = r'c:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\savings_goals\src\test.rs'

with open(file_path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

def fix_content(content):
    # This regex matches client.method(...) and ensures it doesn't already have .unwrap() or try_
    # We use a non-greedy .*? but we need to handle nested parens. 
    # Since Soroban calls don't have deep nesting of different parens usually, we can just look for the next );
    
    methods = ['create_goal', 'add_to_goal', 'withdraw_from_goal']
    for method in methods:
        # Avoid try_ calls
        # We search for client.method( and then the first ); that follows.
        
        # We'll use a state machine instead of regex for safety with multi-line.
        new_content = ""
        pos = 0
        while pos < len(content):
            search_str = f"client.{method}("
            start_idx = content.find(search_str, pos)
            if start_idx == -1:
                new_content += content[pos:]
                break
            
            # Check if it's a try_ call
            is_try = False
            if start_idx >= 4:
                if content[start_idx-4:start_idx] == "try_":
                    is_try = True
            
            # Copy everything up to the match
            new_content += content[pos:start_idx + len(search_str)]
            
            # Find the matching ); 
            # We need to handle nested parens properly to find the true end of the call.
            paren_count = 1
            inner_pos = start_idx + len(search_str)
            end_found = False
            while inner_pos < len(content):
                char = content[inner_pos]
                if char == '(':
                    paren_count += 1
                elif char == ')':
                    paren_count -= 1
                
                if paren_count == 0:
                    # Found the closing paren of the method call.
                    # Check if the next char(s) are .unwrap()
                    if content[inner_pos+1:inner_pos+9] == ".unwrap()":
                        # Already has unwrap, just continue
                        pass
                    elif not is_try:
                        # Needs unwrap
                        new_content += ").unwrap()"
                        pos = inner_pos + 1
                        end_found = True
                        break
                
                new_content += char
                inner_pos += 1
            
            if not end_found:
                # Should not happen if code is valid
                pos = inner_pos
        content = new_content
    return content

with open(file_path, 'r', encoding='utf-8') as f:
    original = f.read()

fixed = fix_content(original)

with open(file_path, 'w', encoding='utf-8', newline='') as f:
    f.write(fixed)

print("Applied state-machine based fixes to multi-line calls.")
