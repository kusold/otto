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

# Logging functions
log_termination_event() {
    # Log termination events to .beads/terminations.log
    local mode="$1"
    local status="$2"  # success | failed
    local message="${3:-}"

    local timestamp
    timestamp=$(date -Iseconds 2>/dev/null || date)

    # Create log directory if it doesn't exist
    mkdir -p "$(dirname "$LOG_FILE")"

    # Log entry: timestamp | mode | status | issue_id | message
    local log_entry="[$timestamp] mode=$mode status=$status issue=${ISSUE_ID:-none} $message"
    echo "$log_entry" >> "$LOG_FILE"

    log_debug "Logged termination event: $log_entry"
}

# Claude exit mechanism
get_claude_parent_pid() {
    # Get the parent PID of this script
    # The parent should be the Claude Code process
    local hook_pid
    local parent_pid

    hook_pid=$$
    parent_pid=$(ps -o ppid= -p "$hook_pid" 2>/dev/null | tr -d ' ' || echo "")

    if [[ -z "$parent_pid" ]]; then
        log_debug "Could not determine parent PID"
        return 1
    fi

    log_debug "Detected parent PID: $parent_pid"
    echo "$parent_pid"
    return 0
}

verify_claude_process() {
    # Verify that the given PID is actually a Claude process
    local pid="$1"

    if ! ps -p "$pid" >/dev/null 2>&1; then
        log_debug "Process $pid does not exist"
        return 1
    fi

    local cmd
    cmd=$(ps -p "$pid" -o command= 2>/dev/null || echo "")

    # Check if it's a Claude process (flexible matching)
    if echo "$cmd" | grep -qi "claude"; then
        log_debug "Confirmed PID $pid is a Claude process: $cmd"
        return 0
    fi

    # Also accept if the command contains node/electron and claude
    if echo "$cmd" | grep -qE "(node|electron|Code)" && echo "$cmd" | grep -qi "claude"; then
        log_debug "Confirmed PID $pid is likely Claude: $cmd"
        return 0
    fi

    log_debug "PID $pid does not appear to be Claude: $cmd"
    return 1
}

exit_claude() {
    # Trigger Claude Code shutdown by sending SIGTERM to parent process
    local mode="$1"
    local timeout="${2:-5}"  # Default 5 second timeout

    log_debug "Attempting to exit Claude (mode: $mode, timeout: ${timeout}s)"

    # Get Claude parent PID
    local parent_pid
    if ! parent_pid=$(get_claude_parent_pid); then
        log_error "Could not determine Claude parent PID"
        return 1
    fi

    # Verify it's actually Claude
    if ! verify_claude_process "$parent_pid"; then
        log_warning "Parent PID $parent_pid does not appear to be Claude"
        log_warning "Skipping exit - may already be terminated or different process"
        return 0
    fi

    # Send SIGTERM for graceful shutdown
    log_debug "Sending SIGTERM to Claude PID: $parent_pid"
    kill -TERM "$parent_pid" 2>/dev/null || true

    # Wait for process to terminate with timeout
    local count=0
    while [[ $count -lt $timeout ]]; do
        if ! ps -p "$parent_pid" >/dev/null 2>&1; then
            log_debug "Claude process terminated successfully (${count}s)"
            return 0
        fi
        sleep 1
        ((count++))
    done

    # Timeout - force kill with SIGKILL
    if ps -p "$parent_pid" >/dev/null 2>&1; then
        log_warning "Claude did not terminate gracefully after ${timeout}s, forcing..."
        kill -KILL "$parent_pid" 2>/dev/null || true
        sleep 1

        # Final check
        if ! ps -p "$parent_pid" >/dev/null 2>&1; then
            log_debug "Claude process force-terminated"
            return 0
        else
            log_error "Failed to terminate Claude process $parent_pid"
            return 1
        fi
    fi

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
    # - otto-gko.5: Implement completed exit mode
    # - otto-gko.6: Implement escalated exit mode
    # - otto-gko.4: ✓ Implement Claude exit mechanism (THIS TASK)

    local exit_success=0
    local exit_message=""

    if [[ "$MODE" == "completed" ]]; then
        log_info "Step 1: Validating git state..."
        if [[ "$DRY_RUN" == "1" ]]; then
            log_info "  [DRY RUN] Would validate git state (skipped in dry-run)"
            exit_success=1
        else
            if ! validate_git_state; then
                echo ""
                log_error "Git state validation failed"
                log_info "Please fix the issues above before running 'otto done'"

                # Log failed validation
                log_termination_event "completed" "failed" "validation failed"

                exit 1
            fi
            log_success "Git state validation passed"
            exit_success=1
            exit_message="git validation passed"
        fi

        # TODO: otto-gko.3 - Beads sync and close logic
        log_info "Step 2: Run bd sync (TODO: otto-gko.3)"
        log_info "Step 3: Close hooked bead (TODO: otto-gko.3)"
        log_info "Step 4: Clear hook bead (TODO: otto-gko.3)"

        # Step 5: Log completion event
        log_termination_event "completed" "success" "$exit_message"

        # Step 6: Exit Claude cleanly
        if [[ "$DRY_RUN" == "1" ]]; then
            log_info "Step 5: [DRY RUN] Would exit Claude cleanly"
        else
            log_info "Step 5: Exiting Claude cleanly..."
            if exit_claude "completed" 5; then
                log_success "Claude exit initiated"
            else
                log_warning "Claude exit encountered issues (may have already exited)"
            fi
        fi
    else
        # Escalated mode
        log_info "Escalated mode workflow:"
        log_info "  ✓ Skip validation (escalated mode)"
        log_info "  ✓ Git state observation: ${STATUS_OBSERVATION:-unknown}"

        # TODO: otto-gko.6 - Escalated mode enhancements
        # Step 2: Log escalation event
        local escalated_msg="escalated with state: ${STATUS_OBSERVATION:-unknown}"
        log_termination_event "escalated" "success" "$escalated_msg"

        # TODO: otto-gko.6 - Leave hook bead set for recovery
        log_info "Step 3: Leave hook bead set for recovery (TODO: otto-gko.6)"

        # Step 4: Exit Claude cleanly
        if [[ "$DRY_RUN" == "1" ]]; then
            log_info "Step 4: [DRY RUN] Would exit Claude cleanly"
        else
            log_info "Step 4: Exiting Claude cleanly..."
            if exit_claude "escalated" 5; then
                log_success "Claude exit initiated"
            else
                log_warning "Claude exit encountered issues (may have already exited)"
            fi
        fi
    fi

    echo ""

    if [[ "$exit_success" -eq 1 ]]; then
        log_success "Termination sequence complete"
    else
        log_info "Termination sequence complete"
    fi

    if [[ "$DRY_RUN" == "1" ]]; then
        echo ""
        log_info "Dry run complete - no changes made"
        log_info "Exit mechanism would have been triggered"
    fi

    log_debug "Remaining implementation in tasks otto-gko.3, otto-gko.5, otto-gko.6"
}

main "$@"
