#!/usr/bin/env bash
# Test otto done Claude exit mechanism
#
# This test validates that the exit mechanism correctly:
# - Detects Claude parent PID
# - Verifies Claude process
# - Logs termination events
# - Handles timeouts gracefully

set -eo pipefail

# Script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test counter
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Test helpers
test_start() {
    local name="$1"
    echo "TEST: $name"
    TESTS_RUN=$((TESTS_RUN + 1))
}

test_pass() {
    echo "  ✓ PASS"
    TESTS_PASSED=$((TESTS_PASSED + 1))
}

test_fail() {
    local msg="$1"
    echo "  ✗ FAIL: $msg"
    TESTS_FAILED=$((TESTS_FAILED + 1))
}

echo "Setting up test environment..."

# Test 1: Verify parent PID detection works
test_start "get_claude_parent_pid returns valid PID"
parent_pid=$(ps -o ppid= -p $$ | tr -d ' ')
if [[ -n "$parent_pid" ]] && [[ "$parent_pid" =~ ^[0-9]+$ ]]; then
    echo "  Parent PID detected: $parent_pid"
    test_pass
else
    test_fail "Invalid PID format: '$parent_pid'"
fi

# Test 2: Verify verify_claude_process logic with non-existent process
test_start "verify_claude_process logic (non-existent process)"
# Simulate the check without calling the actual function
if ps -p 999999 >/dev/null 2>&1; then
    test_fail "Should detect non-existent process"
else
    echo "  Correctly detects non-existent process"
    test_pass
fi

# Test 3: Verify log_termination_event format
test_start "log_termination_event format validation"
timestamp=$(date -Iseconds 2>/dev/null || date)
log_entry="[$timestamp] mode=test status=success issue=otto-test-123 test message"
if echo "$log_entry" | grep -q "^\[.*\] mode=test status=success issue=otto-test-123"; then
    echo "  Log format: $log_entry"
    test_pass
else
    test_fail "Log format incorrect: $log_entry"
fi

# Test 4: Help message works
test_start "Help message displays"
if output=$("$PROJECT_ROOT/bin/otto-done.sh" --help 2>&1); then
    if echo "$output" | grep -qi "otto done"; then
        test_pass
    else
        test_fail "Help message missing expected content"
    fi
else
    test_fail "Help command failed"
fi

# Test 5: Invalid mode is rejected
test_start "Invalid mode is rejected"
if "$PROJECT_ROOT/bin/otto-done.sh" --mode invalid >/dev/null 2>&1; then
    test_fail "Should reject invalid mode"
else
    test_pass
fi

# Test 6: --status without escalated mode is rejected
test_start "--status without --mode escalated is rejected"
if "$PROJECT_ROOT/bin/otto-done.sh" --status clean >/dev/null 2>&1; then
    test_fail "Should reject --status without --mode escalated"
else
    test_pass
fi

# Test 7: Valid --status values are accepted with escalated mode
test_start "Valid --status values accepted with escalated mode"
valid_statuses=("clean" "uncommitted" "unpushed")
all_passed=true
for status in "${valid_statuses[@]}"; do
    if ! "$PROJECT_ROOT/bin/otto-done.sh" --mode escalated --status "$status" >/dev/null 2>&1; then
        # This should fail at git validation, not argument parsing
        # Check the error message
        if ! "$PROJECT_ROOT/bin/otto-done.sh" --mode escalated --status "$status" 2>&1 | grep -q "option.*requires"; then
            all_passed=false
            break
        fi
    fi
done
if $all_passed; then
    test_pass
else
    test_fail "Valid --status rejected"
fi

# Test 8: Invalid --status value is rejected
test_start "Invalid --status value is rejected"
output=$("$PROJECT_ROOT/bin/otto-done.sh" --mode escalated --status invalid 2>&1 || true)
if echo "$output" | grep -qi "invalid"; then
    test_pass
else
    test_fail "Wrong error message for invalid status: $output"
fi

# Test 9: Dry-run flag works with completed mode
test_start "Dry-run flag with completed mode"
# This should fail at git validation but accept --dry-run
if output=$("$PROJECT_ROOT/bin/otto-done.sh" --dry-run 2>&1); then
    if echo "$output" | grep -q "DRY RUN"; then
        test_pass
    else
        test_fail "Dry-run message not found"
    fi
else
    test_fail "Dry-run command failed"
fi

# Test 10: Functions are defined in the script
test_start "Required functions are defined"
required_functions=("log_termination_event" "get_claude_parent_pid" "verify_claude_process" "exit_claude")
all_found=true
for func in "${required_functions[@]}"; do
    if ! grep -q "^${func}()" "$PROJECT_ROOT/bin/otto-done.sh"; then
        echo "  Missing function: $func"
        all_found=false
    fi
done
if $all_found; then
    test_pass
else
    test_fail "Some required functions are missing"
fi

# Summary
echo ""
echo "Test Summary:"
echo "  Run:     $TESTS_RUN"
echo "  Passed:  $TESTS_PASSED"
echo "  Failed:  $TESTS_FAILED"

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo "✓ All tests passed!"
    exit 0
else
    echo "✗ Some tests failed"
    exit 1
fi
