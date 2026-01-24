use clap::Parser;
use ralph_beads::{has_ready_tasks, BeadsError};
use ralph_core::{launch_agent_default, AgentError};
use std::sync::atomic::{AtomicBool, Ordering};

/// Global shutdown flag, set by signal handlers
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ralph - Autonomous agent runner for beads tasks
///
/// Ralph continuously checks for ready-to-work beads tasks and spawns
/// Claude Code agents to complete them.
#[derive(Parser, Debug)]
#[command(name = "ralph")]
#[command(version, about, long_about = None)]
#[command(author = "Mike Kusold")]
struct Args {
    /// Run in watch mode (loop forever, checking for ready tasks)
    ///
    /// When enabled, ralph will continuously loop and spawn agents for ready tasks.
    /// When disabled, ralph will stop when no ready tasks are found.
    #[arg(long, short = 'w')]
    watch: bool,
}

/// Sets up signal handlers for SIGINT (Ctrl+C) and SIGTERM.
///
/// When a signal is received, the shutdown flag is set to true, which
/// will cause the main loop to exit gracefully after the current agent finishes.
fn setup_signal_handlers() {
    use signal_hook::iterator::Signals;

    // We need to fork the signal handling to a separate thread
    // to avoid restrictions on what can be done in a signal handler
    let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
        .expect("failed to register signal handler");

    std::thread::spawn(move || {
        for sig in signals.forever() {
            match sig {
                signal_hook::consts::SIGINT | signal_hook::consts::SIGTERM => {
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

    if args.watch {
        println!("Ralph running in watch mode (infinite loop)");
        println!("Press Ctrl+C to stop\n");
        run_watch_loop();
    } else {
        println!("Ralph running in single-pass mode\n");
        run_single_pass();
    }
}

fn run_single_pass() {
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
                match launch_agent_default() {
                    Ok(()) => {
                        println!("Agent finished");
                    }
                    Err(AgentError::AgentTimeout) => {
                        eprintln!("Warning: Agent timed out");
                    }
                    Err(e) => {
                        eprintln!("Error launching agent: {}", e);
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
                eprintln!("Error: beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                eprintln!("Error checking for ready tasks: {}", e);
                return;
            }
        }
    }
}

fn run_watch_loop() {
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
                match launch_agent_default() {
                    Ok(()) => {
                        println!("Agent finished");
                    }
                    Err(AgentError::AgentTimeout) => {
                        eprintln!("Warning: Agent timed out");
                    }
                    Err(e) => {
                        eprintln!("Error launching agent: {}", e);
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
                eprintln!("Error: beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                eprintln!("Error checking for ready tasks: {}", e);
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
