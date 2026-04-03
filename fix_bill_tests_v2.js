const fs = require('fs');
const path = require('path');

function processDirectory(dir) {
    const files = fs.readdirSync(dir);
    for (const file of files) {
        const fullPath = path.join(dir, file);
        if (fs.statSync(fullPath).isDirectory()) {
            processDirectory(fullPath);
        } else if (fullPath.endsWith('.rs')) {
            let content = fs.readFileSync(fullPath, 'utf8');
            let modified = false;

            // Simple regex to match client.create_bill and try_create_bill.
            // Using a recursive string balancer is hard in standard JS regex, 
            // but we can match up to the semicolon if they are simple calls.
            // However, arguments might contain nested parens.
            // A simpler approach: replace lines containing create_bill if they don't have None or XLM correctly.
            
            // Let's use a function to balance parentheses securely.
            let idx = 0;
            while ((idx = content.indexOf('create_bill(', idx)) !== -1) {
                // Find where the method call starts 
                // Could be `client.create_bill(` or `client.try_create_bill(`
                let startIdx = idx + 'create_bill('.length;
                let openParen = 1;
                let endIdx = startIdx;
                
                while (endIdx < content.length && openParen > 0) {
                    if (content[endIdx] === '(') openParen++;
                    else if (content[endIdx] === ')') openParen--;
                    endIdx++;
                }
                
                if (openParen === 0) {
                    // We found the bounds of the arguments: content.substring(startIdx, endIdx - 1)
                    let argsStr = content.substring(startIdx, endIdx - 1);
                    
                    // Split by comma, but respect nested parens/strings (rudimentary split)
                    let args = [];
                    let currentArg = "";
                    let nestLevel = 0;
                    let inString = false;
                    for (let i = 0; i < argsStr.length; i++) {
                        let c = argsStr[i];
                        if (c === '"' && argsStr[i-1] !== '\\') inString = !inString;
                        if (!inString) {
                            if (c === '(' || c === '<' || c === '{' || c === '[') nestLevel++;
                            else if (c === ')' || c === '>' || c === '}' || c === ']') nestLevel--;
                        }
                        
                        if (c === ',' && nestLevel === 0 && !inString) {
                            args.push(currentArg.trim());
                            currentArg = "";
                        } else {
                            currentArg += c;
                        }
                    }
                    if (currentArg.trim().length > 0) {
                        args.push(currentArg.trim());
                    }
                    
                    // Now we have the arguments array.
                    // The signature we want is 8 arguments.
                    // owner, name, amount, due_date, recurring, freq, ref, currency
                    let originalLength = args.length;
                    
                    if (args.length === 6) {
                        args.push('&None');
                        args.push('&String::from_str(&env, "XLM")');
                    } else if (args.length === 7) {
                        // Usually the 7th is a currency string or None.
                        if (args[6].includes("XLM")) {
                            args[6] = '&None';
                            args.push('&String::from_str(&env, "XLM")');
                        } else if (args[6].includes("None")) {
                            args.push('&String::from_str(&env, "XLM")');
                        } else {
                            // Default back to safely appending.
                            args[6] = '&None';
                            args.push('&String::from_str(&env, "XLM")');
                        }
                    } else if (args.length === 8) {
                        // Already 8, doing nothing.
                    } else if (args.length === 9) {
                         // Sometimes it's duplicated XLM from previous failed script
                         args = [args[0], args[1], args[2], args[3], args[4], args[5], '&None', '&String::from_str(&env, "XLM")'];
                    }

                    if (args.length !== originalLength || originalLength >= 7) {
                        let newArgsStr = args.join(', ');
                        content = content.substring(0, startIdx) + newArgsStr + content.substring(endIdx - 1);
                        modified = true;
                        idx = startIdx + newArgsStr.length + 1; // Move past this call
                    } else {
                        idx = endIdx;
                    }
                } else {
                    idx += 'create_bill('.length;
                }
            }

            // Also fix set_time to set_ledger_time as found previously
            let newContent = content.replace(/set_time\s*\(\s*&env\s*,\s*([0-9a-zA-Z_+\-*/\s]+)\)/g, 'set_ledger_time(&env, 1, $1)');
            if (newContent !== content) {
                content = newContent;
                modified = true;
            }

            if (modified) {
                fs.writeFileSync(fullPath, content);
                console.log(`Updated ${fullPath}`);
            }
        }
    }
}

const targetDir = path.join(process.cwd(), 'bill_payments');
processDirectory(targetDir);
console.log("Finished script.");
