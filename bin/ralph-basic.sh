#!/usr/bin/env bash
# Basic Ralph Wiggum Loop
# Continuously feeds a prompt to Claude Code with fresh context each iteration.
#
# Usage: ./bin/ralph-basic.sh
#
# The loop:
# 1. Feeds PROMPT.md to claude
# 2. Claude completes one task, commits, exits
# 3. Loop restarts with fresh context
# 4. Claude reads updated plan, picks next task
#
# Key files:
# - PROMPT.md - Instructions for each iteration
# - IMPLEMENTATION_PLAN.md - Shared state between iterations (task list)
# - specs/ - Requirements/specifications
# - AGENTS.md - Project-specific build/test commands
#
# Stop with: Ctrl+C

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Check required files
if [ ! -f "PROMPT.md" ]; then
    echo "Error: PROMPT.md not found in $PROJECT_ROOT"
    exit 1
fi

echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "Ralph Wiggum Loop - Basic"
echo "Working dir: $PROJECT_ROOT"
echo "Prompt: PROMPT.md"
echo ""
echo "Press Ctrl+C to stop"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

# Main loop - infinite until Ctrl+C
while :; do
    cat PROMPT.md | claude -p \
        --dangerously-skip-permissions
done
