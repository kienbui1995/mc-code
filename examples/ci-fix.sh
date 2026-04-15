#!/bin/bash
# CI/CD: Auto-fix failing tests with magic-code
# Usage: ./ci-fix.sh
set -e

echo "Running tests..."
if cargo test --workspace 2>&1 | tail -5 | grep -q "FAILED"; then
    echo "Tests failed. Asking magic-code to fix..."
    magic-code --yes --json \
        "The tests are failing. Read the test output, find the bug, and fix it. Run tests again to verify." \
        -o fix-result.json
    
    if [ $? -eq 0 ]; then
        echo "Fix applied. Re-running tests..."
        cargo test --workspace
    else
        echo "magic-code could not fix the issue."
        exit 1
    fi
else
    echo "All tests pass."
fi
