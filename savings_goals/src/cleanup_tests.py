import re
import os

file_path = r'c:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\savings_goals\src\test.rs'

with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Fix double unwraps
fixed = content.replace('.unwrap().unwrap()', '.unwrap()')

# 2. Fix assert_eq! accompanied by accidental unwrap
# Example: assert_eq!(ids[0], 1, "first goal id must be 1").unwrap();
# We look for assert_eq!(...) followed by .unwrap()
lines = fixed.splitlines()
for i in range(len(lines)):
    if 'assert_eq!' in lines[i] and ').unwrap();' in lines[i]:
        lines[i] = lines[i].replace(').unwrap();', ');')

with open(file_path, 'w', encoding='utf-8', newline='') as f:
    f.write('\n'.join(lines) + '\n')

print("Cleaned up test.rs.")
