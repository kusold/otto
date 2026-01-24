# otto-claude Crate Specification

## Overview

The `otto-claude` crate provides a focused interface for interacting with the Claude Code CLI. It extracts all Claude-specific logic from the orchestration layer, providing a clean abstraction for availability detection, process monitoring, and command construction.

**Location**: `/home/mike/Development/otto/crates/otto-claude`

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
    VersionCheckFailed(String),

    /// Process monitoring failed
    ProcessCheckFailed(String),

    /// Agent did not exit within timeout
    AgentTimeout,

    /// Command execution failed
    ExecutionFailed(String),
}
```

**Variants:**

- **`ClaudeNotAvailable`**: Claude Code CLI not installed or not in PATH
- **`VersionCheckFailed(String)`**: `claude --version` command failed
- **`ProcessCheckFailed(String)`**: `pgrep` command failed
- **`AgentTimeout`**: Agent did not exit within specified timeout
- **`ExecutionFailed(String)`**: General execution failure with details

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
    match Command::new("claude").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}
```

#### `get_claude_version() -> ClaudeResult<String>`

Get Claude Code CLI version string.

**Returns:**
- `Ok(version)` - Version string (e.g., "1.2.3")
- `Err(ClaudeError::ClaudeNotAvailable)` - Claude not installed
- `Err(ClaudeError::VersionCheckFailed)` - Version check failed

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
    match Command::new("pgrep")
        .args(["-f", "claude"])
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}
```

#### `wait_for_claude_exit(timeout_secs: u64) -> ClaudeResult<()>`

Wait for Claude Code process to exit, with timeout.

**Parameters:**
- `timeout_secs`: Maximum time to wait in seconds

**Returns:**
- `Ok(())` - Claude exited within timeout
- `Err(ClaudeError::AgentTimeout)` - Timeout exceeded
- `Err(ClaudeError::ProcessCheckFailed)` - pgrep failed

**Implementation:**
```rust
pub fn wait_for_claude_exit(timeout_secs: u64) -> ClaudeResult<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_secs(2);

    while start.elapsed() < timeout {
        if !is_claude_running() {
            return Ok(());
        }
        thread::sleep(poll_interval);
    }

    Err(ClaudeError::AgentTimeout)
}
```

#### `build_agent_prompt(prompt: &str) -> String`

Build a Claude Code command with the given prompt.

**Parameters:**
- `prompt`: The prompt text to send to Claude

**Returns:**
- Complete command string ready to execute

**Example:**
```rust
let cmd = build_agent_prompt("Run tests");
// Returns: "claude \"Run tests\""
```

**Shell Escaping:**
The function properly escapes quotes and special characters:
```rust
let cmd = build_agent_prompt("Fix \"bug\" in code");
// Returns: "claude \"Fix \\\"bug\\\" in code\""
```

## Constants

### `OTTO_AGENT_PROMPT`

The default prompt used by Otto agents:

```rust
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. Exit when done.";
```

**Purpose:** This is the fixed prompt sent to all autonomous agents, ensuring consistent behavior across agent launches.

### `DEFAULT_CLAUDE_TIMEOUT_SECS`

Default timeout for Claude agent completion:

```rust
pub const DEFAULT_CLAUDE_TIMEOUT_SECS: u64 = 300;
```

**Value:** 300 seconds (5 minutes)

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

#### 2. `pgrep -f claude`
- **Purpose:** Check for running Claude processes
- **Usage:** `Command::new("pgrep").args(["-f", "claude"]).output()`
- **Success Criteria:** Exit code 0 means process found
- **Interpretation:** Exit code 1 means no process running

### Process Monitoring Strategy

The crate uses polling-based process monitoring:

```
1. Start monitoring loop
2. Every 2 seconds:
   - Run pgrep -f claude
   - If exit code 1: Claude exited (SUCCESS)
   - If exit code 0: Continue waiting
3. If elapsed time >= timeout: Return AgentTimeout
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

Command construction uses shell escaping to handle special characters:

```rust
pub fn build_agent_prompt(prompt: &str) -> String {
    let escaped = prompt.replace('"', r#"\""#);
    format!("claude \"{}\"", escaped)
}
```

**Limitations:**
- Basic escaping only (quotes)
- Does not handle all shell metacharacters
- Assumes trusted input (Otto's use case)

**Future Enhancement:**
- Use `shlex` crate for proper escaping
- Support Windows command escaping
- Handle edge cases (newlines, backticks)

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
                │otto-claude│ │otto-  │ │ (future:   │
                │          │ │tmux   │ │  other     │
                │          │ └───────┘ │  agents)   │
                └──────────┘           └────────────┘
```

### Dependency Flow

1. **otto-core** depends on **otto-claude** for Claude operations
2. **otto-core** depends on **otto-tmux** for session management
3. **otto** depends on **otto-core** for agent orchestration

### Usage in otto-core

From `/home/mike/Development/otto/crates/otto-core/src/lib.rs`:

**Before (current implementation):**
```rust
// Claude code embedded in otto-core
let has_claude = Command::new("claude")
    .arg("--version")
    .output()
    .map(|o| o.status.success())
    .unwrap_or(false);

if !has_claude {
    return Err(AgentError::ClaudeNotAvailable);
}
```

**After (using otto-claude):**
```rust
use otto_claude::{is_claude_available, wait_for_claude_exit, ClaudeError};

// Check availability
if !is_claude_available() {
    return Err(AgentError::ClaudeNotAvailable);
}

// Wait for completion
wait_for_claude_exit(timeout_secs)?;
```

## Testing Considerations

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_prompt_constant() {
        assert!(OTTO_AGENT_PROMPT.contains("bd ready"));
        assert!(OTTO_AGENT_PROMPT.contains("Exit when done"));
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(DEFAULT_CLAUDE_TIMEOUT_SECS, 300);
    }

    #[test]
    fn test_build_agent_prompt() {
        let cmd = build_agent_prompt("test");
        assert_eq!(cmd, "claude \"test\"");
    }

    #[test]
    fn test_build_agent_prompt_with_quotes() {
        let cmd = build_agent_prompt("say \"hello\"");
        assert!(cmd.contains("say"));
        assert!(cmd.contains("hello"));
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

The crate constructs shell commands:

```rust
format!("claude \"{}\"", escaped_prompt)
```

**Risks:**
- If input contains malicious shell metacharacters
- Current escaping is basic (only quotes)

**Mitigation:**
- Otto uses fixed prompts (no user input)
- Future: Use `shlex` crate for robust escaping
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

## Migration Guide

### For otto-core

**Step 1: Add dependency**

In `crates/otto-core/Cargo.toml`:
```toml
[dependencies]
otto-claude = { path = "../otto-claude" }
```

**Step 2: Update imports**

Replace Claude-specific code:
```rust
// Old
use crate::ClaudeNotAvailable;

// New
use otto_claude::{is_claude_available, wait_for_claude_exit, ClaudeError};
```

**Step 3: Replace Claude operations**

```rust
// Old
let has_claude = Command::new("claude")
    .arg("--version")
    .output()
    .map(|o| o.status.success())
    .unwrap_or(false);

// New
let has_claude = is_claude_available();
```

**Step 4: Update error handling**

```rust
// Old
pub enum AgentError {
    ClaudeNotAvailable,
    // ...
}

// New
pub enum AgentError {
    ClaudeNotAvailable(ClaudeError),  // Wrap the error
    // ...
}
```

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
name = "otto-claude"
version.workspace = true
edition.workspace = true

[dependencies]
# None - zero dependencies!
```

## Conclusion

The `otto-claude` crate provides a focused, reliable interface for Claude Code CLI interactions. Its design prioritizes:

- **Separation of Concerns:** Claude-specific logic isolated from orchestration
- **Testability:** Mockable interface for easy testing
- **Extensibility:** Clear path to support other AI agents
- **Simplicity:** Zero dependencies, straightforward implementation

By extracting Claude interactions from otto-core, the codebase becomes more modular, testable, and ready for future enhancements like supporting multiple AI providers.

## License

MIT

## Author

Mike Kusold
