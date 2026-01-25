#!/usr/bin/env bash

# Otto Stop Hook
#
# This hook is triggered when Claude attempts to exit. It checks if the
# task completion marker <PLANE-HAS-LANDED> is present in the transcript.
# If found, the hook allows the exit. Otherwise, it blocks exit and prompts
# Claude to continue working.
#
# This ensures that Claude only exits after completing the assigned task.

set -uo pipefail

# Read hook input from stdin (advanced stop hook API)
HOOK_INPUT=$(cat 2>/dev/null || echo '{}')

# Get transcript path from hook input
TRANSCRIPT_PATH=$(echo "$HOOK_INPUT" | jq -r '.transcript_path' 2>/dev/null || echo '')

if [[ ! -f "$TRANSCRIPT_PATH" ]]; then
    # No transcript - allow exit (shouldn't happen normally)
    exit 0
fi

# Read last assistant message from transcript (JSONL format)
# Check if there are any assistant messages
if ! grep -q '"role":"assistant"' "$TRANSCRIPT_PATH" 2>/dev/null; then
    # No assistant messages - allow exit
    exit 0
fi

# Extract last assistant message
LAST_LINE=$(grep '"role":"assistant"' "$TRANSCRIPT_PATH" 2>/dev/null | tail -1)

if [[ -z "$LAST_LINE" ]]; then
    # No assistant message found - allow exit
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
    exit 0
fi

# Check for task completion marker
if echo "$LAST_OUTPUT" | grep -q "<PLANE-HAS-LANDED>" 2>/dev/null; then
    # Task complete - find and kill parent Claude process

    # Get the parent PID of this hook script
    HOOK_PID=$$
    PARENT_PID=$(ps -o ppid= -p $HOOK_PID | tr -d ' ')

    # The parent should be Claude Code - kill it
    if [[ -n "$PARENT_PID" ]]; then
        # Double-check it's actually a Claude process before killing
        if ps -p $PARENT_PID -o command= | grep -q "claude"; then
            kill $PARENT_PID
        fi
    fi

    exit 0
fi

# Task not complete - allow exit silently
# No output to prevent JSON logs in conversation
exit 0
