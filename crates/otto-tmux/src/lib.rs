//! Tmux session management for Otto
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
    /// Pane process query failed
    PaneProcessQueryFailed(String),
    /// Invalid pane specification
    InvalidPaneSpec(String),
}

impl std::fmt::Display for TmuxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TmuxError::TmuxNotAvailable => write!(f, "tmux command not found - please install tmux"),
            TmuxError::SessionCreationFailed(msg) => write!(f, "failed to create tmux session: {}", msg),
            TmuxError::CommandExecutionFailed(msg) => write!(f, "failed to execute command in tmux: {}", msg),
            TmuxError::SessionCheckFailed(msg) => write!(f, "failed to check tmux session: {}", msg),
            TmuxError::PaneProcessQueryFailed(msg) => write!(f, "failed to query pane process: {}", msg),
            TmuxError::InvalidPaneSpec(msg) => write!(f, "invalid pane specification: {}", msg),
        }
    }
}

impl std::error::Error for TmuxError {}

/// Result type for tmux operations.
pub type TmuxResult<T> = Result<T, TmuxError>;

/// Default name for the Otto tmux session.
pub const OTTO_SESSION_NAME: &str = "otto";

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

/// Ensures the default Otto session exists.
///
/// Convenience function that uses `OTTO_SESSION_NAME`.
///
/// # Returns
/// - `Ok(())` if the session exists or was created successfully
/// - `Err` if there was an error
pub fn ensure_otto_session() -> TmuxResult<()> {
    ensure_session(OTTO_SESSION_NAME)
}

/// Executes a command in the default Otto session.
///
/// Convenience function that uses `OTTO_SESSION_NAME`.
///
/// # Arguments
/// * `command` - The command to execute
///
/// # Returns
/// - `Ok(())` if the command was sent successfully
/// - `Err` if there was an error
pub fn send_otto_command(command: &str) -> TmuxResult<()> {
    send_command(OTTO_SESSION_NAME, command)
}

/// Gets the process ID (PID) of the running process in a tmux pane.
///
/// This function queries tmux to get the PID of the foreground process
/// running in the specified pane.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:0.0" for session otto, window 0, pane 0)
///
/// # Returns
/// - `Ok(Some(u32))` containing the PID if a process is running
/// - `Ok(None)` if the pane exists but no process is running
/// - `Err(TmuxError::PaneProcessQueryFailed)` if the query fails
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
///
/// # Example
/// ```rust
/// use otto_tmux::get_pane_pid;
///
/// match get_pane_pid("otto:0.0") {
///     Ok(Some(pid)) => println!("Process {} is running", pid),
///     Ok(None) => println!("No process running in pane"),
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// ```
pub fn get_pane_pid(pane_spec: &str) -> TmuxResult<Option<u32>> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    // Validate pane spec format (basic check)
    if !pane_spec.contains(':') || !pane_spec.contains('.') {
        return Err(TmuxError::InvalidPaneSpec(pane_spec.to_string()));
    }

    // Use tmux to get the pane's PID
    // The format "##{pane_pid}" is a tmux format string that returns the PID
    let output = Command::new("tmux")
        .args(["display-message", "-t", pane_spec, "-p", "#{pane_pid}"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

            // If tmux returned an empty string or error, the pane might not exist
            if pid_str.is_empty() {
                return Ok(None);
            }

            // Parse the PID
            pid_str.parse::<u32>()
                .map(Some)
                .map_err(|_| TmuxError::PaneProcessQueryFailed(format!("invalid PID: {}", pid_str)))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If the pane doesn't exist, return None
            if stderr.contains("can't find pane") || stderr.contains("no such pane") {
                return Ok(None);
            }
            Err(TmuxError::PaneProcessQueryFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::PaneProcessQueryFailed(e.to_string())),
    }
}

/// Gets the command line of the running process in a tmux pane.
///
/// This function uses the pane's PID to query the process command line
/// from /proc, which is useful for detecting what program is actually running.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:0.0")
///
/// # Returns
/// - `Ok(Some(String))` containing the command line if a process is running
/// - `Ok(None)` if the pane exists but no process is running
/// - `Err` if there was an error
///
/// # Platform Support
/// This function only works on Linux/Unix systems with /proc filesystem.
/// On other platforms, it will return an error.
pub fn get_pane_command(pane_spec: &str) -> TmuxResult<Option<String>> {
    let pid = match get_pane_pid(pane_spec)? {
        Some(pid) => pid,
        None => return Ok(None),
    };

    // Read /proc/<pid>/cmdline to get the command line
    let cmdline_path = format!("/proc/{}/cmdline", pid);

    match std::fs::read_to_string(&cmdline_path) {
        Ok(cmdline) => {
            // cmdline contains null-separated arguments; convert to spaces
            let command = cmdline.replace('\0', " ").trim().to_string();
            Ok(if command.is_empty() { None } else { Some(command) })
        }
        Err(_) => {
            // Process might have exited or /proc not available
            Ok(None)
        }
    }
}

/// Checks if a specific process name is running in a tmux pane.
///
/// This is a convenience function that combines getting the pane command
/// and checking if it contains a specific process name.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:0.0")
/// * `process_name` - The name of the process to check for (e.g., "claude")
///
/// # Returns
/// - `Ok(true)` if the specified process is running in the pane
/// - `Ok(false)` if the process is not running or pane is empty
/// - `Err` if there was an error querying the pane
///
/// # Example
/// ```rust
/// use otto_tmux::is_process_in_pane;
///
/// if is_process_in_pane("otto:0.0", "claude").unwrap_or(false) {
///     println!("Claude is running in the pane");
/// }
/// ```
pub fn is_process_in_pane(pane_spec: &str, process_name: &str) -> TmuxResult<bool> {
    match get_pane_command(pane_spec)? {
        Some(command) => {
            // Check if the command line contains the process name
            // This handles cases like "/usr/bin/claude" or "claude --arg"
            Ok(command.contains(process_name))
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_name_constant() {
        assert_eq!(OTTO_SESSION_NAME, "otto");
    }

    #[test]
    fn test_is_tmux_available_returns_bool() {
        // This test just verifies the function runs and returns a bool
        let _available = is_tmux_available();
    }

    #[test]
    fn test_invalid_pane_spec_rejected() {
        // Test that invalid pane specs are rejected
        let result = get_pane_pid("invalid-spec");
        assert!(matches!(result, Err(TmuxError::InvalidPaneSpec(_))));

        let result2 = get_pane_pid("nocolon");
        assert!(matches!(result2, Err(TmuxError::InvalidPaneSpec(_))));
    }

    #[test]
    fn test_pane_spec_format_validation() {
        // Test valid formats contain : and .
        assert!("otto:0.0".contains(':') && "otto:0.0".contains('.'));
        assert!("my-session:1.2".contains(':') && "my-session:1.2".contains('.'));
    }

    #[test]
    fn test_process_name_check_function_exists() {
        // Just verify the function signature compiles
        // We can't test actual behavior without a running tmux session
        let _ = is_process_in_pane as fn(&str, &str) -> TmuxResult<bool>;
    }
}
