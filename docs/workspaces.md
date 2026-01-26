# Otto Workspaces

## Overview

Otto workspaces provide isolated git worktrees for agent work, keeping your main repository clean while enabling parallel development and easy cleanup.

**Key Benefits:**
- **Clean main repository**: No stray files from agent work
- **Easy cleanup**: Delete workspaces when done
- **Parallel work**: Multiple agents can work simultaneously
- **Safety**: Failed experiments don't clutter your main repo
- **Isolation**: Each workspace has its own working directory and git state

## Architecture

### Git Worktree Basics

Workspaces are built on Git's worktree feature, which allows multiple working directories to be connected to the same repository.

```
otto/                     # Main repository
├── .git/                # Shared git object database
├── src/
├── tests/
└── ...

../agents/               # Workspace directory
├── otto-123/            # Workspace for issue otto-123
│   ├── .git/            # Worktree metadata (links to main repo)
│   ├── src/             # Working tree files
│   └── .workspace-info  # Workspace metadata
├── otto-456/            # Workspace for issue otto-456
│   └── ...
```

Each workspace:
- Has its own working directory with checked-out files
- Shares the git object database with the main repo
- Uses a unique branch (format: `agent/otto-123-<hash>`)
- Contains a `.workspace-info` metadata file
- Has isolated `.beads` configuration

### Workspace Metadata

Each workspace contains a `.workspace-info` file:

```
workspace_path=/path/to/workspace
branch_name=agent/otto-123-abc123
issue_id=otto-123
original_dir=/path/to/main/repo
```

### Environment Variables

When working in a workspace:
- `OTTO_WORKSPACE` is set to the workspace path
- Agents can detect they're in a workspace and adjust behavior

## Lifecycle Guide

### Creating Workspaces

**Default behavior (automatic workspace creation):**

```bash
otto spawn -i otto-123
```

This creates:
- A git worktree at `../agents/otto-123`
- A unique branch: `agent/otto-123-<hash>`
- Isolated `.beads` configuration
- `.workspace-info` metadata file

**Custom workspace location:**

```bash
otto spawn -i otto-123 --workspace ../agents/my-feature
```

**Disable workspace (work in main repo):**

```bash
otto spawn -i otto-123 --no-workspace
```

Use this for:
- Quick debugging tasks
- When you want immediate visibility of changes
- Simple tasks that don't require isolation

### Working in a Workspace

Once spawned, agents work normally in the workspace:
- Files changed in the workspace don't affect the main repo
- Git operations (add, commit, etc.) work on the workspace branch
- Beads operations are isolated to the workspace

**Example workflow:**

```bash
# Agent is spawned in workspace
cd ../agents/otto-123

# Make changes
vim src/main.rs

# Commit changes
git add .
git commit -m "Implement feature"

# Run tests
cargo test

# Mark work as complete
bd close otto-123
```

### Switching Between Workspaces

```bash
# List all workspaces
otto workspace list

# Switch to a different workspace
cd ../agents/otto-456

# Or switch back to main repo
cd /path/to/otto
```

### Cleaning Up Workspaces

**Automatic cleanup with otto done:**

```bash
# After completing work in workspace
otto done --nuke
```

This:
- Validates git state (clean working tree, everything pushed)
- Syncs beads to remote
- Closes the hooked bead
- **Removes the workspace using `git worktree remove`**
- Clears hook state
- Exits Claude cleanly

**Manual cleanup:**

```bash
# Remove a specific workspace
git worktree remove ../agents/otto-123

# Or manually
cd ../agents/otto-123
git checkout main  # Switch to main branch first
cd ..
rm -rf otto-123
git worktree prune  # Clean up git metadata
```

**Prune all stale worktrees:**

```bash
otto workspace prune
```

This removes worktrees that have been deleted but still have git metadata.

## Command Reference

### otto spawn

Spawn an agent for an issue, optionally creating a workspace.

```bash
# Default: create workspace automatically
otto spawn -i <issue-id>

# Custom workspace location
otto spawn -i <issue-id> --workspace <path>

# Disable workspace (work in main repo)
otto spawn -i <issue-id> --no-workspace
```

**Examples:**
```bash
otto spawn -i otto-123
otto spawn -i otto-123 --workspace ../agents/feature-x
otto spawn -i otto-123 --no-workspace
```

### otto workspace list

List all workspaces.

```bash
otto workspace list
```

**Output:**
```
Workspace           Branch                    Issue       Path
../agents/otto-123  agent/otto-123-abc123    otto-123    /path/to/../agents/otto-123
../agents/otto-456  agent/otto-456-def456    otto-456    /path/to/../agents/otto-456
```

### otto workspace remove

Remove a workspace.

```bash
otto workspace remove <path>
```

**Examples:**
```bash
otto workspace remove ../agents/otto-123
```

This is equivalent to `git worktree remove <path>`.

### otto workspace prune

Remove stale worktree metadata.

```bash
otto workspace prune
```

This is equivalent to `git worktree prune`. Use this after manually deleting workspace directories.

### otto done

Complete work and optionally clean up workspace.

```bash
# Normal completion (no workspace cleanup)
otto done

# Complete work and remove workspace
otto done --nuke
```

**--nuke behavior:**
- Validates git state (must be clean)
- Syncs beads to remote
- Closes the hooked bead
- Removes the workspace directory
- Prunes worktree metadata
- Clears hook state
- Exits Claude cleanly

**Warning:** `--nuke` permanently deletes the workspace. Make sure all work is pushed before using.

## Troubleshooting

### Worktree Already Exists Error

**Error:**
```
fatal: A git worktree with that name already exists
```

**Cause:** A workspace for this issue already exists.

**Solutions:**

1. **Use the existing workspace:**
   ```bash
   cd ../agents/otto-123
   otto ralph otto-123
   ```

2. **Remove the existing workspace first:**
   ```bash
   git worktree remove ../agents/otto-123
   otto spawn -i otto-123
   ```

3. **Use a custom workspace name:**
   ```bash
   otto spawn -i otto-123 --workspace ../agents/otto-123-v2
   ```

### Workspace Not Found Error

**Error:**
```
fatal: '../agents/otto-123' does not exist
```

**Cause:** Workspace directory doesn't exist or was deleted.

**Solutions:**

1. **Create a new workspace:**
   ```bash
   otto spawn -i otto-123
   ```

2. **Clean up stale worktree metadata:**
   ```bash
   git worktree prune
   ```

### Failed to Create Worktree

**Error:**
```
fatal: failed to create worktree
```

**Common causes:**

1. **Branch already exists:**
   ```bash
   # Check if branch exists
   git branch | grep agent/otto-123

   # Delete the branch if it exists
   git branch -D agent/otto-123-abc123

   # Retry spawn
   otto spawn -i otto-123
   ```

2. **File system issues:**
   - Check disk space
   - Verify write permissions
   - Check if parent directory exists

3. **Git repository issues:**
   ```bash
   # Verify git is working
   git status

   # Check worktree list
   git worktree list

   # Prune stale worktrees
   git worktree prune
   ```

### Workspace Won't Delete (Dirty State)

**Error:**
```
fatal: The working tree is not clean
```

**Cause:** Workspace has uncommitted changes.

**Solutions:**

1. **Commit or stash changes:**
   ```bash
   cd ../agents/otto-123

   # Option A: Commit changes
   git add .
   git commit -m "Save work"

   # Option B: Stash changes
   git stash

   # Then remove
   cd ..
   git worktree remove otto-123
   ```

2. **Force remove (not recommended):**
   ```bash
   rm -rf ../agents/otto-123
   git worktree prune
   ```

### Orphaned Worktrees

**Symptoms:** Worktree directories don't exist, but `git worktree list` shows them.

**Solution:**
```bash
# Prune stale worktree metadata
git worktree prune

# Or use otto command
otto workspace prune
```

### Workspace and Beads Out of Sync

**Symptoms:** Beads thinks workspace exists, but it doesn't (or vice versa).

**Solutions:**

1. **Check workspace status:**
   ```bash
   otto workspace list
   git worktree list
   ```

2. **Clean up and resync:**
   ```bash
   # Remove stale worktree metadata
   git worktree prune

   # Sync beads
   bd sync

   # Close issue if work is done
   bd close otto-123
   ```

3. **Update workspace metadata:**
   - Edit `.workspace-info` if incorrect
   - Or recreate workspace with `otto spawn -i otto-123`

## Best Practices

### When to Use Workspaces vs Main Repo

**Use workspaces for:**
- Multi-agent parallel work
- Experimental features
- Long-running tasks
- Work that might fail
- Keeping main repo clean

**Use main repo for:**
- Quick debugging
- Simple one-line fixes
- Documentation updates
- When you need immediate visibility

### Naming Conventions

**Default naming (recommended):**
```bash
otto spawn -i otto-123  # Creates ../agents/otto-123
```

**Custom naming:**
```bash
# Use descriptive names for feature branches
otto spawn -i otto-123 --workspace ../agents/auth-refactor

# Include version for iterations
otto spawn -i otto-123 --workspace ../agents/otto-123-v2
```

### Workspace Cleanup Strategies

**Automatic cleanup (recommended):**
```bash
# Always use --nuke when done
otto done --nuke
```

**Manual cleanup:**
```bash
# Periodic cleanup
cd ../agents
ls
git worktree remove otto-123  # Remove completed workspaces
```

**Cleanup script:**
```bash
# Remove all workspaces older than 7 days
find ../agents -maxdepth 1 -mtime +7 -type d -exec git worktree remove {} \;
```

### Managing Multiple Concurrent Workspaces

**List all active workspaces:**
```bash
otto workspace list
```

**Switch between workspaces:**
```bash
# Use shell aliases for quick switching
alias goto-otto123='cd ../agents/otto-123'
alias goto-otto456='cd ../agents/otto-456'
```

**Track workspace usage:**
```bash
# Check modification time
ls -lt ../agents/

# Check git activity in each workspace
for dir in ../agents/*/; do
  echo "=== $dir ==="
  git -C "$dir" log -1 --format="%h %s"
done
```

### Workspace Size Considerations

**Monitor workspace size:**
```bash
du -sh ../agents/*
```

**Large repositories:**
- Worktrees share the git object database, so disk usage is minimal
- Only the working directory is duplicated
- Use `git gc` in main repo periodically to keep objects clean

### Backup and Recovery

**Backup workspaces:**
```bash
# Workspaces don't need separate backup (shared git db)
# Just ensure main repo is backed up

# To save a workspace's state:
cd ../agents/otto-123
git push origin agent/otto-123-abc123  # Push branch to remote
```

**Recover a workspace:**
```bash
# Clone from remote branch
git worktree add ../agents/otto-123 agent/otto-123-abc123

# Or recreate from issue
otto spawn -i otto-123
```

## FAQ

### Q: Do workspaces use extra disk space?

A: Minimal. Worktrees share the git object database with the main repository. Only the working directory files are duplicated, which is typically much smaller than the `.git` directory.

### Q: Can I push from a workspace?

A: Yes. Workspaces can push to remote branches just like the main repository. The workspace branch will be pushed to the remote.

### Q: What happens to uncommitted changes in a workspace?

A: Uncommitted changes stay in the workspace. They won't appear in the main repo. Always commit or stash changes before cleaning up a workspace.

### Q: Can multiple agents work in the same workspace?

A: No, each workspace is designed for a single agent. Use separate workspaces for parallel work.

### Q: How do I know if I'm in a workspace?

A: Check the `OTTO_WORKSPACE` environment variable or look for the `.workspace-info` file:
```bash
echo $OTTO_WORKSPACE
cat .workspace-info 2>/dev/null || echo "Not in workspace"
```

### Q: Can I rename a workspace?

A: Not directly. You can:
1. Create a new workspace with the desired name
2. Copy over any uncommitted changes
3. Remove the old workspace

Or rename the directory manually and update `.workspace-info`:
```bash
mv ../agents/otto-123 ../agents/otto-123-new
# Edit .workspace-info to update workspace_path
```

### Q: What if I delete a workspace directory without using git worktree remove?

A: The worktree metadata will be stale. Run `git worktree prune` or `otto workspace prune` to clean it up.

### Q: Can I use workspaces without otto?

A: Yes, workspaces are built on git worktrees, which are a native git feature. You can use `git worktree add/remove/list/prune` directly. Otto adds convenience commands and integration with beads.

### Q: Do workspaces work with submodules?

A: Yes, git worktrees support submodules. Each workspace will have its own checkout of submodules.

## Quick Reference

```bash
# Create workspace
otto spawn -i otto-123

# List workspaces
otto workspace list

# Remove workspace
git worktree remove ../agents/otto-123

# Prune stale metadata
git worktree prune

# Complete work and cleanup
otto done --nuke

# Check if in workspace
echo $OTTO_WORKSPACE

# Manual git worktree commands
git worktree list
git worktree add <path> <branch>
git worktree remove <path>
git worktree prune
```
