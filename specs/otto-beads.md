# otto-beads Crate Specification

## Overview

The `otto-beads` crate provides integration between Otto and the [Beads](https://github.com/steveyegge/beads) issue tracking system. Beads is an AI-native, git-based issue tracker that stores issues directly in the repository as JSONL files alongside the code.

This crate is responsible for determining whether there are any ready-to-work tasks (beads) that can be executed by autonomous AI agents. It serves as the bridge between Otto's agent orchestration logic and the beads task management system.

**Location**: `/home/mike/Development/otto/crates/otto-beads`

**Purpose**: Enable Otto to query the beads system for tasks that have no dependencies or blockers, allowing autonomous agents to select and work on them.

## Core Features

### 1. Ready Task Detection

The primary feature is the `has_ready_tasks()` function, which checks if there are any beads tasks that are ready to be worked on. A task is considered "ready" when it has no blocking dependencies or unresolved issues preventing it from starting.

### 2. Beads Availability Detection

The crate automatically detects whether the beads CLI (`bd`) is installed and available in the system PATH, providing clear error messages when dependencies are missing.

### 3. Initialization Validation

The crate verifies that beads has been properly initialized in the current working directory (i.e., a `.beads` directory exists), preventing operations in repositories that haven't been set up for beads tracking.

### 4. Error Handling

Provides comprehensive error types for different failure scenarios:
- Beads CLI not found
- Beads not initialized in the repository
- Command execution failures
- Stderr parsing for specific error conditions

## Beads Integration

### What is Beads?

Beads is a modern, AI-native issue tracking system that:
- Stores issues in `.beads/issues.jsonl` alongside code in the repository
- Uses a CLI-first interface (`bd` command)
- Integrates seamlessly with git workflows
- Supports task dependencies and blockers
- Syncs with git remotes like code
- Provides branch-aware issue tracking

### The `bd ready` Command

The core integration point is the `bd ready` command, which outputs a list of tasks that have no blockers and can be worked on immediately. The output format includes:
- Task count and priority information
- Task identifiers (e.g., `ralph-xxx` where "ralph" is the issue prefix)
- Task titles and metadata

Example output format:
```
1. [● P1] [task] ralph-abc: Implement feature
2. [● P2] [task] ralph-def: Fix bug
```

## Data Model

### BeadsError Enum

Error types for beads operations:

```rust
pub enum BeadsError {
    /// Beads CLI not installed or not in PATH
    BeadsNotAvailable,

    /// No .beads directory (beads not initialized)
    NotInitialized,

    /// Command execution failure with details
    ExecutionFailed(String),
}
```

### BeadsResult Type

Type alias for Result type with BeadsError:

```rust
pub type BeadsResult<T> = Result<T, BeadsError>;
```

## Technical Implementation

### Function: `has_ready_tasks()`

**Signature**:
```rust
pub fn has_ready_tasks() -> BeadsResult<bool>
```

**Returns**:
- `Ok(true)` - Ready beads exist and can be worked on
- `Ok(false)` - No ready beads (all tasks are blocked or completed)
- `Err(BeadsError::NotInitialized)` - Beads not initialized in repository
- `Err(BeadsError::BeadsNotAvailable)` - Beads CLI not found
- `Err(BeadsError::ExecutionFailed)` - Command execution error

**Implementation Details**:

1. **Beads Availability Check**: Runs `bd --version` to verify the beads CLI is installed and accessible
   - Returns `BeadsNotAvailable` if the command fails or isn't found

2. **Ready Task Query**: Executes `bd ready` to get the list of unblocked tasks
   - Returns `NotInitialized` if stderr contains "not initialized" or ".beads"
   - Returns `ExecutionFailed` for other command failures

3. **Output Parsing**: Analyzes stdout for task indicators
   - Looks for lines containing both `[` and `]` characters
   - This pattern matches the beads output format: `[● P1] [task] ralph-xxx: Title`
   - Returns `true` if any matching lines are found, `false` otherwise

### Dependencies

**Current**: None (zero dependencies)

The crate has no external dependencies, relying only on:
- `std::process::Command` for executing shell commands
- `std::error::Error` for error trait implementations
- `std::fmt::Display` for error display formatting

### Design Decisions

1. **Shell Command Integration**: Uses `std::process::Command` rather than a native library to interface with beads, maintaining loose coupling and allowing beads to evolve independently

2. **Simple Parsing Strategy**: Uses a basic heuristic (looking for brackets) rather than complex parsing, making the implementation resilient to minor output format changes

3. **Explicit Error Types**: Provides distinct error variants for common failure modes, enabling calling code to handle different error scenarios appropriately

4. **No Async/Sync**: Purely synchronous implementation, avoiding complexity since the operation is fast (single shell command)

## Integration with Otto

### Main Loop Integration

The `has_ready_tasks()` function is called from Otto's main loop to determine whether to spawn an agent:

**In `crates/otto/src/main.rs`**:
```rust
match has_ready_tasks() {
    Ok(true) => {
        // Ready beads exist, launch an agent
        launch_agent_default()
    }
    Ok(false) => {
        // No ready beads, exit or wait (depending on --watch mode)
        println!("No ready beads, exiting");
    }
    Err(BeadsError::NotInitialized) => {
        eprintln!("Error: beads not initialized (no .beads directory)");
    }
    Err(e) => {
        eprintln!("Error checking for ready tasks: {}", e);
    }
}
```

### Workflow

1. Otto starts and parses command-line arguments
2. Main loop calls `has_ready_tasks()` to check for work
3. If ready tasks exist, Otto launches a Claude Code agent via `otto-core`
4. Agent receives prompt: "Run bd ready, choose a bead, begin work on only that bead. Exit when done."
5. Agent completes work and exits
6. Loop repeats until `has_ready_tasks()` returns `false`
7. In watch mode, waits 10 seconds before checking again

## Usage Example

```rust
use otto_beads::{has_ready_tasks, BeadsError};

fn main() {
    match has_ready_tasks() {
        Ok(true) => println!("There are ready tasks to work on"),
        Ok(false) => println!("No ready tasks found"),
        Err(BeadsError::BeadsNotAvailable) => {
            println!("Please install beads from https://github.com/steveyegge/beads");
        }
        Err(BeadsError::NotInitialized) => {
            println!("Run 'bd init' to initialize beads in this repository");
        }
        Err(BeadsError::ExecutionFailed(msg)) => {
            println!("Error running beads: {}", msg);
        }
    }
}
```

## Testing Considerations

While the current implementation has no test module, potential test scenarios include:

1. **Unit Tests**:
   - Error formatting and display
   - Result type conversions

2. **Integration Tests** (requiring beads installation):
   - Mock `bd --version` responses
   - Mock `bd ready` output parsing
   - Test error scenarios (not initialized, command not found)

3. **Property Tests**:
   - Verify parsing logic handles various output formats
   - Ensure error handling covers all edge cases

## Future Enhancements

Potential improvements to the crate:

1. **Structured Output**: Return detailed task information rather than boolean (task IDs, priorities, titles)

2. **Caching**: Cache results to avoid repeated `bd ready` calls in quick succession

3. **Configurable Timeouts**: Add timeout parameters for beads command execution

4. **Async Support**: Provide async variants for use in async contexts

5. **Filtering**: Support filtering by priority, task type, or labels

6. **Direct JSONL Reading**: Parse `.beads/issues.jsonl` directly for faster lookups (bypassing CLI)

## Dependencies

### External

None - This crate has zero external dependencies.

### Internal

The `otto-beads` crate is used by:
- `otto` (main CLI) - for checking ready tasks in the main loop

## Build Configuration

**From workspace `Cargo.toml`**:
```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Mike Kusold"]
license = "MIT"
```

**Crate `Cargo.toml`**:
```toml
[package]
name = "otto-beads"
version.workspace = true
edition.workspace = true

[dependencies]
# None - zero dependencies!
```

## License

MIT

## Author

Mike Kusold
