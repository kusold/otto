#!/usr/bin/env bash

# Otto Stop Hook
#
# This hook is triggered when Claude attempts to exit. It checks if the
# task completion marker <PLANE-HAS-LANDED> is present in the transcript.
# If found, the hook allows the exit. Otherwise, it blocks exit and prompts
# Claude to continue working.
#
# This ensures that Claude only exits after completing the assigned task.

set -euo pipefail

# Read hook input from stdin (advanced stop hook API)
HOOK_INPUT=$(cat)

# Get transcript path from hook input
TRANSCRIPT_PATH=$(echo "$HOOK_INPUT" | jq -r '.transcript_path')

if [[ ! -f "$TRANSCRIPT_PATH" ]]; then
    # No transcript - allow exit (shouldn't happen normally)
    exit 0
fi

# Read last assistant message from transcript (JSONL format)
# Check if there are any assistant messages
if ! grep -q '"role":"assistant"' "$TRANSCRIPT_PATH"; then
    # No assistant messages - allow exit
    exit 0
fi

# Extract last assistant message
LAST_LINE=$(grep '"role":"assistant"' "$TRANSCRIPT_PATH" | tail -1)

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
' 2>&1)

# Check if jq succeeded
if [[ $? -ne 0 ]]; then
    # JSON parse failed - allow exit
    exit 0
fi

# Check for task completion marker
if echo "$LAST_OUTPUT" | grep -q "<PLANE-HAS-LANDED>"; then
    # Task complete - allow exit
    echo "✅ Otto: Plane has landed, allowing exit"
    exit 0
fi

# Task not complete - block exit and prompt Claude to continue
jq -n \
  --arg msg "⚠️  Task not complete yet. Continue working on the assigned task and output <PLANE-HAS-LANDED> when done." \
  '{
    "decision": "block",
    "reason": "Task completion marker <PLANE-HAS-LANDED> not found. Continue working.",
    "systemMessage": $msg
  }'

exit 0
