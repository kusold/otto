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

