# otto-tmux Crate Specification

## Overview

`otto-tmux` is a Rust crate that provides tmux session management functionality for the Otto project. It serves as the interface layer between Otto and the tmux terminal multiplexer, enabling Otto to create, reuse, and interact with tmux sessions for running Claude Code agents.

### Purpose

The crate abstracts away the complexities of tmux command-line interactions, providing a clean, type-safe Rust API for:

- Creating new tmux sessions
- Checking for existing sessions
- Sending commands to running tmux sessions
- Managing a dedicated "otto" session for agent execution

### Position in Otto Architecture

`otto-tmux` is a foundational crate that sits at the bottom of Otto's dependency chain:

- **otto-tmux**: Provides tmux session management
- **otto-core**: Uses otto-tmux to launch Claude Code agents within tmux sessions
- **otto**: The CLI that orchestrates the agent loop using otto-core

## Core Features

### 1. Tmux Availability Detection

The crate can detect whether tmux is installed and available on the system, providing clear error messages when it's not.

### 2. Session Existence Checking

Before attempting operations, the crate can check if a tmux session with a given name already exists.

### 3. Session Creation

Creates new detached tmux sessions that run in the background, allowing agents to execute without blocking the main process.

### 4. Session Reuse

Implements an "ensure" pattern that creates a session only if it doesn't already exist, enabling efficient session reuse across multiple agent launches.

### 5. Command Execution

Sends commands to running tmux sessions, which are executed as if typed directly in the session.

### 6. Convenience Functions

Provides wrapper functions for the default "otto" session, simplifying common operations.

## Tmux Integration Details

### Tmux Commands Used

The crate interacts with tmux through standard tmux command-line subcommands:

#### Version Check
```bash
tmux -V
```
- Used to verify tmux installation
- Captured via `is_tmux_available()`
- Returns boolean indicating availability

#### Session Existence Check
```bash
tmux has-session -t <session_name>
```
- Used to check if a session exists
- Exit code 0 indicates session exists
- Exit code 1 indicates session doesn't exist
- Captured via `session_exists()`

#### Session Creation
```bash
tmux new-session -d -s <session_name>
```
- `-d`: Start session in detached mode (doesn't attach to terminal)
- `-s`: Specify session name
- Captured via `create_session()`

#### Command Execution
```bash
tmux send-keys -t <session_name> <command> C-m
```
- `-t`: Target session
- `send-keys`: Simulate keyboard input
- `C-m`: Carriage return (Enter key) to execute the command
- Captured via `send_command()`

### Session Management Strategy

The crate implements a **lazy session creation** strategy:

1. Before any operation, check if tmux is available
2. When a session is needed, first check if it already exists
3. Only create a new session if one doesn't exist
4. This allows the same session to be reused across multiple agent launches

### Default Session

The crate defines a default session name constant:

```rust
pub const OTTO_SESSION_NAME: &str = "otto";
```

This session is:
- Created on-demand when first needed
- Reused for all agent operations
- Can be attached to manually for observation: `tmux attach-session -t otto`

## Session Management Concepts

### Session Lifecycle

```
[Non-existent] → [Created] → [Active] → [Persisted]
                    ↑            ↓
                    └────Reuse────┘
```

1. **Non-existent**: Session doesn't exist yet
2. **Created**: Session is created via `create_session()`
3. **Active**: Session is running and can receive commands
4. **Persisted**: Session continues to exist after commands complete

### Idempotent Operations

The `ensure_session()` function provides idempotent session creation:

```rust
ensure_session("otto")  // Creates session if needed
ensure_session("otto")  // No-op if already exists
ensure_session("otto")  // Continues to work
```

This is the primary function used by otto-core, ensuring that:
- The first agent launch creates the session
- Subsequent launches reuse the existing session
- No errors occur from attempting to create duplicate sessions

### Detached Session Model

Sessions are always created in detached mode (`-d` flag), which means:
- They run in the background
- Don't hijack the terminal
- Can be attached to later for observation
- Continue running even after the otto process exits

This design allows Otto to:
- Launch agents without blocking
- Monitor agent progress through tmux
- Enable manual observation of agent work
- Maintain session state across multiple agent launches

## Key Data Structures

### Error Types

#### TmuxError Enum

```rust
pub enum TmuxError {
    TmuxNotAvailable,
    SessionCreationFailed(String),
    CommandExecutionFailed(String),
    SessionCheckFailed(String),
}
```

**Variants:**

- **TmuxNotAvailable**: tmux is not installed or not in PATH
  - Display message: "tmux command not found - please install tmux"
  - Occurs when: `tmux -V` command fails

- **SessionCreationFailed(String)**: Session creation failed
  - Display message: "failed to create tmux session: {msg}"
  - Contains stderr output from tmux command
  - Occurs when: `tmux new-session` returns non-zero exit code

- **CommandExecutionFailed(String)**: Command sending failed
  - Display message: "failed to execute command in tmux: {msg}"
  - Contains stderr output from tmux command
  - Occurs when: `tmux send-keys` returns non-zero exit code

- **SessionCheckFailed(String)**: Session existence check failed
  - Display message: "failed to check tmux session: {msg}"
  - Contains underlying IO error details
  - Occurs when: `tmux has-session` command cannot be executed

**Trait Implementations:**
- `Debug`: Enables error debugging
- `Display`: Provides user-friendly error messages
- `Error`: Implements standard error trait for error propagation

#### Type Alias

```rust
pub type TmuxResult<T> = Result<T, TmuxError>;
```

Convenience type alias for all crate functions, simplifying error handling.

### Constants

```rust
pub const OTTO_SESSION_NAME: &str = "otto";
```

The default session name used throughout the Otto system. This is a compile-time constant that ensures consistency across the codebase.

### Internal Functions

```rust
fn is_tmux_available() -> bool
```

Private helper function that checks tmux availability by running `tmux -V`.

- Returns `true` if tmux is installed and accessible
- Returns `false` if tmux command fails or doesn't exist
- Used internally by all public functions
- Not exposed in public API

## Technical Implementation Details

### Process Spawning

The crate uses Rust's `std::process::Command` for all tmux interactions:

```rust
Command::new("tmux")
    .args(["has-session", "-t", session_name])
    .output()
```

**Key characteristics:**
- Spawns processes synchronously using `.output()`
- Captures stdout, stderr, and exit status
- No stdin interaction (all commands are one-way)
- Processes complete before function returns

### Error Handling Strategy

**Two-layer error handling:**

1. **Command execution errors**: Process spawn failures
   - Wrapped in appropriate `TmuxError` variant
   - Preserves original error message as String

2. **Tmux operation errors**: Non-zero exit codes
   - Checked via `output.status.success()`
   - Stderr captured and included in error message
   - Provides context about what went wrong

**Example:**
```rust
match output {
    Ok(output) if output.status.success() => Ok(()),
    Ok(output) => {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(TmuxError::SessionCreationFailed(stderr.to_string()))
    }
    Err(e) => Err(TmuxError::SessionCreationFailed(e.to_string())),
}
```

### String Handling

The crate uses `String::from_utf8_lossy()` for converting tmux output:

```rust
let stderr = String::from_utf8_lossy(&output.stderr);
```

**Why this approach:**
- Handles invalid UTF-8 gracefully
- Replaces invalid sequences with replacement characters
- Never panics on malformed output
- Suitable for error messages where exact byte representation isn't critical

### Zero Dependencies

The crate has no external dependencies:

```toml
[dependencies]
# Empty - uses only std library
```

**Benefits:**
- Fast compilation
- Minimal binary size
- Reduced attack surface
- No dependency conflicts
- Easy to maintain

### Public API Design

#### General-Purpose Functions

```rust
pub fn session_exists(session_name: &str) -> TmuxResult<bool>
pub fn create_session(session_name: &str) -> TmuxResult<()>
pub fn ensure_session(session_name: &str) -> TmuxResult<()>
pub fn send_command(session_name: &str, command: &str) -> TmuxResult<()>
```

These functions accept a `session_name` parameter, allowing use with any tmux session.

#### Convenience Functions

```rust
pub fn ensure_otto_session() -> TmuxResult<()>
pub fn send_otto_command(command: &str) -> TmuxResult<()>
```

These functions use the `OTTO_SESSION_NAME` constant, simplifying common operations.

**Usage pattern:**
```rust
// General purpose
ensure_session("my-custom-session")?;
send_command("my-custom-session", "ls -la")?;

// Otto-specific (more common)
ensure_otto_session()?;
send_otto_command("cargo build")?;
```

## Testing

### Current Test Coverage

The crate includes basic unit tests:

```rust
#[test]
fn test_session_name_constant() {
    assert_eq!(OTTO_SESSION_NAME, "otto");
}

#[test]
fn test_is_tmux_available_returns_bool() {
    let _available = is_tmux_available();
}
```

**Test characteristics:**
- `test_session_name_constant`: Verifies the session name constant
- `test_is_tmux_available_returns_bool`: Smoke test that the function runs
- Located in `#[cfg(test)]` module (not compiled in release builds)

**Testing limitations:**
- No integration tests with actual tmux
- No mocking of tmux commands
- Tests assume tmux may or may not be installed
- No verification of actual tmux session behavior

## Usage Example

### Basic Usage

```rust
use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};

fn main() -> Result<(), TmuxError> {
    // Ensure the otto session exists (creates if needed)
    ensure_otto_session()?;

    // Send a command to the session
    send_otto_command("cargo build")?;

    Ok(())
}
```

### With Custom Session

```rust
use otto_tmux::{ensure_session, send_command};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = "my-agent-session";

    // Create or reuse session
    ensure_session(session)?;

    // Run multiple commands
    send_command(session, "cd /path/to/project")?;
    send_command(session, "git pull")?;
    send_command(session, "cargo test")?;

    Ok(())
}
```

### Error Handling

```rust
use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};

fn launch_agent() -> Result<String, TmuxError> {
    match ensure_otto_session() {
        Ok(()) => {
            send_otto_command("claude 'Run tests'")?;
            Ok("Agent launched".to_string())
        }
        Err(TmuxError::TmuxNotAvailable) => {
            eprintln!("Error: tmux is not installed");
            Err(TmuxError::TmuxNotAvailable)
        }
        Err(e) => Err(e),
    }
}
```

## Integration with otto-core

The `otto-core` crate uses `otto-tmux` as a dependency:

```rust
use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};

pub fn launch_agent(timeout_secs: Option<u64>) -> AgentResult<()> {
    // Ensure the otto tmux session exists
    ensure_otto_session()?;

    // Construct and send the claude command
    let claude_command = format!("claude \"{}\"", OTTO_AGENT_PROMPT);
    send_otto_command(&claude_command)?;

    // Wait for agent completion...
    Ok(())
}
```

**Error propagation:**
```rust
impl From<TmuxError> for AgentError {
    fn from(err: TmuxError) -> Self {
        AgentError::TmuxError(err)
    }
}
```

This allows `?` operator to automatically convert `TmuxError` to `AgentError`.

## Design Decisions

### Why Detached Sessions?

Sessions are created with the `-d` flag to run detached:

**Reasons:**
1. **Non-blocking**: Otto can continue while agents run
2. **Observability**: Users can attach later to see progress
3. **Persistence**: Sessions survive otto process restarts
4. **Flexibility**: Multiple agents can use the same session

### Why "ensure" Pattern?

The crate provides `ensure_session()` instead of just `create_session()`:

**Reasons:**
1. **Idempotence**: Safe to call multiple times
2. **Reuse**: Avoids creating duplicate sessions
3. **Simplicity**: Callers don't need to check existence first
4. **Efficiency**: No error handling for "already exists" case

### Why String for Errors?

Error variants contain `String` rather than more specific error types:

**Reasons:**
1. **Simplicity**: No complex error type hierarchies
2. **Flexibility**: Can include any tmux error message
3. **Clarity**: Direct representation of what went wrong
4. **No dependencies**: Avoids external error libraries

### No Async/Await

The crate uses synchronous process spawning, not async:

**Reasons:**
1. **Simplicity**: No tokio or async-std dependency
2. **Sufficient**: tmux commands complete quickly
3. **Blocking acceptable**: Short-lived commands don't need async
4. **Easier integration**: Simple function signatures

## Future Considerations

### Potential Enhancements

1. **Session listing**: Function to list all otto sessions
2. **Session cleanup**: Function to kill/terminate sessions
3. **Window management**: Support for multiple windows per session
4. **Pane management**: Support for split panes
5. **Output capture**: Capture command output from sessions
6. **Session status**: Check if session is active/idle

### Limitations

1. **Single server**: Assumes tmux server is running
2. **No session inspection**: Can't see what's running in session
3. **No output retrieval**: Commands are fire-and-forget
4. **Limited error recovery**: No retry logic for failures
5. **No configuration**: Hard-coded session name and behavior

## Security Considerations

### Command Injection

The crate constructs shell commands and sends them to tmux:

```rust
send_command(session, &format!("claude \"{}\"", user_input))
```

**Risks:**
- If `user_input` contains tmux special characters, they will be interpreted
- No sanitization of command strings
- Relies on callers to sanitize input

**Mitigation:**
- Otto uses fixed prompts (no user input in commands)
- Crate is low-level; security is caller's responsibility
- Document that commands should be trusted

### Process Permissions

The crate spawns tmux processes with the same permissions as the calling process:

- No privilege escalation
- No sandboxing
- Runs as the same user

**Assumption:**
- Trusted environment (user's own development machine)
- tmux is trusted software
- Commands run are intended by user

## Performance Characteristics

### Process Spawning Overhead

Each function call spawns at least one tmux process:

- `session_exists()`: 1 process spawn
- `create_session()`: 1 process spawn
- `ensure_session()`: 1-2 process spawns (exists check + optional create)
- `send_command()`: 1 process spawn

**Typical timing:**
- Process spawn: ~1-5ms per call
- Total overhead per agent launch: ~2-10ms
- Acceptable for Otto's use case (agent runs take minutes)

### Memory Footprint

- Zero heap allocations for constants
- String allocations only for error messages
- No persistent state
- Minimal binary size increase

## Platform Support

### Linux/Unix

Primary target platform. tmux is native to Unix-like systems.

- **Linux**: Fully supported
- **macOS**: Fully supported
- **BSD**: Should work (tmux compatible)

### Windows

Not currently supported. Would require:
- Windows tmux port (e.g., via WSL or Cygwin)
- Or different terminal multiplexer (e.g., Windows Terminal)

### NixOS

Specifically mentioned in Otto documentation as primary platform:

```bash
nix develop
```

tmux is available in nixpkgs and works seamlessly.

## Summary

`otto-tmux` is a minimal, focused crate that provides essential tmux session management for the Otto project. It:

- Provides a clean Rust API for tmux operations
- Handles session creation, checking, and command execution
- Uses zero external dependencies
- Implements an idempotent "ensure" pattern for session reuse
- Integrates seamlessly with otto-core through error propagation
- Prioritizes simplicity and reliability over feature completeness

The crate is intentionally minimal, providing only what otto-core needs and nothing more, following Unix philosophy principles of doing one thing well.
