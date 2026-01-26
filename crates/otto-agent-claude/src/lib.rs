//! Claude Code CLI interactions for Otto
//!
//! This crate provides functionality for interacting with the Claude Code CLI,
//! including availability detection, version checking, process monitoring, and
//! command construction.

use std::process::Command;

/// Shell-escapes a string for safe use in shell commands.
fn escape_shell_arg(s: &str) -> String {
    // Simple POSIX shell escaping: wrap in single quotes and escape single quotes
    format!("'{}'", s.replace('\'', "'\\''"))
}

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
/// 4. Output the task completion marker when done
/// 5. Exit after completing the task
///
/// The agent attempts to exit after outputting <PLANE-HAS-LANDED>.
/// The stop hook then allows the exit by detecting the marker.
/// This enables interactive mode while ensuring clean exit after task completion.
pub const OTTO_AGENT_PROMPT: &str =
    "Run bd ready, choose a bead, begin work on only that bead. When done, output <PLANE-HAS-LANDED> and then exit. Land the plane.";

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
        .arg("-x")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Checks if a specific process ID is a Claude process.
///
/// This function validates whether a given PID corresponds to a running
/// Claude Code CLI process by checking the process command line.
///
/// # Arguments
/// * `pid` - The process ID to check
///
/// # Returns
/// - `true` if the PID exists and is a claude process
/// - `false` otherwise
///
/// # Example
/// ```rust
/// use otto_agent_claude::is_claude_process;
///
/// if is_claude_process(12345) {
///     println!("PID 12345 is a Claude process");
/// }
/// ```
pub fn is_claude_process(pid: u32) -> bool {
    // Read /proc/<pid>/cmdline to get the command line
    let cmdline_path = format!("/proc/{}/cmdline", pid);

    match std::fs::read_to_string(&cmdline_path) {
        Ok(cmdline) => {
            // cmdline contains null-separated arguments; convert to spaces
            let command = cmdline.replace('\0', " ");
            // Check if "claude" appears in the command
            command.contains("claude")
        }
        Err(_) => false,
    }
}

/// Callback type for progress updates during agent wait.
///
/// The callback receives the elapsed duration as a parameter.
pub type ProgressCallback = fn(std::time::Duration);

/// Callback type for abort checking during agent wait.
///
/// The callback returns true if the wait should be aborted.
pub type AbortCallback = fn() -> bool;

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
    wait_for_claude_exit_with_progress(timeout_secs, None, None)
}

/// Waits for Claude to exit within the specified timeout, with optional progress updates.
///
/// Polls every 2 seconds to check if claude is still running.
/// If a progress callback is provided, it will be called every 2 seconds with the elapsed time.
/// If an abort callback is provided and returns true, claude will be killed and the function returns Ok.
///
/// # Arguments
/// * `timeout_secs` - Maximum time to wait in seconds
/// * `progress_callback` - Optional callback function for progress updates
/// * `abort_callback` - Optional callback function that returns true if wait should be aborted
///
/// # Returns
/// - `Ok(())` if claude has exited (or was aborted via callback)
/// - `Err(ClaudeError::ClaudeTimeout)` if timeout is reached
pub fn wait_for_claude_exit_with_progress(
    timeout_secs: u64,
    progress_callback: Option<ProgressCallback>,
    abort_callback: Option<AbortCallback>,
) -> ClaudeResult<()> {
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if !is_claude_running() {
            return Ok(());
        }

        // Check if abort is requested
        if let Some(callback) = abort_callback {
            if callback() {
                // Abort requested, kill claude and wait for it to exit
                kill_claude();
                // Wait for claude to actually exit (give it up to 5 seconds)
                let kill_timeout = std::time::Duration::from_secs(5);
                let kill_start = std::time::Instant::now();
                while kill_start.elapsed() < kill_timeout {
                    if !is_claude_running() {
                        return Ok(());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                // If claude is still running after kill_timeout, return anyway
                return Ok(());
            }
        }

        // Call progress callback if provided
        if let Some(callback) = progress_callback {
            callback(start.elapsed());
        }

        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(ClaudeError::ClaudeTimeout)
}

/// Builds a Claude agent command from a prompt string.
///
/// Constructs a claude command with the given prompt, including:
/// - `--dangerously-skip-permissions` for automated execution
///
/// The agent runs in interactive mode and exits when the stop hook detects
/// the <PLANE-HAS-LANDED> marker in the output. This provides full
/// interactivity while ensuring clean exit after task completion.
///
/// # Arguments
/// * `prompt` - The prompt string to pass to claude
///
/// # Returns
/// A shell command string that can be executed
pub fn build_agent_prompt(prompt: &str) -> String {
    // Stop hook handles exit via <PLANE-HAS-LANDED> marker detection
    format!(
        "claude --dangerously-skip-permissions {}",
        escape_shell_arg(prompt)
    )
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

/// Kills all running Claude processes.
///
/// Uses `pkill` to terminate all claude processes immediately.
/// This is a forceful termination intended for emergency shutdown scenarios.
///
/// # Returns
/// - `true` if any claude processes were killed
/// - `false` if no claude processes were running
///
/// # Example
/// ```rust
/// use otto_agent_claude::kill_claude;
///
/// if kill_claude() {
///     println!("Killed running Claude agent");
/// }
/// ```
pub fn kill_claude() -> bool {
    Command::new("pkill")
        .arg("-x")
        .arg("claude")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otto_agent_prompt_constant() {
        assert!(OTTO_AGENT_PROMPT.contains("bd ready"));
        assert!(OTTO_AGENT_PROMPT.contains("<PLANE-HAS-LANDED>"));
    }

    #[test]
    fn test_build_agent_prompt() {
        let cmd = build_agent_prompt("test prompt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(!cmd.contains("--print"));  // Should NOT contain --print
        assert!(!cmd.contains("--output-format"));  // Should NOT contain output-format
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

    // Tests for escape_shell_arg
    #[test]
    fn test_escape_shell_arg_simple() {
        assert_eq!(escape_shell_arg("hello"), "'hello'");
    }

    #[test]
    fn test_escape_shell_arg_with_spaces() {
        assert_eq!(escape_shell_arg("hello world"), "'hello world'");
    }

    #[test]
    fn test_escape_shell_arg_with_single_quote() {
        assert_eq!(escape_shell_arg("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_escape_shell_arg_with_multiple_quotes() {
        assert_eq!(escape_shell_arg("it's a test"), "'it'\\''s a test'");
    }

    #[test]
    fn test_escape_shell_arg_empty() {
        assert_eq!(escape_shell_arg(""), "''");
    }

    #[test]
    fn test_escape_shell_arg_with_special_chars() {
        assert_eq!(escape_shell_arg("hello$world"), "'hello$world'");
        assert_eq!(escape_shell_arg("hello;world"), "'hello;world'");
    }

    // Tests for ClaudeError Display
    #[test]
    fn test_claude_error_display_not_available() {
        let err = ClaudeError::ClaudeNotAvailable;
        let msg = format!("{}", err);
        assert!(msg.contains("claude command not found"));
        assert!(msg.contains("install Claude Code CLI"));
    }

    #[test]
    fn test_claude_error_display_version_error() {
        let err = ClaudeError::VersionError("test error".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("failed to get claude version"));
        assert!(msg.contains("test error"));
    }

    #[test]
    fn test_claude_error_display_start_failed() {
        let err = ClaudeError::ClaudeStartFailed("start error".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("failed to start claude"));
        assert!(msg.contains("start error"));
    }

    #[test]
    fn test_claude_error_display_timeout() {
        let err = ClaudeError::ClaudeTimeout;
        let msg = format!("{}", err);
        assert!(msg.contains("did not exit in expected time"));
    }

    #[test]
    fn test_claude_error_display_execution_failed() {
        let err = ClaudeError::ClaudeExecutionFailed("exec error".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("claude execution failed"));
        assert!(msg.contains("exec error"));
    }

    // Tests for is_claude_process
    #[test]
    fn test_is_claude_process_nonexistent() {
        // Use a very high PID that likely doesn't exist
        assert!(!is_claude_process(999999999));
    }

    #[test]
    fn test_is_claude_process_zero() {
        // PID 0 doesn't exist
        assert!(!is_claude_process(0));
    }

    #[test]
    fn test_is_claude_process_one() {
        // PID 1 is typically init/systemd, not claude
        // The function should handle this gracefully
        let result = is_claude_process(1);
        // Just ensure it doesn't panic and returns a boolean
        let _ = result;
    }

    // Tests for build_agent_prompt edge cases
    #[test]
    fn test_build_agent_prompt_with_quotes() {
        let cmd = build_agent_prompt("test's prompt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test"));
    }

    #[test]
    fn test_build_agent_prompt_empty() {
        let cmd = build_agent_prompt("");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_newlines() {
        let cmd = build_agent_prompt("line1\nline2");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("line1"));
    }

    // Tests for get_prompt with temp file
    #[test]
    fn test_get_prompt_from_file() {
        // Create a temporary file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt.txt");
        let prompt_content = "custom prompt from file\n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        // Should trim whitespace
        assert_eq!(result.unwrap(), "custom prompt from file");

        // Cleanup
        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_with_whitespace() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-ws.txt");
        let prompt_content = "  prompt with spaces  \n  ";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "prompt with spaces");

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_empty_file() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-empty.txt");

        std::fs::write(&file_path, "").unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        std::fs::remove_file(&file_path).ok();
    }
}
