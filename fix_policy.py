"""
Precisely fix all create_policy(...) calls in insurance/src/test.rs
that have exactly 5 args by appending &None as the 6th argument.
Uses a line-by-line state machine to correctly find call boundaries.
"""

import os

FILES = [
    'insurance/src/test.rs',
    'integration_tests/tests/multi_contract_integration.rs',
]

def count_args(lines_inside):
    """Count the number of arguments in the lines inside a call."""
    depth = 0
    count = 1  # starts at 1 because we count the first arg
    for line in lines_inside:
        stripped = line.strip()
        # Strip comments
        code = stripped.split('//')[0].strip()
        for ch in code:
            if ch == '(':
                depth += 1
            elif ch == ')':
                depth -= 1
            elif ch == ',' and depth == 0:
                count += 1
    return count

def fix_file(path):
    if not os.path.exists(path):
        print(f"  [SKIP] {path} does not exist")
        return

    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        lines = f.readlines()

    result = []
    i = 0
    changed = 0

    while i < len(lines):
        line = lines[i]
        stripped = line.rstrip()

        # Detect start of a create_policy call
        if '.create_policy(' in stripped:
            # Gather the complete call
            call_lines = [line]
            paren_depth = stripped.count('(') - stripped.count(')')
            j = i + 1
            while paren_depth > 0 and j < len(lines):
                call_lines.append(lines[j])
                paren_depth += lines[j].count('(') - lines[j].count(')')
                j += 1

            # Count arguments inside
            inner = call_lines[1:-1] if len(call_lines) > 2 else []
            # Include first and last for counting (minus the opening call line and closing paren)
            call_content = ''.join(call_lines)
            # Extract from ( to )
            start = call_content.find('(')
            depth = 0
            end = -1
            for k, ch in enumerate(call_content[start:], start):
                if ch == '(':
                    depth += 1
                elif ch == ')':
                    depth -= 1
                    if depth == 0:
                        end = k
                        break
            inner_content = call_content[start+1:end]

            # Count commas at depth 0 to determine arg count
            depth = 0
            commas = 0
            in_comment = False
            k = 0
            while k < len(inner_content):
                ch = inner_content[k]
                if ch == '/' and k+1 < len(inner_content) and inner_content[k+1] == '/':
                    # Skip to end of line
                    while k < len(inner_content) and inner_content[k] != '\n':
                        k += 1
                    in_comment = False
                elif ch == '(':
                    depth += 1
                elif ch == ')':
                    depth -= 1
                elif ch == ',' and depth == 0:
                    commas += 1
                k += 1

            arg_count = commas + 1 if inner_content.strip() else 0

            if arg_count == 5:
                # Find the last line that has a closing paren and insert &None before it
                # The closing ) should be on the last call line
                last_line = call_lines[-1]
                indent = ' ' * (len(last_line) - len(last_line.lstrip()))
                # Replace the closing paren line to add &None before it
                # Check if the second-to-last line already ends with &None
                second_last = call_lines[-2].rstrip() if len(call_lines) >= 2 else ''
                if '&None' not in second_last:
                    call_lines.insert(-1, indent + '    &None,\n')
                    changed += 1

            result.extend(call_lines)
            i = j
        else:
            result.append(line)
            i += 1

    with open(path, 'w', encoding='utf-8') as f:
        f.writelines(result)

    print(f"  Fixed {changed} calls in {path}")

for p in FILES:
    fix_file(p)
print("Done!")
