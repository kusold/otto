use clap::{Parser, Subcommand};
use otto_agent_claude::AbortCallback;
use otto_beads::{has_ready_tasks, BeadsError};
use otto_core::{color::print_error, color::print_warning, launch_agent_default, start_stuck_window_monitor, AgentError};
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
    /// Start otto in tmux
    ///
    /// Launches otto in a tmux window named 'otto', running in watch mode.
    /// This is the recommended way to run otto persistently.
    Start,

    /// Attach to a tmux window
    ///
    /// Connects to a tmux window in the otto session. With no arguments,
    /// attaches to the 'otto' window. With an argument, attaches to the
    /// specified window.
    Attach {
        /// Window name to attach to (optional)
        ///
        /// Can be a short name like "ralph-crimson" or a full spec like "otto:ralph-crimson".
        /// If not provided, attaches to the 'otto' window.
        window: Option<String>,
    },

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
        /// If provided, reads the prompt from this file. Otherwise, auto-detects
        /// PROMPT_RALPH.md from the repo root, or falls back to the default
        /// OTTO_AGENT_PROMPT.
        #[arg(long, short = 'p')]
        prompt_file: Option<String>,
    },
}

/// Detects if PROMPT_RALPH.md exists in the repository root.
///
/// Returns Some("PROMPT_RALPH.md") if the file exists, None otherwise.
/// This function searches upward from the current directory to find the
/// repository root (indicated by a .beads directory).
fn detect_ralph_prompt() -> Option<&'static str> {
    const PROMPT_FILE: &str = "PROMPT_RALPH.md";
    const BEADS_DIR: &str = ".beads";

    // Start from current directory and search upward
    let mut current_path = std::env::current_dir().ok()?;

    loop {
        // Check if .beads directory exists (indicates repo root)
        let beads_path = current_path.join(BEADS_DIR);
        if beads_path.is_dir() {
            // Found repo root, check for PROMPT_RALPH.md
            let prompt_path = current_path.join(PROMPT_FILE);
            if prompt_path.is_file() {
                return Some(PROMPT_FILE);
            }
            // Found repo root but no prompt file, stop searching
            return None;
        }

        // Move to parent directory
        if !current_path.pop() {
            // Reached filesystem root, not in a repo
            return None;
        }
    }
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
        Some(Commands::Start) => {
            if let Err(e) = start_otto() {
                print_error(&format!("starting otto: {}", e));
                std::process::exit(1);
            }
        }
        Some(Commands::Attach { window }) => {
            if let Err(e) = attach_to_window(window) {
                print_error(&format!("attaching to window: {}", e));
                std::process::exit(1);
            }
        }
        Some(Commands::Ralph { watch, prompt_file }) => {
            // Auto-detect PROMPT_RALPH.md if no prompt file specified
            let prompt_file = if prompt_file.is_none() {
                detect_ralph_prompt()
            } else {
                prompt_file.as_deref()
            };

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
            println!("  start   Start otto in tmux (runs in background)");
            println!("  attach  Attach to a tmux window");
            println!("  ralph   Run the agent loop (default behavior)");
            println!("\nFlags:");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            println!("\nExamples:");
            println!("  otto start              Start otto in tmux");
            println!("  otto attach             Attach to 'otto' window");
            println!("  otto attach ralph-willow Attach to specific window");
            println!("  otto ralph              Run in single-pass mode");
            println!("  otto ralph --watch      Run in watch mode (infinite loop)");
            println!("  otto ralph -p FILE      Use custom prompt file");
            println!("                         (auto-detects PROMPT_RALPH.md if found)");
        }
    }
}

/// Start otto in tmux.
///
/// This function:
/// 1. Ensures tmux server is running (starts if needed)
/// 2. Ensures the 'otto' tmux session exists (creates if needed)
/// 3. Creates a window named 'otto' (if it doesn't exist)
/// 4. Runs 'otto ralph --watch' in that window
/// 5. Prints confirmation with window name
fn start_otto() -> Result<(), Box<dyn std::error::Error>> {
    use otto_tmux::{ensure_session, send_command_to_window, window_exists, OTTO_SESSION_NAME};

    const OTTO_WINDOW_NAME: &str = "otto";

    // Ensure the otto session exists
    ensure_session(OTTO_SESSION_NAME)?;

    // Check if the 'otto' window already exists
    let window_already_existed = window_exists(OTTO_SESSION_NAME, OTTO_WINDOW_NAME)?;

    if !window_already_existed {
        // Create the window named 'otto'
        otto_tmux::create_named_window(OTTO_SESSION_NAME, OTTO_WINDOW_NAME)?;
    }

    // Send 'otto ralph --watch' command to the window
    send_command_to_window(OTTO_SESSION_NAME, OTTO_WINDOW_NAME, "otto ralph --watch")?;

    if window_already_existed {
        println!("Started otto in existing window: {}", OTTO_WINDOW_NAME);
    } else {
        println!("Started otto in new window: {}", OTTO_WINDOW_NAME);
    }
    println!("Attach with: tmux attach-session -t {}:{}", OTTO_SESSION_NAME, OTTO_WINDOW_NAME);

    Ok(())
}

/// Attach to a tmux window.
///
/// This function:
/// 1. Parses the window argument to extract session and window name
/// 2. Checks if the window exists
/// 3. Attaches to the window using tmux attach-session (replaces otto process)
///
/// # Arguments
/// * `window` - Optional window specification. Can be:
///   - None: attaches to 'otto' window
///   - "ralph-xxx": attaches to 'otto:ralph-xxx' window (short form)
///   - "otto:ralph-xxx": attaches to 'otto:ralph-xxx' window (full spec)
fn attach_to_window(window: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use otto_tmux::{attach_to_window as tmux_attach, list_windows, session_exists, OTTO_SESSION_NAME};

    const DEFAULT_WINDOW_NAME: &str = "otto";

    // Parse the window argument
    let (session, window_name) = match window {
        None => (OTTO_SESSION_NAME.to_string(), DEFAULT_WINDOW_NAME.to_string()),
        Some(w) => {
            if w.contains(':') {
                // Full spec: "session:window"
                let parts: Vec<&str> = w.split(':').collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    return Err(format!("Invalid window specification: {}", w).into());
                }
            } else {
                // Short form: just window name, use otto session
                (OTTO_SESSION_NAME.to_string(), w)
            }
        }
    };

    // Check if session exists
    match session_exists(&session) {
        Ok(true) => {}
        Ok(false) => {
            return Err(format!(
                "Session '{}' does not exist. Start otto with 'otto start'",
                session
            )
            .into());
        }
        Err(e) => {
            return Err(format!("Failed to check session: {}", e).into());
        }
    }

    // Check if window exists
    match list_windows(&session) {
        Ok(windows) => {
            if !windows.contains(&window_name) {
                print_error(&format!("Window '{}' does not exist in session '{}'", window_name, session));
                println!("\nAvailable windows:");
                for w in windows {
                    println!("  - {}", w);
                }
                println!("\nUsage:");
                println!("  otto attach              Attach to 'otto' window");
                println!("  otto attach <window>     Attach to specific window (e.g., 'ralph-willow')");
                println!("  otto attach otto:<win>   Attach with full spec");
                return Err("Window not found".into());
            }
        }
        Err(e) => {
            return Err(format!("Failed to list windows: {}", e).into());
        }
    }

    // Attach to the window (this replaces the otto process)
    tmux_attach(&session, &window_name)?;

    // This line is never reached because exec replaces the process
    Ok(())
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
                    Ok((duration, window_name)) => {
                        println!(
                            "Agent finished in {} (duration: {})",
                            window_name,
                            format_duration(duration)
                        );
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
    // Start the stuck window monitoring thread
    let _monitor_handle = start_stuck_window_monitor();

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
                    Ok((duration, window_name)) => {
                        println!(
                            "Agent finished in {} (duration: {})",
                            window_name,
                            format_duration(duration)
                        );
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
