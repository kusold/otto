#!/usr/bin/env bash
# otto pre-flight-check - Validate environment before agent work
#
# Checks that the environment is properly configured for agents to run.
# This should be called before starting work to ensure everything is ready.
#
# Usage: otto pre-flight-check
#
# Environment:
#   OTTO_DEBUG: Enable verbose debug output
#
# Exit codes:
#   0 - All checks passed
#   1 - One or more checks failed

set -euo pipefail

# Script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
DEBUG="${OTTO_DEBUG:-0}"

# Logging functions
log_debug() {
    if [[ "$DEBUG" == "1" ]]; then
        echo "[DEBUG] $*" >&2
    fi
}

log_info() {
    echo "$*"
}

log_error() {
    echo "[ERROR] $*" >&2
}

log_success() {
    echo "✓ $*"
}

log_warning() {
    echo "⚠ $*"
}

# Show help message
show_help() {
    cat <<'EOF'
otto pre-flight-check - Validate environment before agent work

Checks that the environment is properly configured for agents to work.

Usage:
  otto pre-flight-check

Checks performed:
  1. Git repository status
  2. Beads initialization
  3. Beads sync status
  4. No uncommitted changes
  5. No unpushed commits

Exit codes:
  0 - All checks passed
  1 - One or more checks failed

Environment:
  OTTO_DEBUG=1    Enable verbose debug output

Examples:
  otto pre-flight-check           # Run all checks
  OTTO_DEBUG=1 otto pre-flight-check  # Run with debug output

For more information, see AGENTS.md
EOF
}

# Git helper functions
git_main_branch() {
    # Try to detect the default branch name
    local main_branch

    # First check if there's a remote origin with a refs/remotes/origin/HEAD
    if git rev-parse --verify origin/HEAD >/dev/null 2>&1; then
        main_branch=$(git symbolic-ref refs/remotes/origin/HEAD 2>/dev/null | sed 's@^refs/remotes/origin/@@')
        if [[ -n "$main_branch" ]]; then
            echo "$main_branch"
            return 0
        fi
    fi

    # Check if main branch exists
    if git rev-parse --verify main >/dev/null 2>&1; then
        echo "main"
        return 0
    fi

    # Check if master branch exists
    if git rev-parse --verify master >/dev/null 2>&1; then
        echo "master"
        return 0
    fi

    # Fallback to "main" as default
    echo "main"
    return 0
}

git_branch_name() {
    git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "HEAD"
}

is_git_clean() {
    # Check if working tree is clean (no uncommitted changes)
    git diff --quiet && git diff --cached --quiet 2>/dev/null
    return $?
}

is_git_pushed() {
    # Check if all commits are pushed to remote
    local branch
    local main_branch
    local unpushed

    branch=$(git_branch_name)
    main_branch=$(git_main_branch)

    # Handle detached HEAD
    if [[ "$branch" == "HEAD" ]]; then
        # In detached HEAD state, consider it "pushed" if HEAD exists on remote
        # This is a simplification - detached HEAD is edge case
        log_debug "Detached HEAD detected, assuming pushed"
        return 0
    fi

    # Check if branch has a remote tracking branch
    if ! git rev-parse --verify "$branch@{u}" >/dev/null 2>&1; then
        # No remote tracking branch - might be a new branch
        # Check if there are any commits that aren't on remote main branch
        unpushed=$(git log "origin/$main_branch..$branch" 2>/dev/null || echo "")
        if [[ -n "$unpushed" ]]; then
            return 1  # Has unpushed commits
        fi
        return 0
    fi

    # Check for unpushed commits relative to remote tracking branch
    unpushed=$(git log "$branch@{u}..$branch" 2>/dev/null || echo "")
    if [[ -n "$unpushed" ]]; then
        return 1  # Has unpushed commits
    fi

    return 0
}

# Main execution
main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: '$1'"
                echo "Use 'otto pre-flight-check --help' for usage information"
                exit 1
                ;;
        esac
    done

    # Change to project root for all operations
    cd "$PROJECT_ROOT" || exit 1

    local all_passed=0

    log_info "Running otto pre-flight checks..."
    echo ""

    # Check 1: Git repository
    log_info "Check 1: Git repository..."
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        log_error "Not a git repository"
        all_passed=1
    else
        log_success "Git repository detected"
    fi
    echo ""

    # Check 2: Beads initialized
    log_info "Check 2: Beads initialization..."
    if [[ ! -d "$PROJECT_ROOT/.beads" ]]; then
        log_error "Beads not initialized (no .beads directory)"
        log_info "Run 'bd init' to initialize beads"
        all_passed=1
    else
        log_success "Beads initialized"
    fi
    echo ""

    # Check 3: Beads sync status
    log_info "Check 3: Beads sync status..."
    if ! bd sync --status >/dev/null 2>&1; then
        log_warning "Beads sync may be needed"
        log_info "Run 'bd sync' to synchronize with remote"
        # Don't fail on this, just warn
    else
        log_success "Beads sync status OK"
    fi
    echo ""

    # Check 4: Working tree clean
    log_info "Check 4: Working tree status..."
    if ! is_git_clean; then
        log_error "Working tree has uncommitted changes"
        log_info "Run 'git status' to see changes"
        log_info "Commit or stash changes before starting work"
        all_passed=1
    else
        log_success "Working tree is clean"
    fi
    echo ""

    # Check 5: Commits pushed
    log_info "Check 5: Commit push status..."
    if ! is_git_pushed; then
        log_error "There are unpushed commits"
        log_info "Run 'git push' to push commits"
        all_passed=1
    else
        log_success "All commits are pushed"
    fi
    echo ""

    # Summary
    if [[ $all_passed -eq 0 ]]; then
        log_success "All pre-flight checks passed!"
        echo ""
        log_info "Environment is ready for agent work"
        return 0
    else
        log_error "Some pre-flight checks failed"
        echo ""
        log_info "Please fix the issues above before starting work"
        return 1
    fi
}

main "$@"
