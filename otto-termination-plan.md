# Otto Termination Improvement Plan

## Executive Summary

Otto currently uses **reactive blocking** (stop hooks that prevent bad exits) and should transition to **proactive self-termination** (explicit command that agents run to complete their session). This plan outlines the evolution from a blocking hook approach to a cooperative self-cleaning model.

**Core principle**: Remove exit blocking and replace with explicit `otto done` command that agents run to self-terminate.

---

## Current State Analysis

### Current Mechanism

Otto uses a **stop hook** (`otto-stop-hook.sh`) that:

1. **Triggers**: When Claude attempts to exit
2. **Checks**: Scans transcript for `<PLANE-HAS-LANDED>` marker
3. **Blocks**: Exits without marker are rejected
4. **Allows**: Exits with marker proceed

### Current Agent Protocol

The "Landing the Plane" protocol in `AGENTS.md` requires:

```bash
[ ] 1. File issues for remaining work
[ ] 2. Run quality gates (tests, linters, builds)
[ ] 3. Update AGENTS.md with learnings
[ ] 4. Create/update spec files
[ ] 5. Update issue status (close finished work)
[ ] 6. PUSH TO REMOTE (MANDATORY)
[ ] 7. Clean up (stashes, branches)
[ ] 8. Verify (git status clean)
[ ] 9. Say <PLANE-HAS-LANDED>
```

### Problems with Current Approach

1. **Reactive, not proactive**: Hook blocks bad exits but doesn't help agents exit correctly
2. **Manual protocol**: 9-step checklist is easy to forget or skip
3. **No self-cleanup**: Agent must manually run each step
4. **Single exit mode**: Only supports "done", not "escalated", "deferred", etc.
5. **Marker-based detection**: Vulnerable to false positives
6. **No workspace isolation**: Agents work in main repo
7. **Process kill only**: Hook terminates Claude but doesn't clean up resources
8. **No state tracking**: Can't detect stalled agents or zombies
9. **Adversarial relationship**: Hook is a gatekeeper that prevents agent behavior
10. **Fragile parsing**: Relies on transcript scanning which can fail

---

## Proposed Solution

### Philosophy Shift

**From**: "Prevent bad exits through blocking"
**To**: "Enable good exits through cooperation"

When agents have a reliable command that they actually run, exit blocking becomes unnecessary.

---

## Implementation Plan

### Phase 1: Remove Stop Hook (Immediate)

**Goal**: Eliminate the blocking hook and transition to proactive self-termination.

#### 1.1 Remove Hook Files

```bash
# Remove stop hook from Claude Code configuration
rm .claude/hooks/otto-stop-hook.sh

# Update Claude Code settings to remove hook reference
```

#### 1.2 Strengthen Agent Instructions

Adopt emphatic warnings:

```markdown
## 🚨 NEVER BE AN IDLE AGENT 🚨

**After "Landing The Plane", you MUST run `otto done`. No exceptions.**

The "Idle Agent" is a critical system failure: an agent that completed work
but sits idle at the prompt instead of running `otto done`.

**If you have finished your implementation work, your ONLY next action is to
run `otto done` to properly terminate your session.**

Do NOT:
- Sit idle waiting for more work (there is no more work - you're done)
- Say "work complete" without running `otto done`
- Try other commands (only `otto done` signals completion)
- Wait for confirmation or approval (just run `otto done`)

**Your session should end with `otto done`, not with manual exit.**
```

#### 1.4 Create Pre-Flight Validation Script

```bash
otto pre-flight-check
```

**Checks**:
- [ ] Git working tree clean
- [ ] All commits pushed to remote
- [ ] Beads synced
- [ ] No stashes
- [ ] Quality gates passed (optional, configured per project)

**Output**: "✓ All checks passed, safe to run `otto done`"

---

### Phase 2: Self-Termination Command (Core)

**Goal**: Add explicit termination command that orchestrates cleanup.

#### 2.1 Create `otto done` Command

```bash
otto done [options]
```

**Options**:
| Flag | Purpose | Values |
|------|---------|--------|
| `--issue <id>` | Explicit issue ID | `otto-123` |
| `--mode <type>` | Exit mode | `completed`, `escalated` |
| `--status <type>` | Git state observation | `clean`, `uncommitted`, `unpushed` |

**Behavior** (`completed` mode):
1. Validate working directory clean
2. Validate branch pushed to remote
3. Run `bd sync`
4. Close hooked bead (if any)
5. Clear hook bead
6. Log completion event
7. Exit Claude cleanly

**Behavior** (`escalated` mode):
1. Skip validation
2. Log escalation event
3. Leave hook bead set (for recovery)
4. Exit Claude cleanly

#### 2.2 Update Agent Protocol

Update "Landing the Plane" in `AGENTS.md`:

```bash
9. **FINAL STEP** - Run `otto done` to complete the session:
   ```
   otto done
   ```

The `otto done` command will:
- Validate git state (clean working tree, pushed)
- Run `bd sync`
- Close hooked bead (if any)
- Clear hook bead
- Exit Claude cleanly
```

---

### Phase 3: Workspace Isolation (Long Term)

### Phase 3: Workspace Isolation (Long Term)

**Goal**: Ephemeral workspaces for better isolation and cleanup.

#### 3.1 Add Workspace Support

```bash
# Spawn agent in isolated workspace
otto agent spawn --issue otto-123 --workspace ../agents/default

# Creates git worktree
git worktree add ../agents/default -b agent/default-otto-123

# Start Claude in workspace
cd ../agents/default
claude
```

#### 3.2 Add Workspace Cleanup

```bash
# `otto done --nuke` deletes workspace after completion
git worktree remove ../agents/default
```

**Safety**: Only nuke if `--status=clean` (work pushed).

**Benefits**:
- Clean isolation between agents
- Easy cleanup (nuke the workspace)
- No pollution of main repository
- Clear lifecycle (workspace exists only for task duration)

---

## Roadmap

### Priority Matrix

| Phase | Impact | Effort | Priority |
|-------|--------|--------|----------|
| Phase 1: Remove Hook | High | Low | **P0** |
| Phase 2: Done Command | Critical | Medium | **P0** |
| Phase 3: Workspace | High | High | P1 |

### Sprint Breakdown

#### Sprint 1: Remove Hook & Add Command

- [ ] Remove `.claude/hooks/otto-stop-hook.sh`
- [ ] Remove hook reference from Claude Code settings
- [ ] Implement `otto done` subcommand
- [ ] Add validation logic (git state, bead sync)
- [ ] Add bead state management (close, clear hook)
- [ ] Add exit modes (completed, escalated)
- [ ] Update AGENTS.md with emphatic "idle agent" pattern
- [ ] Create `otto pre-flight-check` script
- [ ] Test all exit modes

#### Sprint 2: Workspace

- [ ] Implement `otto ralph spawn` with workspace option
- [ ] Add git worktree management
- [ ] Add `--nuke` option to `otto done`
- [ ] Update spawn logic to use workspaces
- [ ] Add workspace cleanup commands

---

## Exit Modes Reference

| Mode | Use Case | Behavior |
|------|----------|----------|
| `completed` | Work is done | Validate, push, sync beads, close hook, exit |
| `escalated` | Blocked, needs human | Skip validation, leave hook bead for recovery, exit (preserve work) |

---

## Success Metrics

- **Reduced stranded work**: Fewer instances with unpushed commits
- **Faster termination**: Agents exit promptly after completion
- **Better compliance**: Agents follow protocol more consistently
- **Improved observability**: Can detect and recover from failures
- **Simpler mental model**: "Run `otto done` when done" vs 9-step checklist

---

## Key Principles

1. **Proactive > Reactive**: Self-termination commands are more reliable than blocking hooks
2. **Self-Cleaning**: Automate cleanup to reduce error surface
3. **Multiple Exit Modes**: Support escalation, completed
4. **Workspace Isolation**: Ephemeral workspaces enable clean teardown
5. **Emphatic Instructions**: Strong warnings improve compliance
6. **Trust Over Blocking**: Cooperative agents vs adversarial enforcement

---

*Document created: 2026-01-25*
