//! Tmux session management for Ralph
//!
//! Provides functionality to create, reuse, and interact with tmux sessions
//! for running Claude Code agents.

use std::process::Command;

/// Error type for tmux operations.
#[derive(Debug)]
pub enum TmuxError {
    /// Tmux is not installed or not available
    TmuxNotAvailable,
    /// Session creation failed
    SessionCreationFailed(String),
    /// Command execution in session failed
    CommandExecutionFailed(String),
    /// Session check failed
    SessionCheckFailed(String),
}

impl std::fmt::Display for TmuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TmuxError::TmuxNotAvailable => write!(f, "tmux command not found - please install tmux"),
            TmuxError::SessionCreationFailed(msg) => write!(f, "failed to create tmux session: {}", msg),
            TmuxError::CommandExecutionFailed(msg) => write!(f, "failed to execute command in tmux: {}", msg),
            TmuxError::SessionCheckFailed(msg) => write!(f, "failed to check tmux session: {}", msg),
        }
    }
}

impl std::error::Error for TmuxError {}

/// Result type for tmux operations.
pub type TmuxResult<T> = Result<T, TmuxError>;

/// Default name for the Ralph tmux session.
pub const RALPH_SESSION_NAME: &str = "ralph";

/// Checks if tmux is available on the system.
fn is_tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Checks if a tmux session with the given name exists.
///
/// # Arguments
/// * `session_name` - The name of the session to check
///
/// # Returns
/// - `Ok(true)` if the session exists
/// - `Ok(false)` if the session does not exist
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCheckFailed)` if the check command fails
pub fn session_exists(session_name: &str) -> TmuxResult<bool> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let output = Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output();

    match output {
        Ok(output) => {
            // Exit code 0 means session exists, 1 means it doesn't
            Ok(output.status.success())
        }
        Err(e) => Err(TmuxError::SessionCheckFailed(e.to_string())),
    }
}

/// Creates a new tmux session with the given name.
///
/// # Arguments
/// * `session_name` - The name for the new session
///
/// # Returns
/// - `Ok(())` if the session was created successfully
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCreationFailed)` if creation fails
pub fn create_session(session_name: &str) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let output = Command::new("tmux")
        .args(["new-session", "-d", "-s", session_name])
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(TmuxError::SessionCreationFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::SessionCreationFailed(e.to_string())),
    }
}

/// Ensures a tmux session exists, creating it if necessary.
///
/// This is the main function to use when you need a session - it will
/// create a new one if it doesn't exist, or return success if it already exists.
///
/// # Arguments
/// * `session_name` - The name of the session to ensure exists
///
/// # Returns
/// - `Ok(())` if the session exists or was created successfully
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCreationFailed)` if creation fails
pub fn ensure_session(session_name: &str) -> TmuxResult<()> {
    match session_exists(session_name)? {
        true => Ok(()),
        false => create_session(session_name),
    }
}

/// Executes a command within the tmux session.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `command` - The command to execute
///
/// # Returns
/// - `Ok(())` if the command was sent successfully
/// - `Err(TmuxError::CommandExecutionFailed)` if sending the command fails
pub fn send_command(session_name: &str, command: &str) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let output = Command::new("tmux")
        .args(["send-keys", "-t", session_name, command, "C-m"])
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(TmuxError::CommandExecutionFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::CommandExecutionFailed(e.to_string())),
    }
}

/// Ensures the default Ralph session exists.
///
/// Convenience function that uses `RALPH_SESSION_NAME`.
///
/// # Returns
/// - `Ok(())` if the session exists or was created successfully
/// - `Err` if there was an error
pub fn ensure_ralph_session() -> TmuxResult<()> {
    ensure_session(RALPH_SESSION_NAME)
}

/// Executes a command in the default Ralph session.
///
/// Convenience function that uses `RALPH_SESSION_NAME`.
///
/// # Arguments
/// * `command` - The command to execute
///
/// # Returns
/// - `Ok(())` if the command was sent successfully
/// - `Err` if there was an error
pub fn send_ralph_command(command: &str) -> TmuxResult<()> {
    send_command(RALPH_SESSION_NAME, command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_name_constant() {
        assert_eq!(RALPH_SESSION_NAME, "ralph");
    }

    #[test]
    fn test_is_tmux_available_returns_bool() {
        // This test just verifies the function runs and returns a bool
        let _available = is_tmux_available();
    }
}
