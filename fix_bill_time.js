const fs = require('fs');
const path = require('path');

const filePath = path.join(process.cwd(), 'bill_payments', 'src', 'test.rs');

if (fs.existsSync(filePath)) {
    let content = fs.readFileSync(filePath, 'utf8');

    // Fix set_time(&env, T) -> set_ledger_time(&env, 1, T)
    content = content.replace(/set_time\s*\(\s*&env\s*,\s*([0-9a-zA-Z_+\-*/\s]+)\)/g, 'set_ledger_time(&env, 1, $1)');

    fs.writeFileSync(filePath, content);
    console.log(`Successfully updated ${filePath}`);
} else {
    console.log("File not found.");
}
