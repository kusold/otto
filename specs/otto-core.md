# otto-core Crate Specification

## Overview

The `otto-core` crate provides agent orchestration functionality for the Otto project. It coordinates the launching and monitoring of AI agents by integrating tmux session management (via `otto-tmux`) and Claude Code CLI interactions (via `otto-agent-claude`), enabling autonomous task execution through a simple, well-defined interface.

**Location**: `/home/mike/Development/otto/crates/otto-core`

**Purpose**: Orchestrate the agent lifecycle by coordinating tmux sessions and Claude Code CLI, providing a clean API for the main Otto CLI to spawn agents that work on beads tasks.

**Note**: This crate focuses on orchestration and coordination. Claude-specific operations are delegated to the `otto-agent-claude` crate, and tmux operations are delegated to the `otto-tmux` crate.

**Version**: 0.1.0

## Core Features

### 1. Agent Orchestration
- Coordinates tmux window creation/reuse and Claude Code agent launching
- Ensures the otto tmux session exists before launching
- Reuses idle `ralph-*` windows when available
- Sends agent commands to specific tmux windows
- Waits for agent completion with configurable timeout
- Shows progress on stderr while waiting

### 2. Window Management
- Automatically finds or creates agent windows (prefixed with `ralph-`)
- Tracks window lifecycle (pane spec, PID monitoring)
- Supports multiple concurrent agent windows
- Cleans up stuck windows via background watchdog thread

### 3. Lifecycle Coordination
- Checks Claude Code availability before launching
- Spawns agents by sending commands to tmux windows
- Monitors agent process lifecycle via pane PID tracking
- Handles timeout and abort scenarios
- Provides abort callback support for graceful termination

### 4. Error Handling
- Comprehensive error type covering all failure modes
- Wraps and propagates errors from otto-tmux and otto-agent-claude
- Clear error messages for users
- Proper error context for debugging

### 5. Progress Reporting
- Real-time progress updates on stderr
- Colorized output for better UX
- Elapsed time tracking with human-readable formatting
- Window name display for multi-agent scenarios

### 6. Watchdog Monitoring
- Background thread monitors all `ralph-*` windows
- Closes windows where Claude process died
- Closes windows with no output for 10 minutes
- Logs all actions to `~/.otto/watchdog.log`

## Module Structure

The crate consists of two modules:

### `lib.rs` - Main orchestration logic
- Agent launching and monitoring
- Window lifecycle management
- Watchdog monitoring thread
- Error types and result types

### `color.rs` - Colorized output utilities
- Semantic colorization for error/warning/info/progress messages
- Uses termcolor for cross-platform terminal colors

## Data Types and Structures

### Error Types

#### `AgentError`

The primary error type for all agent operations.

```rust
pub enum AgentError {
    /// Claude Code CLI is not available
    ClaudeNotAvailable,
    /// Tmux operation failed
    TmuxError(TmuxError),
    /// Agent failed to start
    AgentStartFailed(String),
    /// Agent did not exit in time
    AgentTimeout,
    /// Failed to read prompt file
    PromptFileError(String, std::io::Error),
}
```

**Variants:**

- **`ClaudeNotAvailable`**: Claude Code CLI is not installed or not in PATH
- **`TmuxError(TmuxError)`**: Wraps errors from the `otto-tmux` crate
- **`AgentStartFailed(String)`**: Agent failed to start (e.g., command construction errors, execution failures)
- **`AgentTimeout`**: Agent didn't exit within the timeout period
- **`PromptFileError(String, std::io::Error)`**: Failed to read the specified prompt file

#### `AgentResult<T>`

Type alias for Result type with AgentError:

```rust
pub type AgentResult<T> = Result<T, AgentError>;
```

### Window Monitor State

#### `WindowState`

Internal state tracking for the stuck window monitor:

```rust
struct WindowState {
    last_content_hash: Option<String>,  // Hash of pane content from last check
    unchanged_count: u32,               // Consecutive checks with unchanged content
}
```

## Constants

### `DEFAULT_AGENT_TIMEOUT_SECS`

Default timeout for agent completion:

```rust
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 1800;
```

**Value**: 1800 seconds (30 minutes)

**Purpose**: Prevents agents from running indefinitely. If an agent takes longer than 30 minutes, it's considered timed out.

**Note**: This was increased from 300 seconds (5 minutes) to 1800 seconds (30 minutes) to allow for longer-running tasks.

## Public API

### Functions

#### `launch_agent(timeout_secs, prompt_file, abort_callback) -> AgentResult<(Duration, String)>`

Launches a Claude Code agent within the Otto tmux session with specified parameters.

**Parameters:**
- `timeout_secs: Option<u64>` - Maximum time to wait for agent completion in seconds
  - `Some(seconds)`: Custom timeout
  - `None`: Uses default timeout (1800 seconds)
- `prompt_file: Option<&str>` - Optional path to a file containing the custom prompt
  - `Some(path)`: Read prompt from file
  - `None`: Use default OTTO_AGENT_PROMPT
- `abort_callback: Option<AbortCallback>` - Optional callback that returns true if agent should be aborted
  - Used for signal handling and graceful shutdown

**Return Value:**
- `Ok((duration, window_name))`: Agent completed successfully
  - `duration`: Time elapsed from launch to completion
  - `window_name`: Name of the tmux window (e.g., "ralph-word")
- `Err(AgentError::ClaudeNotAvailable)`: Claude Code CLI not installed
- `Err(AgentError::TmuxError)`: Tmux operation failed
- `Err(AgentError::AgentStartFailed)`: Agent failed to start
- `Err(AgentError::AgentTimeout)`: Agent didn't exit in time
- `Err(AgentError::PromptFileError)`: Prompt file could not be read

**Algorithm:**

1. **Check Claude Availability**: Call `is_claude_available()` to verify Claude Code CLI is installed
2. **Ensure Tmux Session**: Call `ensure_otto_session()` to create/reuse the "otto" session
3. **Get or Create Window**: Call `get_or_create_agent_window()` to find an idle `ralph-*` window or create a new one
4. **Read Prompt**: Get prompt from file or use default via `get_prompt()`
5. **Construct Command**: Use `build_agent_prompt()` to format the command
6. **Send Command**: Use `send_command_to_window()` to execute in the specific window
7. **Monitor with Progress**: Poll pane PID every 2 seconds, showing progress on stderr
8. **Handle Abort**: If abort callback returns true, kill the Claude process
9. **Return Result**: Clear progress line, return duration and window name

**Example Usage:**

```rust
// Launch with default timeout and prompt
let (duration, window) = launch_agent(None, None, None)?;

// Launch with custom timeout
let (duration, window) = launch_agent(Some(600), None, None)?;

// Launch with custom prompt file
let (duration, window) = launch_agent(None, Some("/path/to/prompt.txt"), None)?;

// Launch with abort callback for signal handling
let (duration, window) = launch_agent(None, None, Some(aborts_on_sigint))?;
```

#### `launch_agent_default(prompt_file, abort_callback) -> AgentResult<(Duration, String)>`

Convenience function that launches an agent with the default timeout.

**Parameters:**
- `prompt_file: Option<&str>` - Optional path to prompt file
- `abort_callback: Option<AbortCallback>` - Optional abort callback

**Return Value:** Same as `launch_agent(None, prompt_file, abort_callback)`

**Purpose:** Provides a simpler API when the default timeout (30 minutes) is acceptable.

#### `is_claude_active_in_pane(pane_spec) -> AgentResult<bool>`

Checks if Claude is currently running in a specific tmux pane.

**Parameters:**
- `pane_spec: Option<&str>` - The pane specification (e.g., "otto:0.0")
  - `Some(spec)`: Check specific pane
  - `None`: Use default "otto:0.0"

**Return Value:**
- `Ok(true)`: Claude is running in the pane
- `Ok(false)`: Claude is not running in the pane
- `Err(AgentError::TmuxError)`: Tmux operation failed

**How it works:**
1. Queries tmux for the PID of the process in the pane via `get_pane_pid()`
2. Validates that the PID corresponds to a Claude process via `is_claude_process()`

**Purpose:** Reliable detection without false positives from other processes.

#### `wait_for_claude_in_pane(pane_spec, timeout_secs) -> AgentResult<()>`

Waits for Claude to exit in a specific tmux pane.

**Parameters:**
- `pane_spec: &str` - The pane specification (e.g., "otto:ralph-word.0")
- `timeout_secs: u64` - Maximum time to wait in seconds

**Return Value:**
- `Ok(())`: Claude has exited from the pane
- `Err(AgentError::AgentTimeout)`: Timeout reached

**Algorithm:**
- Polls every 2 seconds via `is_claude_active_in_pane()`
- Returns when Claude is no longer detected

#### `wait_for_claude_in_pane_with_progress(pane_spec, timeout_secs, progress_callback, abort_callback) -> AgentResult<()>`

Waits for Claude to exit with optional progress and abort callbacks.

**Parameters:**
- `pane_spec: &str` - The pane specification
- `timeout_secs: u64` - Maximum time to wait in seconds
- `progress_callback: Option<Box<dyn Fn(Duration)>>` - Optional callback for progress updates
- `abort_callback: Option<AbortCallback>` - Optional callback that returns true if wait should abort

**Return Value:**
- `Ok(())`: Claude has exited or was aborted
- `Err(AgentError::AgentTimeout)`: Timeout reached

**Behavior:**
- Polls every 2 seconds
- Calls progress_callback with elapsed time each iteration
- If abort_callback returns true, kills Claude process in the pane
- Waits up to 5 seconds for graceful termination after kill

#### `start_stuck_window_monitor() -> JoinHandle<()>`

Starts the background stuck window monitoring thread.

**Return Value:**
- `JoinHandle<()>`: Handle for the monitoring thread

**Behavior:**
- Spawns a background thread that runs indefinitely
- Every 5 minutes, checks all `ralph-*` windows
- Closes windows where Claude process is not running
- Closes windows where content unchanged for 10 minutes (2 checks)
- Logs all closures to `~/.otto/watchdog.log`

**Purpose:** Prevents resource leaks from abandoned or stuck agent windows.

### Color Module Functions

The `color` module provides colorized output functions:

#### `print_error(message)`

Prints a red error message with "✗ Error:" prefix to stderr.

#### `print_warning(message)`

Prints a yellow warning message with "⚠ Warning:" prefix to stderr.

#### `print_info(message)`

Prints a blue info message with "ℹ Info:" prefix to stderr.

#### `print_progress(message)`

Prints a cyan progress message with "→ " prefix to stderr **without newline**, suitable for overwriting progress indicators.

## Technical Implementation Details

### Dependencies

The crate has three dependencies:

**otto-agent-claude** (path: `../otto-agent-claude`)
- Provides Claude Code CLI interaction functionality
- Used functions:
  - `is_claude_available()`: Checks if Claude Code CLI is installed
  - `is_claude_process(pid)`: Checks if a PID is a Claude process
  - `build_agent_prompt(prompt)`: Constructs claude commands
  - `get_prompt(file)`: Gets prompt from file or default
  - `AbortCallback`: Type alias for abort callback

**otto-tmux** (path: `../otto-tmux`)
- Provides tmux session management functionality
- Used functions:
  - `ensure_otto_session()`: Ensures the "otto" tmux session exists
  - `get_or_create_agent_window()`: Finds idle ralph-* window or creates new one
  - `send_command_to_window()`: Sends commands to specific windows
  - `get_pane_pid()`: Gets PID of process in a pane
  - `get_pane_spec()`: Constructs pane spec string
  - `list_windows_by_pattern()`: Lists windows matching a pattern
  - `kill_window()`: Closes a window
  - `capture_pane()`: Captures pane content
- Used constants:
  - `OTTO_SESSION_NAME`: "otto"
  - `AGENT_WINDOW_PREFIX`: "ralph-"
- Used types:
  - `TmuxError`: Error type for tmux operations

**termcolor** (version: "1.4")
- Cross-platform terminal color support
- Used by color module for colored output

**chrono** (version: "0.4")
- Date and time handling
- Used for timestamps in watchdog log

### Process Lifecycle

The agent launch and monitoring process follows this lifecycle:

```
1. PRE-LAUNCH CHECKS
   ├─ Verify claude command exists
   └─ Ensure otto tmux session exists

2. WINDOW SELECTION
   ├─ List all ralph-* windows
   ├─ Check each for idle state (no Claude process)
   ├─ Reuse idle window if found
   └─ Create new ralph-* window if none idle

3. AGENT LAUNCH
   ├─ Read prompt from file or use default
   ├─ Construct: claude "<prompt>"
   └─ Send to specific window

4. MONITORING LOOP (every 2 seconds)
   ├─ Get pane PID from tmux
   ├─ Check if PID is Claude process
   ├─ If process found: Continue waiting
   │   ├─ Update progress on stderr
   │   └─ Check abort callback
   └─ If process not found: Agent exited (SUCCESS)

5. TIMEOUT HANDLING
   ├─ If elapsed time >= timeout: Return AgentTimeout
   └─ Default timeout: 1800 seconds (30 minutes)

6. ABORT HANDLING
   ├─ If abort callback returns true
   ├─ Kill Claude process
   ├─ Wait up to 5 seconds for graceful exit
   └─ Return Ok(())
```

### Watchdog Monitoring

The background watchdog thread operates independently:

```
WATCHDOG LOOP (every 5 minutes)
├─ List all ralph-* windows
├─ For each window:
│   ├─ Check if Claude process running
│   │   └─ If not: Close window, log to watchdog.log
│   ├─ Capture pane content
│   ├─ Compute hash of content
│   ├─ Compare with previous hash
│   │   ├─ If unchanged for 2 checks (10 min): Close window
│   │   └─ If changed: Reset counter
│   └─ Store hash in state map
└─ Repeat forever
```

### Threading Model

- **Main thread**: Synchronous blocking operations for agent launching
- **Watchdog thread**: Independent background thread for window cleanup
- **Sleep-based polling**: Uses `std::thread::sleep(Duration::from_secs(2))` for monitoring intervals
- **No async/await**: Deliberately simple synchronous design

### Error Propagation

The crate implements proper error propagation:

1. **From<TmuxError> for AgentError**: Automatic conversion from tmux errors
2. **From<ClaudeError> for AgentError**: Automatic conversion from Claude errors
3. **Display trait**: All errors implement user-friendly display
4. **Error trait**: All errors implement std::error::Error for proper error handling
5. **Upward propagation**: Errors propagate to caller for handling

## Algorithms and Patterns

### Polling Pattern

The crate uses a simple polling pattern to monitor agent lifecycle:

```rust
while start.elapsed() < timeout {
    has_claude = is_claude_active_in_pane(Some(pane_spec))?;
    if !has_claude {
        return Ok(());  // Success
    }
    if let Some(callback) = abort_callback {
        if callback() {
            // Kill and return
            kill_claude_in_pane(pane_spec)?;
            return Ok(());
        }
    }
    if let Some(progress_cb) = progress_callback {
        progress_cb(start.elapsed());
    }
    sleep(2 seconds);
}
return Err(AgentTimeout);
```

**Advantages:**
- Simple and reliable
- Works across platforms
- Easy to understand and maintain
- Supports abort and progress callbacks

**Trade-offs:**
- 2-second latency in detecting completion
- CPU overhead from repeated process checks
- Not as efficient as event-driven approaches

### Window Reuse Pattern

The crate prefers reusing idle windows over creating new ones:

```rust
let window_name = get_or_create_agent_window(OTTO_SESSION_NAME)?;
```

**Benefits:**
- Reduces window proliferation
- Natural cleanup of finished tasks
- User can observe previous agent output
- Efficient resource usage

### Progress Reporting Pattern

Real-time progress updates via stderr:

```rust
let progress_callback = Box::new(move |elapsed| {
    eprint!("\r");  // Carriage return to overwrite line
    print_progress(&format!(
        "Agent working in {}... ({})",
        window_name,
        format_duration(elapsed)
    ));
});
```

**Features:**
- Continuously overwritten line (no scrolling)
- Human-readable duration format (e.g., "1h 5m 30s")
- Window name for multi-agent scenarios
- Cleared on completion

### Content Hashing for Stuck Detection

Uses content hashing to detect stuck agents:

```rust
let mut hasher = DefaultHasher::new();
content.hash(&mut hasher);
let hash = format!("{:x}", hasher.finish());

if hash == last_hash {
    unchanged_count += 1;
    if unchanged_count >= 2 {
        // Close window (no output for 10 minutes)
    }
}
```

**Advantages:**
- Detects agents that are alive but not producing output
- Efficient O(1) comparison
- Works with any pane content

## Testing

### Unit Tests

The crate includes basic unit tests:

1. **test_default_timeout**: Confirms the timeout is 1800 seconds

### Color Module Tests

The color module includes non-crashing tests:

1. **test_print_error_doesnt_crash**: Verifies print_error works
2. **test_print_warning_doesnt_crash**: Verifies print_warning works
3. **test_print_info_doesnt_crash**: Verifies print_info works
4. **test_print_progress_doesnt_crash**: Verifies print_progress works

## Integration with Otto Ecosystem

### Role in the Architecture

```
┌─────────────────┐
│   otto (CLI)    │  Main loop, signal handling
└────────┬────────┘
         │
         ├────────────────────────┐
         │                        │
┌────────▼────────┐    ┌─────────▼──────────┐
│  otto-beads     │    │   otto-core         │  ← THIS CRATE
│  Task checking  │    │   Agent orchestration│
└─────────────────┘    └─────────┬──────────┘
                                │
                       ┌────────┼────────┐
                       │        │        │
                ┌──────▼───┐ ┌─▼──────┐ ┌▼───────────┐
                │otto-agent│ │otto-tmux│ │ (future:   │
                │-claude   │ │Session  │ │  other     │
                │Claude CLI │ │mgmt    │ │  agents)   │
                │interactions│ │        │ │            │
                └───────────┘ └────────┘ └────────────┘
```

### Dependencies Flow

1. **otto** depends on **otto-core** for agent orchestration
2. **otto-core** depends on **otto-agent-claude** for Claude Code CLI interactions
3. **otto-core** depends on **otto-tmux** for tmux session operations
4. **otto** also depends on **otto-beads** for task checking

### Usage in Main CLI

From `/home/mike/Development/otto/crates/otto/src/main.rs`:

```rust
use otto_core::{launch_agent_default, AgentError};

// In the main loop:
match has_ready_tasks() {
    Ok(true) => {
        println!("Starting agent...");
        match launch_agent_default(None, Some(abort_callback)) {
            Ok((duration, window)) => {
                println!("Agent finished in {} ({})",
                    format_duration(duration), window);
            }
            Err(AgentError::AgentTimeout) => {
                eprintln!("Warning: Agent timed out");
            }
            Err(e) => eprintln!("Error launching agent: {}", e),
        }
    }
    // ...
}
```

## Design Decisions

### Why Polling Instead of Signals?

**Decision**: Use pane PID polling instead of process signals or waitpid()

**Rationale:**
- The Claude process is spawned by tmux, not directly by otto-core
- No direct parent-child relationship exists
- Tmux owns the process, making waitpid() unusable
- Polling is simpler than implementing tmux-specific monitoring

### Why Window Reuse?

**Decision**: Reuse idle `ralph-*` windows instead of always creating new ones

**Rationale:**
- Reduces window proliferation
- Natural cleanup of completed tasks
- Users can observe previous agent output
- More efficient resource usage

### Why Default 30-Minute Timeout?

**Decision**: 1800 seconds (30 minutes) default timeout

**Rationale:**
- Increased from 300 seconds (5 minutes)
- Longer tasks (e.g., building large projects) need more time
- Still prevents indefinite hangs
- Can be customized via API if needed

### Why Synchronous API?

**Decision**: No async/await, simple blocking calls

**Rationale:**
- Otto is inherently sequential (one agent at a time)
- No concurrency benefits from async
- Simpler code and dependencies
- Easier to understand and maintain
- Caller can thread if needed

### Why Watchdog Thread?

**Decision**: Background thread monitors and closes stuck windows

**Rationale:**
- Prevents resource leaks from abandoned windows
- Detects both dead processes and stuck agents (no output)
- Operates independently of main agent launch flow
- Logs all actions for debugging

### Why Abort Callback?

**Decision**: Support abort callback for graceful termination

**Rationale:**
- Enables signal handling (e.g., Ctrl+C)
- Allows graceful shutdown instead of hard kills
- Caller controls abort conditions
- Integrates cleanly with main CLI signal handling

### Why Custom Prompts?

**Decision**: Support prompt_file parameter for custom prompts

**Rationale:**
- Flexibility for different agent behaviors
- A/B testing of prompts
- Task-specific prompts without code changes
- Default prompt still works for most cases

## Limitations and Considerations

### Current Limitations

1. **No Parallel Execution**: Only one agent per window, but multiple windows possible
2. **No Output Capture**: Agent output goes to tmux, not captured by otto-core
3. **Platform Specific**: Uses Unix-specific commands (/proc filesystem)
4. **No Agent Configuration**: Fixed timeout, configurable prompt
5. **Watchdog is Independent**: Watchdog thread cannot be stopped after starting

### Error Recovery

- **Transient Failures**: Not handled (e.g., temporary tmux issues)
- **Timeout**: Returns error, watchdog will eventually clean up
- **Claude Not Available**: Fails fast, clear error message
- **Abort on Timeout**: Doesn't attempt to kill process on timeout (watchdog handles it)

### Security Considerations

- **Process Injection**: Commands are formatted, not sanitized (tmux session is trusted)
- **No Input Validation**: Assumes trusted environment
- **Process Permissions**: Relies on Claude Code CLI's own security
- **Kill Command**: Abort callback uses `kill` command without sanitization (trusted PIDs)

## Future Extensions

Potential areas for enhancement (not currently implemented):

1. **Output Capture**: Stream agent output back to caller
2. **Metrics**: Track agent duration, success rate, window reuse rate
3. **Platform Support**: Windows support via different process monitoring
4. **Configurable Watchdog**: Allow customization of check intervals and timeouts
5. **Graceful Timeout**: Attempt SIGTERM before giving up on timeout
6. **Watchdog Control**: Ability to start/stop watchdog thread
7. **Multi-Agent Coordination**: Orchestrate multiple agents working in parallel
8. **Window Naming**: Configurable window prefix instead of fixed "ralph-"

## Conclusion

The `otto-core` crate provides a focused, reliable API for launching Claude Code agents. Its design prioritizes simplicity and predictability while offering flexibility through custom prompts, abort callbacks, and window reuse. The crate successfully abstracts away the complexity of tmux session management and process monitoring, providing a clean interface for the main Otto CLI.

Key improvements over earlier versions include:
- Longer default timeout (30 minutes vs 5 minutes)
- Window reuse to reduce proliferation
- Progress reporting with elapsed time
- Abort callback support for graceful shutdown
- Background watchdog for stuck window cleanup
- Support for custom prompt files
- Colorized output for better UX

The polling-based monitoring and watchdog thread are intentional design choices that align with Otto's philosophy of simple, autonomous operation. While the crate has limitations, it effectively fulfills its role as the core agent launching component of the Otto system.
