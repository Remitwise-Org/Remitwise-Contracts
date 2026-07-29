const fs = require('fs');
const path = require('path');

function fixFile(filePath) {
    let content = fs.readFileSync(filePath, 'utf8');
    
    // 1. Fix create_bill / try_create_bill (8 arguments)
    // Signature: owner, name, amount, due, recurring, freq, ref, currency
    content = content.replace(/(client\.(?:try_)?create_bill\s*\()([\s\S]*?)(\);)/g, (match, prefix, argsStr, suffix) => {
        let args = argsStr.split(',').map(s => s.trim()).filter(s => s.length > 0);
        
        // Handle varying counts
        if (args.length === 6) {
            args.push('&None');
            args.push('&String::from_str(&env, "XLM")');
        } else if (args.length === 7) {
            // Usually 7th was currency or frequency. 
            // If it's 7, it's missing external_ref.
            let last = args[6];
            if (last.includes('XLM')) {
                 args[6] = '&None';
                 args.push(last);
            } else {
                 args.push('&None');
                 args.push('&String::from_str(&env, "XLM")');
            }
        } else if (args.length === 9) {
            // Clean up previous mangling (6 + extra XLM + None + XLM)
            args = [args[0], args[1], args[2], args[3], args[4], args[5], '&None', '&String::from_str(&env, "XLM")'];
        }
        
        return prefix + '\n            ' + args.join(',\n            ') + '\n        ' + suffix;
    });

    // 2. Fix paginated queries (3 arguments)
    // client.get_unpaid_bills(&owner) -> client.get_unpaid_bills(&owner, &0, &50)
    // But we also need to handle .items if it was expecting a Vec.
    // This is trickier with regex. Let's just fix the signature first.
    content = content.replace(/client\.(get_unpaid_bills|get_overdue_bills|get_archived_bills)\s*\(\s*&(\w+)\s*\)/g, 'client.$1(&$2, &0, &50)');

    // 3. Fix set_time -> set_ledger_time
    content = content.replace(/set_time\s*\(\s*&env\s*,\s*([^)]+)\)/g, 'set_ledger_time(&env, 1, $1)');

    // 4. Clean up corrupted suffix in test.rs if this is src/test.rs
    if (filePath.endsWith('test.rs')) {
        const corruptedMarker = '// ---------------------------------------------------------------------------';
        const lines = content.split('\n');
        let cutIdx = -1;
        for (let i = 0; i < lines.length; i++) {
            if (lines[i].includes('// Te    }')) {
                cutIdx = i - 1; 
                break;
            }
        }
        if (cutIdx !== -1) {
            content = lines.slice(0, cutIdx).join('\n') + '\n    }\n}\n';
        }
    }

    fs.writeFileSync(filePath, content);
    console.log(`Fixed ${filePath}`);
}

const filesToFix = [
    path.join(process.cwd(), 'bill_payments', 'src', 'test.rs'),
    path.join(process.cwd(), 'bill_payments', 'tests', 'gas_bench.rs'),
    path.join(process.cwd(), 'bill_payments', 'tests', 'stress_tests.rs'),
    path.join(process.cwd(), 'bill_payments', 'tests', 'stress_test_large_amounts.rs'),
    path.join(process.cwd(), 'bill_payments', 'tests', 'test_notifications.rs')
];

filesToFix.forEach(f => {
    if (fs.existsSync(f)) fixFile(f);
});
