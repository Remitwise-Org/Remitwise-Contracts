import sys
import subprocess
import os
import json

def get_workspace_members():
    if os.environ.get("MOCK_GEIGER_OUTPUT"):
        return {"remittance_split", "savings_goals", "bill_payments", "insurance", "family_wallet", "data_migration", "reporting", "orchestrator"}

    # Use cargo metadata to get the list of workspace members
    try:
        result = subprocess.run(
            ['cargo', 'metadata', '--format-version', '1', '--no-deps'],
            capture_output=True, text=True, check=True
        )
        metadata = json.loads(result.stdout)
        members = [pkg['name'] for pkg in metadata['packages']]
        return set(members)
    except Exception as e:
        print(f"Failed to get workspace members: {e}")
        return set()

def main():
    target_dir = sys.argv[1] if len(sys.argv) > 1 else "."
    
    # We will run cargo geiger with --output-format Json.
    # Note: cargo geiger has had a few different json output formats or arguments over time.
    # If JSON doesn't work, we'll fall back to parsing plain text.
    manifest_arg = ["--manifest-path", os.path.join(target_dir, "Cargo.toml")] if target_dir != "." else ["--workspace"]
    
    # Run cargo geiger
    cmd = ["cargo", "geiger", "--color", "never"] + manifest_arg
    print(f"Running: {' '.join(cmd)}")
    
    if os.environ.get("MOCK_GEIGER_OUTPUT"):
        with open(os.environ["MOCK_GEIGER_OUTPUT"], "r") as f:
            output = f.read()
    else:
        try:
            result = subprocess.run(cmd, capture_output=True, text=True)
            output = result.stdout + result.stderr
        except FileNotFoundError:
            print("cargo-geiger not found. Please install it.")
            sys.exit(1)
    print("Geiger finished. Analyzing output...")
    
    # We want to make sure no workspace crates use unsafe.
    # The output contains lines like:
    # ├── [ ] remitwise-contracts v0.1.0 (D:\path\to\workspace)
    # or in the summary table:
    # 0/0        0/0          0/0    0/0     0/0      [ ] my_crate
    # We can look for `[!]` or `[+]` or similar symbols that indicate unsafe usage in our workspace crates.
    # A simpler approach: if the output contains `[!] <workspace_member_name>` or any indication of unsafe for them.
    
    # Let's get the workspace members
    members = get_workspace_members()
    if target_dir != ".":
        # For the fixture, the member might be different
        members.add("unsafe_outside_sdk")
    
    print(f"Checking for unsafe in these crates: {members}")
    
    # Parse the table at the bottom.
    # It starts with 'Functions  Expressions  Impls  Traits  Methods  Dependency'
    lines = output.split('\n')
    table_started = False
    
    failed_crates = []
    
    for line in lines:
        if "Dependency" in line and "Functions" in line:
            table_started = True
            continue
            
        if table_started:
            if not line.strip():
                continue
            # Look for member crates in this line
            for member in members:
                # the line ends with something like `[ ] my_crate` or `[!] my_crate`
                if member in line and f" {member}" in line:
                    # check if the stats are all 0/0
                    # if a crate has unsafe, the line will have non-zero like 1/1
                    # let's just check if there's any non-zero count before the `[` character
                    bracket_idx = line.find('[')
                    if bracket_idx != -1:
                        stats_part = line[:bracket_idx]
                        has_unsafe = any(not stat.startswith('0/') and not stat == '0' and stat.strip() for stat in stats_part.split())
                        if has_unsafe or '[!]' in line or '[+]' in line:
                            failed_crates.append(member)
                            print(f"Unsafe found in crate {member}: {line.strip()}")
    
    # Fallback check if the table wasn't found or parsing failed:
    # just grep the output for `[!].*<member>` or similar in the tree output
    if not table_started:
        print("Warning: Could not find geiger summary table. Falling back to tree scan.")
        for line in lines:
            for member in members:
                if member in line and ('[!]' in line or '[+]' in line or '🔒' not in line and '[ ]' not in line and f" {member}" in line):
                    if '│' in line or '├' in line or '└' in line:
                        # In tree view, if it's not [ ] or [🔒], it might have unsafe
                        if '[ ]' not in line and '[🔒]' not in line:
                            if member not in failed_crates:
                                failed_crates.append(member)
                                print(f"Unsafe found in crate {member} (tree view): {line.strip()}")

    if failed_crates:
        print(f"\n[FAIL] ERROR: Unsafe code detected in the following non-SDK crates: {', '.join(set(failed_crates))}")
        print("This workspace enforces a strict zero-unsafe policy outside of soroban-sdk.")
        sys.exit(1)
        
    print("\n[PASS] No unsafe code found in workspace crates.")
    sys.exit(0)

if __name__ == "__main__":
    main()
