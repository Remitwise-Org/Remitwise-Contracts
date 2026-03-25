"""
Precise fix: add &None to all create_policy() calls with exactly 5 args.
A call with 5 args ends in:
        &<number>,
    );
We need to change to:
        &<number>,
        &None,
    );
"""
import re, os

def fix_create_policy_n_args(path, target_arg_count, insert_arg):
    if not os.path.exists(path):
        return

    with open(path, encoding='utf-8', errors='ignore') as f:
        content = f.read()

    # Find all create_policy calls
    pattern = re.compile(r'(\.create_policy\s*\()(\s[^;]*?)(\s*\))', re.DOTALL)
    changes = 0

    def replacer(m):
        nonlocal changes
        open_paren = m.group(1)
        body = m.group(2)
        close_paren = m.group(3)

        # Count top-level commas
        depth = 0
        commas = 0
        in_line_comment = False
        i = 0
        b = body
        while i < len(b):
            ch = b[i]
            if ch == '/' and i+1 < len(b) and b[i+1] == '/':
                while i < len(b) and b[i] != '\n':
                    i += 1
                continue
            if ch == '(':
                depth += 1
            elif ch == ')':
                depth -= 1
            elif ch == ',' and depth == 0:
                commas += 1
            i += 1

        arg_count = commas + 1 if body.strip() else 0

        if arg_count == target_arg_count:
            # Find where to insert: before the closing whitespace + )
            # trailing = leading whitespace of close_paren
            trailing_ws = re.match(r'(\s*)', close_paren).group(1)
            # Determine indent from first arg
            first_arg_indent = re.search(r'\n(\s+)', body)
            indent = first_arg_indent.group(1) if first_arg_indent else '        '
            new_body = body.rstrip()
            if not new_body.endswith(','):
                new_body += ','
            new_body += f'\n{indent}{insert_arg},'
            changes += 1
            return open_paren + new_body + '\n' + trailing_ws.lstrip('\n') + close_paren.lstrip()
        return m.group(0)

    new_content = pattern.sub(replacer, content)

    with open(path, 'w', encoding='utf-8') as f:
        f.write(new_content)

    print(f"Fixed {changes} calls in {path}")

FILES_5_ARG_POLICY = [
    'insurance/src/test.rs',
    'integration_tests/tests/multi_contract_integration.rs',
]
for p in FILES_5_ARG_POLICY:
    fix_create_policy_n_args(p, 5, '&None')

FILES_7_ARG_BILL = [
    'integration_tests/tests/multi_contract_integration.rs',
    'scenarios/tests/flow.rs',
]
for p in FILES_7_ARG_BILL:
    def fix_create_bill_n_args(path, target_arg_count=7, insert_arg='&None'):
        if not os.path.exists(path):
            return
        with open(path, encoding='utf-8', errors='ignore') as f:
            content = f.read()
        pattern = re.compile(r'(\.create_bill\s*\()(\s[^;]*?)(\s*\))', re.DOTALL)
        changes = 0
        def replacer(m):
            nonlocal changes
            open_paren = m.group(1)
            body = m.group(2)
            close_paren = m.group(3)
            depth = 0
            commas = 0
            i = 0
            b = body
            while i < len(b):
                ch = b[i]
                if ch == '/' and i+1 < len(b) and b[i+1] == '/':
                    while i < len(b) and b[i] != '\n':
                        i += 1
                    continue
                if ch == '(':
                    depth += 1
                elif ch == ')':
                    depth -= 1
                elif ch == ',' and depth == 0:
                    commas += 1
                i += 1
            arg_count = commas + 1 if body.strip() else 0
            if arg_count == target_arg_count:
                trailing_ws = re.match(r'(\s*)', close_paren).group(1)
                first_arg_indent = re.search(r'\n(\s+)', body)
                indent = first_arg_indent.group(1) if first_arg_indent else '        '
                new_body = body.rstrip()
                if not new_body.endswith(','):
                    new_body += ','
                new_body += f'\n{indent}{insert_arg},'
                changes += 1
                return open_paren + new_body + '\n' + trailing_ws.lstrip('\n') + close_paren.lstrip()
            return m.group(0)
        new_content = pattern.sub(replacer, content)
        with open(path, 'w', encoding='utf-8') as f:
            f.write(new_content)
        print(f"Fixed {changes} create_bill calls in {path}")
    fix_create_bill_n_args(p)

print("All done!")
