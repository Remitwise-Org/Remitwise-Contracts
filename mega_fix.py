import os
import re

# =========================================================
# Fix 1: insurance/src/test.rs - broken ", &None);" pattern
# The code has:
#   &10000, // coverage_amount
# , &None);
# Should be:
#   &10000, // coverage_amount
#   &None);
# =========================================================
for path in ['insurance/src/test.rs']:
    if not os.path.exists(path):
        continue
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()
    # Pattern: a line ending with a comment, then newline, then "    , &None);"
    content = re.sub(r'(// [^\n]*)\n(\s*), (&None\);)', r'\1\n\2\3', content)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

# =========================================================
# Fix 2: scenarios/tests/flow.rs - missing &None in create_bill
# =========================================================
path = 'scenarios/tests/flow.rs'
if os.path.exists(path):
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()
    # Find multiline calls to bills_client.create_bill with 7 args and add &None before currency
    # Pattern: the last arg before currency is &30, (frequency_days)
    content = content.replace(
        '        &30,\n        &String::from_str(&env, "USDC"),\n    );',
        '        &30,\n        &None,\n        &String::from_str(&env, "USDC"),\n    );'
    )
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

# =========================================================
# Fix 3: All example files - remove .unwrap() and fix println!
# =========================================================
for root, dirs, files in os.walk('examples'):
    for fname in files:
        if not fname.endswith('.rs'):
            continue
        path = os.path.join(root, fname)
        with open(path, 'r', encoding='utf-8', errors='ignore') as f:
            content = f.read()

        # Remove .unwrap() chains - they won't work on primitive return types
        content = content.replace('.unwrap()', '')

        # Fix println! format strings - change {} to {:?} for all occurrences
        # (safe: {:?} works for everything, {} only works for Display)
        content = re.sub(r'\{([^}:!]*?)\}', lambda m: '{:?}' if m.group(1) == '' else m.group(0), content)

        with open(path, 'w', encoding='utf-8') as f:
            f.write(content)

# =========================================================
# Fix 4: examples/family_wallet_example.rs - FamilyRole import
# =========================================================
path = 'examples/family_wallet_example.rs'
if os.path.exists(path):
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()
    # Replace import
    content = content.replace(
        'use family_wallet::{FamilyWallet, FamilyWalletClient, FamilyRole};',
        'use family_wallet::{FamilyWallet, FamilyWalletClient};\nuse remitwise_common::FamilyRole;'
    )
    # Also handle the case without trailing whitespace variations
    content = re.sub(
        r'use family_wallet::\{([^}]*),\s*FamilyRole\s*\};',
        lambda m: f'use family_wallet::{{{m.group(1)}}};\nuse remitwise_common::FamilyRole;',
        content
    )
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

# =========================================================
# Fix 5: examples/reporting_example.rs - Category import and unwrap
# =========================================================
path = 'examples/reporting_example.rs'
if os.path.exists(path):
    with open(path, 'r', encoding='utf-8', errors='ignore') as f:
        content = f.read()
    # Fix Category import - it comes from remitwise_common, not reporting
    content = re.sub(
        r'use reporting::\{([^}]*),\s*Category\s*\};',
        lambda m: f'use reporting::{{{m.group(1)}}};\nuse remitwise_common::Category;',
        content
    )
    content = content.replace(
        'use reporting::{ReportingContractClient, Category};',
        'use reporting::{ReportingContractClient};\nuse remitwise_common::Category;'
    )
    with open(path, 'w', encoding='utf-8') as f:
        f.write(content)

print("Done!")
