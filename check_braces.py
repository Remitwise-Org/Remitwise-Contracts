def check_braces(filename):
    with open(filename, 'r', encoding='utf-8') as f:
        lines = f.readlines()
        
    stack = []
    for i, line in enumerate(lines):
        for j, char in enumerate(line):
            if char == '{':
                stack.append((i+1, j+1))
            elif char == '}':
                if not stack:
                    print(f"Extra closing brace at {filename}:{i+1}:{j+1}")
                else:
                    stack.pop()
                    
    if stack:
        print(f"Unmatched opening braces in {filename}:")
        for line, col in stack:
            print(f"  Line {line}, col {col}: {lines[line-1].strip()}")
    else:
        print(f"Braces matched in {filename}!")
        
check_braces('insurance/src/lib.rs')
check_braces('insurance/src/test.rs')
