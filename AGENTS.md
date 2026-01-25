# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. If you learn anything important, update the AGENTS.md nearest to the finding
4. If you implemented a user story, make sure a markdown file in specs/ documents it. If you create a spec file, update the lookup table at specs/README.md
5. **Update issue status** - Close finished work, update in-progress items
6. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
7. **Clean up** - Clear stashes, prune remote branches
8. **Verify** - All changes committed AND pushed
9. **LAST MESSAGE** - Say <PLANE-HAS-LANDED> so that claude exits

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
- Ensure you say <PLANE-HAS-LANDED> after successfully completing all steps

## Session Termination: `otto done`

**IMPORTANT**: This section describes the NEW `otto done` command. During the transition, you may still see references to the old `<PLANE-HAS-LANDED>` protocol.

### When to Use Escalated vs Completed Mode

Use `otto done` to terminate your session. Choose the appropriate mode:

**Completed Mode** (`otto done` or `otto done --mode completed`):
- ✅ Work is fully done and pushed
- ✅ All validations pass (clean working tree, everything pushed)
- ✅ Ready to close the hooked bead and clear hook state
- ✅ Normal completion flow

**Escalated Mode** (`otto done --mode escalated`):
- ⚠️ You're BLOCKED by external dependency (API down, waiting for human)
- ⚠️ You encountered unexpected error you cannot resolve
- ⚠️ You need human intervention or guidance
- ⚠️ Work is partially complete but you cannot continue

**Escalated Mode Behavior**:
- ❌ No validations (git state ignored)
- ✅ Attempts `bd sync` best-effort (continues on failure)
- ✅ Leaves hooked bead OPEN for recovery
- ✅ Preserves hook bead state (`OTTO_CURRENT_BEAD` stays set)
- ✅ Logs escalation event with context
- ✅ Exits Claude cleanly

**Git State Observations** (escalated mode only):
Use `--status` to record what you observed (optional but helpful):
```bash
otto done --mode escalated --status clean         # All good, but blocked
otto done --mode escalated --status uncommitted   # Have uncommitted changes
otto done --mode escalated --status unpushed      # Committed but not pushed
```

### How to Resume Escalated Sessions

When you escalate, the bead stays open and hook state is preserved:

1. **Check what you were working on**:
   ```bash
   bd show <bead-id>    # From the escalation log or .beads/hook
   ```

2. **Resume work**:
   - The hook bead is already set, just continue working
   - Or use `otto ralph <bead-id>` to explicitly set it

3. **When done, use completed mode**:
   ```bash
   otto done    # Normal completion when work is actually done
   ```

### Best Practices for Escalation

✅ **DO escalate when**:
- You need human input or decision
- External service is down or blocked
- You hit a technical blocker you cannot resolve
- You're uncertain about approach and need guidance

❌ **DON'T escalate when**:
- You just haven't finished the work yet
- You're being lazy about validation
- Work can be completed with effort
- You haven't tried to resolve the blocker

📝 **Always provide context**:
- Use `--status` to indicate git state
- The issue should describe what blocked you
- Check `.beads/terminations.log` for escalation history

## Debugging Hooks

### Otto Stop Hook Debug Mode

The `otto-stop-hook.sh` hook can run in verbose/debug mode to help diagnose issues with the exit blocking behavior.

**Enable debug mode:**
```bash
export OTTO_STOP_HOOK_DEBUG=1
# or
export OTTO_DEBUG=1
```

**View debug logs:**
```bash
tail -f .beads/stop-hook.log
```

**What gets logged:**
- Transcript path being checked
- Last assistant message content (first 200 chars)
- Whether `<PLANE-HAS-LANDED>` was found
- Exit decision (allow or block)
- Parent PID and Claude process detection

This is useful when investigating why Claude is being blocked from exiting or when the hook is behaving unexpectedly.

