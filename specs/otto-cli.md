# Otto CLI Specification

## Overview

Otto is a command-line tool that autonomously executes AI coding agents in a continuous loop. It integrates with the beads issue tracking system and Claude Code CLI to automate task completion. The primary purpose of Otto is to enable autonomous AFK (away from keyboard) coding by continuously running AI agents against a task queue without complex orchestration.

### Vision

Enable autonomous coding by running a simple loop where AI agents independently pick tasks from a queue, work on them, and exit. The system prioritizes simplicity over complexity, with no configuration files or state management.

## Core Features

### 1. Single-Pass Mode (Default)
- Runs agents until no ready beads (tasks) remain
- Exits automatically when the task queue is empty
- Suitable for one-time batch processing of tasks

### 2. Watch Mode (`--watch` or `-w`)
- Runs indefinitely in a continuous loop
- When no ready tasks exist, waits 10 seconds and checks again
- Designed for long-running autonomous operation
- Continues until manually stopped with Ctrl+C

### 3. Tmux Integration
- Spawns all Claude Code agents within a reusable tmux session named "otto"
- Automatically creates the session if it doesn't exist
- Allows users to attach and observe agent work in real-time
- Maintains session state across multiple agent launches

### 4. Graceful Shutdown
- Handles SIGINT (Ctrl+C) and SIGTERM signals
- Waits for the current agent to finish before exiting
- Prevents task interruption during shutdown
- Provides clear user feedback during shutdown process

### 5. Beads Integration
- Checks for ready-to-work tasks using `bd ready` command
- Identifies tasks with no blockers (dependencies satisfied)
- Works exclusively with the beads git-based issue tracking system
- Requires beads to be initialized in the project directory

### 6. Fixed Agent Prompting
- All agents receive the same fixed prompt: "Run bd ready, choose a bead, begin work on only that bead. Exit when done."
- Ensures each agent focuses on a single task
- Maintains clear boundaries between iterations
- Prevents agents from working on multiple tasks simultaneously

## Command-Line Interface

### Binary Name
`otto`

### Arguments

#### `--watch` / `-w`
**Type**: Boolean flag
**Default**: false
**Description**: Run in watch mode (loop forever, checking for ready tasks)

When enabled:
- Otto runs in an infinite loop
- When no ready tasks exist, waits 10 seconds before checking again
- Continues until stopped with Ctrl+C

When disabled (default):
- Otto exits when no ready tasks are found
- Suitable for single-pass batch processing

### Usage Examples

```bash
# Single-pass mode (exit when no tasks remain)
otto

# Watch mode (continuous operation)
otto --watch

# Watch mode with short flag
otto -w
```

### User Output

Otto provides console output at key points:

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
Agent finished
```

**No tasks available:**
```
No ready beads, exiting          # Single-pass mode
No ready beads, waiting...        # Watch mode
```

**Shutdown:**
```
^CShutdown signal received, waiting for agent to finish...
Shutting down gracefully
```

**Error messages:**
```
Error: beads not initialized (no .beads directory)
Error launching agent: <error details>
Warning: Agent timed out
```

## Dependencies

### Internal Crates

#### `otto-core`
**Path**: `../otto-core`
**Purpose**: Provides core agent launching functionality
**Key Functions**:
- `launch_agent_default()`: Launches Claude Code agent with default timeout
- `launch_agent(timeout_secs: Option<u64>)`: Launches agent with custom timeout

**Key Types**:
- `AgentError`: Error type for agent operations
  - `ClaudeNotAvailable`: Claude Code CLI not installed
  - `TmuxError(TmuxError)`: Tmux operation failed
  - `AgentStartFailed(String)`: Agent failed to start
  - `AgentTimeout`: Agent did not exit in time

**Constants**:
- `OTTO_AGENT_PROMPT`: Fixed prompt sent to all agents
- `DEFAULT_AGENT_TIMEOUT_SECS`: 300 seconds (5 minutes)

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

#### `otto-tmux` (transitive dependency via otto-core)
**Path**: `../otto-tmux`
**Purpose**: Provides tmux session management
**Key Functions**:
- `ensure_otto_session()`: Ensures "otto" tmux session exists
- `send_otto_command(command: &str)`: Executes command in otto session

**Key Types**:
- `TmuxError`: Error type for tmux operations
  - `TmuxNotAvailable`: tmux not installed
  - `SessionCreationFailed(String)`: Session creation failed
  - `CommandExecutionFailed(String)`: Command execution failed
  - `SessionCheckFailed(String)`: Session check failed

**Constants**:
- `OTTO_SESSION_NAME`: "otto"

### External Dependencies

#### `clap` (version 4.5)
**Features**: derive
**Purpose**: Command-line argument parsing
**Usage**: Derive `Parser` trait for `Args` struct

#### `signal-hook` (version 0.3)
**Features**: iterator
**Purpose**: Signal handling for graceful shutdown
**Usage**:
- Register handlers for SIGINT and SIGTERM
- Signals iterator for background signal handling

## Key Data Structures and Types

### `Args` Structure
```rust
#[derive(Parser, Debug)]
struct Args {
    #[arg(long, short = 'w')]
    watch: bool,
}
```
**Purpose**: Holds command-line arguments
**Fields**:
- `watch`: Boolean flag for watch mode

### `SHUTDOWN_REQUESTED` Global
```rust
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
```
**Purpose**: Thread-safe flag for shutdown coordination
**Type**: `AtomicBool`
**Access Pattern**: Load/store with `Ordering::SeqCst`
**Usage**: Signal handlers set to true; main loop checks before each iteration

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
3. Fork signal handling to a separate thread (required by signal safety rules)
4. Display mode-specific startup message

#### Execution Phase

**Single-Pass Mode (`run_single_pass()`)**:
```
loop:
    1. Check SHUTDOWN_REQUESTED flag
       - If true: print "Shutting down gracefully" and return
    2. Call has_ready_tasks()
       - If Ok(true): Launch agent
         - Call launch_agent_default()
         - Handle AgentTimeout (warning, continue)
         - Handle other errors (error message, return)
         - Check shutdown flag again
       - If Ok(false): Print "No ready beads, exiting" and return
       - If Err(NotInitialized): Print error and return
       - If Err(other): Print error and return
```

**Watch Mode (`run_watch_loop()`)**:
```
loop:
    1. Check SHUTDOWN_REQUESTED flag
       - If true: print "Shutting down gracefully" and return
    2. Call has_ready_tasks()
       - If Ok(true): Launch agent
         - Call launch_agent_default()
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
4. On signal receipt:
   - Checks if already requested (prevents duplicate messages)
   - Sets `SHUTDOWN_REQUESTED` to true
   - Prints shutdown message

**Thread Safety**:
- Uses `AtomicBool` with `Ordering::SeqCst` for guaranteed visibility
- No race conditions due to atomic operations

### Agent Lifecycle

**Launch Process** (delegated to `otto-core`):
1. Check if Claude Code CLI is available (`claude --version`)
2. Ensure "otto" tmux session exists
3. Construct command: `claude "Run bd ready, choose a bead, begin work on only that bead. Exit when done."`
4. Send command to tmux session
5. Poll for completion (check for running `claude` process)
6. Wait up to 300 seconds (5 minutes) default
7. Return `Ok(())` on completion, `AgentTimeout` on timeout

**Monitoring**:
- Polls every 2 seconds using `pgrep -f claude`
- Checks process existence rather than exit codes
- Timeout is configurable but defaults to 300 seconds

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

### Error Handling Strategy

**Fatal Errors** (immediate exit):
- `BeadsError::NotInitialized`: beads not initialized
- Non-timeout `AgentError` variants in single-pass mode
- All errors in single-pass mode except `AgentTimeout`

**Non-Fatal Errors** (continue operation):
- `AgentError::AgentTimeout`: Warning message, continue loop
- Errors in watch mode (except `NotInitialized`): Error message, sleep, continue

**Error Messages**:
- Printed to stderr using `eprintln!()`
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

### No Configuration Files

Otto intentionally has no configuration file support. All behavior is determined by:
1. Command-line arguments (`--watch` flag)
2. Compile-time constants
3. Beads repository state (read from `.beads/` directory)

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

**I/O**:
- Spawns subprocesses (claude, bd, tmux, pgrep)
- Reads from system process table
- Writes to tmux session
- Console output to stdout/stderr

**Concurrency**:
- Signal handling thread (daemon)
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
   - No performance metrics

3. **No Configuration Files**
   - Fixed behavior (except `--watch` flag)
   - No customization without recompilation
   - Simplifies operation

4. **No Plugin System**
   - Only Claude Code is supported
   - No support for other AI agents
   - No extensibility

5. **No Parallel Execution**
   - Only one agent at a time
   - Sequential task processing
   - Single tmux session

6. **Fixed Agent Prompt**
   - All agents receive identical prompt
   - No context or history passed between runs
   - Agent must rediscover tasks each time

### Design Philosophy

**Simplicity Over Features**:
- Minimal configuration
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

## Future Considerations

### Potential Enhancements (Not Currently Planned)

1. **Configuration File Support**
   - Configurable timeouts
   - Custom agent prompts
   - Session name configuration

2. **Metrics Collection**
   - Tasks completed
   - Time per task
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

### Extension Points

Current architecture allows extension through:

1. **Modify `OTTO_AGENT_PROMPT`**: Change agent behavior
2. **Add new CLI arguments**: Extend functionality
3. **Create new dependency crates**: Add integrations
4. **Implement custom error handling**: Modify recovery strategies

## License and Authorship

**License**: MIT
**Author**: Mike Kusold
**Version**: 0.1.0
**Edition**: Rust 2024

---

*This specification documents the otto CLI crate as of version 0.1.0*
