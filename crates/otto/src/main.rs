use clap::{Parser, Subcommand};
use otto_agent_claude::AbortCallback;
use otto_beads::{has_ready_tasks, BeadsError};
use otto_core::{color::print_error, color::print_warning, launch_agent_default, AgentError};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Global shutdown flag, set by signal handlers
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global counter for number of shutdown signals received
/// First signal: graceful shutdown (wait for agent)
/// Second signal: force kill and exit immediately
static SHUTDOWN_COUNT: AtomicU8 = AtomicU8::new(0);

/// Otto - Autonomous agent runner for beads tasks
///
/// Otto continuously checks for ready-to-work beads tasks and spawns
/// Claude Code agents to complete them.
#[derive(Parser, Debug)]
#[command(name = "otto")]
#[command(version, about, long_about = None)]
#[command(author = "Mike Kusold")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the agent loop (default behavior)
    ///
    /// Continuously checks for ready-to-work beads tasks and spawns
    /// Claude Code agents to complete them.
    Ralph {
        /// Run in watch mode (loop forever, checking for ready tasks)
        ///
        /// When enabled, otto will continuously loop and spawn agents for ready tasks.
        /// When disabled, otto will stop when no ready tasks are found.
        #[arg(long, short = 'w')]
        watch: bool,

        /// Path to a custom prompt file for Claude Code agents
        ///
        /// If provided, reads the prompt from this file. Otherwise, uses the default
        /// OTTO_AGENT_PROMPT.
        #[arg(long, short = 'p')]
        prompt_file: Option<String>,
    },

    /// Manage Claude Code integration
    ///
    /// Install and configure hooks for Claude Code integration.
    Claude {
        #[command(subcommand)]
        claude_command: ClaudeCommands,
    },
}

#[derive(Subcommand, Debug)]
enum ClaudeCommands {
    /// Install the otto stop hook for Claude Code
    ///
    /// This command installs the otto stop hook to ~/.claude/hooks/ and configures
    /// Claude Code settings to use the hook. The hook ensures Claude only exits
    /// after outputting the <PLANE-HAS-LANDED> marker.
    Install,
}

/// Formats a duration into a human-readable string.
///
/// Examples:
/// - "1m 23s" for 83 seconds
/// - "45s" for 45 seconds
/// - "1h 5m 30s" for 3930 seconds
fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();

    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }

    parts.join(" ")
}

/// Sets up signal handlers for SIGINT (Ctrl+C) and SIGTERM.
///
/// Signal handling behavior:
/// - First signal: Set shutdown flag, wait for agent to finish gracefully
/// - Second signal (Ctrl+C only): Kill running agent immediately and exit
///
/// SIGTERM always triggers graceful shutdown (no force kill).
fn setup_signal_handlers() {
    use signal_hook::iterator::Signals;

    // We need to fork the signal handling to a separate thread
    // to avoid restrictions on what can be done in a signal handler
    let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
        .expect("failed to register signal handler");

    std::thread::spawn(move || {
        for sig in signals.forever() {
            match sig {
                signal_hook::consts::SIGINT => {
                    let count = SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);

                    if count == 0 {
                        // First Ctrl+C: graceful shutdown, kill agent if running
                        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
                        println!("\nShutdown signal received, terminating agent...");
                        println!("Agent will be killed gracefully. Press Ctrl+C again to force exit.");
                    } else {
                        // Second Ctrl+C: force exit immediately
                        println!("\nForce exit requested");
                        std::process::exit(130); // 128 + SIGINT (standard exit code for Ctrl+C)
                    }
                }
                signal_hook::consts::SIGTERM => {
                    if !SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
                        println!("\nShutdown signal received, waiting for agent to finish...");
                    }
                }
                _ => {}
            }
        }
    });
}

fn main() {
    let args = Args::parse();

    // Set up signal handlers for graceful shutdown
    setup_signal_handlers();

    match args.command {
        Some(Commands::Ralph { watch, prompt_file }) => {
            // Convert the prompt_file Option<String> to Option<&str>
            let prompt_file = prompt_file.as_deref();

            if watch {
                println!("Otto running in watch mode (infinite loop)");
                println!("Press Ctrl+C to stop\n");
                run_watch_loop(prompt_file);
            } else {
                println!("Otto running in single-pass mode\n");
                run_single_pass(prompt_file);
            }
        }
        Some(Commands::Claude { claude_command }) => match claude_command {
            ClaudeCommands::Install => {
                if let Err(e) = install_claude_hook() {
                    print_error(&format!("installing Claude hook: {}", e));
                    std::process::exit(1);
                }
            }
        },
        None => {
            // No subcommand provided, print help
            println!("Otto - Autonomous agent runner for beads tasks\n");
            println!("Usage: otto <COMMAND>\n");
            println!("Commands:");
            println!("  ralph   Run the agent loop (default behavior)");
            println!("  claude  Manage Claude Code integration");
            println!("\nFlags:");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            println!("\nExamples:");
            println!("  otto ralph              Run in single-pass mode");
            println!("  otto ralph --watch      Run in watch mode (infinite loop)");
            println!("  otto ralph -p promp.txt Use custom prompt file");
            println!("  otto claude install     Install Claude Code stop hook");
        }
    }
}

fn run_single_pass(prompt_file: Option<&str>) {
    loop {
        // Check if shutdown was requested
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("Shutting down gracefully");
            return;
        }

        // Check if there are ready beads
        match has_ready_tasks() {
            Ok(true) => {
                // Ready beads exist, launch an agent
                println!("Starting agent...");
                // Create abort callback that checks SHUTDOWN_REQUESTED
                let abort_callback: AbortCallback = || {
                    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
                };
                match launch_agent_default(prompt_file, Some(abort_callback)) {
                    Ok(duration) => {
                        println!("Agent finished (duration: {})", format_duration(duration));
                    }
                    Err(AgentError::AgentTimeout) => {
                        print_warning("Agent timed out");
                    }
                    Err(e) => {
                        print_error(&format!("launching agent: {}", e));
                        return;
                    }
                }

                // Check for shutdown again after agent finishes
                if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                    println!("Shutting down gracefully");
                    return;
                }
            }
            Ok(false) => {
                // No ready beads, exit
                println!("No ready beads, exiting");
                return;
            }
            Err(BeadsError::NotInitialized) => {
                print_error("beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                print_error(&format!("checking for ready tasks: {}", e));
                return;
            }
        }
    }
}

fn run_watch_loop(prompt_file: Option<&str>) {
    loop {
        // Check if shutdown was requested
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("Shutting down gracefully");
            return;
        }

        // Check if there are ready beads
        match has_ready_tasks() {
            Ok(true) => {
                // Ready beads exist, launch an agent
                println!("Starting agent...");
                // Create abort callback that checks SHUTDOWN_REQUESTED
                let abort_callback: AbortCallback = || {
                    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
                };
                match launch_agent_default(prompt_file, Some(abort_callback)) {
                    Ok(duration) => {
                        println!("Agent finished (duration: {})", format_duration(duration));
                    }
                    Err(AgentError::AgentTimeout) => {
                        print_warning("Agent timed out");
                    }
                    Err(e) => {
                        print_error(&format!("launching agent: {}", e));
                        // In watch mode, continue on errors
                    }
                }

                // Check for shutdown again after agent finishes
                if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                    println!("Shutting down gracefully");
                    return;
                }
            }
            Ok(false) => {
                // No ready beads, wait a bit before checking again
                println!("No ready beads, waiting...");

                // Sleep in 1-second intervals to allow shutdown checking
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        println!("Shutting down gracefully");
                        return;
                    }
                }
            }
            Err(BeadsError::NotInitialized) => {
                print_error("beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                print_error(&format!("checking for ready tasks: {}", e));
                // In watch mode, continue on errors

                // Sleep in 1-second intervals to allow shutdown checking
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        println!("Shutting down gracefully");
                        return;
                    }
                }
            }
        }
    }
}

/// Install the otto stop hook for Claude Code
///
/// This function:
/// 1. Creates ~/.claude/hooks/ directory if it doesn't exist
/// 2. Writes the otto-stop-hook.sh script to the hooks directory
/// 3. Makes the script executable
/// 4. Creates or updates ~/.claude/settings.json with the hook configuration
fn install_claude_hook() -> Result<(), Box<dyn std::error::Error>> {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    // Get the Claude config directory
    let mut claude_dir = PathBuf::from(
        std::env::var("HOME")
            .map_err(|_| "Could not determine HOME directory")?,
    );
    claude_dir.push(".claude");

    // Create hooks directory
    let hooks_dir = claude_dir.join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    // Stop hook script content
    let hook_script = r#"#!/usr/bin/env bash

# Otto Stop Hook
#
# This hook is triggered when Claude attempts to exit. It checks if the
# task completion marker <PLANE-HAS-LANDED> is present in the transcript.
# If found, the hook allows the exit. Otherwise, it blocks exit and prompts
# Claude to continue working.
#
# This ensures that Claude only exits after completing the assigned task.

set -euo pipefail

# Read hook input from stdin (advanced stop hook API)
HOOK_INPUT=$(cat)

# Get transcript path from hook input
TRANSCRIPT_PATH=$(echo "$HOOK_INPUT" | jq -r '.transcript_path')

if [[ ! -f "$TRANSCRIPT_PATH" ]]; then
    # No transcript - allow exit (shouldn't happen normally)
    exit 0
fi

# Read last assistant message from transcript (JSONL format)
# Check if there are any assistant messages
if ! grep -q '"role":"assistant"' "$TRANSCRIPT_PATH"; then
    # No assistant messages - allow exit
    exit 0
fi

# Extract last assistant message
LAST_LINE=$(grep '"role":"assistant"' "$TRANSCRIPT_PATH" | tail -1)

if [[ -z "$LAST_LINE" ]]; then
    # No assistant message found - allow exit
    exit 0
fi

# Parse JSON to extract text content
LAST_OUTPUT=$(echo "$LAST_LINE" | jq -r '
  .message.content |
  map(select(.type == "text")) |
  map(.text) |
  join("\n")
' 2>&1)

# Check if jq succeeded
if [[ $? -ne 0 ]]; then
    # JSON parse failed - allow exit
    exit 0
fi

# Check for task completion marker
if echo "$LAST_OUTPUT" | grep -q "<PLANE-HAS-LANDED>"; then
    # Task complete - find and kill parent Claude process
    echo "✅ Plane has landed, terminating Claude..."

    # Get the parent PID of this hook script
    HOOK_PID=$$
    PARENT_PID=$(ps -o ppid= -p $HOOK_PID | tr -d ' ')

    # The parent should be Claude Code - kill it
    if [[ -n "$PARENT_PID" ]]; then
        # Double-check it's actually a Claude process before killing
        if ps -p $PARENT_PID -o command= | grep -q "claude"; then
            kill $PARENT_PID
            echo "✅ Terminated Claude process (PID: $PARENT_PID)"
        else
            echo "⚠️  Parent process doesn't appear to be Claude, not killing"
        fi
    else
        echo "⚠️  Could not determine parent PID"
    fi

    exit 0
fi

# Task not complete - block exit and prompt Claude to continue
jq -n \
  --arg msg "⚠️  Task not complete yet. Continue working on the assigned task and output <PLANE-HAS-LANDED> when done." \
  '{
    "decision": "block",
    "reason": "Task completion marker <PLANE-HAS-LANDED> not found. Continue working.",
    "systemMessage": $msg
  }'

exit 0
"#;

    // Write hook script
    let hook_path = hooks_dir.join("otto-stop-hook.sh");
    let mut file = fs::File::create(&hook_path)?;
    file.write_all(hook_script.as_bytes())?;
    file.sync_all()?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;
    }

    println!("✅ Created stop hook at: {}", hook_path.display());

    // Create or update settings.json
    let settings_path = claude_dir.join("settings.json");
    let settings_content = if settings_path.exists() {
        fs::read_to_string(&settings_path)?
    } else {
        // Start with empty JSON if settings file doesn't exist
        "{}".to_string()
    };

    // Parse existing settings
    let mut settings: serde_json::Value = serde_json::from_str(&settings_content)
        .unwrap_or(serde_json::json!({}));

    // Ensure hooks object exists
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }

    // Define the otto stop hook
    let otto_hook = serde_json::json!({
        "hooks": [{
            "type": "command",
            "command": "~/.claude/hooks/otto-stop-hook.sh"
        }]
    });

    // Get existing Stop hooks, or create empty array
    let stop_hooks = settings["hooks"]["Stop"].as_array_mut();

    if let Some(hooks_array) = stop_hooks {
        // Check if otto hook already exists (by checking command path)
        let otto_hook_exists = hooks_array.iter().any(|hook| {
            hook.get("hooks")
                .and_then(|h| h.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("command"))
                .and_then(|cmd| cmd.as_str())
                .map(|cmd| cmd.contains("otto-stop-hook.sh"))
                .unwrap_or(false)
        });

        // Only add if not already present
        if !otto_hook_exists {
            hooks_array.push(otto_hook);
        }
    } else {
        // No Stop hooks exist, create with otto hook
        settings["hooks"]["Stop"] = serde_json::json!([otto_hook]);
    }

    // Write updated settings
    let settings_json = serde_json::to_string_pretty(&settings)?;
    let mut settings_file = fs::File::create(&settings_path)?;
    settings_file.write_all(settings_json.as_bytes())?;
    settings_file.sync_all()?;

    println!("✅ Updated settings at: {}", settings_path.display());
    println!("\n🎉 Otto stop hook installed successfully!");
    println!("   Claude Code will now require the <PLANE-HAS-LANDED> marker to exit.");

    Ok(())
}
