# Otto CLI Specification

## Overview

Otto is a command-line tool that autonomously executes AI coding agents in a continuous loop. It integrates with the beads issue tracking system and Claude Code CLI to automate task completion. The primary purpose of Otto is to enable autonomous AFK (away from keyboard) coding by continuously running AI agents against a task queue with integrated tmux session management.

### Vision

Enable autonomous coding by running a simple loop where AI agents independently pick tasks from a queue, work on them, and exit. The system prioritizes simplicity over complexity, with tmux integration for persistent sessions and observability.

## Core Features

### 1. Subcommand-Based CLI
- Uses subcommands for different operations (not simple flags)
- Commands: `start`, `attach`, `ralph`
- Each command serves a distinct purpose in the workflow

### 2. Tmux Session Integration
- **`otto start`**: Spawns otto in a tmux session named "otto" in a window named "otto"
- Automatically creates the session and window if they don't exist
- Reuses existing session/window if already running
- Allows users to attach and observe agent work in real-time

### 3. Window Attachment
- **`otto attach [window]`**: Attach to any window in the otto tmux session
- Supports short form: `otto attach ralph-willow` (assumes otto session)
- Supports full spec: `otto attach otto:ralph-willow`
- Defaults to 'otto' window if no argument provided
- Lists available windows if requested window doesn't exist

### 4. Agent Loop (Ralph Command)
- **`otto ralph`**: Run the agent loop in single-pass mode (exits when no tasks)
- **`otto ralph --watch`**: Run in watch mode (continuous operation)
- **`otto ralph -p custom.txt`**: Use custom prompt file instead of default

### 5. Custom Prompt Support
- **`--prompt-file` / `-p`**: Specify a custom prompt file for Claude Code agents
- If not provided, auto-detects `PROMPT_RALPH.md` from repository root (indicated by `.beads` directory)
- Falls back to the default `OTTO_AGENT_PROMPT` from otto-core if no custom prompt found
- Explicit `-p` flag always takes precedence over auto-detection
- Reads prompt content from file and passes to Claude Code

### 6. Self-Termination Protocol
- **Agents MUST proactively run `otto done`** when work is complete
- **`otto done`**: Orchestrates clean agent exit with validation
- **`otto done --mode escalated`**: Exit when blocked or needing intervention
- **`otto pre-flight-check`**: Validate environment before starting work
- Agents are responsible for their own termination - no blocking hooks

### 7. Graceful Shutdown
- Handles SIGINT (Ctrl+C) and SIGTERM signals
- **First Ctrl+C**: Sets shutdown flag, terminates agent, waits gracefully
- **Second Ctrl+C**: Force exits immediately (exit code 130)
- SIGTERM always triggers graceful shutdown (no force kill)

### 8. Stuck Window Monitoring
- Watch mode includes background monitoring for stuck tmux windows
- Automatically detects and handles non-responsive agent windows
- Started via `start_stuck_window_monitor()` in watch mode

### 9. Enhanced Agent Reporting
- Agents return duration and window name on completion
- Output shows: "Agent finished in {window_name} (duration: {formatted_duration})"
- Duration formatted as "1h 5m 30s" or "45s" or "1m 23s"

## Command-Line Interface

### Binary Name
`otto`

### Commands

#### `start`
**Purpose**: Start otto in tmux (runs in background)

**Behavior**:
1. Ensures tmux server is running
2. Ensures 'otto' tmux session exists (creates if needed)
3. Creates 'otto' window if it doesn't exist
4. Runs `otto ralph --watch` in that window
5. Prints confirmation with attachment instructions

**Output**:
```
Started otto in new window: otto
Attach with: tmux attach-session -t otto:otto
```

#### `attach [window]`
**Purpose**: Attach to a tmux window

**Arguments**:
- `window` (optional): Window name or session:window spec
  - None: attaches to 'otto' window
  - "ralph-willow": attaches to 'otto:ralph-willow' (short form)
  - "otto:ralph-willow": attaches to 'otto:ralph-willow' (full spec)

**Error Handling**:
- If session doesn't exist: error with instructions to run `otto start`
- If window doesn't exist: lists available windows and shows usage
- Replaces otto process with tmux attach (exec)

#### `ralph`
**Purpose**: Run the agent loop (main execution mode)

**Subcommand Arguments**:

##### `--watch` / `-w`
**Type**: Boolean flag
**Default**: false
**Description**: Run in watch mode (loop forever, checking for ready tasks)

When enabled:
- Otto runs in an infinite loop
- When no ready tasks exist, waits 10 seconds before checking again
- Continues until stopped with Ctrl+C
- Starts stuck window monitoring thread

When disabled (default):
- Otto exits when no ready tasks are found
- Suitable for single-pass batch processing

##### `--prompt-file` / `-p`
**Type**: String path
**Default**: None (auto-detects PROMPT_RALPH.md, or uses OTTO_AGENT_PROMPT from otto-core)
**Description**: Path to a custom prompt file for Claude Code agents

**Behavior**:
- If provided: Reads file contents and passes to Claude Code as the agent prompt
- If not provided: Auto-detects `PROMPT_RALPH.md` in repository root
  - Searches upward from current directory for `.beads` directory (repo root)
  - If `PROMPT_RALPH.md` exists at repo root, uses it automatically
  - If not found, falls back to `OTTO_AGENT_PROMPT` from otto-core
- Explicit `-p` flag always takes precedence over auto-detection
- Useful for testing different prompts or specialized workflows

#### `done` (via shell wrapper)
**Purpose**: Agent self-termination command

**Behavior**:
- Validates git state (clean working tree, everything pushed)
- Syncs beads to remote
- Closes the hooked bead
- Clears hook state
- Exits Claude cleanly

See `bin/otto-done.sh` for full implementation.

#### `pre-flight-check` (via shell wrapper)
**Purpose**: Validate environment before starting work

**Behavior**:
- Checks git repository status
- Validates beads initialization
- Checks beads sync status
- Validates no uncommitted changes
- Validates no unpushed commits

See `bin/otto-pre-flight-check.sh` for full implementation.

### Usage Examples

```bash
# Start otto in tmux (recommended for persistent operation)
otto start

# Attach to the main otto window
otto attach

# Attach to a specific window (short form)
otto attach ralph-willow

# Attach to a specific window (full spec)
otto attach otto:ralph-willow

# Run in single-pass mode (exits when no tasks)
otto ralph

# Run in watch mode (continuous operation)
otto ralph --watch

# Use custom prompt file (explicitly specified)
otto ralph -p my-custom-prompt.txt

# Use PROMPT_RALPH.md (auto-detected if exists)
otto ralph

# Watch mode with custom prompt
otto ralph --watch --prompt-file special-prompt.txt

# Run with no subcommand (shows help)
otto
```

### User Output

**No subcommand:**
```
Otto - Autonomous agent runner for beads tasks

Usage: otto <COMMAND>

Commands:
  start   Start otto in tmux (runs in background)
  attach  Attach to a tmux window
  ralph   Run the agent loop (default behavior)

Flags:
  -h, --help     Print help
  -V, --version  Print version

Examples:
  otto start              Start otto in tmux
  otto attach             Attach to 'otto' window
  otto attach ralph-willow Attach to specific window
  otto ralph              Run in single-pass mode
  otto ralph --watch      Run in watch mode (infinite loop)
  otto ralph -p FILE      Use custom prompt file
                         (auto-detects PROMPT_RALPH.md if found)
```

**Watch mode startup:**
```
Otto running in watch mode (infinite loop)
Press Ctrl+C to stop
```

**Single-pass mode startup:**
```
Otto running in single-pass mode
```

**Agent execution:**
```
Starting agent...
Agent finished in ralph-crimson (duration: 2m 15s)
```

**No tasks available:**
```
No ready beads, exiting          # Single-pass mode
No ready beads, waiting...        # Watch mode
```

**Shutdown (first Ctrl+C):**
```
^CShutdown signal received, terminating agent...
Agent will be killed gracefully. Press Ctrl+C again to force exit.
Shutting down gracefully
```

**Shutdown (second Ctrl+C):**
```
^CForce exit requested
[process exits with code 130]
```

**Error messages:**
```
Error: beads not initialized (no .beads directory)
Error launching agent: <error details>
Warning: Agent timed out
Error: Session 'otto' does not exist. Start otto with 'otto start'
Error: Window 'ralph-xyz' does not exist in session 'otto'
```

**Shell Wrapper Commands**:
The `otto` binary includes shell wrapper scripts for additional functionality:

**`otto done`** (implemented in `bin/otto-done.sh`):
- Normal completion: `otto done`
- Escalation mode: `otto done --mode escalated`
- With explicit issue: `otto done --issue otto-123`

**`otto pre-flight-check`** (implemented in `bin/otto-pre-flight-check.sh`):
- Validate environment: `otto pre-flight-check`
- With debug output: `OTTO_DEBUG=1 otto pre-flight-check`

## Dependencies

### Internal Crates

#### `otto-core`
**Path**: `../otto-core`
**Purpose**: Provides core agent launching functionality
**Key Functions**:
- `launch_agent_default(prompt_file: Option<&str>, abort_callback: Option<AbortCallback>) -> Result<(Duration, String), AgentError>`
  - Launches Claude Code agent with default timeout
  - Returns duration and window name on success
- `start_stuck_window_monitor() -> JoinHandle<()>`: Starts background stuck window monitoring (watch mode)
- `color::print_error(msg: &str)`: Prints error message to stderr
- `color::print_warning(msg: &str)`: Prints warning message to stderr

**Key Types**:
- `AgentError`: Error type for agent operations
  - `ClaudeNotAvailable`: Claude Code CLI not installed
  - `TmuxError(TmuxError)`: Tmux operation failed
  - `AgentStartFailed(String)`: Agent failed to start
  - `AgentTimeout`: Agent did not exit in time

**Constants**:
- `OTTO_AGENT_PROMPT`: Fixed prompt sent to all agents (when no custom prompt provided)
- `DEFAULT_AGENT_TIMEOUT_SECS`: 300 seconds (5 minutes)

#### `otto-agent-claude`
**Path**: `../otto-agent-claude`
**Purpose**: Provides Claude Code CLI integration and abort callback functionality
**Key Types**:
- `AbortCallback`: `fn() -> bool` - Callback function that returns true if agent should abort

#### `otto-beads`
**Path**: `../otto-beads`
**Purpose**: Provides beads integration for task checking
**Key Functions**:
- `has_ready_tasks() -> BeadsResult<bool>`: Checks if ready tasks exist

**Key Types**:
- `BeadsError`: Error type for beads operations
  - `BeadsNotAvailable`: beads command not found
  - `NotInitialized`: beads not initialized (no .beads directory)
  - `ExecutionFailed(String)`: Command execution failed

#### `otto-tmux`
**Path**: `../otto-tmux`
**Purpose**: Provides tmux session management
**Key Functions**:
- `ensure_session(session_name: &str)`: Ensures tmux session exists
- `create_named_window(session: &str, window: &str)`: Creates a new tmux window
- `send_command_to_window(session: &str, window: &str, command: &str)`: Executes command in tmux window
- `window_exists(session: &str, window: &str) -> Result<bool, TmuxError>`: Checks if window exists
- `list_windows(session: &str) -> Result<Vec<String>, TmuxError>`: Lists all windows in session
- `session_exists(session: &str) -> Result<bool, TmuxError>`: Checks if session exists
- `attach_to_window(session: &str, window: &str)`: Attaches to window (replaces process via exec)
- `OTTO_SESSION_NAME`: "otto" constant

**Key Types**:
- `TmuxError`: Error type for tmux operations
  - `TmuxNotAvailable`: tmux not installed
  - `SessionCreationFailed(String)`: Session creation failed
  - `CommandExecutionFailed(String)`: Command execution failed
  - `SessionCheckFailed(String)`: Session check failed

### External Dependencies

#### `clap` (version 4.5)
**Features**: derive
**Purpose**: Command-line argument parsing
**Usage**: Derive `Parser` and `Subcommand` traits for `Args` and `Commands` structs

#### `signal-hook` (version 0.3)
**Features**: iterator
**Purpose**: Signal handling for graceful shutdown
**Usage**:
- Register handlers for SIGINT and SIGTERM
- Signals iterator for background signal handling

#### `serde_json`
**Purpose**: JSON parsing and generation for Claude Code settings
**Usage**: Read and modify `~/.claude/settings.json` when installing hooks

## Key Data Structures and Types

### `Args` Structure
```rust
#[derive(Parser, Debug)]
#[command(name = "otto")]
#[command(version, about, long_about = None)]
#[command(author = "Mike Kusold")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}
```
**Purpose**: Holds top-level command-line arguments
**Fields**:
- `command`: Optional subcommand (start, attach, ralph)

### `Commands` Enum
```rust
#[derive(Subcommand, Debug)]
enum Commands {
    Start,
    Attach { window: Option<String> },
    Ralph { watch: bool, prompt_file: Option<String> },
}
```

### `SHUTDOWN_REQUESTED` Global
```rust
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
```
**Purpose**: Thread-safe flag for shutdown coordination
**Type**: `AtomicBool`
**Access Pattern**: Load/store with `Ordering::SeqCst`
**Usage**: Signal handlers set to true; main loop checks before each iteration

### `SHUTDOWN_COUNT` Global
```rust
static SHUTDOWN_COUNT: AtomicU8 = AtomicU8::new(0);
```
**Purpose**: Counts number of shutdown signals received
**Type**: `AtomicU8`
**Usage**:
- 0: No shutdown requested
- 1: Graceful shutdown (first Ctrl+C)
- 2+: Force exit (second Ctrl+C)

### Error Type Wrappers

The main crate uses error types from dependencies:

**From `otto_core::AgentError`**:
- `AgentTimeout`: Used to detect and continue after timeout (non-fatal)
- Other variants: Treated as fatal errors

**From `otto_beads::BeadsError`**:
- `NotInitialized`: Fatal error (user must initialize beads)
- Other variants: Fatal errors

## Technical Implementation Details

### Main Flow

#### Initialization Phase
1. Parse command-line arguments using `clap`
2. Set up signal handlers for SIGINT and SIGTERM
3. Match on command subcommand:
   - `start`: Execute `start_otto()`
   - `attach`: Execute `attach_to_window(window)`
   - `ralph`: Execute `run_single_pass()` or `run_watch_loop()`
   - None: Print help text

#### Execution Phase

**Single-Pass Mode (`run_single_pass(prompt_file: Option<&str>)`)**:
```
loop:
    1. Check SHUTDOWN_REQUESTED flag
       - If true: print "Shutting down gracefully" and return
    2. Call has_ready_tasks()
       - If Ok(true): Launch agent
         - Create abort_callback that checks SHUTDOWN_REQUESTED
         - Call launch_agent_default(prompt_file, Some(abort_callback))
         - Returns Ok((duration, window_name)): Print formatted message
         - Handle AgentTimeout (warning, continue)
         - Handle other errors (error message, return)
         - Check shutdown flag again
       - If Ok(false): Print "No ready beads, exiting" and return
       - If Err(NotInitialized): Print error and return
       - If Err(other): Print error and return
```

**Watch Mode (`run_watch_loop(prompt_file: Option<&str>)`)**:
```
1. Start stuck window monitoring thread
loop:
    1. Check SHUTDOWN_REQUESTED flag
       - If true: print "Shutting down gracefully" and return
    2. Call has_ready_tasks()
       - If Ok(true): Launch agent
         - Create abort_callback that checks SHUTDOWN_REQUESTED
         - Call launch_agent_default(prompt_file, Some(abort_callback))
         - Returns Ok((duration, window_name)): Print formatted message
         - Handle AgentTimeout (warning, continue)
         - Handle other errors (error message, continue in watch mode)
         - Check shutdown flag again
       - If Ok(false): Print "No ready beads, waiting..."
         - Sleep for 10 seconds (in 1-second intervals)
         - Check shutdown flag during sleep intervals
       - If Err(NotInitialized): Print error and return
       - If Err(other): Print error message
         - Sleep for 10 seconds (in 1-second intervals)
         - Check shutdown flag during sleep intervals
```

### Signal Handling

**Setup Function**: `setup_signal_handlers()`

**Implementation Details**:
1. Creates a `Signals` iterator for SIGINT and SIGTERM
2. Spawns a dedicated thread for signal handling
3. Thread blocks on `signals.forever()` iterator
4. On SIGINT (Ctrl+C):
   - Increment SHUTDOWN_COUNT
   - If count == 0: Set SHUTDOWN_REQUESTED to true, print shutdown message
   - If count >= 1: Exit immediately with code 130 (128 + SIGINT)
5. On SIGTERM:
   - Check if already requested (prevents duplicate messages)
   - Set SHUTDOWN_REQUESTED to true
   - Print shutdown message

**Thread Safety**:
- Uses `AtomicBool` and `AtomicU8` with `Ordering::SeqCst` for guaranteed visibility
- No race conditions due to atomic operations

**Behavior Difference**:
- First Ctrl+C: Graceful shutdown (terminates agent, waits)
- Second Ctrl+C: Immediate force exit
- SIGTERM: Always graceful (no force kill option)

### Tmux Integration

#### `start_otto()` Function

**Steps**:
1. Ensure 'otto' tmux session exists (creates if needed)
2. Check if 'otto' window exists in session
3. If window doesn't exist, create it
4. Send command `otto ralph --watch` to the window
5. Print confirmation with attachment instructions

**Output Messages**:
- "Started otto in new window: otto" (if window created)
- "Started otto in existing window: otto" (if window existed)
- "Attach with: tmux attach-session -t otto:otto"

#### `attach_to_window(window: Option<String>)` Function

**Steps**:
1. Parse window argument:
   - None: Use (OTTO_SESSION_NAME, "otto")
   - "session:window": Parse as full spec
   - "window": Use (OTTO_SESSION_NAME, window)
2. Check if session exists (error with instructions if not)
3. Check if window exists (list available windows if not)
4. Attach to window using tmux (exec replaces process)

**Error Handling**:
- Session doesn't exist: Print error, suggest `otto start`
- Window doesn't exist: List available windows, show usage
- Both are fatal errors (return Result::Err)

### Agent Lifecycle

**Launch Process** (delegated to `otto-core`):
1. Check if Claude Code CLI is available (`claude --version`)
2. Ensure "otto" tmux session exists
3. Read custom prompt file if provided (or use default)
4. Create new tmux window with unique name (e.g., "ralph-crimson")
5. Construct command: `claude "PROMPT"`
6. Send command to tmux window
7. Poll for completion (check for running `claude` process)
8. Check abort callback during polling
9. Wait up to 300 seconds (5 minutes) default
10. Return `Ok((duration, window_name))` on completion, `AgentTimeout` on timeout

**Monitoring**:
- Polls every 2 seconds using `pgrep -f claude`
- Checks process existence rather than exit codes
- Timeout is configurable but defaults to 300 seconds
- Aborts if abort callback returns true

### Sleep Behavior

**Watch Mode Waiting**:
- When no ready tasks or on error, sleeps for 10 seconds
- Implemented as 10 × 1-second sleeps
- Each 1-second interval checks `SHUTDOWN_REQUESTED`
- Allows responsive shutdown (max 1-second delay)

**Code Pattern**:
```rust
for _ in 0..10 {
    std::thread::sleep(std::time::Duration::from_secs(1));
    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
        println!("Shutting down gracefully");
        return;
    }
}
```

### Duration Formatting

**`format_duration(duration: Duration) -> String`**:
- Converts Duration to human-readable string
- Examples: "1h 5m 30s", "45s", "1m 23s"
- Only includes non-zero components
- Always includes seconds if all other components are zero

### Error Handling Strategy

**Fatal Errors** (immediate exit):
- `BeadsError::NotInitialized`: beads not initialized
- Non-timeout `AgentError` variants in single-pass mode
- All errors in single-pass mode except `AgentTimeout`
- Tmux session/window not found errors

**Non-Fatal Errors** (continue operation):
- `AgentError::AgentTimeout`: Warning message, continue loop
- Errors in watch mode (except `NotInitialized`): Error message, sleep, continue

**Error Messages**:
- Printed to stderr using `print_error()` wrapper
- Include descriptive context from error types
- `Display` trait implemented for all error types

## Configuration

### Compile-Time Configuration

No runtime configuration files. Behavior is fixed at compile time:

**Constants in `otto-core`**:
- `DEFAULT_AGENT_TIMEOUT_SECS`: 300 seconds
- `OTTO_SESSION_NAME`: "otto"
- `OTTO_AGENT_PROMPT`: Fixed prompt string

**To Modify**:
- Change timeout: Edit `DEFAULT_AGENT_TIMEOUT_SECS` in `otto-core/src/lib.rs`
- Change session name: Edit `OTTO_SESSION_NAME` in `otto-tmux/src/lib.rs`
- Change agent prompt: Edit `OTTO_AGENT_PROMPT` in `otto-core/src/lib.rs`

### Runtime Configuration

**Custom Prompt Files**:
- Auto-detection: If `PROMPT_RALPH.md` exists at repository root, used automatically
- Explicit override: Specify via `--prompt-file` / `-p` flag
- Explicit flag always takes precedence over auto-detection
- Fallback: `OTTO_AGENT_PROMPT` from otto-core if no custom prompt found
- File content read and passed directly to Claude Code

### Agent Self-Termination

**Agents MUST proactively terminate**:
- Run `otto done` when work is complete
- Run `otto done --mode escalated` if blocked
- No blocking hooks - agents are responsible for their own termination
- See AGENTS.md for full protocol

**No Configuration Files**:
Otto itself has no configuration file support. All behavior is determined by:
1. Command-line arguments and subcommands
2. Compile-time constants
3. Beads repository state (read from `.beads/` directory)
4. Custom prompt files (optional, via CLI flag)

## Platform Requirements

### Required External Tools

1. **tmux** (Terminal Multiplexer)
   - Used for session management
   - Required for agent isolation
   - Must be available in PATH

2. **Claude Code CLI** (claude)
   - The AI coding agent
   - Must be available in PATH
   - Requires directory trust approval

3. **beads** (bd)
   - Git-based issue tracking
   - Must be available in PATH
   - Must be initialized in working directory (`bd init`)

4. **pgrep** (Process grep)
   - Used for agent monitoring
   - Standard Unix utility (usually installed by default)

### Platform Support

**Supported**:
- Linux (primary target)
- macOS (with Brew-installed dependencies)
- Unix-like systems with tmux, pgrep

**Not Supported**:
- Windows (no native tmux/pgrep support)

### Rust Edition

- **Edition**: 2024
- **Version**: 0.1.0
- **Workspace**: Yes (part of otto workspace)

## Build and Runtime Behavior

### Compilation

**Workspace Structure**:
```
otto/
├── Cargo.toml (workspace)
├── crates/
│   ├── otto/          (binary crate)
│   ├── otto-core/     (library)
│   ├── otto-beads/    (library)
│   ├── otto-agent-claude/ (library)
│   └── otto-tmux/     (library)
```

**Build Commands**:
```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test
```

### Binary Output

**Debug**: `/home/mike/Development/otto/target/debug/otto`
**Release**: `/home/mike/Development/otto/target/release/otto`

### Runtime Characteristics

**Memory**:
- Minimal footprint (no state persistence)
- Each iteration is independent
- No memory accumulation between loops

**CPU**:
- Idle during agent execution (waiting on process)
- Polling every 2 seconds during agent monitoring
- Sleep intervals during waiting periods
- Stuck window monitoring runs in background thread (watch mode)

**I/O**:
- Spawns subprocesses (claude, bd, tmux, pgrep)
- Reads from system process table
- Writes to tmux session
- Console output to stdout/stderr

**Concurrency**:
- Signal handling thread (daemon)
- Stuck window monitoring thread (watch mode only)
- Main thread (control loop)
- No other threads used

### Testing

**Unit Tests** (in dependency crates):

`otto-core/src/lib.rs`:
- `test_agent_prompt_constant`: Verifies prompt content
- `test_default_timeout`: Verifies timeout value

`otto-tmux/src/lib.rs`:
- `test_session_name_constant`: Verifies session name
- `test_is_tmux_available_returns_bool`: Verifies function returns bool

**No Integration Tests**:
- Main crate has no test functions
- Integration would require external tools (tmux, claude, bd)

## Limitations and Design Decisions

### Intentional Limitations

1. **No State Management**
   - Each agent run is independent
   - No persistence between runs
   - No task result tracking

2. **No Metrics or Logging**
   - Simple console output only
   - No structured logging
   - No performance metrics (except duration per agent)

3. **No Configuration Files**
   - Fixed behavior (except subcommands and prompt file)
   - No customization without recompilation
   - Simplifies operation

4. **No Plugin System**
   - Only Claude Code is supported
   - No support for other AI agents
   - No extensibility

5. **No Parallel Execution**
   - Only one agent at a time
   - Sequential task processing
   - Single tmux session (multiple windows)

6. **Fixed Agent Prompt**
   - All agents receive identical prompt (unless custom file specified)
   - No context or history passed between runs
   - Agent must rediscover tasks each time

7. **Subcommand-Based CLI**
   - Cannot run agent loop without explicit `ralph` subcommand
   - Running `otto` without args shows help (doesn't execute)
   - Intentional design for clarity

### Design Philosophy

**Simplicity Over Features**:
- Minimal configuration (only prompt files via CLI)
- Clear, predictable behavior
- Easy to understand and debug

**Autonomy Over Control**:
- Agents operate independently
- No human intervention during execution
- Trust agent to complete tasks correctly

**Reliability Over Performance**:
- Sequential execution (no race conditions)
- Graceful shutdown (no task interruption)
- Clear error messages (easy troubleshooting)

**Tmux-Native**:
- Uses tmux for session management (not reinventing the wheel)
- Easy observability (attach to watch agents work)
- Persistent operation (start and detach)

## Future Considerations

### Potential Enhancements (Not Currently Planned)

1. **Configuration File Support**
   - Configurable timeouts
   - Default prompt file location
   - Session name configuration

2. **Metrics Collection**
   - Tasks completed per session
   - Time per task (already shown per agent)
   - Success/failure rates

3. **Parallel Execution**
   - Multiple agents simultaneously
   - Task queue management
   - Resource limiting

4. **Enhanced Logging**
   - Structured log output
   - Log file support
   - Log level configuration

5. **Agent Support**
   - Other AI coding agents
   - Pluggable agent system
   - Agent-specific configuration

6. **Window Management Commands**
   - `otto list`: List all windows in otto session
   - `otto kill <window>`: Kill a specific window
   - `otto restart`: Restart the main otto window

### Extension Points

Current architecture allows extension through:

1. **Modify prompt files**: Change agent behavior without recompilation
2. **Add new subcommands**: Extend functionality (e.g., `otto list`)
3. **Create new dependency crates**: Add integrations
4. **Implement custom error handling**: Modify recovery strategies

## License and Authorship

**License**: MIT
**Author**: Mike Kusold
**Version**: 0.1.0
**Edition**: Rust 2024

---

*This specification documents the otto CLI crate as of version 0.1.0*
