#!/usr/bin/env bash
# Unit tests for otto done git validation logic
#
# Tests git state validation functions
#
# Usage: ./tests/test-otto-done-git-validation.sh

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

# Setup: Create temp directory for test repo
TEST_DIR=$(mktemp -d)
ORIGINAL_DIR="$(pwd)"
cd "$TEST_DIR"

echo "Test directory: $TEST_DIR"

# Initialize git repo
git init -q
git config user.email "test@example.com"
git config user.name "Test User"
git config init.defaultBranch main  # Ensure default is main

# Create initial commit
echo "initial" > file.txt
git add file.txt
git commit -q -m "initial commit"

# Rename to main if it's not already (some git versions default to master)
if ! git rev-parse --verify main >/dev/null 2>&1; then
    git branch -m master main
fi

# Add a remote (without actually pushing anywhere)
git remote add origin https://github.com/test/test.git

# Set up remote tracking
git branch -u origin/main 2>/dev/null || true

# Define logging functions (needed by validate_git_state)
log_debug() {
    if [[ "${OTTO_DEBUG:-0}" == "1" ]]; then
        echo "[DEBUG] $*" >&2
    fi
}

log_error() {
    echo "[ERROR] $*" >&2
}

# Load the git helper functions from otto-done.sh
OTTO_DONE="$ORIGINAL_DIR/bin/otto-done.sh"

# Helper: Extract and define a function from otto-done.sh
load_function() {
    local func_name="$1"
    local start_line=$(grep -n "^${func_name}()" "$OTTO_DONE" | cut -d: -f1)
    if [[ -z "$start_line" ]]; then
        echo "ERROR: Function $func_name not found in $OTTO_DONE" >&2
        exit 1
    fi

    # Extract function from start to the closing brace
    tail -n "+$start_line" "$OTTO_DONE" | awk '/^}/ {print; exit} {print}' > /tmp/func_$$.sh
    source /tmp/func_$$.sh
    rm -f /tmp/func_$$.sh
}

# Load all git helper functions
load_function "git_main_branch"
load_function "git_branch_name"
load_function "is_git_clean"
load_function "is_git_pushed"
load_function "has_stashes"
load_function "validate_git_state"

# Helper: Run git command from test repo
run_in_test_repo() {
    (
        cd "$TEST_DIR"
        "$@"
    )
}

# Test: git_main_branch returns "main" when main exists
test_start "git_main_branch returns 'main' when main branch exists"
branch=$(run_in_test_repo git_main_branch)
if [[ "$branch" == "main" ]]; then
    test_pass
else
    test_fail "'main'" "'$branch'"
fi

# Test: git_main_branch returns "master" when only master exists
test_start "git_main_branch returns 'master' when only master branch exists"
run_in_test_repo git branch -m main master
branch=$(run_in_test_repo git_main_branch)
if [[ "$branch" == "master" ]]; then
    test_pass
else
    test_fail "'master'" "'$branch'"
fi
# Reset to main
run_in_test_repo git branch -m master main

# Test: git_branch_name returns current branch
test_start "git_branch_name returns 'main' on main branch"
branch=$(run_in_test_repo git_branch_name)
if [[ "$branch" == "main" ]]; then
    test_pass
else
    test_fail "'main'" "'$branch'"
fi

# Test: is_git_clean returns 0 when working tree is clean
test_start "is_git_clean returns 0 (clean) when working tree is clean"
if run_in_test_repo is_git_clean; then
    test_pass
else
    test_fail "exit code 0 (clean)" "exit code non-zero"
fi

# Test: is_git_clean returns 1 when there are unstaged changes
test_start "is_git_clean returns 1 (dirty) when there are unstaged changes"
run_in_test_repo sh -c 'echo "changed" >> file.txt'
if run_in_test_repo is_git_clean; then
    test_fail "exit code 1 (dirty)" "exit code 0 (clean)"
else
    test_pass
fi

# Test: is_git_clean returns 1 when there are staged changes
test_start "is_git_clean returns 1 (dirty) when there are staged changes"
run_in_test_repo git add file.txt
if run_in_test_repo is_git_clean; then
    test_fail "exit code 1 (dirty)" "exit code 0 (clean)"
else
    test_pass
fi

# Clean up for next tests
run_in_test_repo git reset -q --hard HEAD

# Test: has_stashes returns 0 when no stashes
test_start "has_stashes returns 0 (no stashes) when stash list is empty"
if run_in_test_repo has_stashes; then
    test_fail "exit code 0 (no stashes)" "exit code non-zero (has stashes)"
else
    test_pass
fi

# Test: has_stashes returns 1 when there are stashes
test_start "has_stashes returns 1 (has stashes) when stash list has entries"
run_in_test_repo sh -c 'echo "stash test" >> file.txt && git stash -q'
if run_in_test_repo has_stashes; then
    test_pass
else
    test_fail "exit code 1 (has stashes)" "exit code 0 (no stashes)"
fi

# Clean up stash
run_in_test_repo git stash drop -q

# Test: is_git_pushed returns 0 when all commits are pushed
test_start "is_git_pushed returns 0 (pushed) when local matches remote"
# Since we have a fake remote, we need to set up tracking branch properly
# Create a fake remote tracking branch
run_in_test_repo git update-ref refs/remotes/origin/main HEAD
if run_in_test_repo is_git_pushed; then
    test_pass
else
    test_fail "exit code 0 (pushed)" "exit code non-zero (unpushed)"
fi

# Test: is_git_pushed returns 1 when there are unpushed commits
test_start "is_git_pushed returns 1 (unpushed) when there are local commits not on remote"
run_in_test_repo sh -c 'echo "new file" >> newfile.txt && git add newfile.txt && git commit -q -m "new commit"'
if run_in_test_repo is_git_pushed; then
    test_fail "exit code 1 (unpushed)" "exit code 0 (pushed)"
else
    test_pass
fi

# Test: validate_git_state passes when all checks pass
test_start "validate_git_state returns 0 when git state is clean and pushed"
run_in_test_repo git reset -q --hard HEAD^  # Go back to just the initial commit
if run_in_test_repo validate_git_state; then
    test_pass
else
    test_fail "exit code 0 (valid)" "exit code non-zero (invalid)"
fi

# Test: validate_git_state fails when working tree is dirty
test_start "validate_git_state fails with error when working tree has uncommitted changes"
run_in_test_repo sh -c 'echo "dirty" >> file.txt'
output=$(run_in_test_repo validate_git_state 2>&1 || true)
if [[ "$output" == *"Working tree has uncommitted changes"* ]]; then
    test_pass
else
    test_fail "'Working tree has uncommitted changes'" "'$output'"
fi

# Test: validate_git_state fails when there are unpushed commits
test_start "validate_git_state fails with error when there are unpushed commits"
run_in_test_repo git add file.txt
run_in_test_repo git commit -q -m "unpushed commit"
output=$(run_in_test_repo validate_git_state 2>&1 || true)
if [[ "$output" == *"There are unpushed commits"* ]]; then
    test_pass
else
    test_fail "'There are unpushed commits'" "'$output'"
fi

# Test: validate_git_state fails when there are stashes
test_start "validate_git_state fails with error when there are stashes"
run_in_test_repo git reset -q --hard HEAD^  # Reset to clean state
run_in_test_repo sh -c 'echo "stash" >> file.txt && git stash -q'
output=$(run_in_test_repo validate_git_state 2>&1 || true)
if [[ "$output" == *"You have git stashes"* ]]; then
    test_pass
else
    test_fail "'You have git stashes'" "'$output'"
fi

# Test: validate_git_state is fail-fast (stops at first error)
test_start "validate_git_state stops at first error (fail-fast)"
run_in_test_repo sh -c 'echo "dirty" >> file.txt && echo "dirty2" >> file2.txt'
output=$(run_in_test_repo validate_git_state 2>&1 || true)
# Should report uncommitted changes, not check for stashes or unpushed
if [[ "$output" == *"Working tree has uncommitted changes"* ]] && \
   [[ "$output" != *"There are unpushed commits"* ]] && \
   [[ "$output" != *"You have git stashes"* ]]; then
    test_pass
else
    test_fail "only first error (uncommitted)" "'$output'"
fi

# Test: validate_git_state checks in order: clean -> pushed -> stashes
test_start "validate_git_state checks in correct order"
# Reset to clean state but with a new commit (simulating unpushed)
run_in_test_repo git reset -q --hard HEAD
run_in_test_repo git stash drop -q 2>/dev/null || true
run_in_test_repo sh -c 'echo "unpushed" >> file.txt && git add file.txt && git commit -q -m "unpushed"'
output=$(run_in_test_repo validate_git_state 2>&1 || true)
# Should report unpushed commits (second check), not stashes (third check)
if [[ "$output" == *"There are unpushed commits"* ]] && \
   [[ "$output" != *"You have git stashes"* ]]; then
    test_pass
else
    test_fail "second error (unpushed), not third (stashes)" "'$output'"
fi

# Cleanup
cd "$ORIGINAL_DIR"
rm -rf "$TEST_DIR"

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
