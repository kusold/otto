# Product Requirements Document (PRD)

## Project: Otto

### Overview
Otto is a simple CLI program that runs AI coding agents in a loop. On each iteration, it launches an agent which picks a task from the beads system, works on it, then exits. Otto repeats this until no ready tasks remain.

### Vision
Enable autonomous AFK (away from keyboard) coding by continuously running AI agents against a task queue. No complex orchestration—just a simple loop that lets agents work through tasks independently.

### Target Users
- Developers who want to automate coding tasks while they sleep
- Teams using beads who want continuous AI agent workflows

---

## Functional Requirements

### Core Features

#### 1. Simple Loop
- Run a simple loop: `while true; do launch_agent; check_tasks; done`
- Launch coding agent in a tmux session named `otto` (reuse same session each iteration)
- Agent receives a fixed prompt: *"Run bd ready, choose a bead, begin work on only that bead. Exit when done."*
- Check if more ready beads exist after agent exits
- Stop when no ready beads remain (or run forever with `--watch` flag)
- No state maintained between iterations—each agent run is independent

#### 2. Agent Support
- **Claude Code** - invoked via `claude` command (MVP)
- Future agents can be added, but no plugin architecture—just add the code

#### 3. Beads Integration
- Uses `bd ready` to check for available tasks
- Agents are responsible for updating bead status via beads CLI
- Otto only checks if ready beads exist; doesn't track state

#### 4. Basic Output
- Simple console output: "Starting agent...", "Agent finished", "No ready beads, exiting"
- No metrics, no timing, no logging

---

## Technical Specifications

### Technology Stack
- **Language**: Rust
- **Package Management**: Cargo
- **Development Environment**: NixOS flake dev shells
- **CLI Framework**: clap

### Architecture

#### Components (simplified to 3)
1. **CLI Interface**: clap-based command parsing
2. **Loop Runner**: Simple while loop that spawns agent processes and checks for ready beads
3. **Tmux Integration**: Creates/reuses tmux session named `otto` for agent execution

#### What Otto Does NOT Have (by design)
- No monitoring/failure recovery
- No plugin system
- No configuration files
- No metrics/logging
- No state management

---

## User Stories

### MVP
- As a developer, I want to run `otto` and have it work through my beads tasks automatically
- As a developer, I want to run `otto --watch` to keep agents running continuously

---

## Success Metrics
- Does it run agents in a loop? Yes/No
- Do agents complete beads? Yes/No

---

## Open Questions
None - keep it simple.
