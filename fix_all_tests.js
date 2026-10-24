const fs = require('fs');
const path = require('path');

const projectRoot = process.cwd();

const filesToFix = [
    path.join(projectRoot, 'orchestrator', 'src', 'test.rs')
];

filesToFix.forEach(filePath => {
    if (!fs.existsSync(filePath)) return;
    let content = fs.readFileSync(filePath, 'utf8');

    // Ensure the orchestrator test target compiles: soroban-sdk testutils
    // must be enabled for the test target.
    const cargoTomlPath = path.join(projectRoot, 'orchestrator', 'Cargo.toml');
    if (fs.existsSync(cargoTomlPath)) {
        let cargoToml = fs.readFileSync(cargoTomlPath, 'utf8');
        if (!/\[dev-dependencies\]/.test(cargoToml)) {
            cargoToml += '\n[dev-dependencies]\nsoroban-sdk = { workspace = true, features = ["testutils"] }\n';
        } else if (!/soroban-sdk[^\n]*testutils/.test(cargoToml)) {
            cargoToml = cargoToml.replace(/\[dev-dependencies\]/, '[dev-dependencies]\nsoroban-sdk = { workspace = true, features = ["testutils"] }');
        }
        fs.writeFileSync(cargoTomlPath, cargoToml);
        console.log(`Successfully updated ${cargoTomlPath}`);
    }

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
