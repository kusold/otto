# otto-agent-claude Crate Specification

## Overview

The `otto-agent-claude` crate provides a focused interface for interacting with the Claude Code CLI. It extracts all Claude-specific logic from the orchestration layer, providing a clean abstraction for availability detection, process monitoring, and command construction.

**Location**: `/home/mike/Development/otto/crates/otto-agent-claude`

**Purpose**: Abstract Claude Code CLI interactions into a dedicated crate, enabling:
- Easy testing (mockable Claude interactions)
- Future support for other AI agents (ollama, gemini, etc.)
- Clear separation of concerns
- Reusable Claude-specific utilities

**Version**: 0.1.0

## Core Features

### 1. Availability Detection
- Check if Claude Code CLI is installed and accessible
- Parse and return version information
- Provide clear error messages when Claude is unavailable

### 2. Process Monitoring
- Detect running Claude Code processes
- Wait for process completion with timeout
- Poll-based monitoring using `pgrep`

### 3. Command Construction
- Build Claude Code commands with proper escaping
- Construct agent prompts for autonomous execution
- Handle shell quoting and special characters

### 4. Error Handling
- Comprehensive error types for all failure modes
- Clear error messages for troubleshooting
- Proper error propagation

## Module Structure

### Single Module Architecture

The crate consists of a single module (`lib.rs`) containing all functionality, following the focused scope pattern established by other Otto crates.

## Data Types and Structures

### Error Types

#### `ClaudeError`

```rust
pub enum ClaudeError {
    /// Claude Code CLI is not available
    ClaudeNotAvailable,

    /// Failed to get Claude version
    VersionError(String),

    /// Claude process failed to start
    ClaudeStartFailed(String),

    /// Agent did not exit within timeout
    ClaudeTimeout,

    /// Claude execution failed at runtime
    ClaudeExecutionFailed(String),
}
```

**Variants:**

- **`ClaudeNotAvailable`**: Claude Code CLI not installed or not in PATH
- **`VersionError(String)`**: `claude --version` command failed
- **`ClaudeStartFailed(String)`**: Failed to start Claude process
- **`ClaudeTimeout`**: Agent did not exit within specified timeout
- **`ClaudeExecutionFailed(String)`**: Runtime execution failure

#### `ClaudeResult<T>`

```rust
pub type ClaudeResult<T> = Result<T, ClaudeError>;
```

Type alias for Result with ClaudeError.

## Public API

### Functions

#### `is_claude_available() -> bool`

Check if Claude Code CLI is installed and accessible.

**Returns:**
- `true` - Claude is available
- `false` - Claude is not found

**Implementation:**
```rust
pub fn is_claude_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
```

#### `get_claude_version() -> ClaudeResult<String>`

Get Claude Code CLI version string.

**Returns:**
- `Ok(version)` - Version string (e.g., "1.2.3")
- `Err(ClaudeError::VersionError)` - Version check failed

**Example:**
```rust
match get_claude_version() {
    Ok(version) => println!("Claude version: {}", version),
    Err(ClaudeError::ClaudeNotAvailable) => {
        eprintln!("Claude Code CLI not installed");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

#### `is_claude_running() -> bool`

Check if any Claude Code process is currently running.

**Returns:**
- `true` - At least one Claude process is running
- `false` - No Claude processes found

**Implementation:**
```rust
pub fn is_claude_running() -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
```

**Note:** Uses `-x` flag for exact process name matching (more precise than `-f`).

#### `wait_for_claude_exit(timeout_secs: u64) -> ClaudeResult<()>`

Wait for Claude Code process to exit, with timeout.

**Parameters:**
- `timeout_secs`: Maximum time to wait in seconds

**Returns:**
- `Ok(())` - Claude exited within timeout
- `Err(ClaudeError::ClaudeTimeout)` - Timeout exceeded

**Implementation:**
```rust
pub fn wait_for_claude_exit(timeout_secs: u64) -> ClaudeResult<()> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_claude_running() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(ClaudeError::ClaudeTimeout)
}
```

#### `build_agent_prompt(prompt: &str) -> String`

Build a Claude Code command with the given prompt.

The agent runs in interactive mode and must proactively terminate with `otto done` when complete. The `<PLANE-HAS-LANDED>` marker is used to indicate task completion, but agents are responsible for running the termination command themselves.

**Parameters:**
- `prompt`: The prompt text to send to Claude

**Returns:**
- Complete command string ready to execute

**Example:**
```rust
let cmd = build_agent_prompt("Run tests");
// Returns: "claude --dangerously-skip-permissions 'Run tests'"
```

#### `get_prompt(prompt_file: Option<&str>) -> Result<String, std::io::Error>`

Read a prompt from a file, or return the default prompt.

**Parameters:**
- `prompt_file`: Optional path to a file containing the custom prompt

**Returns:**
- `Ok(String)` - The prompt (from file or default)
- `Err(std::io::Error)` - File read error

#### `is_claude_process(pid: u32) -> bool`

Check if a specific process ID is a Claude process.

Validates whether a given PID corresponds to a running Claude Code CLI process by checking the process command line via `/proc/<pid>/cmdline`.

**Parameters:**
- `pid`: The process ID to check

**Returns:**
- `true` - The PID exists and is a claude process
- `false` - Otherwise

**Implementation:**
```rust
pub fn is_claude_process(pid: u32) -> bool {
    let cmdline_path = format!("/proc/{}/cmdline", pid);
    match std::fs::read_to_string(&cmdline_path) {
        Ok(cmdline) => {
            let command = cmdline.replace('\0', " ");
            command.contains("claude")
        }
        Err(_) => false,
    }
}
```

**Note:** Linux/Unix specific (requires procfs).

#### `wait_for_claude_exit_with_progress(timeout_secs: u64, progress_callback: Option<ProgressCallback>, abort_callback: Option<AbortCallback>) -> ClaudeResult<()>`

Extended version of `wait_for_claude_exit` with callback support.

Polls every 2 seconds to check if claude is still running. If a progress callback is provided, it will be called every 2 seconds with the elapsed time. If an abort callback is provided and returns true, claude will be killed and the function returns Ok.

**Parameters:**
- `timeout_secs`: Maximum time to wait in seconds
- `progress_callback`: Optional callback for progress updates (receives elapsed Duration)
- `abort_callback`: Optional callback that returns true if wait should be aborted

**Returns:**
- `Ok(())` - Claude exited (or was aborted via callback)
- `Err(ClaudeError::ClaudeTimeout)` - Timeout reached

#### `kill_claude() -> bool`

Kills all running Claude processes immediately.

Uses `pkill` to terminate all claude processes. This is a forceful termination intended for emergency shutdown scenarios or abort callbacks.

**Returns:**
- `true` - Any claude processes were killed
- `false` - No claude processes were running

**Implementation:**
```rust
pub fn kill_claude() -> bool {
    Command::new("pkill")
        .arg("-x")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
```

## Callback Types

### `ProgressCallback`

Type alias for progress callback functions during agent wait.

```rust
pub type ProgressCallback = fn(std::time::Duration);
```

The callback receives the elapsed duration as a parameter, allowing callers to display progress or log waiting time.

### `AbortCallback`

Type alias for abort checking functions during agent wait.

```rust
pub type AbortCallback = fn() -> bool;
```

The callback returns true if the wait should be aborted. When aborted, `kill_claude()` is called to terminate the agent.

## Constants

### `OTTO_AGENT_PROMPT`

The default prompt used by Otto agents:

```rust
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. When done, output <PLANE-HAS-LANDED> and then exit. Land the plane.";
```

**Purpose:** This is the fixed prompt sent to all autonomous agents, ensuring consistent behavior across agent launches. The prompt instructs agents to output the `<PLANE-HAS-LANDED>` marker when complete, then proactively run `otto done` to terminate cleanly. Agents are responsible for their own termination - no blocking hooks are used.

## Technical Implementation Details

### Dependencies

**External:** None (zero dependencies)

**Internal:** Used by `otto-core` for agent orchestration

### External Commands

The crate interacts with these system commands:

#### 1. `claude --version`
- **Purpose:** Check Claude availability and version
- **Usage:** `Command::new("claude").arg("--version").output()`
- **Success Criteria:** Exit code 0
- **Output Parsing:** Extract version from stdout

#### 2. `pgrep -x claude`
- **Purpose:** Check for running Claude processes (exact name match)
- **Usage:** `Command::new("pgrep").arg("-x").arg("claude").output()`
- **Success Criteria:** Exit code 0 means process found
- **Interpretation:** Exit code 1 means no process running

#### 3. `pkill -x claude`
- **Purpose:** Kill all running Claude processes
- **Usage:** `Command::new("pkill").arg("-x").arg("claude").output()`
- **Success Criteria:** Exit code 0 means processes were killed

#### 4. `/proc/<pid>/cmdline`
- **Purpose:** Read process command line to verify if it's a Claude process
- **Usage:** `std::fs::read_to_string(format!("/proc/{}/cmdline", pid))`
- **Platform:** Linux/Unix only (procfs required)

### Process Monitoring Strategy

The crate uses polling-based process monitoring:

```
1. Start monitoring loop
2. Every 2 seconds:
   - Run pgrep -f claude
   - If exit code 1: Claude exited (SUCCESS)
   - If exit code 0: Continue waiting
3. If elapsed time >= timeout: Return ClaudeTimeout
```

**Advantages:**
- Simple and reliable
- Works across Unix-like systems
- No complex signal handling
- Easy to understand

**Trade-offs:**
- 2-second latency in detecting completion
- CPU overhead from repeated pgrep calls
- Unix-specific (requires pgrep)

### Shell Escaping

Command construction uses POSIX shell escaping via `escape_shell_arg`:

```rust
fn escape_shell_arg(s: &str) -> String {
    // Simple POSIX shell escaping: wrap in single quotes and escape single quotes
    format!("'{}", s.replace('\'', "'\\''"))
}

pub fn build_agent_prompt(prompt: &str) -> String {
    format!(
        "claude --dangerously-skip-permissions {}",
        escape_shell_arg(prompt)
    )
}
```

**Note:** The escaping wraps the prompt in single quotes and escapes any embedded single quotes with `'\''`. This is sufficient for trusted prompts. For production use with untrusted input, consider using the `shlex` crate for more robust shell escaping.

## Integration with Otto Ecosystem

### Role in Architecture

```
┌─────────────────┐
│   otto (CLI)    │  Main loop, signal handling
└────────┬────────┘
         │
         ├────────────────────────┐
         │                        │
┌────────▼────────┐    ┌─────────▼──────────┐
│  otto-beads     │    │   otto-core         │
│  Task checking  │    │   Agent orchestration│
└─────────────────┘    └─────────┬──────────┘
                                │
                       ┌────────┼────────┐
                       │        │        │
                ┌──────▼───┐ ┌─▼─────┐ ┌─▼──────────┐
                │otto-agent-│ │otto-  │ │ (future:   │
                │claude    │ │tmux   │ │  other     │
                │          │ └───────┘ │  agents)   │
                └──────────┘           └────────────┘
```

### Dependency Flow

1. **otto-core** depends on **otto-agent-claude** for Claude operations
2. **otto-core** depends on **otto-tmux** for session management
3. **otto** depends on **otto-core** for agent orchestration

### Usage in otto-core

From `/home/mike/Development/otto/crates/otto-core/src/lib.rs`:

```rust
use otto_agent_claude::{
    build_agent_prompt, get_prompt, is_claude_available, is_claude_process, AbortCallback,
    ClaudeError,
};
```

The actual implementation uses `is_claude_process` for PID validation and the callback-based `wait_for_claude_exit_with_progress` for agent monitoring with abort support.

## Testing Considerations

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otto_agent_prompt_constant() {
        assert!(OTTO_AGENT_PROMPT.contains("bd ready"));
        assert!(OTTO_AGENT_PROMPT.contains("<PLANE-HAS-LANDED>"));
    }

    #[test]
    fn test_build_agent_prompt() {
        let cmd = build_agent_prompt("test prompt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(!cmd.contains("--print"));  // Should NOT contain --print
        assert!(!cmd.contains("--output-format"));  // Should NOT contain output-format
        assert!(cmd.contains("test prompt"));
    }

    #[test]
    fn test_get_prompt_default() {
        let prompt = get_prompt(None).unwrap();
        assert_eq!(prompt, OTTO_AGENT_PROMPT);
    }

    #[test]
    fn test_get_prompt_file_not_found() {
        let result = get_prompt(Some("/nonexistent/file.txt"));
        assert!(result.is_err());
    }
}
```

### Integration Tests

Integration tests would require Claude Code CLI to be installed:
- Mock `claude --version` responses
- Mock `pgrep` output for process monitoring
- Test error scenarios

## Design Decisions

### Why Separate Crate?

**Decision:** Extract Claude logic from otto-core

**Rationale:**
1. **Separation of Concerns:** otto-core should orchestrate, not handle Claude specifics
2. **Testability:** Can mock Claude interactions without mocking entire orchestration
3. **Extensibility:** Easy to add support for other AI agents (ollama, gemini-cli)
4. **Clarity:** Makes Claude-specific code obvious and contained

### Why Polling Instead of Signals?

**Decision:** Use `pgrep` polling for process monitoring

**Rationale:**
1. Claude process is spawned by tmux, not otto-core directly
2. No parent-child relationship, so `waitpid()` is unavailable
3. Polling is simpler than tmux-specific monitoring
4. Works reliably across Unix-like systems

### Why Fixed Prompt?

**Decision:** Provide `OTTO_AGENT_PROMPT` as constant

**Rationale:**
1. Otto's philosophy is simple autonomous operation
2. Agents are intelligent enough to choose tasks
3. Consistent behavior across all agent launches
4. Reduces complexity in orchestration

## Future Enhancements

### Potential Improvements

1. **Async Support:** Provide async variants for all functions
2. **Structured Output:** Parse Claude version into semver::Version
3. **Event Streaming:** Emit events during process monitoring
4. **Better Escaping:** Use `shlex` crate for robust shell escaping
5. **Windows Support:** Alternative process monitoring for Windows
6. **Agent Discovery:** Detect multiple Claude processes

### Extension Points

1. **Other AI Agents:** Create `otto-ollama`, `otto-gemini` crates with same interface
2. **Agent Abstraction:** Define `AgentProvider` trait for polymorphic agent support
3. **Configuration:** Make prompt and timeout configurable
4. **Metrics:** Track Claude invocation count, success rate, average duration

## Limitations

### Current Limitations

1. **Unix-Only:** Uses `pgrep`, not available on Windows
2. **No Output Capture:** Cannot see what Claude is doing
3. **Basic Escaping:** Shell escaping is simplistic
4. **No Async:** Synchronous-only operations
5. **No Progress:** Binary state (running/not running)

### Error Recovery

- **Transient Failures:** Not handled (e.g., temporary pgrep failures)
- **Timeout:** Does not attempt to kill the process
- **Claude Not Available:** Fails fast, clear error message

## Security Considerations

### Command Injection

The crate constructs shell commands using POSIX shell escaping:

```rust
fn escape_shell_arg(s: &str) -> String {
    format!("'{}", s.replace('\'', "'\\''"))
}
```

**Risks:**
- If input contains malicious shell metacharacters
- Current escaping uses single quotes with internal quote escaping

**Mitigation:**
- Otto uses fixed prompts (no user input)
- Single-quote escaping is reasonably robust for trusted input
- Future: Use `shlex` crate for more comprehensive escaping
- Document that commands should be trusted

### Process Permissions

The crate spawns processes with same permissions as calling process:
- No privilege escalation
- No sandboxing
- Runs as same user

**Assumption:**
- Trusted environment (user's development machine)
- Claude Code CLI is trusted software
- Commands run are intended by user

## Performance Characteristics

### Process Spawning Overhead

Each function call spawns at least one process:
- `is_claude_available()`: 1 process spawn
- `get_claude_version()`: 1 process spawn
- `is_claude_running()`: 1 process spawn
- `wait_for_claude_exit()`: N spawns (every 2 seconds)

**Typical timing:**
- Process spawn: ~1-5ms per call
- Monitoring overhead: ~0.5-2.5ms per poll
- Acceptable for Otto's use case (agent runs take minutes)

### Memory Footprint

- Zero heap allocations for constants
- String allocations only for error messages and command construction
- No persistent state
- Minimal binary size increase

## Platform Support

### Linux/Unix

Primary target platform. `pgrep` is native to Unix-like systems.

- **Linux**: Fully supported
- **macOS**: Fully supported (pgrep available)
- **BSD**: Should work (pgrep compatible)

### Windows

Not currently supported. Would require:
- Windows process monitoring via `tasklist` or PowerShell
- Different command escaping
- Or WSL/Cygwin environment

## Build Configuration

**From workspace `Cargo.toml`:**
```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Mike Kusold"]
license = "MIT"
```

**Crate `Cargo.toml`:**
```toml
[package]
name = "otto-agent-claude"
version.workspace = true
edition.workspace = true

[dependencies]
# None - zero dependencies!
```

## Conclusion

The `otto-agent-claude` crate provides a focused, reliable interface for Claude Code CLI interactions. Its design prioritizes:

- **Separation of Concerns:** Claude-specific logic isolated from orchestration
- **Testability:** Mockable interface for easy testing
- **Extensibility:** Clear path to support other AI agents
- **Simplicity:** Zero dependencies, straightforward implementation

By extracting Claude interactions from otto-core, the codebase becomes more modular, testable, and ready for future enhancements like supporting multiple AI providers.

## License

MIT

## Author

Mike Kusold
