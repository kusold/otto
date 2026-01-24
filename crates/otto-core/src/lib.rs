//! Core functionality for Otto
//!
//! Provides the main agent launching logic for spawning Claude Code agents
//! within tmux sessions.

use std::process::Command;
use std::time::Duration;
use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};

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
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::ClaudeNotAvailable => write!(f, "claude command not found - please install Claude Code CLI"),
            AgentError::TmuxError(e) => write!(f, "tmux error: {}", e),
            AgentError::AgentStartFailed(msg) => write!(f, "failed to start agent: {}", msg),
            AgentError::AgentTimeout => write!(f, "agent did not exit in expected time"),
        }
    }
}

impl std::error::Error for AgentError {}

impl From<TmuxError> for AgentError {
    fn from(err: TmuxError) -> Self {
        AgentError::TmuxError(err)
    }
}

/// Result type for agent operations.
pub type AgentResult<T> = Result<T, AgentError>;

/// The fixed prompt used for all Claude Code agents launched by Otto.
///
/// This prompt directs the agent to:
/// 1. Check for ready beads tasks
/// 2. Choose one task
/// 3. Work only on that task
/// 4. Exit when done
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. Exit when done.";

/// Default timeout for agent completion (5 minutes).
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 300;

/// Checks if Claude Code CLI is available.
fn is_claude_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Launches a Claude Code agent within the Otto tmux session.
///
/// This function:
/// 1. Ensures the otto tmux session exists
/// 2. Sends the claude command with the fixed prompt to the session
/// 3. Waits for the agent to complete by checking if the process is still running
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait for agent completion (None for default)
///
/// # Returns
/// - `Ok(())` if the agent completed successfully
/// - `Err(AgentError::ClaudeNotAvailable)` if claude is not installed
/// - `Err(AgentError::TmuxError)` if tmux operations fail
/// - `Err(AgentError::AgentStartFailed)` if agent fails to start
/// - `Err(AgentError::AgentTimeout)` if agent doesn't exit in time
///
pub fn launch_agent(timeout_secs: Option<u64>) -> AgentResult<()> {
    if !is_claude_available() {
        return Err(AgentError::ClaudeNotAvailable);
    }

    // Ensure the otto tmux session exists
    ensure_otto_session()?;

    // Construct the command to run claude with the fixed prompt
    let claude_command = format!("claude \"{}\"", OTTO_AGENT_PROMPT);

    // Send the command to the tmux session
    send_otto_command(&claude_command)?;

    // Wait for the agent to complete
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_AGENT_TIMEOUT_SECS));
    let start = std::time::Instant::now();

    // Poll to check if claude process is still running
    while start.elapsed() < timeout {
        // Check if there's a claude process running
        let has_claude = Command::new("pgrep")
            .arg("-f")
            .arg("claude")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !has_claude {
            // Agent has exited
            return Ok(());
        }

        // Wait before checking again
        std::thread::sleep(Duration::from_secs(2));
    }

    Err(AgentError::AgentTimeout)
}

/// Launches a Claude Code agent with the default timeout.
///
/// Convenience function that uses the default 5-minute timeout.
///
/// # Returns
/// - `Ok(())` if the agent completed successfully
/// - `Err` if there was an error
pub fn launch_agent_default() -> AgentResult<()> {
    launch_agent(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_prompt_constant() {
        assert!(OTTO_AGENT_PROMPT.contains("bd ready"));
        assert!(OTTO_AGENT_PROMPT.contains("Exit when done"));
    }

    #[test]
    fn test_default_timeout() {
        assert_eq!(DEFAULT_AGENT_TIMEOUT_SECS, 300);
    }
}
