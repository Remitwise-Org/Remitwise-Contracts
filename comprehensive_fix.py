"""
Final comprehensive fix for ALL create_policy and create_bill calls.
Uses line-by-line approach with Windows CRLF handling.
"""
import os

def fix_calls_in_file(path, method_name, expected_args, insert_before_last=True):
    """
    Fix all multiline calls to method_name in file at path.
    If a call has exactly expected_args arguments, inserts &None.
    """
    if not os.path.exists(path):
        print(f"  SKIP: {path}")
        return 0

    with open(path, 'rb') as f:
        raw = f.read()

    content = raw.decode('utf-8', errors='ignore')
    lines = content.split('\n')

    result_lines = []
    i = 0
    fixes = 0

    while i < len(lines):
        line = lines[i]

        # Check if this line starts a create_policy/create_bill call
        if f'.{method_name}(' in line:
            # Collect the full call (from this line to the closing );)
            call_start = i
            call_lines = [line]
            # Count open/close parens
            paren_depth = line.count('(') - line.count(')')
            j = i + 1
            while paren_depth > 0 and j < len(lines):
                call_lines.append(lines[j])
                paren_depth += lines[j].count('(') - lines[j].count(')')
                j += 1

            # Count arguments by counting commas at depth 0
            full_call = '\n'.join(call_lines)
            # Find the method call opening paren
            idx = full_call.find(f'.{method_name}(')
            if idx == -1:
                result_lines.append(line)
                i += 1
                continue

            start_paren = idx + len(f'.{method_name}(')
            depth = 1
            commas = 0
            k = start_paren
            while k < len(full_call) and depth > 0:
                ch = full_call[k]
                if ch == '/' and k+1 < len(full_call) and full_call[k+1] == '/':
                    # Skip to end of line
                    while k < len(full_call) and full_call[k] != '\n':
                        k += 1
                    continue
                if ch == '(':
                    depth += 1
                elif ch == ')':
                    depth -= 1
                    if depth == 0:
                        break
                elif ch == ',' and depth == 1:
                    commas += 1
                k += 1

            arg_count = commas + 1

            if arg_count == expected_args:
                # Need to insert &None before the closing );
                # Find the last line that contains ); and insert &None before it
                # The closing ); is on the last line of call_lines
                last_line = call_lines[-1]
                # Determine indentation from the second line (first arg line)
                if len(call_lines) >= 2:
                    second_line = call_lines[1]
                    indent = ''
                    for ch in second_line:
                        if ch in (' ', '\t'):
                            indent += ch
                        else:
                            break
                else:
                    indent = '        '

                # Insert &None line before closing
                call_lines.insert(-1, indent + '&None,')
                fixes += 1

            result_lines.extend(call_lines)
            i = j
        else:
            result_lines.append(line)
            i += 1

    new_content = '\n'.join(result_lines)
    with open(path, 'wb') as f:
        f.write(new_content.encode('utf-8'))

    print(f"  Fixed {fixes} {method_name} calls in {path}")
    return fixes

# Fix all create_policy calls with 5 args (should have 6)
policy_files = [
    'insurance/src/test.rs',
    'insurance/tests/stress_tests.rs',
    'insurance/tests/gas_bench.rs',
    'integration_tests/tests/multi_contract_integration.rs',
    'examples/insurance_example.rs',
]

for f in policy_files:
    fix_calls_in_file(f, 'create_policy', 5)

# Fix all create_bill calls with 7 args (should have 8)
bill_files = [
    'bill_payments/src/test.rs',
    'bill_payments/tests/stress_tests.rs',
    'bill_payments/tests/stress_test_large_amounts.rs',
    'scenarios/tests/flow.rs',
    'integration_tests/tests/multi_contract_integration.rs',
    'examples/bill_payments_example.rs',
]

for f in bill_files:
    fix_calls_in_file(f, 'create_bill', 7)

print("\nAll done!")
