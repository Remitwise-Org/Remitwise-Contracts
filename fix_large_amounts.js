const fs = require('fs');
const path = require('path');

const filePath = path.join(process.cwd(), 'bill_payments', 'tests', 'stress_test_large_amounts.rs');
let content = fs.readFileSync(filePath, 'utf8');

// Fix create_bill(7 args -> 8 args)
content = content.replace(/(client\.create_bill\s*\()([\s\S]*?)(\);)/g, (match, prefix, argsStr, suffix) => {
    let args = argsStr.split(',').map(s => s.trim()).filter(s => s.length > 0);
    if (args.length === 7) {
        // Most calls have user, name, amount, due, recurring, None, currency
        // 5: recurring
        // 6: None (this was external_ref in old 7-arg, or currency in old 6-arg)
        // Correct 8-arg: owner, name, amount, due, recurring, freq, ref, currency
        
        // Let's see: 
        // 4: recurring (&false or &true)
        // 5: &None
        // 6: &String::from_str(&env, "XLM")
        
        let recurring = args[4];
        let freq = (recurring.includes('true')) ? '&30u32' : '&0u32';
        let extRef = '&None';
        let currency = args[6];
        
        return prefix + '\n            ' + args[0] + ',\n            ' + args[1] + ',\n            ' + args[2] + ',\n            ' + args[3] + ',\n            ' + recurring + ',\n            ' + freq + ',\n            ' + extRef + ',\n            ' + currency + '\n        ' + suffix;
    }
    return match;
});

// Fix get_unpaid_bills call at end
content = content.replace(/client\.get_unpaid_bills\(&owner, &0, &10\)/g, 'client.get_unpaid_bills(&owner, &0, &10)');
// Actually it was already 3 args in some places? Let's check.

fs.writeFileSync(filePath, content);
console.log('Fixed stress_test_large_amounts.rs');
