@AGENTS.md

## Hooks Configuration

### PreCompact Hook
This project uses a PreCompact hook to automatically trigger bead splitting when conversations grow too large.

**Hook Script:** `bin/pre-compact-hook.sh`

**Behavior:**
- Triggers when token count approaches compaction threshold
- Creates a P0 (critical) blocking bead requiring bead splitting
- Adds dependency relationship to block current bead until splitting is complete
- Logs actions to `.beads/hook.log`

**Environment Variables:**
- `BEAD_ID` - Optional. Current bead being worked on. If not set, hook attempts auto-detection.
- `CLAUDE_SESSION_ID` - Optional. Current Claude session identifier.

**Setup:**
The hook is automatically configured via Claude Code settings. The script should be executable and located at `bin/pre-compact-hook.sh` from the project root.
