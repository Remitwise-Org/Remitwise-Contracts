const fs = require('fs');
const path = require('path');

const filePath = path.join(process.cwd(), 'bill_payments', 'src', 'test.rs');

if (fs.existsSync(filePath)) {
    let content = fs.readFileSync(filePath, 'utf8');

    // Fix create_bill / try_create_bill to have 8 arguments
    // Signature: owner, name, amount, due_date, recurring, frequency, external_ref, currency
    content = content.replace(/client\.(?:try_)?create_bill\s*\(([\s\S]*?)\);/g, (match, p1) => {
        let args = p1.split(',').map(s => s.trim()).filter(s => s.length > 0);
        
        // Remove duplicates or extra XLM strings if they exist
        // Some calls have 9 args where one is an extra XLM
        if (args.length > 8) {
             // Heuristic: if we have 9 args, find the extra one. usually it's the extra XLM at index 6 or 7.
             // Correct order: 0:owner, 1:name, 2:amount, 3:due, 4:rec, 5:freq, 6:ref, 7:curr
             args = [args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[args.length-1]];
        } else if (args.length === 6) {
             // Add default ref and currency
             args.push('&None');
             args.push('&String::from_str(&env, "XLM")');
        } else if (args.length === 7) {
             // Add default currency
             args.push('&String::from_str(&env, "XLM")');
        }

        const isTry = match.includes('try_');
        return `client.${isTry ? 'try_create_bill' : 'create_bill'}(${args.join(', ')});`;
    });

    fs.writeFileSync(filePath, content);
    console.log(`Successfully updated ${filePath}`);
} else {
    console.log("File not found.");
}
