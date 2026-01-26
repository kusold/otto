#!/usr/bin/env bash
# otto workspace - Git worktree management for agent workspaces
#
# Manage git worktrees used as agent workspaces for isolated work.
#
# Usage: otto workspace <command> [options]
#
# Commands:
#   list              List all worktrees and their status
#   remove <path>     Remove a specific workspace
#   prune             Remove orphaned worktrees
#   help              Show this help message
#
# Environment:
#   OTTO_DEBUG: Enable verbose debug output
#
# Exit codes:
#   0 - Success
#   1 - Error

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

# Git helper functions

# Check if workspace is clean (no uncommitted changes or untracked files)
is_workspace_clean() {
    local workspace_path="$1"
    pushd "$workspace_path" >/dev/null 2>&1 || return 1

    # Check for unstaged and staged changes
    git diff --quiet && git diff --cached --quiet 2>/dev/null
    local diff_status=$?

    # Check for untracked files (excluding .workspace-info and .beads which are expected)
    local untracked
    untracked=$(git ls-files --others --exclude-standard 2>/dev/null | grep -v "^.workspace-info$" | grep -v "^.beads/" || echo "")

    popd >/dev/null 2>&1 || true

    # Return error if there are changes OR untracked files
    if [[ $diff_status -ne 0 ]] || [[ -n "$untracked" ]]; then
        return 1
    fi
    return 0
}

# Get workspace metadata from .workspace-info file
get_workspace_info() {
    local workspace_path="$1"
    local info_file="$workspace_path/.workspace-info"

    if [[ -f "$info_file" ]]; then
        # Parse key=value format
        while IFS='=' read -r key value; do
            # Skip comments and empty lines
            [[ "$key" =~ ^#.*$ || -z "$key" ]] && continue
            echo "$key=$value"
        done < "$info_file"
    fi
}

# Get workspace age in human-readable format
get_workspace_age() {
    local workspace_path="$1"

    # Try to get creation time from .workspace-info
    local info_file="$workspace_path/.workspace-info"
    if [[ -f "$info_file" ]]; then
        # Check if git has a creation commit (first commit on the branch)
        local branch
        branch=$(cd "$workspace_path" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
        if [[ "$branch" != "unknown" && "$branch" != "HEAD" ]]; then
            # Get the first commit timestamp on this branch
            local first_commit
            first_commit=$(cd "$workspace_path" && git rev-list --max-parents=0 HEAD 2>/dev/null || echo "")
            if [[ -n "$first_commit" ]]; then
                local timestamp
                timestamp=$(cd "$workspace_path" && git show -s --format=%ct "$first_commit" 2>/dev/null || echo "")
                if [[ -n "$timestamp" ]]; then
                    local now
                    now=$(date +%s)
                    local age=$((now - timestamp))
                    local days=$((age / 86400))
                    local hours=$((age / 3600))
                    if [[ $days -gt 0 ]]; then
                        echo "${days}d"
                    elif [[ $hours -gt 0 ]]; then
                        echo "${hours}h"
                    else
                        local minutes=$((age / 60))
                        echo "${minutes}m"
                    fi
                    return
                fi
            fi
        fi
    fi

    # Fallback: check directory modification time
    if [[ -d "$workspace_path" ]]; then
        local mtime
        mtime=$(stat -c %Y "$workspace_path" 2>/dev/null || stat -f %m "$workspace_path" 2>/dev/null || echo "0")
        local now
        now=$(date +%s)
        local age=$((now - mtime))
        local days=$((age / 86400))
        local hours=$((age / 3600))
        if [[ $days -gt 0 ]]; then
            echo "${days}d"
        elif [[ $hours -gt 0 ]]; then
            echo "${hours}h"
        else
            local minutes=$((age / 60))
            echo "${minutes}m"
        fi
    else
        echo "unknown"
    fi
}

# Get current workspace (from OTTO_WORKSPACE env or .workspace-info in cwd)
get_current_workspace() {
    # First check OTTO_WORKSPACE environment variable
    if [[ -n "${OTTO_WORKSPACE:-}" ]]; then
        echo "$OTTO_WORKSPACE"
        return 0
    fi

    # Check if we're in a workspace (has .workspace-info)
    local cwd
    cwd="$(pwd)"
    while [[ "$cwd" != "/" ]]; do
        if [[ -f "$cwd/.workspace-info" ]]; then
            echo "$cwd"
            return 0
        fi
        cwd="$(dirname "$cwd")"
    done

    return 1
}

# Commands

# Show help message
show_help() {
    cat <<'EOF'
otto workspace - Git worktree management for agent workspaces

Manage git worktrees used as agent workspaces for isolated work.

Usage:
  otto workspace <command> [options]

Commands:
  list              List all worktrees and their status
  remove <path>     Remove a specific workspace
  prune             Remove orphaned worktrees
  help              Show this help message

List Command:
  Shows all git worktrees with their status information.

  Output columns:
    WORKSPACE  - Path to the workspace (relative to project root)
    BRANCH     - Git branch name
    STATUS     - Clean or dirty (has uncommitted changes)
    BEAD       - Bead ID from .workspace-info (if available)
    AGE        - Time since workspace was created

  Example:
    $ otto workspace list
    WORKSPACE              BRANCH                     STATUS    BEAD        AGE
    ../agents/default      agent/default-otto-123     clean     otto-123    1h
    ../agents/feature-x    agent/feature-x-otto-456   dirty     otto-456    2d

Remove Command:
  Remove a specific workspace directory.

  Requires confirmation unless --force flag is provided.
  Checks that workspace is clean before removal.

  Usage:
    otto workspace remove <path> [--force]

  Options:
    --force    Skip confirmation prompt

  Example:
    $ otto workspace remove ../agents/default
    Remove workspace '../agents/default'? [y/N] y
    ✓ Removed workspace ../agents/default

Prune Command:
  Remove orphaned worktrees that have been deleted manually.

  This runs 'git worktree prune' to clean up git's worktree metadata.
  Safe to run anytime.

  Example:
    $ otto workspace prune
    ✓ Pruned orphaned worktrees

Environment:
  OTTO_DEBUG=1    Enable verbose debug output
  OTTO_WORKSPACE  Current workspace path (set by spawn)

Examples:
  otto workspace list                    # List all workspaces
  otto workspace remove ../agents/foo    # Remove a workspace
  otto workspace remove ../agents/foo --force   # Remove without confirmation
  otto workspace prune                   # Clean up orphaned worktrees

For more information, see AGENTS.md
EOF
}

# List all worktrees with status
cmd_list() {
    local force=0

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: '$1'"
                echo "Use 'otto workspace list --help' for usage information"
                exit 1
                ;;
        esac
    done

    # Change to project root
    cd "$PROJECT_ROOT" || exit 1

    # Get list of worktrees
    local worktrees_output
    if ! worktrees_output=$(git worktree list 2>/dev/null); then
        log_error "Failed to list worktrees"
        exit 1
    fi

    # Parse worktree list
    # Format: <worktree_path> <commit_sha> [<branch_name>]
    local main_worktree=""
    local worktree_paths=()

    while IFS= read -r line; do
        local path
        path=$(echo "$line" | awk '{print $1}')

        # Skip main worktree (project root)
        if [[ "$path" == "$PROJECT_ROOT" ]]; then
            main_worktree="$path"
            continue
        fi

        worktree_paths+=("$path|$line")
    done <<< "$worktrees_output"

    # Check if there are any workspaces
    if [[ ${#worktree_paths[@]} -eq 0 ]]; then
        log_info "No agent workspaces found"
        echo ""
        log_info "Workspaces are created when using 'otto spawn --workspace <path>'"
        return 0
    fi

    # Print header
    printf "%-22s %-26s %-9s %-11s %s\n" "WORKSPACE" "BRANCH" "STATUS" "BEAD" "AGE"
    printf "%s\n" "---------------------------------------------------------------------------------------"

    # Process each workspace
    for entry in "${worktree_paths[@]}"; do
        local path="${entry%%|*}"
        local full_line="${entry#*|}"

        # Get relative path from project root
        # Use relative path for "../agents/*" style paths, absolute otherwise
        local rel_path
        if [[ "$path" == "$PROJECT_ROOT"/../agents/* ]]; then
            rel_path=$(realpath --relative-to="$PROJECT_ROOT" "$path" 2>/dev/null || echo "$path")
        else
            # For workspaces not in ../agents, show basename or short path
            if [[ "$path" == /tmp/* ]]; then
                rel_path=$(basename "$path")
            else
                rel_path=$(realpath --relative-to="$PROJECT_ROOT" "$path" 2>/dev/null || echo "$path")
                # If relative path is too long or complex, use basename
                if [[ "${#rel_path}" -gt 30 ]]; then
                    rel_path=$(basename "$path")
                fi
            fi
        fi

        # Get branch name
        # git worktree list format: <path> <commit> [<branch>]
        local branch
        branch=$(echo "$full_line" | sed -n 's/.*\[\(.*\)\].*/\1/p')

        # Handle detached HEAD
        if [[ "$branch" == "" ]] || [[ "$branch" == "(detached"* ]]; then
            branch=$(cd "$path" && git rev-parse --short HEAD 2>/dev/null || echo "unknown")
        fi

        # Check if workspace is clean
        local status="clean"
        if ! is_workspace_clean "$path"; then
            status="dirty"
        fi

        # Get bead ID from .workspace-info
        local bead_id=""
        if [[ -f "$path/.workspace-info" ]]; then
            bead_id=$(grep "^issue_id=" "$path/.workspace-info" 2>/dev/null | cut -d'=' -f2 || echo "")
        fi

        # Get workspace age
        local age
        age=$(get_workspace_age "$path")

        # Get current workspace marker
        local current_workspace=""
        current_workspace=$(get_current_workspace) || true
        local marker=""
        if [[ "$path" == "$current_workspace" ]]; then
            marker="*"
        fi

        # Print row
        printf "%-22s %-26s %-9s %-11s %s%s\n" "$rel_path$marker" "$branch" "$status" "$bead_id" "$age"
    done

    echo ""
    if [[ -n "$current_workspace" ]]; then
        log_info "* Current workspace"
    fi
}

# Remove a workspace
cmd_remove() {
    local force=0
    local workspace_path=""

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --force)
                force=1
                shift
                ;;
            --help|-h)
                cat <<'EOF'
otto workspace remove - Remove a specific workspace

Usage:
  otto workspace remove <path> [--force]

Arguments:
  <path>          Path to the workspace to remove

Options:
  --force         Skip confirmation prompt

Examples:
  otto workspace remove ../agents/default
  otto workspace remove ../agents/default --force

For more information, see AGENTS.md
EOF
                exit 0
                ;;
            *)
                if [[ -z "$workspace_path" ]]; then
                    workspace_path="$1"
                else
                    log_error "Too many arguments"
                    exit 1
                fi
                shift
                ;;
        esac
    done

    # Validate workspace path
    if [[ -z "$workspace_path" ]]; then
        log_error "Missing workspace path"
        echo "Usage: otto workspace remove <path> [--force]"
        exit 1
    fi

    # Resolve path
    if [[ ! -d "$workspace_path" ]]; then
        log_error "Workspace path does not exist: $workspace_path"
        exit 1
    fi

    # Get absolute path
    workspace_path=$(cd "$workspace_path" && pwd)

    # Check if it's a worktree
    local is_worktree=0
    local worktrees_output
    worktrees_output=$(git worktree list 2>/dev/null || echo "")
    while IFS= read -r line; do
        local path
        path=$(echo "$line" | awk '{print $1}')
        if [[ "$path" == "$workspace_path" ]]; then
            is_worktree=1
            break
        fi
    done <<< "$worktrees_output"

    if [[ $is_worktree -eq 0 ]]; then
        log_warning "Path is not a known git worktree: $workspace_path"
        read -p "Remove directory anyway? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Cancelled"
            exit 0
        fi
    fi

    # Check if workspace is clean (unless force flag is set)
    if [[ $force -eq 0 ]] && ! is_workspace_clean "$workspace_path"; then
        log_error "Workspace has uncommitted changes"
        echo "Commit or stash changes before removing workspace"
        echo "Use --force to remove anyway"
        exit 1
    fi

    # Show workspace info
    local rel_path
    rel_path=$(realpath --relative-to="$PROJECT_ROOT" "$workspace_path" 2>/dev/null || echo "$workspace_path")
    local branch
    branch=$(cd "$workspace_path" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
    local bead_id=""
    if [[ -f "$workspace_path/.workspace-info" ]]; then
        bead_id=$(grep "^issue_id=" "$workspace_path/.workspace-info" 2>/dev/null | cut -d'=' -f2 || echo "")
    fi

    echo "Workspace: $rel_path"
    echo "Branch: $branch"
    if [[ -n "$bead_id" ]]; then
        echo "Bead: $bead_id"
    fi
    echo ""

    # Confirm removal
    if [[ $force -eq 0 ]]; then
        read -p "Remove workspace '$rel_path'? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_info "Cancelled"
            exit 0
        fi
    fi

    # Remove worktree
    log_debug "Removing worktree: $workspace_path"

    local output
    if output=$(git worktree remove --force "$workspace_path" 2>&1); then
        log_success "Removed workspace $rel_path"
    else
        log_error "Failed to remove worktree"
        echo "$output"
        exit 1
    fi
}

# Prune orphaned worktrees
cmd_prune() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --help|-h)
                cat <<'EOF'
otto workspace prune - Remove orphaned worktrees

Usage:
  otto workspace prune

Removes worktrees that have been deleted manually from the filesystem.
This cleans up git's worktree metadata.

This is safe to run anytime.

Example:
  otto workspace prune

For more information, see AGENTS.md
EOF
                exit 0
                ;;
            *)
                log_error "Unknown option: '$1'"
                echo "Use 'otto workspace prune --help' for usage information"
                exit 1
                ;;
        esac
    done

    # Change to project root
    cd "$PROJECT_ROOT" || exit 1

    log_debug "Pruning orphaned worktrees"

    if git worktree prune 2>/dev/null; then
        log_success "Pruned orphaned worktrees"
    else
        log_error "Failed to prune worktrees"
        exit 1
    fi
}

# Main command dispatcher
main() {
    # No arguments provided
    if [[ $# -eq 0 ]]; then
        log_error "No command specified"
        echo ""
        show_help
        exit 1
    fi

    # Parse command
    local command="$1"
    shift

    case "$command" in
        list|ls)
            log_debug "Executing 'otto workspace list' with args: $*"
            cmd_list "$@"
            ;;
        remove|rm)
            log_debug "Executing 'otto workspace remove' with args: $*"
            cmd_remove "$@"
            ;;
        prune)
            log_debug "Executing 'otto workspace prune' with args: $*"
            cmd_prune "$@"
            ;;
        help|--help|-h)
            show_help
            exit 0
            ;;
        *)
            log_error "Unknown command: '$command'"
            echo ""
            show_help
            exit 1
            ;;
    esac
}

main "$@"
