import re
import sys

file_path = '/home/maryjane/Desktop/workspace/job/HushLuxe/drip/Remitwise-Contracts/savings_goals/src/lib.rs'

with open(file_path, 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    new_lines.append(line)
    # Match env.storage().persistent().set(&DataKey::XXXX(id), &val);
    match = re.search(r'env\.storage\(\)\.persistent\(\)\.set\((&DataKey::[A-Za-z0-9]+\([^)]+\)), &[^)]+\);', line)
    if match:
        key = match.group(1)
        # Check if the next line already has extend_ttl
        # (simplified check: look ahead 1-2 lines)
        already_has = False
        # We'll just add it and then we can deduplicate if needed, but safer to check.
    
    # Actually, a simpler way is to just look for the set and add the extend_ttl if the next non-empty line isn't it.

# Let's try a different approach.
content = "".join(lines)

def add_ttl(m):
    key = m.group(1)
    full_set = m.group(0)
    # Check if already followed by extend_ttl
    rest = content[m.end():m.end()+200]
    if 'extend_ttl' in rest and key in rest:
        return full_set
    
    indent = re.match(r'^\s*', full_set).group(0)
    return f"{full_set}\n{indent}env.storage().persistent().extend_ttl({key}, INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);"

# Match pattern: env.storage().persistent().set(&DataKey::...(...), &...);
pattern = re.compile(r'env\.storage\(\)\.persistent\(\)\.set\((&DataKey::[A-Za-z0-9]+(?:\([^)]*\))?), &[^;]+\);')

new_content = pattern.sub(add_ttl, content)

with open(file_path, 'w') as f:
    f.write(new_content)
