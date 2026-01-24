# Otto Specifications

This directory contains comprehensive technical specifications for all crates in the Otto project.

## Overview

Otto is a command-line tool that autonomously executes AI coding agents in a continuous loop. It integrates with the [Beads](https://github.com/steveyegge/beads) issue tracking system and Claude Code CLI to automate task completion, enabling autonomous AFK (away from keyboard) coding.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        otto (CLI)                           │
│  Main loop, signal handling, watch mode, graceful shutdown  │
└───────────────┬────────────────────┬────────────────────────┘
                │                    │
        ┌───────▼─────────┐  ┌──────▼──────────┐
        │   otto-beads    │  │   otto-core     │
        │  Task checking  │  │ Agent orchestration│
        └─────────────────┘  └──────┬──────────┘
                                     │
                    ┌────────────────┼────────────────┐
                    │                │                │
            ┌───────▼────────┐ ┌────▼─────┐ ┌───────▼──────────┐
            │   otto-claude  │ │otto-tmux │ │ (future: other  │
            │  Claude CLI    │ │session   │ │  agent providers)│
            │  interactions  │ │management│ └──────────────────┘
            └────────────────┘ └──────────┘
```

## Crate Specifications

| Crate | Description | Specification |
|-------|-------------|---------------|
| **otto** | Main CLI binary - orchestrates the autonomous agent loop with signal handling and watch mode | [otto-cli.md](./otto-cli.md) |
| **otto-core** | Core agent orchestration - coordinates agent launching, monitoring, and lifecycle management | [otto-core.md](./otto-core.md) |
| **otto-claude** | Claude Code CLI integration - availability detection, process monitoring, command construction | [otto-claude.md](./otto-claude.md) |
| **otto-beads** | Beads issue tracking integration - checks for ready-to-work tasks with no blockers | [otto-beads.md](./otto-beads.md) |
| **otto-tmux** | Tmux session management - provides interface for creating and managing tmux sessions | [otto-tmux.md](./otto-tmux.md) |

## Domain Topics

| Topic | Description | Relevant Specs |
|-------|-------------|----------------|
| **CLI Design** | Command-line interface, argument parsing, user output messages | [otto-cli.md](./otto-cli.md) |
| **Agent Lifecycle** | Spawning, monitoring, timeout handling for Claude Code agents | [otto-core.md](./otto-core.md) |
| **Claude Integration** | Claude Code CLI availability, process monitoring, command construction | [otto-claude.md](./otto-claude.md) |
| **Session Management** | Tmux session creation, reuse, command execution | [otto-tmux.md](./otto-tmux.md) |
| **Task Queue Integration** | Beads issue tracking, ready task detection, dependency resolution | [otto-beads.md](./otto-beads.md) |
| **Signal Handling** | SIGINT/SIGTERM handling, graceful shutdown, atomic coordination | [otto-cli.md](./otto-cli.md) |
| **Process Monitoring** | Polling-based agent monitoring with pgrep, timeout detection | [otto-claude.md](./otto-claude.md) |
| **Error Handling** | Error types, propagation, user-friendly messages across all crates | [All specs](./otto-cli.md) |
| **Concurrency Model** | Signal handling thread, main control loop, synchronous operations | [otto-cli.md](./otto-cli.md) |

## Quick Reference

### For Users

- **Installation & Usage**: See [otto-cli.md](./otto-cli.md) for command-line interface details
- **Requirements**: tmux, Claude Code CLI, beads (bd), pgrep
- **Basic Usage**: `otto` (single-pass) or `otto --watch` (continuous)

### For Developers

- **Contributing**: Start with [otto-core.md](./otto-core.md) for the core launching logic
- **Dependencies**: See each spec for internal/external dependency breakdowns
- **Design Philosophy**: Simplicity, autonomy, reliability over flexibility

### For Integrators

- **Beads Integration**: [otto-beads.md](./otto-beads.md) explains the task queue bridge
- **Tmux Integration**: [otto-tmux.md](./otto-tmux.md) documents session management API
- **Extending**: Each spec includes "Future Considerations" and design decision rationale

## Version Information

- **Version**: 0.1.0
- **Rust Edition**: 2024
- **License**: MIT
- **Author**: Mike Kusold

## File Index

1. [otto-cli.md](./otto-cli.md) - 569 lines
   - CLI interface, watch mode, signal handling, main loop behavior

2. [otto-tmux.md](./otto-tmux.md) - 605 lines
   - Tmux session management, command execution, availability detection

3. [otto-core.md](./otto-core.md) - 455 lines
   - Agent orchestration, lifecycle management, coordination

4. [otto-claude.md](./otto-claude.md) - TBD lines
   - Claude Code CLI integration, process monitoring, command construction

5. [otto-beads.md](./otto-beads.md) - 267 lines
   - Beads integration, ready task detection, error handling
