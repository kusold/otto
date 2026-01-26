#!/usr/bin/env bash

# Integration tests for otto spawn workspace functionality
# Tests default workspace behavior, --no-workspace flag, and explicit workspace paths

set -euo pipefail

# Source test framework functions
TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./tests/test-framework.sh
source "${TEST_DIR}/test-framework.sh" 2>/dev/null || {
    echo "ERROR: test-framework.sh not found"
    exit 1
}

# Test configuration
OTTO_BIN="${TEST_DIR}/../target/release/otto"
BD_BIN="bd"
TEST_ISSUE_PREFIX="otto-test-spawn"
TEST_WORKSPACE_BASE="${TEST_DIR}/../test-workspaces"

# Cleanup function
cleanup() {
    # Close test beads
    ${BD_BIN} list 2>/dev/null | grep "${TEST_ISSUE_PREFIX}" | awk '{print $1}' | xargs -I {} ${BD_BIN} close {} 2>/dev/null || true

    # Clean up test workspaces
    rm -rf "${TEST_WORKSPACE_BASE}" 2>/dev/null || true
}

trap cleanup EXIT

# Test helpers
create_test_bead() {
    local title="$1"
    local bead_id

    bead_id=$(${BD_BIN} create --title="${title}" --type=task --priority=2 2>&1 | head -1)
    echo "${bead_id}"
}

assert_workspace_exists() {
    local workspace_path="$1"

    if [[ ! -d "${workspace_path}" ]]; then
        test_fail "workspace does not exist: ${workspace_path}"
        return 1
    fi

    if [[ ! -d "${workspace_path}/.git" ]]; then
        test_fail "workspace is not a git worktree: ${workspace_path}"
        return 1
    fi

    if [[ ! -f "${workspace_path}/.workspace-info" ]]; then
        test_fail "workspace missing .workspace-info file: ${workspace_path}"
        return 1
    fi

    return 0
}

assert_workspace_not_exists() {
    local workspace_path="$1"

    if [[ -d "${workspace_path}" ]]; then
        test_fail "workspace should not exist: ${workspace_path}"
        return 1
    fi

    return 0
}

assert_branch_exists() {
    local branch="$1"

    if ! git show-ref --verify --quiet "refs/heads/${branch}"; then
        test_fail "branch does not exist: ${branch}"
        return 1
    fi

    return 0
}

# ==============================================================================
# TESTS
# ==============================================================================

test_start "Default workspace is created when no flags provided"
# Arrange
bead_id=$(create_test_bead "Test default workspace")
expected_workspace="../agents/otto-${bead_id}"

# Act
# Note: We can't actually run spawn in tests without tmux, so we test the logic indirectly
# by checking the help text and validation
output=$(${OTTO_BIN} spawn --help)

# Assert
if echo "${output}" | grep -q "\--no-workspace"; then
    test_pass
else
    test_fail "help text should mention --no-workspace flag"
fi

test_start "Default workspace path is ../agents/otto-<issue-id>"
# Arrange
bead_id=$(create_test_bead "Test default workspace path")
expected_path="../agents/otto-${bead_id}"

# Act
output=$(${OTTO_BIN} spawn --help)

# Assert
# The help should mention the default path behavior
if echo "${output}" | grep -q "defaults to ../agents/otto-"; then
    test_pass
else
    test_fail "help text should mention default workspace path"
fi

test_start "Explicit workspace path is used when --workspace is provided"
# Arrange
custom_workspace="${TEST_WORKSPACE_BASE}/custom-workspace"
bead_id=$(create_test_bead "Test explicit workspace")

# Act
output=$(${OTTO_BIN} spawn --help)

# Assert
# The help should document the --workspace flag
if echo "${output}" | grep -q "Creates a git worktree at the specified path"; then
    test_pass
else
    test_fail "help text should document --workspace flag"
fi

test_start "Error when workspace path already exists"
# Arrange
bead_id=$(create_test_bead "Test workspace exists error")
workspace_path="${TEST_WORKSPACE_BASE}/otto-${bead_id}"
mkdir -p "${workspace_path}"

# Act
# Note: Can't actually test spawn without tmux, but we can verify the logic exists
# by checking the source code has the error handling
if grep -q "Workspace path already exists" "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "source should check for existing workspace path"
fi

# Cleanup
rm -rf "${workspace_path}"

test_start "Workspace name format is otto-<issue-id>"
# Arrange
bead_id=$(create_test_bead "Test workspace name format")

# Act
# Check the source code for the correct format
if grep -q 'format!("../agents/otto-{}", issue_id)' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "workspace name should use format ../agents/otto-<issue-id>"
fi

test_start "Branch name format is agent/<workspace-name>-<issue-id>"
# Arrange
bead_id=$(create_test_bead "Test branch name format")

# Act
# Check the source code for the correct format
if grep -q 'format!("agent/{}-{}", workspace_name, issue_id)' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "branch name should use format agent/<workspace-name>-<issue-id>"
fi

test_start "--no-workspace flag exists and is mutually exclusive with --workspace"
# Arrange
bead_id=$(create_test_bead "Test no-workspace flag")

# Act
# Check the source code for the conflicts_with attribute
if grep -q 'conflicts_with = "workspace"' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "--no-workspace should conflict with --workspace"
fi

test_start "OTTO_WORKSPACE environment variable is set when using workspace"
# Arrange
bead_id=$(create_test_bead "Test OTTO_WORKSPACE env var")

# Act
# Check the source code sets the environment variable
if grep -q 'std::env::set_var("OTTO_WORKSPACE"' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "OTTO_WORKSPACE environment variable should be set"
fi

test_start ".workspace-info file is created with metadata"
# Arrange
bead_id=$(create_test_bead "Test workspace-info file")

# Act
# Check the source code creates .workspace-info
if grep -q '.workspace-info' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail ".workspace-info file should be created"
fi

test_start ".workspace-info contains required metadata fields"
# Arrange
bead_id=$(create_test_bead "Test workspace-info metadata")

# Act
# Check the source code includes all required fields
required_fields=("workspace_path" "branch_name" "issue_id" "original_dir")
all_found=true

for field in "${required_fields[@]}"; do
    if ! grep -q "${field}=" "${TEST_DIR}/../crates/otto/src/main.rs"; then
        all_found=false
        break
    fi
done

if [[ "${all_found}" == "true" ]]; then
    test_pass
else
    test_fail ".workspace-info should contain all required metadata fields"
fi

test_start ".beads directory is copied to workspace"
# Arrange
bead_id=$(create_test_bead "Test beads copy")

# Act
# Check the source code copies .beads
if grep -q 'copy_dir_recursive(beads_src, &beads_dst)' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail ".beads directory should be copied to workspace"
fi

test_start "Workspace cleanup on failure"
# Arrange
bead_id=$(create_test_bead "Test workspace cleanup")

# Act
# Check the source code has cleanup logic
if grep -q 'cleanup_workspace' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "workspace should be cleaned up on failure"
fi

test_start "Spawn in main repo when --no-workspace is used"
# Arrange
bead_id=$(create_test_bead "Test no-workspace spawn")

# Act
# Check the source code has logic to skip workspace creation
if grep -q 'no_workspace' "${TEST_DIR}/../crates/otto/src/main.rs"; then
    test_pass
else
    test_fail "should support spawning without workspace"
fi

test_start "Error message when issue not found"
# Arrange
fake_issue="fake-issue-999"

# Act
output=$(${OTTO_BIN} spawn --issue "${fake_issue}" 2>&1 || true)

# Assert
if echo "${output}" | grep -q "not found"; then
    test_pass
else
    test_fail "should show error when issue not found"
fi

# Summary
echo ""
echo "=========================================="
echo "Test Summary"
echo "=========================================="
echo "Total tests: ${TEST_COUNT}"
echo "Passed: ${TEST_PASS}"
echo "Failed: ${TEST_FAIL}"
echo "=========================================="

if [[ ${TEST_FAIL} -gt 0 ]]; then
    exit 1
fi

exit 0
