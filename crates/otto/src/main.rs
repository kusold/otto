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
        None => {
            // No subcommand provided, print help
            println!("Otto - Autonomous agent runner for beads tasks\n");
            println!("Usage: otto <COMMAND>\n");
            println!("Commands:");
            println!("  ralph  Run the agent loop (default behavior)");
            println!("\nFlags:");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            println!("\nExamples:");
            println!("  otto ralph              Run in single-pass mode");
            println!("  otto ralph --watch      Run in watch mode (infinite loop)");
            println!("  otto ralph -p promp.txt Use custom prompt file");
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
