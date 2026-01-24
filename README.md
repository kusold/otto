# Otto

Otto is a simple CLI tool that autonomously runs AI coding agents in a loop. On each iteration, it launches an agent which picks a task from the [beads](https://github.com/heartsucker/bd) system, works on it, then exits. Otto repeats this until no ready tasks remain.

## Vision

Enable autonomous AFK (away from keyboard) coding by continuously running AI agents against a task queue. No complex orchestration—just a simple loop that lets agents work through tasks independently.

## Features

- **Simple Loop**: Runs agents until no ready beads remain
- **Watch Mode**: With `--watch`, runs continuously checking for new tasks
- **Tmux Integration**: Spawns agents in a reusable `otto` tmux session
- **Graceful Shutdown**: Handles Ctrl+C cleanly, waiting for agents to finish
- **Beads Integration**: Works with the beads issue tracking system

## Prerequisites

Before using Otto, ensure you have the following installed:

1. **tmux** - Terminal multiplexer for session management
   - NixOS: `tmux` is in nixpkgs
   - Ubuntu/Debian: `sudo apt install tmux`
   - macOS: `brew install tmux`

2. **Claude Code CLI** - The AI coding agent
   - Install from: https://github.com/anthropics/claude-code

3. **beads (bd)** - Git-based issue tracking system
   - Install from: https://github.com/heartsucker/bd
   - Initialize in your project: `bd init`

## Installation

### Using Cargo

```bash
cargo install otto
```

### Using Nix (NixOS/NixOS-friendly)

```bash
nix develop
cargo install --path .
```

### Building from Source

```bash
git clone <repository-url>
cd otto
cargo build --release
```

The binary will be available at `target/release/otto`.

## Usage

### Basic Usage

Run Otto in a project with beads initialized:

```bash
otto
```

Otto will:
1. Check for ready beads (tasks with no blockers)
2. Launch a Claude Code agent in a tmux session named `otto`
3. Wait for the agent to complete
4. Repeat until no ready beads remain
5. Exit

### Watch Mode

For continuous operation, use the `--watch` (or `-w`) flag:

```bash
otto --watch
```

In watch mode, Otto will:
- Loop indefinitely
- When no ready beads exist, wait 10 seconds and check again
- Continue until you stop it with Ctrl+C

### Observing the Agent

To watch the agent work, attach to the tmux session:

```bash
tmux attach-session -t otto
```

To detach without stopping Ralph:
- Press `Ctrl+B`, then `D` (the default tmux detach keybinding)

## How It Works

Otto runs a simple loop:

```
while true:
    1. Check for ready beads (via `bd ready`)
    2. If ready beads exist:
       a. Launch Claude Code agent in tmux session
       b. Agent receives fixed prompt: "Run bd ready, choose a bead, begin work on only that bead. Exit when done."
       c. Wait for agent to complete (default: 5 minute timeout)
       d. Repeat
    3. If no ready beads:
       a. Normal mode: Exit
       b. Watch mode: Wait 10 seconds, then check again
```

### The Agent Prompt

All Claude Code agents launched by Otto receive the same fixed prompt:

```
Run bd ready, choose a bead, begin work on only that bead. Exit when done.
```

This prompt instructs the agent to:
1. Check for available tasks
2. Select one task
3. Work only on that task (not multiple tasks)
4. Exit when complete

This design ensures each agent focuses on a single task, maintaining clear boundaries between iterations.

## Troubleshooting

### "beads not initialized (no .beads directory)"

Otto requires beads to be initialized in your project directory.

```bash
bd init
```

### "tmux command not found - please install tmux"

Install tmux using your system package manager:
- NixOS: Already available in nixpkgs
- Ubuntu/Debian: `sudo apt install tmux`
- macOS: `brew install tmux`

### "claude command not found - please install Claude Code CLI"

Install the Claude Code CLI from: https://github.com/anthropics/claude-code

### "No ready beads, exiting"

This is expected behavior when all tasks are completed or blocked. To add work:

```bash
bd create --title="Your task here" --type=task --priority=2
```

### "Agent did not exit in expected time"

The default agent timeout is 5 minutes. If your tasks regularly take longer, you may need to adjust the timeout in the source code (currently defined as `DEFAULT_AGENT_TIMEOUT_SECS` in `otto-core`).

### Claude Code Directory Trust Prompt

When running Otto in a new directory, Claude Code may prompt:

```
Do you trust the files in this folder?
```

This is a security feature and requires manual intervention. **Workaround**: Run Otto in a directory you've previously approved as trusted.

## Limitations

Otto is intentionally simple:

- **No state management**: Each agent run is independent
- **No metrics or logging**: Simple console output only
- **No configuration files**: Behavior is fixed (except for `--watch` flag)
- **No plugin system**: Only Claude Code is supported
- **Claude Code only**: No support for other AI agents currently

## Development

### Project Structure

```
otto/
├── crates/
│   ├── otto/          # CLI interface and main loop
│   ├── otto-core/     # Agent launching logic
│   ├── otto-beads/    # Beads integration
│   └── otto-tmux/     # Tmux session management
├── flake.nix           # Nix flake for development
└── Cargo.toml          # Workspace configuration
```

### Building

```bash
cargo build
```

### Running Tests

```bash
cargo test
```

### Development Shell (Nix)

```bash
nix develop
```

## License

MIT

## Author

Mike Kusold
