#!/usr/bin/env bash
# PreCompact hook for Claude Code - Bead splitting on compaction
#
# This hook intercepts Claude Code's compaction process and spawns a background
# agent to create a blocking bead that forces bead splitting before context is lost.
#
# Environment variables that may be available:
# - CLAUDE_SESSION_ID: Current Claude session identifier
# - BEAD_ID: Current bead being worked on (if set by agent)
# - PWD: Working directory (should be project root)

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
LOG_FILE="$PROJECT_ROOT/.beads/hook.log"

# Logging function
log() {
    echo "[$(date -Iseconds)] [PreCompact] $*" >> "$LOG_FILE"
}

log "PreCompact hook triggered"

# Detect current bead ID
# Priority: 1) BEAD_ID env var, 2) Try to parse from .beads state, 3) Ask user
CURRENT_BEAD_ID="${BEAD_ID:-}"

if [[ -z "$CURRENT_BEAD_ID" ]]; then
    # Try to find the most recently updated bead
    if [[ -f "$PROJECT_ROOT/.beads/issues.jsonl" ]]; then
        # Get the last modified bead (most recent in git history or file mtime)
        CURRENT_BEAD_ID=$(tail -1 "$PROJECT_ROOT/.beads/issues.jsonl" | jq -r '.id // empty' 2>/dev/null || true)
        log "Detected bead ID from issues.jsonl: $CURRENT_BEAD_ID"
    fi
fi

if [[ -z "$CURRENT_BEAD_ID" ]]; then
    log "ERROR: Could not detect current bead ID. Skipping hook."
    exit 0
fi

log "Current bead ID: $CURRENT_BEAD_ID"

# Check if bd is available
if ! command -v bd &> /dev/null; then
    log "ERROR: 'bd' command not found. Cannot create bead."
    exit 0
fi

# Change to project root (bd commands need to run from repo root)
cd "$PROJECT_ROOT" || exit 1

# Create the blocking bead (silent mode to suppress JSON output)
SPLIT_BEAD_ID=$(
    bd create \
        --title="Split bead $CURRENT_BEAD_ID into smaller focused tasks" \
        --type=chore \
        --priority=0 \
        --description="The bead $CURRENT_BEAD_ID has grown too large and needs to be split into multiple smaller, focused beads before work can continue.

**Action Required:**
1. Review the conversation in bead $CURRENT_BEAD_ID
2. Identify distinct themes/work items that can be separated
3. Create new beads for each separate item
4. Link dependencies appropriately
5. Close this bead once splitting is complete

**Goal:** Ensure each bead focuses on a single coherent piece of work.

This bead was automatically created by the PreCompact hook when conversation size approached compaction threshold." \
        --silent >/dev/null 2>&1
)

log "Created blocking bead: $SPLIT_BEAD_ID"

# Add dependency relationship: current bead depends on split bead (suppress all output)
if [[ -n "$SPLIT_BEAD_ID" ]]; then
    bd dep add "$CURRENT_BEAD_ID" "$SPLIT_BEAD_ID" >/dev/null 2>&1
    log "Added dependency: $CURRENT_BEAD_ID depends on $SPLIT_BEAD_ID"
else
    log "ERROR: Failed to create split bead"
    exit 1
fi

log "PreCompact hook completed successfully. Bead $CURRENT_BEAD_ID is now blocked by $SPLIT_BEAD_ID"

# Exit cleanly - Claude will see that work is blocked and should prompt user
exit 0
