//! Claude Code CLI interactions for Otto
//!
//! This crate provides functionality for interacting with the Claude Code CLI,
//! including availability detection, version checking, process monitoring, and
//! command construction.

use std::process::Command;

/// Error type for Claude operations.
#[derive(Debug)]
pub enum ClaudeError {
    /// Claude Code CLI is not available
    ClaudeNotAvailable,
    /// Failed to get Claude version
    VersionError(String),
    /// Claude process failed to start
    ClaudeStartFailed(String),
    /// Claude did not exit in time
    ClaudeTimeout,
    /// Claude execution failed at runtime
    ClaudeExecutionFailed(String),
}

impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClaudeError::ClaudeNotAvailable => {
                write!(f, "claude command not found - please install Claude Code CLI")
            }
            ClaudeError::VersionError(msg) => write!(f, "failed to get claude version: {}", msg),
            ClaudeError::ClaudeStartFailed(msg) => write!(f, "failed to start claude: {}", msg),
            ClaudeError::ClaudeTimeout => write!(f, "claude did not exit in expected time"),
            ClaudeError::ClaudeExecutionFailed(msg) => write!(f, "claude execution failed: {}", msg),
        }
    }
}

impl std::error::Error for ClaudeError {}

/// Result type for Claude operations.
pub type ClaudeResult<T> = Result<T, ClaudeError>;

/// The default prompt used for all Claude Code agents launched by Otto.
///
/// This prompt directs the agent to:
/// 1. Check for ready beads tasks
/// 2. Choose one task
/// 3. Work only on that task
/// 4. Exit when done
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. Exit when done.";

/// Checks if Claude Code CLI is available.
///
/// # Returns
/// - `true` if the `claude` command is found in PATH
/// - `false` otherwise
pub fn is_claude_available() -> bool {
    Command::new("claude")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Gets the Claude Code CLI version string.
///
/// # Returns
/// - `Ok(String)` containing the version output
/// - `Err(ClaudeError::VersionError)` if version cannot be retrieved
pub fn get_claude_version() -> ClaudeResult<String> {
    let output = Command::new("claude")
        .arg("--version")
        .output()
        .map_err(|e| ClaudeError::VersionError(format!("failed to execute: {}", e)))?;

    if !output.status.success() {
        return Err(ClaudeError::VersionError(
            "claude --version returned non-zero exit code".to_string(),
        ));
    }

    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|e| ClaudeError::VersionError(format!("failed to parse output: {}", e)))
}

/// Checks if Claude Code CLI is currently running.
///
/// Uses `pgrep` to check for any running claude processes.
///
/// # Returns
/// - `true` if at least one claude process is running
/// - `false` otherwise
pub fn is_claude_running() -> bool {
    Command::new("pgrep")
        .arg("-f")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Waits for Claude to exit within the specified timeout.
///
/// Polls every 2 seconds to check if claude is still running.
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait in seconds
///
/// # Returns
/// - `Ok(())` if claude has exited
/// - `Err(ClaudeError::ClaudeTimeout)` if timeout is reached
pub fn wait_for_claude_exit(timeout_secs: u64) -> ClaudeResult<()> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_claude_running() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(ClaudeError::ClaudeTimeout)
}

/// Builds a Claude agent command from a prompt string.
///
/// Constructs a claude command with the given prompt, including the
/// `--dangerously-skip-permissions` flag and proper shell escaping.
///
/// # Arguments
/// * `prompt` - The prompt string to pass to claude
///
/// # Returns
/// A shell command string that can be executed
pub fn build_agent_prompt(prompt: &str) -> String {
    format!("claude --dangerously-skip-permissions \"{}\"", prompt)
}

/// Reads a prompt from a file, or returns the default prompt.
///
/// # Arguments
/// * `prompt_file` - Optional path to a file containing the custom prompt
///
/// # Returns
/// - `Ok(String)` containing the prompt (from file or default)
/// - `Err` if the file cannot be read
pub fn get_prompt(prompt_file: Option<&str>) -> Result<String, std::io::Error> {
    match prompt_file {
        Some(path) => std::fs::read_to_string(path).map(|s| s.trim().to_string()),
        None => Ok(OTTO_AGENT_PROMPT.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otto_agent_prompt_constant() {
        assert!(OTTO_AGENT_PROMPT.contains("bd ready"));
        assert!(OTTO_AGENT_PROMPT.contains("Exit when done"));
    }

    #[test]
    fn test_build_agent_prompt() {
        let cmd = build_agent_prompt("test prompt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test prompt"));
    }

    #[test]
    fn test_get_prompt_default() {
        let prompt = get_prompt(None).unwrap();
        assert_eq!(prompt, OTTO_AGENT_PROMPT);
    }

    #[test]
    fn test_get_prompt_file_not_found() {
        let result = get_prompt(Some("/nonexistent/file.txt"));
        assert!(result.is_err());
    }
}
