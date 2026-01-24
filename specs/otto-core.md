# otto-core Crate Specification

## Overview

The `otto-core` crate provides the core agent launching functionality for the Otto project. It is responsible for spawning and managing Claude Code AI agents within tmux sessions, enabling autonomous task execution through a simple, well-defined interface.

**Location**: `/home/mike/Development/otto/crates/otto-core`

**Purpose**: Abstract away the complexity of launching AI agents, providing a clean API for the main Otto CLI to spawn agents that work on beads tasks.

**Version**: 0.1.0

## Core Features

### 1. Agent Launching
- Spawns Claude Code agents within a tmux session
- Ensures the tmux session exists before launching
- Sends a fixed, well-defined prompt to each agent
- Waits for agent completion with configurable timeout

### 2. Process Monitoring
- Poll-based monitoring of agent process lifecycle
- Uses `pgrep` to detect when Claude Code process exits
- Configurable timeout mechanism (default: 5 minutes)
- Polling interval: 2 seconds

### 3. Error Handling
- Comprehensive error type covering all failure modes
- Clear error messages for users
- Proper error propagation from tmux operations

### 4. Fixed Prompt Architecture
- All agents receive the same fixed prompt
- Ensures consistent behavior across agent launches
- Prompts agents to work on a single beads task and exit

## Module Structure

### Single Module Architecture

The crate consists of a single module (`lib.rs`) containing all functionality. This design is intentional given the crate's focused scope.

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
}
```

**Variants:**

- **`ClaudeNotAvailable`**: Returned when the `claude` command is not found on the system. Indicates Claude Code CLI is not installed or not in PATH.

- **`TmuxError(TmuxError)`**: Wraps errors from the `otto-tmux` crate. Propagates tmux session management failures.

- **`AgentStartFailed(String)`**: Indicates the agent process failed to start. The String contains details about the failure.

- **`AgentTimeout`**: Returned when the agent does not complete within the specified timeout period. Default is 300 seconds (5 minutes).

#### `AgentResult<T>`

Type alias for Result type with AgentError:

```rust
pub type AgentResult<T> = Result<T, AgentError>;
```

## Constants

### `OTTO_AGENT_PROMPT`

The fixed prompt sent to all Claude Code agents:

```rust
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. Exit when done.";
```

**Purpose**: Directs the agent to:
1. Check for ready beads tasks using `bd ready`
2. Choose one task from the list
3. Work only on that single task
4. Exit when complete

This design ensures:
- Single-task focus per agent invocation
- Clear boundaries between iterations
- Predictable agent behavior
- Easy detection of completion

### `DEFAULT_AGENT_TIMEOUT_SECS`

Default timeout for agent completion:

```rust
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 300;
```

**Value**: 300 seconds (5 minutes)

**Purpose**: Prevents agents from running indefinitely. If an agent takes longer than 5 minutes, it's considered timed out.

## Public API

### Functions

#### `launch_agent(timeout_secs: Option<u64>) -> AgentResult<()>`

Launches a Claude Code agent within the Otto tmux session with a specified timeout.

**Parameters:**
- `timeout_secs`: Maximum time to wait for agent completion in seconds
  - `Some(seconds)`: Custom timeout
  - `None`: Uses default timeout (300 seconds)

**Return Value:**
- `Ok(())`: Agent completed successfully
- `Err(AgentError::ClaudeNotAvailable)`: Claude Code CLI not installed
- `Err(AgentError::TmuxError)`: Tmux operation failed
- `Err(AgentError::AgentStartFailed)`: Agent failed to start
- `Err(AgentError::AgentTimeout)`: Agent didn't exit in time

**Algorithm:**

1. **Check Claude Availability**: Run `claude --version` to verify Claude Code CLI is installed
2. **Ensure Tmux Session**: Call `otto_tmux::ensure_otto_session()` to create/reuse the "otto" session
3. **Construct Command**: Format the Claude command with the fixed prompt
4. **Send Command**: Use `otto_tmux::send_otto_command()` to execute the command in tmux
5. **Monitor Process**: Poll every 2 seconds using `pgrep -f claude` to check if process is running
6. **Wait for Completion**: Continue polling until:
   - Process exits (return Ok)
   - Timeout elapses (return AgentTimeout)

**Example Usage:**

```rust
// Launch with default 5-minute timeout
launch_agent(None)?;

// Launch with custom 10-minute timeout
launch_agent(Some(600))?;
```

#### `launch_agent_default() -> AgentResult<()>`

Convenience function that launches an agent with the default timeout.

**Parameters:** None

**Return Value:** Same as `launch_agent(None)`

**Purpose:** Provides a simpler API when the default timeout is acceptable.

**Example Usage:**

```rust
match launch_agent_default() {
    Ok(()) => println!("Agent completed successfully"),
    Err(AgentError::AgentTimeout) => eprintln!("Agent took too long"),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Technical Implementation Details

### Dependencies

The crate has one external dependency:

**otto-tmux** (path: `../otto-tmux`)
- Provides tmux session management functionality
- Used functions:
  - `ensure_otto_session()`: Ensures the "otto" tmux session exists
  - `send_otto_command()`: Sends commands to the tmux session

### External Commands

The crate interacts with several system commands:

#### 1. `claude --version`
- **Purpose**: Check if Claude Code CLI is available
- **Usage**: `Command::new("claude").arg("--version").output()`
- **Success Criteria**: Exit code 0
- **Error Handling**: Returns false if command fails or returns non-zero exit code

#### 2. `pgrep -f claude`
- **Purpose**: Check if Claude Code process is still running
- **Usage**: `Command::new("pgrep").arg("-f").arg("claude").output()`
- **Polling Interval**: Every 2 seconds
- **Success Criteria**: Exit code 0 means process is running
- **Interpretation**: Exit code 1 (no match) means agent has exited

#### 3. Tmux commands (via otto-tmux)
- **Session Management**: Create/check for "otto" session
- **Command Execution**: Send claude command to the session
- **Implementation**: Delegated to otto-tmux crate

### Process Lifecycle

The agent launch and monitoring process follows this lifecycle:

```
1. PRE-LAUNCH CHECKS
   ├─ Verify claude command exists
   └─ Ensure otto tmux session exists

2. AGENT LAUNCH
   ├─ Construct: claude "Run bd ready, choose a bead, begin work on only that bead. Exit when done."
   └─ Send to tmux session

3. MONITORING LOOP (every 2 seconds)
   ├─ Check: pgrep -f claude
   ├─ If process found: Continue waiting
   └─ If process not found: Agent exited (SUCCESS)

4. TIMEOUT HANDLING
   ├─ If elapsed time >= timeout: Return AgentTimeout
   └─ Default timeout: 300 seconds
```

### Threading Model

- **Single-threaded operation**: The crate uses synchronous blocking operations
- **Sleep-based polling**: Uses `std::thread::sleep(Duration::from_secs(2))` for polling intervals
- **No async/await**: Deliberately simple synchronous design

### Error Propagation

The crate implements proper error propagation:

1. **From<TmuxError> for AgentError**: Automatic conversion from tmux errors
2. **Display trait**: All errors implement user-friendly display
3. **Error trait**: All errors implement std::error::Error for proper error handling
4. **Upward propagation**: Errors propagate to caller for handling

## Algorithms and Patterns

### Polling Pattern

The crate uses a simple polling pattern to monitor agent lifecycle:

```rust
while start.elapsed() < timeout {
    has_claude = check_process_running();
    if !has_claude {
        return Ok(());  // Success
    }
    sleep(2 seconds);
}
return Err(AgentTimeout);
```

**Advantages:**
- Simple and reliable
- No complex signal handling
- Works across platforms
- Easy to understand and maintain

**Trade-offs:**
- 2-second latency in detecting completion
- CPU overhead from repeated process checks
- Not as efficient as event-driven approaches

### Fixed Prompt Pattern

All agents receive identical prompts:

```rust
const OTTO_AGENT_PROMPT: &str = "Run bd ready, choose a bead, begin work on only that bead. Exit when done.";
```

**Rationale:**
- **Simplicity**: No prompt construction logic needed
- **Consistency**: Every agent behaves the same way
- **Testability**: Predictable agent behavior
- **Maintainability**: Single source of truth for agent instructions

**Implications:**
- Agents must be autonomous (no dynamic instructions)
- Task selection is delegated to the agent
- Completion detection is based on agent exit

### Session Reuse Pattern

The crate ensures a tmux session exists but doesn't manage its lifecycle:

```rust
ensure_otto_session()?;  // Create if needed, reuse if exists
send_otto_command(&claude_command)?;
```

**Benefits:**
- Session persists across agent launches
- User can attach to observe agents
- No session cleanup complexity
- Natural observability

## Testing

### Unit Tests

The crate includes basic unit tests:

1. **test_agent_prompt_constant**: Verifies the prompt contains expected text
2. **test_default_timeout**: Confirms the timeout is 300 seconds

These tests ensure the core constants remain correct as the code evolves.

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
│  Task checking  │    │   Agent launching   │
└─────────────────┘    └─────────┬──────────┘
                                │
                       ┌────────▼──────────┐
                       │   otto-tmux       │
                       │   Session mgmt    │
                       └───────────────────┘
```

### Dependencies Flow

1. **otto** depends on **otto-core** for agent launching
2. **otto-core** depends on **otto-tmux** for tmux operations
3. **otto** also depends on **otto-beads** for task checking

### Usage in Main CLI

From `/home/mike/Development/otto/crates/otto/src/main.rs`:

```rust
use otto_core::{launch_agent_default, AgentError};

// In the main loop:
match has_ready_tasks() {
    Ok(true) => {
        println!("Starting agent...");
        match launch_agent_default() {
            Ok(()) => println!("Agent finished"),
            Err(AgentError::AgentTimeout) => eprintln!("Warning: Agent timed out"),
            Err(e) => eprintln!("Error launching agent: {}", e),
        }
    }
    // ...
}
```

## Design Decisions

### Why Polling Instead of Signals?

**Decision**: Use `pgrep` polling instead of process signals or waitpid()

**Rationale:**
- The Claude process is spawned by tmux, not directly by otto-core
- No direct parent-child relationship exists
- Tmux owns the process, making waitpid() unusable
- Polling is simpler than implementing tmux-specific monitoring

### Why Fixed Prompt?

**Decision**: All agents receive identical prompt

**Rationale:**
- Otto's philosophy is simple autonomous operation
- No need for dynamic task assignment
- Agents are intelligent enough to choose tasks
- Reduces complexity in otto-core
- Makes agent behavior predictable

### Why Default 5-Minute Timeout?

**Decision**: 300 seconds (5 minutes) default timeout

**Rationale:**
- Most coding tasks should complete in < 5 minutes
- Prevents runaway agents from blocking indefinitely
- Long enough for meaningful work
- Short enough for responsive operation
- Can be customized via API if needed

### Why Synchronous API?

**Decision**: No async/await, simple blocking calls

**Rationale:**
- Otto is inherently sequential (one agent at a time)
- No concurrency benefits from async
- Simpler code and dependencies
- Easier to understand and maintain
- Caller can thread if needed

## Limitations and Considerations

### Current Limitations

1. **No Parallel Execution**: Only one agent at a time
2. **No Progress Reporting**: Binary state (running/completed)
3. **No Output Capture**: Agent output goes to tmux, not captured
4. **Platform Specific**: Uses Unix-specific commands (pgrep)
5. **No Agent Configuration**: Fixed prompt, fixed timeout

### Error Recovery

- **Transient Failures**: Not handled (e.g., temporary tmux issues)
- **Timeout**: Returns error, doesn't attempt to kill process
- **Claude Not Available**: Fails fast, clear error message

### Security Considerations

- **Process Injection**: Commands are formatted, not sanitized (tmux session is trusted)
- **No Input Validation**: Assumes trusted environment
- **Process Permissions**: Relies on Claude Code CLI's own security

## Future Extensions

Potential areas for enhancement (not currently implemented):

1. **Configurable Prompts**: Allow custom prompts per launch
2. **Output Capture**: Stream agent output back to caller
3. **Progress Events**: Emit events during agent execution
4. **Graceful Termination**: Send SIGTERM to timed-out agents
5. **Metrics**: Track agent duration, success rate, etc.
6. **Platform Support**: Windows support via different process monitoring

## Conclusion

The `otto-core` crate provides a focused, reliable API for launching Claude Code agents. Its design prioritizes simplicity and predictability over flexibility, making it easy to understand and maintain. The crate successfully abstracts away the complexity of tmux session management and process monitoring, providing a clean interface for the main Otto CLI.

The fixed-prompt architecture and polling-based monitoring are intentional design choices that align with Otto's philosophy of simple, autonomous operation. While the crate has limitations, it effectively fulfills its role as the core agent launching component of the Otto system.
