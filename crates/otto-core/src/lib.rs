//! Core functionality for Otto
//!
//! Provides the main agent launching logic for spawning Claude Code agents
//! within tmux sessions.

pub mod color;

use crate::color::print_progress;
use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};
use otto_agent_claude::{
    build_agent_prompt, get_prompt, is_claude_available, is_claude_process,
    wait_for_claude_exit_with_progress, ClaudeError,
};

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
/// 2. Sends the claude command with the fixed prompt to the session
/// 3. Waits for the agent to complete by checking if the process is still running
/// 4. Shows progress on stderr while waiting (continuously rewritten line with elapsed time)
/// 5. Tracks and returns the session duration
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait for agent completion (None for default)
/// * `prompt_file` - Optional path to a file containing the custom prompt
///
/// # Returns
/// - `Ok(duration)` if the agent completed successfully, where duration is the session length
/// - `Err(AgentError::ClaudeNotAvailable)` if claude is not installed
/// - `Err(AgentError::TmuxError)` if tmux operations fail
/// - `Err(AgentError::AgentStartFailed)` if agent fails to start
/// - `Err(AgentError::AgentTimeout)` if agent doesn't exit in time
/// - `Err(AgentError::PromptFileError)` if prompt file cannot be read
///
pub fn launch_agent(timeout_secs: Option<u64>, prompt_file: Option<&str>) -> AgentResult<std::time::Duration> {
    let session_start = std::time::Instant::now();

    if !is_claude_available() {
        return Err(AgentError::ClaudeNotAvailable);
    }

    // Ensure the otto tmux session exists
    ensure_otto_session()?;

    // Get the prompt from file or use default
    let prompt = get_prompt(prompt_file)
        .map_err(|e| AgentError::PromptFileError(prompt_file.unwrap_or("default").to_string(), e))?;

    // Construct the command to run claude with the prompt
    let claude_command = build_agent_prompt(&prompt);

    // Send the command to the tmux session
    send_otto_command(&claude_command)?;

    // Wait for the agent to complete with progress callback
    let timeout = timeout_secs.unwrap_or(DEFAULT_AGENT_TIMEOUT_SECS);

    // Define progress callback that updates stderr with elapsed time
    let progress_callback = |elapsed: std::time::Duration| {
        eprint!("\r");
        print_progress(&format!("Agent working... ({})", format_duration(elapsed)));
        // Note: print_progress doesn't add newline, so the carriage return above
        // ensures we overwrite the previous line
    };

    wait_for_claude_exit_with_progress(timeout, Some(progress_callback))?;

    // Clear the progress line when done
    eprint!("\r{}\r", " ".repeat(80));

    let duration = session_start.elapsed();
    Ok(duration)
}

/// Launches a Claude Code agent with the default timeout and optional prompt file.
///
/// Convenience function that uses the default 30-minute timeout.
///
/// # Arguments
/// * `prompt_file` - Optional path to a file containing the custom prompt
///
/// # Returns
/// - `Ok(duration)` if the agent completed successfully, where duration is the session length
/// - `Err` if there was an error
pub fn launch_agent_default(prompt_file: Option<&str>) -> AgentResult<std::time::Duration> {
    launch_agent(None, prompt_file)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeout() {
        assert_eq!(DEFAULT_AGENT_TIMEOUT_SECS, 1800);
    }
}
