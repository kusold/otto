//! Core functionality for Otto
//!
//! Provides the main agent launching logic for spawning Claude Code agents
//! within tmux sessions.

pub mod color;

use crate::color::print_progress;
use otto_tmux::{
    capture_pane, create_agent_window, ensure_otto_session, find_idle_ralph_window, get_pane_pid,
    get_pane_spec, kill_window, list_windows_by_pattern, send_command_to_window, AGENT_WINDOW_PREFIX,
    OTTO_SESSION_NAME, TmuxError,
};
use otto_agent_claude::{
    build_agent_prompt, get_prompt, is_claude_available, is_claude_process, AbortCallback,
    ClaudeError,
};
use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::Duration;

/// Error type for agent operations.
#[derive(Debug)]
pub enum AgentError {
    /// Claude Code CLI is not available
    ClaudeNotAvailable,
    /// Tmux operation failed
    TmuxError(TmuxError),
    /// Agent failed to start
    AgentStartFailed(String),
    /// Agent did not exit in time
    AgentTimeout,
    /// Failed to read prompt file
    PromptFileError(String, std::io::Error),
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::ClaudeNotAvailable => write!(f, "claude command not found - please install Claude Code CLI"),
            AgentError::TmuxError(e) => write!(f, "tmux error: {}", e),
            AgentError::AgentStartFailed(msg) => write!(f, "failed to start agent: {}", msg),
            AgentError::AgentTimeout => write!(f, "agent did not exit in expected time"),
            AgentError::PromptFileError(path, e) => write!(f, "failed to read prompt file '{}': {}", path, e),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<TmuxError> for AgentError {
    fn from(err: TmuxError) -> Self {
        AgentError::TmuxError(err)
    }
}

impl From<ClaudeError> for AgentError {
    fn from(err: ClaudeError) -> Self {
        match err {
            ClaudeError::ClaudeNotAvailable => AgentError::ClaudeNotAvailable,
            ClaudeError::ClaudeTimeout => AgentError::AgentTimeout,
            ClaudeError::ClaudeStartFailed(msg) => AgentError::AgentStartFailed(msg),
            ClaudeError::ClaudeExecutionFailed(msg) => AgentError::AgentStartFailed(msg),
            ClaudeError::VersionError(msg) => AgentError::AgentStartFailed(msg),
        }
    }
}

/// Result type for agent operations.
pub type AgentResult<T> = Result<T, AgentError>;

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

/// Default timeout for agent completion (30 minutes).
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 1800;

/// Launches a Claude Code agent within the Otto tmux session.
///
/// This function:
/// 1. Ensures the otto tmux session exists
/// 2. Creates a new tmux window with a unique 'ralph-*' name
/// 3. Sends the claude command with the fixed prompt to that window
/// 4. Waits for the agent to complete by checking the specific pane
/// 5. Shows progress on stderr while waiting (continuously rewritten line with elapsed time)
/// 6. Tracks and returns the session duration and window name
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait for agent completion (None for default)
/// * `prompt_file` - Optional path to a file containing the custom prompt
/// * `abort_callback` - Optional callback that returns true if agent should be aborted
///
/// # Returns
/// - `Ok((duration, window_name))` if the agent completed successfully
/// - `Err(AgentError::ClaudeNotAvailable)` if claude is not installed
/// - `Err(AgentError::TmuxError)` if tmux operations fail
/// - `Err(AgentError::AgentStartFailed)` if agent fails to start
/// - `Err(AgentError::AgentTimeout)` if agent doesn't exit in time
/// - `Err(AgentError::PromptFileError)` if prompt file cannot be read
///
pub fn launch_agent(
    timeout_secs: Option<u64>,
    prompt_file: Option<&str>,
    abort_callback: Option<AbortCallback>,
) -> AgentResult<(std::time::Duration, String)> {
    let session_start = std::time::Instant::now();

    if !is_claude_available() {
        return Err(AgentError::ClaudeNotAvailable);
    }

    // Ensure the otto tmux session exists
    ensure_otto_session()?;

    // Create a new window with a unique agent name
    let window_name = create_agent_window(otto_tmux::OTTO_SESSION_NAME)?;

    // Get the prompt from file or use default
    let prompt = get_prompt(prompt_file)
        .map_err(|e| AgentError::PromptFileError(prompt_file.unwrap_or("default").to_string(), e))?;

    // Construct the command to run claude with the prompt
    let claude_command = build_agent_prompt(&prompt);

    // Send the command to the specific window
    send_command_to_window(otto_tmux::OTTO_SESSION_NAME, &window_name, &claude_command)?;

    // Construct the pane spec for monitoring
    // We need to find the pane number, but tmux will default to pane 0 for new windows
    // Format: "session:window.pane" -> "otto:ralph-word.0"
    let pane_spec = format!("{}:{}.0", otto_tmux::OTTO_SESSION_NAME, window_name);

    // Wait for the agent to complete with progress callback
    let timeout = timeout_secs.unwrap_or(DEFAULT_AGENT_TIMEOUT_SECS);

    // Clone window_name for use in the progress callback
    let window_name_for_display = window_name.clone();

    // Define progress callback that updates stderr with elapsed time and window name
    let progress_callback: Box<dyn Fn(std::time::Duration)> = Box::new(move |elapsed| {
        eprint!("\r");
        print_progress(&format!(
            "Agent working in {}... ({})",
            window_name_for_display,
            format_duration(elapsed)
        ));
        // Note: print_progress doesn't add newline, so the carriage return above
        // ensures we overwrite the previous line
    });

    // Wait for Claude to exit in the specific pane
    wait_for_claude_in_pane_with_progress(
        &pane_spec,
        timeout,
        Some(progress_callback),
        abort_callback,
    )?;

    // Clear the progress line when done
    eprint!("\r{}\r", " ".repeat(80));

    let duration = session_start.elapsed();
    Ok((duration, window_name))
}

/// Launches a Claude Code agent with the default timeout and optional prompt file.
///
/// Convenience function that uses the default 30-minute timeout.
///
/// # Arguments
/// * `prompt_file` - Optional path to a file containing the custom prompt
/// * `abort_callback` - Optional callback that returns true if agent should be aborted
///
/// # Returns
/// - `Ok((duration, window_name))` if the agent completed successfully
/// - `Err` if there was an error
pub fn launch_agent_default(
    prompt_file: Option<&str>,
    abort_callback: Option<AbortCallback>,
) -> AgentResult<(std::time::Duration, String)> {
    launch_agent(None, prompt_file, abort_callback)
}

/// Checks if Claude is currently active in a specific tmux pane.
///
/// This function combines tmux pane process tracking with Claude process
/// validation to reliably determine if a Claude Code agent is running
/// in the specified pane.
///
/// It works by:
/// 1. Querying tmux for the PID of the process in the pane
/// 2. Validating that the PID corresponds to a Claude process
///
/// This two-step approach ensures we don't get false positives from
/// other processes that might be running in the pane.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:0.0" for session otto, window 0, pane 0)
///                 If `None`, uses the default "otto:0.0" pane
///
/// # Returns
/// - `Ok(true)` if Claude is running in the pane
/// - `Ok(false)` if Claude is not running in the pane
/// - `Err(AgentError::TmuxError)` if tmux operations fail
///
/// # Example
/// ```rust
/// use otto_core::is_claude_active_in_pane;
///
/// match is_claude_active_in_pane(Some("otto:0.0")) {
///     Ok(true) => println!("Claude is working"),
///     Ok(false) => println!("Pane is idle or running something else"),
///     Err(e) => eprintln!("Error checking pane: {}", e),
/// }
/// ```
///
/// # Notes
/// - This function uses /proc filesystem to read process information
/// - Only works on Linux/Unix systems with /proc support
/// - Returns false if the pane doesn't exist (rather than an error)
/// - Handles multiple Claude instances correctly by checking specific panes
pub fn is_claude_active_in_pane(pane_spec: Option<&str>) -> AgentResult<bool> {
    use otto_tmux::get_pane_pid;

    let pane = pane_spec.unwrap_or("otto:0.0");

    // Get the PID of the process in the pane
    match get_pane_pid(pane)? {
        Some(pid) => {
            // Check if this PID is a Claude process
            Ok(is_claude_process(pid))
        }
        None => {
            // No process running in the pane
            Ok(false)
        }
    }
}

/// Waits for Claude to exit in a specific tmux pane.
///
/// Unlike `wait_for_claude_exit` which checks for any Claude process,
/// this function specifically monitors a tmux pane for Claude activity
/// and waits until Claude is no longer running there.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:0.0")
/// * `timeout_secs` - Maximum time to wait in seconds
///
/// # Returns
/// - `Ok(())` if Claude has exited from the pane
/// - `Err(AgentError::AgentTimeout)` if timeout is reached
pub fn wait_for_claude_in_pane(pane_spec: &str, timeout_secs: u64) -> AgentResult<()> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_claude_active_in_pane(Some(pane_spec))? {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(AgentError::AgentTimeout)
}

/// Waits for Claude to exit in a specific tmux pane, with progress callbacks.
///
/// Unlike `wait_for_claude_in_pane`, this function supports optional progress
/// callbacks and abort checking.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:ralph-word.0")
/// * `timeout_secs` - Maximum time to wait in seconds
/// * `progress_callback` - Optional callback for progress updates (boxed closure)
/// * `abort_callback` - Optional callback that returns true if wait should be aborted
///
/// # Returns
/// - `Ok(())` if Claude has exited from the pane
/// - `Err(AgentError::AgentTimeout)` if timeout is reached
pub fn wait_for_claude_in_pane_with_progress(
    pane_spec: &str,
    timeout_secs: u64,
    progress_callback: Option<Box<dyn Fn(std::time::Duration)>>,
    abort_callback: Option<AbortCallback>,
) -> AgentResult<()> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_claude_active_in_pane(Some(pane_spec))? {
            return Ok(());
        }

        // Check if abort is requested
        if let Some(callback) = abort_callback {
            if callback() {
                // Abort requested - kill Claude process in this pane
                if let Ok(Some(pid)) = otto_tmux::get_pane_pid(pane_spec) {
                    // Kill the specific Claude process
                    let _ = std::process::Command::new("kill")
                        .arg(pid.to_string())
                        .output();
                    // Wait a bit for it to exit
                    let kill_start = std::time::Instant::now();
                    while kill_start.elapsed() < std::time::Duration::from_secs(5) {
                        if !is_claude_active_in_pane(Some(pane_spec))? {
                            return Ok(());
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
                return Ok(());
            }
        }

        // Call progress callback if provided
        if let Some(callback) = &progress_callback {
            callback(start.elapsed());
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(AgentError::AgentTimeout)
}

/// State tracking for a window in the stuck window monitor.
struct WindowState {
    /// Hash of the pane content from the last check
    last_content_hash: Option<String>,
    /// Number of consecutive checks with unchanged content
    unchanged_count: u32,
}

/// Starts the stuck window monitoring thread.
///
/// This spawns a background thread that monitors all 'ralph-*' windows
/// and closes those where Claude is not running or has produced no output.
///
/// # Returns
/// A `JoinHandle` for the monitoring thread
///
/// # Behavior
/// - Every 5 minutes, checks all ralph-* windows
/// - Closes windows where Claude process is not running
/// - Closes windows where content unchanged for 10 minutes (2 checks)
/// - Logs all closures to ~/.otto/watchdog.log
pub fn start_stuck_window_monitor() -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut window_states: HashMap<String, WindowState> = HashMap::new();

        // Log monitoring start
        if let Err(e) = log_watchdog("Monitoring started") {
            eprintln!("Watchdog: failed to log start: {}", e);
        }

        loop {
            std::thread::sleep(Duration::from_secs(300)); // 5 minutes

            if let Err(e) = cleanup_stuck_windows(&mut window_states) {
                eprintln!("Watchdog error: {}", e);
            }
        }
    })
}

/// Cleans up stuck windows by checking process and content.
///
/// This is called periodically by the monitoring thread.
///
/// # Arguments
/// * `states` - Mutable hashmap tracking window states across checks
///
/// # Returns
/// - `Ok(())` if cleanup completed (even if no windows were closed)
/// - `Err(AgentError)` if there was a fatal error
fn cleanup_stuck_windows(states: &mut HashMap<String, WindowState>) -> AgentResult<()> {
    // List all ralph-* windows
    let ralph_windows = match list_windows_by_pattern(OTTO_SESSION_NAME, AGENT_WINDOW_PREFIX) {
        Ok(windows) => windows,
        Err(TmuxError::TmuxNotAvailable) => {
            // Tmux not available, just skip this check
            return Ok(());
        }
        Err(e) => return Err(AgentError::TmuxError(e)),
    };

    for window_name in ralph_windows {
        let pane_spec = get_pane_spec(OTTO_SESSION_NAME, &window_name);

        // Check 1: Is Claude process running?
        let claude_running = match get_pane_pid(&pane_spec) {
            Ok(Some(pid)) => is_claude_process(pid),
            Ok(None) => false, // No process running
            Err(TmuxError::TmuxNotAvailable) => {
                // Tmux not available, skip
                continue;
            }
            Err(_) => false, // Error querying, assume not running
        };

        if !claude_running {
            // Process died, close the window
            log_watchdog(&format!("Closed window {}: claude process not running", window_name))
                .ok();
            kill_window(OTTO_SESSION_NAME, &window_name).ok();
            states.remove(&window_name);
            continue;
        }

        // Check 2: Is content changing?
        match capture_pane(&pane_spec) {
            Ok(content) => {
                // Compute hash of content
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                content.hash(&mut hasher);
                let content_hash = format!("{:x}", hasher.finish());

                // Get or create state for this window
                let state = states.entry(window_name.clone()).or_insert(WindowState {
                    last_content_hash: None,
                    unchanged_count: 0,
                });

                // Check if content changed
                if Some(&content_hash) == state.last_content_hash.as_ref() {
                    // Content unchanged, increment counter
                    state.unchanged_count += 1;

                    // If unchanged for 2 checks (10 minutes), close window
                    if state.unchanged_count >= 2 {
                        log_watchdog(&format!(
                            "Closed window {}: no output for 10 minutes",
                            window_name
                        ))
                        .ok();
                        kill_window(OTTO_SESSION_NAME, &window_name).ok();
                        states.remove(&window_name);
                    }
                } else {
                    // Content changed, reset counter
                    state.last_content_hash = Some(content_hash);
                    state.unchanged_count = 0;
                }
            }
            Err(_) => {
                // Failed to capture pane, skip this check
                // (pane might have been closed)
            }
        }
    }

    Ok(())
}

/// Logs a watchdog message to the log file.
///
/// Logs to ~/.otto/watchdog.log with timestamp.
///
/// # Arguments
/// * `message` - The message to log
fn log_watchdog(message: &str) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;

    // Get home directory
    let home = std::env::var("HOME")
        .map_err(|_| std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "HOME directory not found"
        ))?;

    // Create .otto directory if it doesn't exist
    let otto_dir = std::path::PathBuf::from(home).join(".otto");
    std::fs::create_dir_all(&otto_dir)?;

    // Open log file in append mode
    let log_path = otto_dir.join("watchdog.log");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Write timestamped message
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    writeln!(file, "[{}] {}", timestamp, message)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeout() {
        assert_eq!(DEFAULT_AGENT_TIMEOUT_SECS, 1800);
    }
}
