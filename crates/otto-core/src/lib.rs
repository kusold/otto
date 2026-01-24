//! Core functionality for Otto
//!
//! Provides the main agent launching logic for spawning Claude Code agents
//! within tmux sessions.

use otto_tmux::{ensure_otto_session, send_otto_command, TmuxError};
use otto_agent_claude::{
    build_agent_prompt, get_prompt, is_claude_available, wait_for_claude_exit,
    ClaudeError,
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

/// Default timeout for agent completion (5 minutes).
const DEFAULT_AGENT_TIMEOUT_SECS: u64 = 300;

/// Launches a Claude Code agent within the Otto tmux session.
///
/// This function:
/// 1. Ensures the otto tmux session exists
/// 2. Sends the claude command with the fixed prompt to the session
/// 3. Waits for the agent to complete by checking if the process is still running
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait for agent completion (None for default)
/// * `prompt_file` - Optional path to a file containing the custom prompt
///
/// # Returns
/// - `Ok(())` if the agent completed successfully
/// - `Err(AgentError::ClaudeNotAvailable)` if claude is not installed
/// - `Err(AgentError::TmuxError)` if tmux operations fail
/// - `Err(AgentError::AgentStartFailed)` if agent fails to start
/// - `Err(AgentError::AgentTimeout)` if agent doesn't exit in time
/// - `Err(AgentError::PromptFileError)` if prompt file cannot be read
///
pub fn launch_agent(timeout_secs: Option<u64>, prompt_file: Option<&str>) -> AgentResult<()> {
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

    // Wait for the agent to complete
    let timeout = timeout_secs.unwrap_or(DEFAULT_AGENT_TIMEOUT_SECS);
    wait_for_claude_exit(timeout)?;

    Ok(())
}

/// Launches a Claude Code agent with the default timeout and optional prompt file.
///
/// Convenience function that uses the default 5-minute timeout.
///
/// # Arguments
/// * `prompt_file` - Optional path to a file containing the custom prompt
///
/// # Returns
/// - `Ok(())` if the agent completed successfully
/// - `Err` if there was an error
pub fn launch_agent_default(prompt_file: Option<&str>) -> AgentResult<()> {
    launch_agent(None, prompt_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timeout() {
        assert_eq!(DEFAULT_AGENT_TIMEOUT_SECS, 300);
    }
}
