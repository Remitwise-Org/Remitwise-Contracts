import re
with open('bill_payments/src/lib.rs', 'r', encoding='utf-8') as f:
    text = f.read()

# Replace occurrences like , &String::from_str(&env, "XLM") or , &String::from_str(env, "XLM")
text = re.sub(r',\s*&String::from_str\((?:&?env),\s*"XLM"\)', r', &None, &String::from_str(&env, "XLM")', text)

with open('bill_payments/src/lib.rs', 'w', encoding='utf-8') as f:
    f.write(text)
