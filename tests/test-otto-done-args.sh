#!/usr/bin/env bash
# Unit tests for otto done argument parsing
#
# Tests all valid and invalid argument combinations for otto done command
#
# Usage: ./tests/test-otto-done-args.sh

set -euo pipefail

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test framework functions
test_start() {
    TESTS_RUN=$((TESTS_RUN + 1))
    echo -n "[$TESTS_RUN] $1... "
}

test_pass() {
    TESTS_PASSED=$((TESTS_PASSED + 1))
    echo -e "${GREEN}PASS${NC}"
}

test_fail() {
    TESTS_FAILED=$((TESTS_FAILED + 1))
    echo -e "${RED}FAIL${NC}"
    echo "  Expected: $1"
    echo "  Got: $2"
}

# Helper: Run otto done and capture exit code and output
# Uses temp file to avoid issues with newlines and special characters
run_otto_done() {
    local tmp_file=$(mktemp)
    local exit_code=0

    "./bin/otto" done "$@" >"$tmp_file" 2>&1 || exit_code=$?
    if [[ -z "$exit_code" ]]; then
        exit_code=0
    fi

    # Output exit code on first line, then output
    echo "$exit_code"
    cat "$tmp_file"
    rm -f "$tmp_file"
}

# Test: No arguments (should succeed with defaults)
# Note: Uses escalated mode to skip git validation for testing purposes
test_start "otto done with no arguments (uses escalated mode)"
result=$(run_otto_done --mode escalated)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if [[ "$exit_code" == "0" ]]; then
    if echo "$output" | grep -q "Mode: escalated"; then
        test_pass
    else
        test_fail "Mode: escalated" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --help should show help and exit 0
test_start "otto done --help"
result=$(run_otto_done --help)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "otto done - Agent self-termination command"; then
        test_pass
    else
        test_fail "help text" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: -h should show help and exit 0
test_start "otto done -h"
result=$(run_otto_done -h)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "otto done - Agent self-termination command"; then
        test_pass
    else
        test_fail "help text" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --issue with valid value
test_start "otto done --issue otto-123"
result=$(run_otto_done --issue otto-123 --mode escalated)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Issue: otto-123"; then
        test_pass
    else
        test_fail "Issue: otto-123" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --issue without value should fail
test_start "otto done --issue (missing value) should fail"
result=$(run_otto_done --issue)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" != "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Option --issue requires an argument"; then
        test_pass
    else
        test_fail "error message" "$output"
    fi
else
    test_fail "non-zero exit code" "exit code $exit_code"
fi

# Test: --mode completed (with --dry-run to skip actual validation)
test_start "otto done --mode completed --dry-run"
result=$(run_otto_done --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Mode: completed"; then
        test_pass
    else
        test_fail "Mode: completed" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --mode escalated
test_start "otto done --mode escalated"
result=$(run_otto_done --mode escalated)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Mode: escalated"; then
        test_pass
    else
        test_fail "Mode: escalated" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --mode with invalid value should fail
test_start "otto done --mode invalid should fail"
result=$(run_otto_done --mode invalid)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" != "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Invalid mode"; then
        test_pass
    else
        test_fail "error message" "$output"
    fi
else
    test_fail "non-zero exit code" "exit code $exit_code"
fi

# Test: --status clean with escalated mode
test_start "otto done --mode escalated --status clean"
result=$(run_otto_done --mode escalated --status clean)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "observation: clean"; then
        test_pass
    else
        test_fail "observation: clean" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --status uncommitted with escalated mode
test_start "otto done --mode escalated --status uncommitted"
result=$(run_otto_done --mode escalated --status uncommitted)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "observation: uncommitted"; then
        test_pass
    else
        test_fail "observation: uncommitted" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --status unpushed with escalated mode
test_start "otto done --mode escalated --status unpushed"
result=$(run_otto_done --mode escalated --status unpushed)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "observation: unpushed"; then
        test_pass
    else
        test_fail "observation: unpushed" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: --status with invalid value should fail
test_start "otto done --mode escalated --status invalid should fail"
result=$(run_otto_done --mode escalated --status invalid)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" != "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Invalid status"; then
        test_pass
    else
        test_fail "error message" "$output"
    fi
else
    test_fail "non-zero exit code" "exit code $exit_code"
fi

# Test: --status without escalated mode should fail
test_start "otto done --status clean (without escalated) should fail"
result=$(run_otto_done --status clean)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" != "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "can only be used with --mode escalated"; then
        test_pass
    else
        test_fail "error message" "$output"
    fi
else
    test_fail "non-zero exit code" "exit code $exit_code"
fi

# Test: --dry-run
test_start "otto done --dry-run"
result=$(run_otto_done --dry-run)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "DRY RUN MODE"; then
        test_pass
    else
        test_fail "DRY RUN MODE" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Multiple valid options combined
test_start "otto done --issue otto-456 --mode completed --dry-run"
result=$(run_otto_done --issue otto-456 --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" == "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Issue: otto-456" && \
       echo "$output" | grep -q "Mode: completed" && \
       echo "$output" | grep -q "DRY RUN MODE"; then
        test_pass
    else
        test_fail "all options present" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Unknown option should fail
test_start "otto done --unknown-option should fail"
result=$(run_otto_done --unknown-option)
exit_code=$(echo "$result" | head -1)
if [[ "$exit_code" != "0" ]]; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Unknown option"; then
        test_pass
    else
        test_fail "error message" "$output"
    fi
else
    test_fail "non-zero exit code" "exit code $exit_code"
fi

# Print summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Tests run:    $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
if [[ $TESTS_FAILED -gt 0 ]]; then
    echo -e "Tests failed: ${RED}$TESTS_FAILED${NC}"
else
    echo "Tests failed: $TESTS_FAILED"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Exit with appropriate code
if [[ $TESTS_FAILED -gt 0 ]]; then
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
