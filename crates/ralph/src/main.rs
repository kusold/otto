use clap::Parser;
use ralph_beads::{has_ready_tasks, BeadsError};
use ralph_core::{launch_agent_default, AgentError};

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

fn main() {
    let args = Args::parse();

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
            }
            Ok(false) => {
                // No ready beads, wait a bit before checking again
                println!("No ready beads, waiting...");
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
            Err(BeadsError::NotInitialized) => {
                eprintln!("Error: beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                eprintln!("Error checking for ready tasks: {}", e);
                // In watch mode, continue on errors
                std::thread::sleep(std::time::Duration::from_secs(10));
            }
        }
    }
}
