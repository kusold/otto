#!/usr/bin/env bash
# Unit tests for otto done beads sync and close logic
#
# Tests beads operations: sync, close, clear hook
#
# Usage: ./tests/test-otto-done-beads.sh

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

# Helper: Check if exit code is valid (0 or 144)
# Exit code 144 is the expected exit code for otto done
is_valid_exit_code() {
    local exit_code="$1"
    [[ "$exit_code" == "0" ]] || [[ "$exit_code" == "144" ]]
}

# Helper: Run otto done and capture exit code and output
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

# Helper: Create a temporary hook file
create_hook_file() {
    local bead_id="$1"
    echo "$bead_id" > .beads/hook
}

# Helper: Remove hook file if it exists
remove_hook_file() {
    rm -f .beads/hook
}

# Helper: Create a test bead
create_test_bead() {
    local bead_id="$1"
    bd create --id="$bead_id" --title="Test bead for otto done" --type=task --priority=2 >/dev/null 2>&1 || true
}

# Helper: Get bead status
get_bead_status() {
    local bead_id="$1"
    bd show "$bead_id" 2>/dev/null | grep "Status:" | awk '{print $2}' || echo "unknown"
}

# Save current git state
GIT_STASH_BEFORE=$(git stash list)

# Test: Completed mode with dry-run should show sync/close steps
test_start "Completed mode --dry-run shows beads sync/close steps"
result=$(run_otto_done --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Syncing beads" && \
       echo "$output" | grep -q "Closing hooked bead"; then
        test_pass
    else
        test_fail "beads sync/close messages" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Escalated mode with dry-run should show best-effort sync
test_start "Escalated mode --dry-run shows best-effort sync"
result=$(run_otto_done --mode escalated --dry-run)
exit_code=$(echo "$result" | head -1)
if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Attempting bd sync (best effort)"; then
        test_pass
    else
        test_fail "best-effort sync message" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Escalated mode with hook file should detect it
test_start "Escalated mode detects hooked bead from .beads/hook"
create_test_bead "otto-test-001"
create_hook_file "otto-test-001"
result=$(run_otto_done --mode escalated --dry-run)
exit_code=$(echo "$result" | head -1)
remove_hook_file
bd close "otto-test-001" >/dev/null 2>&1 || true

if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Leaving hooked bead open: otto-test-001"; then
        test_pass
    else
        test_fail "hooked bead detection message" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Escalated mode without hook file should handle gracefully
test_start "Escalated mode without hook file handles gracefully"
remove_hook_file
result=$(run_otto_done --mode escalated --dry-run)
exit_code=$(echo "$result" | head -1)
if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    # Should complete successfully even without a hooked bead
    if echo "$output" | grep -q "Termination sequence complete"; then
        test_pass
    else
        test_fail "successful completion message" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Hook file cleanup (dry-run should mention it)
test_start "Completed mode --dry-run mentions clearing hook bead state"
result=$(run_otto_done --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Clearing hook bead state"; then
        test_pass
    else
        test_fail "clear hook bead message" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Bead close with explicit issue ID
test_start "Completed mode closes bead specified by --issue"
create_test_bead "otto-test-002"
# We can't actually close it in dry-run mode without valid git state
# So just check that the issue ID is used correctly
result=$(run_otto_done --issue otto-test-002 --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
bd close "otto-test-002" >/dev/null 2>&1 || true

if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    if echo "$output" | grep -q "Issue: otto-test-002"; then
        test_pass
    else
        test_fail "issue ID in output" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
fi

# Test: Sync happens before close in completed mode
test_start "Completed mode order: sync before close (dry-run)"
result=$(run_otto_done --mode completed --dry-run)
exit_code=$(echo "$result" | head -1)
if is_valid_exit_code "$exit_code"; then
    output=$(echo "$result" | tail -n +2)
    # Check that both steps appear and are numbered sequentially
    if echo "$output" | grep -q "Step 2: Syncing beads" && \
       echo "$output" | grep -q "Step 3: Closing hooked bead"; then
        test_pass
    else
        test_fail "sync and close steps present" "$output"
    fi
else
    test_fail "exit code 0" "exit code $exit_code"
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

# Restore git state
GIT_STASH_AFTER=$(git stash list)
if [[ "$GIT_STASH_BEFORE" != "$GIT_STASH_AFTER" ]]; then
    echo "Warning: Git stash state changed during tests"
fi

# Exit with appropriate code
if [[ $TESTS_FAILED -gt 0 ]]; then
    exit 1
else
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
fi
