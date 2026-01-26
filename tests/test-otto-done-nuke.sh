#!/usr/bin/env bash
# Unit tests for otto done --nuke workspace cleanup functionality
#
# Tests the --nuke flag for workspace cleanup
#
# Usage: ./tests/test-otto-done-nuke.sh

set -euo pipefail

# Script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Test setup
TEST_DIR=$(mktemp -d)
export TEST_DIR
TEST_REPO="$TEST_DIR/test-repo"
TEST_WORKSPACE="$TEST_DIR/test-workspace"

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
run_otto_done() {
    local tmp_file=$(mktemp)
    local exit_code=0

    cd "$PROJECT_ROOT"
    "./bin/otto-done.sh" "$@" >"$tmp_file" 2>&1 || exit_code=$?
    if [[ -z "$exit_code" ]]; then
        exit_code=0
    fi

    # Output exit code on first line, then output
    echo "$exit_code"
    cat "$tmp_file"
    rm -f "$tmp_file"
}

# Helper: Setup test repo
setup_test_repo() {
    mkdir -p "$TEST_REPO"
    cd "$TEST_REPO"
    git init -q
    git config user.email "test@example.com"
    git config user.name "Test User"
    echo "test" > test.txt
    git add test.txt
    git commit -q -m "Initial commit"
}

# Helper: Create a workspace
create_test_workspace() {
    local workspace_path="$1"
    cd "$TEST_REPO"
    git worktree add -q -b "test-branch" "$workspace_path" 2>/dev/null || true
    if [[ -d "$workspace_path" ]]; then
        echo "workspace content" > "$workspace_path/workspace.txt"
        (cd "$workspace_path" && git add workspace.txt && git commit -q -m "Add workspace file" 2>/dev/null || true)
    fi
}

# Setup
setup_test_repo

# Test: --nuke with escalated mode should fail
test_start "otto done --nuke requires completed mode"
result=$(run_otto_done --nuke --mode escalated 2>&1)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "can only be used with --mode completed"; then
    test_pass
else
    test_fail "error message about --nuke requiring completed mode" "$output"
fi

# Test: --nuke with no OTTO_WORKSPACE should handle gracefully
test_start "otto done --nuke with no OTTO_WORKSPACE"
unset OTTO_WORKSPACE
result=$(run_otto_done --nuke --dry-run)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "No workspace to clean up"; then
    test_pass
else
    test_fail "'No workspace to clean up' message" "$output"
fi

# Test: --nuke with nonexistent workspace should handle gracefully
test_start "otto done --nuke with nonexistent workspace path"
export OTTO_WORKSPACE="/tmp/nonexistent-workspace-$$"
result=$(run_otto_done --nuke --dry-run)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "Workspace path does not exist"; then
    test_pass
else
    test_fail "'Workspace path does not exist' message" "$output"
fi

# Test: --nuke --dry-run should show commands
test_start "otto done --nuke --dry-run shows commands to be executed"
create_test_workspace "$TEST_WORKSPACE"
export OTTO_WORKSPACE="$TEST_WORKSPACE"
result=$(run_otto_done --nuke --dry-run)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "Would run: git worktree remove"; then
    test_pass
else
    test_fail "'Would run: git worktree remove' message" "$output"
fi

# Test: --nuke --yes should skip confirmation
test_start "otto done --nuke --yes skips confirmation prompt"
create_test_workspace "$TEST_WORKSPACE"
export OTTO_WORKSPACE="$TEST_WORKSPACE"
result=$(OTTO_DEBUG=1 run_otto_done --nuke --yes --dry-run 2>&1 || true)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "Skipping confirmation"; then
    test_pass
else
    test_fail "'Skipping confirmation' message" "$output"
fi

# Test: --nuke displays workspace information
test_start "otto done --nuke displays workspace path, branch, and bead"
create_test_workspace "$TEST_WORKSPACE"
export OTTO_WORKSPACE="$TEST_WORKSPACE"

# Create .workspace-info with bead ID
cat > "$TEST_WORKSPACE/.workspace-info" <<EOF
issue_id=otto-123
branch=test-branch
EOF

result=$(run_otto_done --nuke --dry-run)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "Workspace:" && \
   echo "$output" | grep -q "Branch:" && \
   echo "$output" | grep -q "Bead: otto-123"; then
    test_pass
else
    test_fail "workspace, branch, and bead information" "$output"
fi

# Test: --nuke shows workspace cleanup step
test_start "otto done --nuke shows workspace cleanup step"
create_test_workspace "$TEST_WORKSPACE"
export OTTO_WORKSPACE="$TEST_WORKSPACE"
result=$(run_otto_done --nuke --dry-run)
exit_code=$(echo "$result" | head -1)
output=$(echo "$result" | tail -n +2)
if echo "$output" | grep -q "Step 5: Cleaning up workspace"; then
    test_pass
else
    test_fail "'Step 5: Cleaning up workspace' message" "$output"
fi

# Cleanup
cd "$PROJECT_ROOT"
rm -rf "$TEST_DIR"

# Print summary
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Test Summary"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Tests run:    $TESTS_RUN"
echo "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo "Tests failed: ${RED}$TESTS_FAILED${NC}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [[ $TESTS_FAILED -eq 0 ]]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    exit 1
fi
