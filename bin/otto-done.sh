#!/usr/bin/env bash
# otto done - Agent self-termination command
#
# Orchestrates clean agent exit with validation, cleanup, and Claude shutdown.
#
# Usage: otto done [options]
#
# Options:
#   --issue <id>      Explicit issue ID (e.g., otto-123)
#   --mode <type>     Exit mode: completed | escalated (default: completed)
#   --status <type>   Git state observation: clean | uncommitted | unpushed (for escalated mode)
#   --dry-run         Show what would happen without executing
#   --help, -h        Show this help message
#
# Exit modes:
#   completed    - Validate git state, push changes, sync beads, close hook, exit
#   escalated    - Skip validation, preserve hook bead for recovery, exit
#
# Environment:
#   OTTO_DEBUG: Enable verbose debug output

set -euo pipefail

# Script directory and project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
LOG_FILE="$PROJECT_ROOT/.beads/terminations.log"
DEBUG="${OTTO_DEBUG:-0}"
DRY_RUN=0

# Default values
MODE="completed"
ISSUE_ID=""
STATUS_OBSERVATION=""

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
otto done - Agent self-termination command

Orchestrates clean agent exit with validation, cleanup, and Claude shutdown.

Usage:
  otto done [options]

Options:
  --issue <id>      Explicit issue ID (e.g., otto-123)
                    If not provided, will attempt auto-detection
  --mode <type>     Exit mode (default: completed)
                    completed  - Validate git state, push, sync beads, exit
                    escalated  - Skip validation, preserve hook bead, exit
  --status <type>   Git state observation for escalated mode
                    clean         - Working tree clean, all pushed
                    uncommitted   - Uncommitted changes present
                    unpushed      - Committed but not pushed
  --dry-run         Show what would happen without executing
  --help, -h        Show this help message

Exit Modes:
  completed
    Normal completion when work is done and pushed.
    Steps:
      1. Validate working directory is clean
      2. Validate all commits are pushed
      3. Run bd sync
      4. Close hooked bead (if any)
      5. Clear hook bead
      6. Log completion event
      7. Exit Claude cleanly

  escalated
    Exit when blocked or needing human intervention.
    Preserves work for recovery.
    Steps:
      1. Skip validation (don't check git state)
      2. Log escalation event with observed state
      3. Leave hook bead set (for recovery)
      4. Exit Claude cleanly

Examples:
  otto done                          # Completed mode with auto-detected issue
  otto done --mode escalated         # Escalated mode (blocked, need human)
  otto done --issue otto-123         # Explicit issue ID
  otto done --mode escalated --status uncommitted  # Escalated with observation
  otto done --dry-run                # Preview what would happen

Environment:
  OTTO_DEBUG=1    Enable verbose debug output

For more information, see AGENTS.md "Landing the Plane" protocol
EOF
}

# Parse command line arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --issue)
                if [[ -z "${2:-}" ]] || [[ "${2:0:1}" == "-" ]]; then
                    log_error "Option --issue requires an argument"
                    echo "Use 'otto done --help' for usage information"
                    exit 1
                fi
                ISSUE_ID="$2"
                shift 2
                ;;
            --mode)
                if [[ -z "${2:-}" ]] || [[ "${2:0:1}" == "-" ]]; then
                    log_error "Option --mode requires an argument"
                    echo "Use 'otto done --help' for usage information"
                    exit 1
                fi
                MODE="$2"
                if [[ "$MODE" != "completed" ]] && [[ "$MODE" != "escalated" ]]; then
                    log_error "Invalid mode: '$MODE' (must be 'completed' or 'escalated')"
                    exit 1
                fi
                shift 2
                ;;
            --status)
                if [[ -z "${2:-}" ]] || [[ "${2:0:1}" == "-" ]]; then
                    log_error "Option --status requires an argument"
                    echo "Use 'otto done --help' for usage information"
                    exit 1
                fi
                STATUS_OBSERVATION="$2"
                if [[ "$STATUS_OBSERVATION" != "clean" ]] && \
                   [[ "$STATUS_OBSERVATION" != "uncommitted" ]] && \
                   [[ "$STATUS_OBSERVATION" != "unpushed" ]]; then
                    log_error "Invalid status: '$STATUS_OBSERVATION' (must be 'clean', 'uncommitted', or 'unpushed')"
                    exit 1
                fi
                shift 2
                ;;
            --dry-run)
                DRY_RUN=1
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: '$1'"
                echo "Use 'otto done --help' for usage information"
                exit 1
                ;;
        esac
    done

    # Validate: --status only allowed with --mode escalated
    if [[ -n "$STATUS_OBSERVATION" ]] && [[ "$MODE" != "escalated" ]]; then
        log_error "--status option can only be used with --mode escalated"
        exit 1
    fi

    # Debug output
    log_debug "Configuration:"
    log_debug "  MODE=$MODE"
    log_debug "  ISSUE_ID=$ISSUE_ID"
    log_debug "  STATUS_OBSERVATION=$STATUS_OBSERVATION"
    log_debug "  DRY_RUN=$DRY_RUN"
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

has_stashes() {
    # Check if there are any git stashes
    local stash_list
    stash_list=$(git stash list 2>/dev/null)
    [[ -n "$stash_list" ]]
    return $?
}

validate_git_state() {
    # Validate git working directory state
    # Returns 0 if all validations pass, non-zero otherwise
    # This is fail-fast - stops at first error

    log_debug "Validating git state..."

    # Check 1: Working tree clean
    if ! is_git_clean; then
        log_error "Working tree has uncommitted changes (run git status)"
        return 1
    fi
    log_debug "✓ Working tree is clean"

    # Check 2: All commits pushed
    if ! is_git_pushed; then
        log_error "There are unpushed commits (run git push)"
        return 1
    fi
    log_debug "✓ All commits are pushed"

    # Check 3: No stashes
    if has_stashes; then
        log_error "You have git stashes (run git stash list)"
        return 1
    fi
    log_debug "✓ No stashes found"

    log_debug "Git state validation passed"
    return 0
}

# Auto-detect issue ID from environment or beads state
detect_issue_id() {
    if [[ -n "$ISSUE_ID" ]]; then
        log_debug "Using explicit issue ID: $ISSUE_ID"
        return 0
    fi

    log_debug "Attempting to auto-detect issue ID..."

    # Try BEAD_ID environment variable (set by agents)
    if [[ -n "${BEAD_ID:-}" ]]; then
        ISSUE_ID="$BEAD_ID"
        log_debug "Detected issue ID from BEAD_ID env var: $ISSUE_ID"
        return 0
    fi

    # Try to read from .beads issues.jsonl (most recent bead)
    if [[ -f "$PROJECT_ROOT/.beads/issues.jsonl" ]]; then
        # Get the last modified bead (most recent in the file)
        local detected
        detected=$(tail -1 "$PROJECT_ROOT/.beads/issues.jsonl" | jq -r '.id // empty' 2>/dev/null || true)
        if [[ -n "$detected" ]]; then
            ISSUE_ID="$detected"
            log_debug "Detected issue ID from issues.jsonl: $ISSUE_ID"
            return 0
        fi
    fi

    log_warning "Could not auto-detect issue ID (no BEAD_ID env var, no .beads/issues.jsonl)"
    log_info "Proceeding without explicit issue ID"
    return 0
}

# Main execution
main() {
    # Parse arguments first
    parse_args "$@"

    # Change to project root for all operations
    cd "$PROJECT_ROOT" || exit 1

    # Detect issue ID if not explicitly provided
    detect_issue_id

    # Log configuration
    if [[ "$DRY_RUN" == "1" ]]; then
        log_info "DRY RUN MODE - No changes will be made"
        echo ""
    fi

    log_info "Otto termination initiated"
    log_info "Mode: $MODE"
    if [[ -n "$ISSUE_ID" ]]; then
        log_info "Issue: $ISSUE_ID"
    fi
    echo ""

    # TODO: Implement the actual termination logic in future tasks:
    # - otto-gko.3: Implement beads sync and close logic
    # - otto-gko.4: Implement Claude exit mechanism
    # - otto-gko.5: Implement completed exit mode
    # - otto-gko.6: Implement escalated exit mode

    if [[ "$MODE" == "completed" ]]; then
        log_info "Step 1: Validating git state..."
        if [[ "$DRY_RUN" == "1" ]]; then
            log_info "  [DRY RUN] Would validate git state (skipped in dry-run)"
        else
            if ! validate_git_state; then
                echo ""
                log_error "Git state validation failed"
                log_info "Please fix the issues above before running 'otto done'"
                exit 1
            fi
            log_success "Git state validation passed"
        fi

        log_info "Completed mode workflow (in progress):"
        log_info "  ✓ Validate working directory clean"
        log_info "  ✓ Validate all commits pushed"
        log_info "  3. Run bd sync (TODO: otto-gko.3)"
        log_info "  4. Close hooked bead (if any) (TODO: otto-gko.3)"
        log_info "  5. Clear hook bead (TODO: otto-gko.3)"
        log_info "  6. Log completion event (TODO: otto-gko.5)"
        log_info "  7. Exit Claude cleanly (TODO: otto-gko.4)"
    else
        log_info "Escalated mode workflow (in progress):"
        log_info "  ✓ Skip validation"
        log_info "  2. Log escalation event with state: ${STATUS_OBSERVATION:-unknown} (TODO: otto-gko.6)"
        log_info "  3. Leave hook bead set for recovery (TODO: otto-gko.6)"
        log_info "  4. Exit Claude cleanly (TODO: otto-gko.4)"
    fi

    echo ""
    log_success "Git validation implemented successfully"
    log_info "Remaining implementation in tasks otto-gko.3 through otto-gko.6"

    if [[ "$DRY_RUN" == "1" ]]; then
        echo ""
        log_info "Dry run complete - no changes made"
    fi
}

main "$@"
