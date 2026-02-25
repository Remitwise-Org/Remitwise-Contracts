#!/bin/bash
# Script to run integration tests
# This demonstrates that the tests compile and are ready for CI

set -e

echo "Building integration tests..."
cargo build -p integration_tests

echo ""
echo "Running integration tests..."
cargo test -p integration_tests --verbose

echo ""
echo "✅ Integration tests completed successfully!"
