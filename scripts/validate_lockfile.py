#!/usr/bin/env python3
import sys
import os

def check_lockfile(lockfile_content, expected_version):
    # Standardize line endings
    lockfile_content = lockfile_content.replace('\r\n', '\n')
    
    # A simple state machine to find [[package]] blocks
    packages = lockfile_content.split('[[package]]')
    found = False
    for pkg in packages:
        # Trim whitespace and skip empty lines
        lines = [line.strip() for line in pkg.splitlines() if line.strip()]
        is_soroban_sdk = False
        version = None
        for line in lines:
            if line.startswith('name = "soroban-sdk"'):
                is_soroban_sdk = True
            elif line.startswith('version = '):
                version = line.split('=', 1)[1].strip().strip('"')
        
        if is_soroban_sdk:
            found = True
            if version is None:
                return False, "soroban-sdk package entry in Cargo.lock is missing a version."
            if version != expected_version:
                return False, f"soroban-sdk version in Cargo.lock is '{version}', but expected '{expected_version}'."
                
    if not found:
        return False, "soroban-sdk package entry not found in Cargo.lock."
        
    return True, "Lockfile is valid."

def run_tests():
    print("Running validate_lockfile.py self-tests...")
    
    # Happy path test
    happy_content = """
[[package]]
name = "other-package"
version = "1.0.0"

[[package]]
name = "soroban-sdk"
version = "21.7.7"
source = "registry+https://github.com/rust-lang/crates.io-index"
"""
    ok, msg = check_lockfile(happy_content, "21.7.7")
    assert ok, f"Happy path failed: {msg}"
    print("✓ Happy path passed")

    # Sad path 1: unexpected version
    sad_content_1 = """
[[package]]
name = "soroban-sdk"
version = "22.0.0"
"""
    ok, msg = check_lockfile(sad_content_1, "21.7.7")
    assert not ok, "Sad path 1 (unexpected version) should have failed but passed"
    assert "expected '21.7.7'" in msg, f"Expected message to contain version details, got: {msg}"
    print("✓ Sad path (unexpected version) passed")

    # Sad path 2: missing package
    sad_content_2 = """
[[package]]
name = "other-package"
version = "1.0.0"
"""
    ok, msg = check_lockfile(sad_content_2, "21.7.7")
    assert not ok, "Sad path 2 (missing package) should have failed but passed"
    assert "not found" in msg, f"Expected message to mention not found, got: {msg}"
    print("✓ Sad path (missing package) passed")

    print("All self-tests passed successfully!")

def main():
    if len(sys.argv) > 1 and sys.argv[1] == '--test':
        run_tests()
        sys.exit(0)

    expected = "21.7.7"
    lockfile_path = "Cargo.lock"
    
    # Allow passing lockfile path or expected version
    if len(sys.argv) > 1:
        lockfile_path = sys.argv[1]
    if len(sys.argv) > 2:
        expected = sys.argv[2]

    if not os.path.exists(lockfile_path):
        # Try resolving relative to workspace root if called from subdirs
        potential_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "Cargo.lock")
        if os.path.exists(potential_path):
            lockfile_path = potential_path
        else:
            print(f"Error: Lockfile not found at '{lockfile_path}'", file=sys.stderr)
            sys.exit(1)

    try:
        with open(lockfile_path, 'r', encoding='utf-8') as f:
            content = f.read()
    except Exception as e:
        print(f"Error reading '{lockfile_path}': {e}", file=sys.stderr)
        sys.exit(1)

    ok, msg = check_lockfile(content, expected)
    if not ok:
        print("=================================================================", file=sys.stderr)
        print("❌ CARGO.LOCK VALIDATION FAILURE", file=sys.stderr)
        print(f"Details: {msg}", file=sys.stderr)
        print(f"Action: Ensure Cargo.lock uses soroban-sdk version '{expected}'.", file=sys.stderr)
        print("        Do not silently update Cargo.lock or commit mismatched versions.", file=sys.stderr)
        print("=================================================================", file=sys.stderr)
        sys.exit(1)

    print(f"✓ Cargo.lock validation passed (soroban-sdk == {expected})")
    sys.exit(0)

if __name__ == '__main__':
    main()
