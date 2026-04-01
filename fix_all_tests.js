const fs = require('fs');
const path = require('path');

const projectRoot = process.cwd();

const filesToFix = [
    path.join(projectRoot, 'insurance', 'src', 'test.rs')
];

filesToFix.forEach(filePath => {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');

    // Fix set_ledger_time(&env, 1000) -> set_ledger_time(&env, 1, 1000)
    content = content.replace(/set_ledger_time\s*\(\s*&env\s*,\s*([0-9a-zA-Z_+\-*/\s]+)\)/g, 'set_ledger_time(&env, 1, $1)');

    // Fix any remaining CoverageType issues in create_policy
    const enumMap = {
        "health": "CoverageType::Health",
        "life": "CoverageType::Life",
        "auto": "CoverageType::Auto",
        "property": "CoverageType::Property",
        "emergency": "CoverageType::Health",
        "type": "CoverageType::Health"
    };

    content = content.replace(/client\.(?:try_)?create_policy\s*\(([\s\S]*?)\);/g, (match, p1) => {
        let args = p1.split(',').map(s => s.trim());
        if (args.length >= 3) {
            for (const [key, val] of Object.entries(enumMap)) {
                if (args[2].toLowerCase().includes(`"${key}"`) || args[2].toLowerCase().includes(`::${key}`)) {
                    args[2] = `&${val}`;
                    break;
                }
            }
        }
        if (args.length === 5) {
            args.push('&None');
        }
        return `client.create_policy(${args.join(', ')});`;
    });

    fs.writeFileSync(filePath, content);
    console.log(`Successfully updated ${filePath}`);
});

console.log("Done.");
