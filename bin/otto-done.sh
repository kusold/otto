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
    # - otto-gko.2: Implement git state validation logic
    # - otto-gko.3: Implement beads sync and close logic
    # - otto-gko.4: Implement Claude exit mechanism
    # - otto-gko.5: Implement completed exit mode
    # - otto-gko.6: Implement escalated exit mode

    if [[ "$MODE" == "completed" ]]; then
        log_info "Completed mode workflow (placeholder):"
        log_info "  1. Validate working directory clean"
        log_info "  2. Validate all commits pushed"
        log_info "  3. Run bd sync"
        log_info "  4. Close hooked bead (if any)"
        log_info "  5. Clear hook bead"
        log_info "  6. Log completion event"
        log_info "  7. Exit Claude cleanly"
    else
        log_info "Escalated mode workflow (placeholder):"
        log_info "  1. Skip validation"
        log_info "  2. Log escalation event with state: ${STATUS_OBSERVATION:-unknown}"
        log_info "  3. Leave hook bead set for recovery"
        log_info "  4. Exit Claude cleanly"
    fi

    echo ""
    log_success "Command structure validated successfully"
    log_info "Implementation will continue in tasks otto-gko.2 through otto-gko.6"

    if [[ "$DRY_RUN" == "1" ]]; then
        echo ""
        log_info "Dry run complete - no changes made"
    fi
}

main "$@"
