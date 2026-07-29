#!/bin/bash
set -e

echo "Running unsafe check on main workspace (should pass)..."
export MOCK_GEIGER_OUTPUT=tests/fixtures/unsafe_outside_sdk/mock_pass.txt
python3 scripts/check_unsafe.py .

echo "Running unsafe check on fixture crate (should fail)..."
export MOCK_GEIGER_OUTPUT=tests/fixtures/unsafe_outside_sdk/mock_fail.txt
if python3 scripts/check_unsafe.py tests/fixtures/unsafe_outside_sdk; then
    echo "❌ Fixture should have failed the unsafe check, but it passed!"
    exit 1
else
    echo "✅ Fixture failed as expected."
fi
