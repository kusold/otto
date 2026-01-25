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

**When ending a work session**, follow the workflow below. The final step MUST be `otto done`.

**PRE-WORK (before running otto done):**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. If you learn anything important, update the AGENTS.md nearest to the finding
4. If you implemented a user story, make sure a markdown file in specs/ documents it. If you create a spec file, update the lookup table at specs/README.md
5. **Update issue status** - Close finished work, update in-progress items

**FINAL STEP (run otto done):**

```bash
# Normal completion (work fully done and pushed):
otto done

# Escalation (blocked, needs human intervention):
otto done --mode escalated
```

**What otto done does automatically:**
- ✅ Validates git state (clean working tree, everything pushed)
- ✅ Syncs beads to remote
- ✅ Closes the hooked bead
- ✅ Clears hook state
- ✅ Exits Claude cleanly

**CRITICAL RULES:**
- Work is NOT complete until `otto done` succeeds
- NEVER stop before running `otto done` - that leaves work stranded locally
- If `otto done` fails, resolve the issue and retry
- After `otto done` completes successfully, your session is complete

## Session Termination: `otto done`

The `otto done` command is the MANDATORY final step for session completion. It orchestrates all cleanup and validation.

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

### Troubleshooting

**Problem: otto done fails with validation errors**

Common causes:
- Uncommitted changes: Run `git status` to see what needs to be committed
- Unpushed commits: Run `git push` to push your work
- Beads not synced: Run `bd sync` manually to fix sync issues

Solution:
1. Check git state: `git status`
2. Commit any uncommitted work
3. Push to remote: `git push`
4. Sync beads: `bd sync`
5. Retry: `otto done`

**Problem: otto done hangs or doesn't exit**

This usually means Claude is waiting for something. Check:
- Are there background operations running?
- Did a previous command fail silently?
- Is there an interactive prompt?

Solution:
1. Check terminal for any prompts
2. Try Ctrl+C to interrupt, then retry `otto done`
3. If persistent, check `.beads/terminations.log` for clues

**Problem: Need to resume after escalation**

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

