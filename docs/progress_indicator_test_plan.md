# Progress Indicator Test Plan

## Overview
This document describes how to test the progress indicator feature implemented in Otto (issues otto-5rw, otto-c8p, otto-o1s).

## What Was Implemented

### 1. Progress Indicator (otto-o1s)
- Continuously rewritten progress line on stderr
- Updates every 2 seconds while agent is working
- Shows elapsed time in human-readable format (e.g., "1m 23s")
- Automatically clears when agent completes

**Location:** `crates/otto-core/src/lib.rs:137-144`

### 2. Session Duration Tracking (otto-c8p)
- Tracks total session time from agent start to completion
- Returns `Duration` from `launch_agent()` function
- Uses `std::time::Instant` for accurate timing

**Location:** `crates/otto-core/src/lib.rs:114, 146`

### 3. Session Duration Printing (otto-5rw)
- Prints formatted duration when agent finishes
- Format: "Agent finished (duration: Xh Xm Xs)"
- Uses `format_duration()` helper function

**Location:** `crates/otto/src/main.rs:121, 170`

## Manual Testing Procedure

### Prerequisites
1. Otto is built: `cargo build --release`
2. Beads is initialized: `bd init` (already done)
3. Test beads exist: Run `./test_progress_indicator.sh`

### Test 1: Single Agent Session
```bash
# Create a simple test task
bd create --title="Create hello.txt with 'Hello World'" --type=task --priority=3

# Run otto in single-pass mode
./target/release/otto
```

**Expected Output:**
```
Otto running in single-pass mode

Starting agent...
Agent working... (5s)      # Updates every 2 seconds: 2s, 4s, 6s, etc.
Agent finished (duration: 12s)   # Example duration
No ready beads, exiting
```

**What to Verify:**
- ✓ "Agent working..." appears on stderr (not in main output)
- ✓ Time updates in-place (carriage return \r overwrites line)
- ✓ Time increments: 2s → 4s → 6s → ...
- ✓ Progress line clears when agent completes
- ✓ Duration appears in final message
- ✓ Format matches "Xh Xm Xs" or "Xm Xs" or "Xs"

### Test 2: Multiple Agent Sessions
```bash
# Create multiple test tasks
bd create --title="Task 1: Create file1.txt" --type=task --priority=3
bd create --title="Task 2: Create file2.txt" --type=task --priority=3
bd create --title="Task 3: Create file3.txt" --type=task --priority=3

# Run otto (will process all three tasks)
./target/release/otto
```

**Expected Output:**
```
Otto running in single-pass mode

Starting agent...
Agent working... (8s)
Agent finished (duration: 15s)

Starting agent...
Agent working... (6s)
Agent finished (duration: 12s)

Starting agent...
Agent working... (10s)
Agent finished (duration: 18s)

No ready beads, exiting
```

**What to Verify:**
- ✓ Progress indicator appears for each agent session
- ✓ Each session has its own duration tracked independently
- ✓ Progress line clears between sessions
- ✓ Final duration reflects actual time for that session

### Test 3: Watch Mode
```bash
# Run otto in watch mode (loops forever)
./target/release/otto --watch
```

**Expected Behavior:**
- Progress indicator appears for each agent
- When no ready beads: "No ready beads, waiting..."
- After 10 seconds, checks again
- When Ctrl+C pressed: "Shutdown signal received, waiting for agent to finish..."
- Graceful shutdown after current agent completes

**What to Verify:**
- ✓ Progress indicator works the same in watch mode
- ✓ Multiple sessions show progress correctly
- ✓ Graceful shutdown doesn't interrupt progress display

### Test 4: Duration Formatting
Test that various durations format correctly:

| Duration | Expected Output |
|----------|-----------------|
| 5 seconds | "5s" |
| 65 seconds | "1m 5s" |
| 3750 seconds | "1h 2m 30s" |

**Verification:**
The `format_duration()` function in `crates/otto-core/src/lib.rs:68-87` handles this.

### Test 5: Error Handling
```bash
# Test timeout (agent takes longer than 30 minutes)
# Note: This is difficult to test manually, but the code handles it:
# - If agent times out, progress indicator stops
# - Warning message printed: "Warning: Agent timed out"
```

## Automated Testing

### Unit Tests
The code includes unit tests in `crates/otto-core/src/lib.rs:246-254`:

```rust
#[test]
fn test_default_timeout() {
    assert_eq!(DEFAULT_AGENT_TIMEOUT_SECS, 1800);
}
```

Run with: `cargo test`

### Integration Test Script
Run: `./test_progress_indicator.sh`

This script:
1. Builds otto
2. Creates test beads
3. Runs otto once
4. Displays output for verification

## Code Locations

| Feature | File | Lines |
|---------|------|-------|
| Progress callback | crates/otto-core/src/lib.rs | 137-144 |
| Duration formatting | crates/otto-core/src/lib.rs | 68-87 |
| Duration tracking | crates/otto-core/src/lib.rs | 114, 146 |
| Duration printing (single-pass) | crates/otto/src/main.rs | 121 |
| Duration printing (watch) | crates/otto/src/main.rs | 170 |
| Progress callback type | crates/otto-agent-claude/src/lib.rs | 142 |
| Wait with progress | crates/otto-agent-claude/src/lib.rs | 170-191 |

## Verification Checklist

Run through this checklist after testing:

- [ ] Progress indicator updates every 2 seconds
- [ ] Progress indicator shows on stderr, not stdout
- [ ] Progress line overwrites itself (carriage return)
- [ ] Progress line clears when agent completes
- [ ] Session duration is tracked accurately
- [ ] Duration formats correctly (Xh Xm Xs)
- [ ] Duration prints in "Agent finished" message
- [ ] Multiple sessions each track independent durations
- [ ] Watch mode shows progress correctly
- [ ] Graceful shutdown doesn't break progress display
- [ ] Unit tests pass: `cargo test`
- [ ] Build succeeds: `cargo build --release`

## Known Limitations

1. Progress indicator only shows time, not task details
2. Updates every 2 seconds (fixed interval)
3. No visual spinner or animation
4. Progress appears on stderr, which may be separated from stdout in some redirects

## Future Improvements

Potential enhancements (not currently implemented):

- Add task title to progress line
- Show estimated time remaining
- Add visual progress bar
- Make update interval configurable
- Add color/ formatting to progress line
- Log progress to file for long-running sessions
