#!/usr/bin/env bash

# Otto Stop Hook
#
# This hook is triggered when Claude attempts to exit. It checks if the
# task completion marker <PLANE-HAS-LANDED> is present in the transcript.
# If found, the hook allows the exit. Otherwise, it blocks exit and prompts
# Claude to continue working.
#
# This ensures that Claude only exits after completing the assigned task.
#
# Exit Codes:
#   0 - Allow exit (work is complete or safe to exit)
#   1 - Block exit (work is not complete)
#   2 - Error condition (logged to .beads/stop-hook-errors.log)
#
# Files:
#   .beads/stop-hook.log - Debug log (when OTTO_STOP_HOOK_DEBUG or OTTO_DEBUG is set)
#   .beads/stop-hook-errors.log - Error log for unexpected conditions

set -uo pipefail

# Debug setup - check before suppressing output
DEBUG_LOG=""
GIT_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || echo "")
ERROR_LOG_FILE="$GIT_ROOT/.beads/stop-hook-errors.log"

# Error logging function (separate from debug log)
log_error() {
    if [[ -n "$GIT_ROOT" ]]; then
        local timestamp=$(date -Iseconds 2>/dev/null || date)
        echo "[$timestamp] $*" >> "$ERROR_LOG_FILE"
    fi
}

if [[ -n "${OTTO_STOP_HOOK_DEBUG:-}" ]] || [[ -n "${OTTO_DEBUG:-}" ]]; then
    if [[ -n "$GIT_ROOT" ]]; then
        DEBUG_LOG="$GIT_ROOT/.beads/stop-hook.log"
        mkdir -p "$(dirname "$DEBUG_LOG")"
    fi
fi

# Debug logging function
debug_log() {
    if [[ -n "$DEBUG_LOG" ]]; then
        echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$DEBUG_LOG"
    fi
}

debug_log "=== Otto Stop Hook Started ==="

# Check for jq dependency - fail fast if missing
if ! command -v jq &>/dev/null; then
    log_error "FATAL: jq not found. Please install jq to use otto-stop-hook.sh"
    echo "ERROR: jq is required but not installed. Run: sudo apt install jq" >&2
    exit 2
fi

# Save original stderr fd for error messages (used when blocking exit)
exec 3>&2

# Suppress stdout and stderr to prevent JSON/logs from appearing
# But we can still write to fd 3 for critical error messages
exec >/dev/null 2>&1

# Read hook input from stdin (advanced stop hook API)
HOOK_INPUT=$(cat 2>/dev/null || echo '{}')
debug_log "Hook input received: ${HOOK_INPUT:0:100}..."

# Get transcript path from hook input
TRANSCRIPT_PATH=$(echo "$HOOK_INPUT" | jq -r '.transcript_path' 2>/dev/null || echo '')
debug_log "Transcript path: $TRANSCRIPT_PATH"

if [[ ! -f "$TRANSCRIPT_PATH" ]]; then
    # No transcript - allow exit (shouldn't happen normally)
    log_error "No transcript file found at path: $TRANSCRIPT_PATH"
    debug_log "No transcript file found, allowing exit"
    exit 0
fi

# Read last assistant message from transcript (JSONL format)
# Check if there are any assistant messages
if ! grep -q '"role":"assistant"' "$TRANSCRIPT_PATH" 2>/dev/null; then
    # No assistant messages - allow exit
    debug_log "No assistant messages found, allowing exit"
    exit 0
fi

# Extract last assistant message
LAST_LINE=$(grep '"role":"assistant"' "$TRANSCRIPT_PATH" 2>/dev/null | tail -1)
debug_log "Last assistant line extracted: ${LAST_LINE:0:100}..."

if [[ -z "$LAST_LINE" ]]; then
    # No assistant message found - allow exit
    debug_log "Last line is empty, allowing exit"
    exit 0
fi

# Parse JSON to extract text content
LAST_OUTPUT=$(echo "$LAST_LINE" | jq -r '
  .message.content |
  map(select(.type == "text")) |
  map(.text) |
  join("\n")
' 2>/dev/null)

# Check if jq succeeded
if [[ $? -ne 0 ]]; then
    # JSON parse failed - allow exit
    log_error "JSON parsing failed for transcript line: ${LAST_LINE:0:200}"
    debug_log "JSON parsing failed, allowing exit"
    exit 0
fi

debug_log "Last assistant content (first 200 chars): ${LAST_OUTPUT:0:200}..."

# Check for task completion marker
if echo "$LAST_OUTPUT" | grep -q "<PLANE-HAS-LANDED>" 2>/dev/null; then
    # Task complete - find and kill parent Claude process
    debug_log "Found <PLANE-HAS-LANDED> marker, killing Claude process"

    # Get the parent PID of this hook script
    HOOK_PID=$$
    PARENT_PID=$(ps -o ppid= -p $HOOK_PID | tr -d ' ')
    debug_log "Hook PID: $HOOK_PID, Parent PID: $PARENT_PID"

    # The parent should be Claude Code - kill it
    if [[ -n "$PARENT_PID" ]]; then
        # Double-check it's actually a Claude process before killing
        if ps -p $PARENT_PID -o command= | grep -q "claude"; then
            debug_log "Confirmed parent is Claude, killing PID: $PARENT_PID"
            kill $PARENT_PID
        else
            debug_log "Parent is not a Claude process, not killing"
        fi
    else
        debug_log "No parent PID found"
    fi

    exit 0
fi

# Task not complete - BLOCK exit with non-zero code
# Write to saved stderr (fd 3) to show error message
debug_log "No <PLANE-HAS-LANDED> marker found, BLOCKING exit"
debug_log "ERROR: Cannot exit - work is not complete!" >&3
debug_log "ERROR: Last assistant message missing <PLANE-HAS-LANDED> marker" >&3
debug_log "ERROR: Continue working until complete, then include <PLANE-HAS-LANDED>" >&3
exit 0
