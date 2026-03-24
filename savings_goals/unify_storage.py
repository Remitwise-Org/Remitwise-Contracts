import codecs
import re

path = r'c:\Users\ADMIN\Desktop\remmy-drips\Remitwise-Contracts\savings_goals\src\lib.rs'
with codecs.open(path, 'r', 'utf-8') as f:
    c = f.read()

# Replace all .instance() with .persistent()
# This is safe here because we want to move the entire state.
c = c.replace('.instance()', '.persistent()')

# Now handle extend_ttl. 
# Persistent storage extend_ttl requires a key.
# env.storage().persistent().extend_ttl(threshold, bump) -> needs to be fixed.
# But wait, did we have a helper fn extend_instance_ttl?
# Let's check for calls and then fix the helper or the calls.

with codecs.open(path, 'w', 'utf-8') as f:
    f.write(c)

print("Globally replaced .instance() with .persistent() in lib.rs")
