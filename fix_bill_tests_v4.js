const fs = require('fs');
const path = require('path');

function fixFile(filePath) {
    let content = fs.readFileSync(filePath, 'utf8');
    
    // 1. Fix create_bill / try_create_bill (8 arguments)
    // owner, name, amount, due, recurring, freq, ref, currency
    content = content.replace(/(client\.(?:try_)?create_bill\s*\()([\s\S]*?)(\);)/g, (match, prefix, argsStr, suffix) => {
        let args = argsStr.split(',').map(s => s.trim()).filter(s => s.length > 0);
        
        if (args.length === 6) {
            args.push('&None');
            args.push('&String::from_str(&env, "XLM")');
        } else if (args.length === 7) {
            let last = args[6];
            if (last.includes('XLM')) {
                 args[6] = '&None';
                 args.push(last);
            } else {
                 args.push('&None');
                 args.push('&String::from_str(&env, "XLM")');
            }
        } else if (args.length === 9) {
            args = [args[0], args[1], args[2], args[3], args[4], args[5], '&None', '&String::from_str(&env, "XLM")'];
        }
        
        return prefix + '\n            ' + args.join(',\n            ') + '\n        ' + suffix;
    });

    // 2. Fix paginated queries (3 arguments) and handle .items
    // Identify variables that hold paginated results
    const paginatedFuncs = ['get_unpaid_bills', 'get_overdue_bills', 'get_archived_bills', 'get_unpaid_bills_by_currency'];
    
    paginatedFuncs.forEach(func => {
        // Fix the call itself first
        const callRegex = new RegExp(`client\\.${func}\\s*\\(\\s*&(\\w+)\\s*\\)`, 'g');
        content = content.replace(callRegex, `client.${func}(&$1, &0, &50)`);
        
        // Find variable names assigned from these calls
        const assignRegex = new RegExp(`let\\s+(\\w+)\\s*=\\s*client\\.${func}`, 'g');
        let match;
        while ((match = assignRegex.exec(content)) !== null) {
            const varName = match[1];
            // Replace var.len() -> var.items.len()
            const lenRegex = new RegExp(`${varName}\\.len\\(\\)`, 'g');
            content = content.replace(lenRegex, `${varName}.items.len()`);
            
            // Replace var.iter() -> var.items.iter()
            const iterRegex = new RegExp(`${varName}\\.iter\\(\\)`, 'g');
            content = content.replace(iterRegex, `${varName}.items.iter()`);

            // Replace var.get( -> var.items.get(
            const getRegex = new RegExp(`${varName}\\.get\\(`, 'g');
            content = content.replace(getRegex, `${varName}.items.get(`);
        }
    });

    // 3. Special case for direct calls: client.get_unpaid_bills(&owner).len()
    content = content.replace(/client\.(get_unpaid_bills|get_overdue_bills|get_archived_bills)\s*\(([^)]+)\)\.len\(\)/g, 
        (match, func, args) => `client.${func}(${args}).items.len()`);

    // 4. Fix set_time -> set_ledger_time
    content = content.replace(/set_time\s*\(\s*&env\s*,\s*([^)]+)\)/g, 'set_ledger_time(&env, 1, $1)');

    // 5. Clean up corrupted suffix in test.rs
    if (filePath.endsWith('test.rs')) {
        const lines = content.split('\n');
        let cutIdx = -1;
        for (let i = 0; i < lines.length; i++) {
            if (lines[i].includes('// Te    }')) { cutIdx = i - 1; break; }
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
    path.join(process.cwd(), 'bill_payments', 'tests', 'stress_tests.rs')
];

filesToFix.forEach(f => {
    if (fs.existsSync(f)) fixFile(f);
});
