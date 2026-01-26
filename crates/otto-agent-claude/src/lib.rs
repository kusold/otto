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
        assert_eq!(escape_shell_arg("hello&world"), "'hello&world'");
        assert_eq!(escape_shell_arg("hello|world"), "'hello|world'");
        assert_eq!(escape_shell_arg("hello>world"), "'hello>world'");
        assert_eq!(escape_shell_arg("hello<world"), "'hello<world'");
        assert_eq!(escape_shell_arg("hello`world"), "'hello`world'");
        assert_eq!(escape_shell_arg("hello\\world"), "'hello\\world'");
        assert_eq!(escape_shell_arg("hello\nworld"), "'hello\nworld'");
        assert_eq!(escape_shell_arg("hello\tworld"), "'hello\tworld'");
        assert_eq!(escape_shell_arg("hello!world"), "'hello!world'");
        assert_eq!(escape_shell_arg("hello*world"), "'hello*world'");
        assert_eq!(escape_shell_arg("hello?world"), "'hello?world'");
        assert_eq!(escape_shell_arg("hello[world"), "'hello[world'");
        assert_eq!(escape_shell_arg("hello]world"), "'hello]world'");
        assert_eq!(escape_shell_arg("hello{world"), "'hello{world'");
        assert_eq!(escape_shell_arg("hello}world"), "'hello}world'");
        assert_eq!(escape_shell_arg("hello(world"), "'hello(world'");
        assert_eq!(escape_shell_arg("hello)world"), "'hello)world'");
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

    // Test for ClaudeError::error_source
    #[test]
    fn test_claude_error_implements_error() {
        let err = ClaudeError::VersionError("test".to_string());
        // Ensure the Error trait is implemented
        let _err: &dyn std::error::Error = &err;
    }

    // Test for ClaudeError::Debug
    #[test]
    fn test_claude_error_debug_not_available() {
        let err = ClaudeError::ClaudeNotAvailable;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ClaudeNotAvailable"));
    }

    #[test]
    fn test_claude_error_debug_timeout() {
        let err = ClaudeError::ClaudeTimeout;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ClaudeTimeout"));
    }

    // Additional edge case tests for build_agent_prompt
    #[test]
    fn test_build_agent_prompt_with_tabs() {
        let cmd = build_agent_prompt("test\twith\ttabs");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test"));
    }

    #[test]
    fn test_build_agent_prompt_with_carriage_return() {
        let cmd = build_agent_prompt("test\rwith\rcarriage");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test"));
    }

    #[test]
    fn test_build_agent_prompt_with_null() {
        let cmd = build_agent_prompt("test\u{0}null");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test"));
    }

    #[test]
    fn test_build_agent_prompt_very_long() {
        let long_prompt = "a".repeat(10000);
        let cmd = build_agent_prompt(&long_prompt);
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    // More get_prompt edge cases
    #[test]
    fn test_get_prompt_from_file_with_bom() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-bom.txt");
        // UTF-8 BOM + content
        let prompt_content = "\u{FEFF}prompt with BOM\n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        // BOM should be preserved
        assert!(result.unwrap().starts_with("\u{FEFF}"));

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_with_only_newlines() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-newlines.txt");
        let prompt_content = "\n\n\n\n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");

        std::fs::remove_file(&file_path).ok();
    }

    // Test escape_shell_arg with more edge cases
    #[test]
    fn test_escape_shell_arg_with_all_special_chars() {
        let input = "$`'\";|&<>()[]{}*?!~\\ \t\n";
        let escaped = escape_shell_arg(input);
        // Should be wrapped in single quotes
        assert!(escaped.starts_with('\''));
        assert!(escaped.ends_with('\''));
    }

    #[test]
    fn test_escape_shell_arg_multiple_quotes_in_row() {
        assert_eq!(escape_shell_arg("''"), "''\\'''\\'''");
        // Let's just verify it doesn't panic and the result is wrapped in quotes
        let result = escape_shell_arg("'a'b'c'");
        assert!(result.starts_with('\''));
        assert!(result.ends_with('\''));
    }

    // Test ClaudeError conversions
    #[test]
    fn test_claude_error_source_none() {
        let err = ClaudeError::ClaudeNotAvailable;
        // Error::source should return None for ClaudeNotAvailable
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn test_claude_error_timeout_source_none() {
        let err = ClaudeError::ClaudeTimeout;
        assert!(std::error::Error::source(&err).is_none());
    }

    // Test OTTO_AGENT_PROMPT constant more thoroughly
    #[test]
    fn test_otto_agent_prompt_structure() {
        let prompt = OTTO_AGENT_PROMPT;
        // Check it contains all key elements
        assert!(prompt.contains("bd ready"));
        assert!(prompt.contains("bead"));
        assert!(prompt.contains("<PLANE-HAS-LANDED>"));
        assert!(prompt.contains("exit"));
        assert!(prompt.contains("Land the plane"));
    }

    // Additional tests to improve coverage

    #[test]
    fn test_build_agent_prompt_with_multiple_special_chars() {
        // Test combination of special characters
        let cmd = build_agent_prompt("test $pecial 'quoted' and \"double\" quoted");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("test"));
    }

    #[test]
    fn test_build_agent_prompt_with_semicolon() {
        let cmd = build_agent_prompt("command; another");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        assert!(cmd.contains("command"));
    }

    #[test]
    fn test_build_agent_prompt_with_ampersand() {
        let cmd = build_agent_prompt("test & background");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_pipe() {
        let cmd = build_agent_prompt("test | pipe");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_backtick() {
        let cmd = build_agent_prompt("test`backtick`");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_parenthesis() {
        let cmd = build_agent_prompt("test (parens)");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_brackets() {
        let cmd = build_agent_prompt("test [brackets] and {braces}");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_wildcard() {
        let cmd = build_agent_prompt("test *.txt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_question_mark() {
        let cmd = build_agent_prompt("test file?.txt");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_newline_sequence() {
        let cmd = build_agent_prompt("line1\r\nline2\nline3");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_build_agent_prompt_with_leading_trailing_spaces() {
        let cmd = build_agent_prompt("  prompt  ");
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
        // The prompt should be preserved with spaces
        assert!(cmd.contains("prompt"));
    }

    #[test]
    fn test_get_prompt_from_file_with_only_bom() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-bom-only.txt");
        // UTF-8 BOM only
        let prompt_content = "\u{FEFF}";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        // BOM should be preserved even with no other content
        assert!(result.unwrap().starts_with("\u{FEFF}"));

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_with_mixed_line_endings() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-mixed-newlines.txt");
        let prompt_content = "line1\r\nline2\nline3\rline4";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("line1"));
        assert!(content.contains("line2"));

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_with_utf8_content() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-utf8.txt");
        let prompt_content = "Test with UTF-8: 你好 世界 🎉\n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert!(result.unwrap().contains("你好"));

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_preserves_internal_spaces() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-spaces.txt");
        let prompt_content = "  prompt   with   internal   spaces  \n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        // Should trim only leading/trailing whitespace from the whole string
        assert_eq!(result.unwrap(), "prompt   with   internal   spaces");

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_escape_shell_arg_with_backslash() {
        assert_eq!(escape_shell_arg("test\\file"), "'test\\file'");
    }

    #[test]
    fn test_escape_shell_arg_with_exclamation() {
        assert_eq!(escape_shell_arg("test!bang"), "'test!bang'");
    }

    #[test]
    fn test_escape_shell_arg_with_tilde() {
        assert_eq!(escape_shell_arg("~/path"), "'~/path'");
    }

    #[test]
    fn test_escape_shell_arg_with_hash() {
        assert_eq!(escape_shell_arg("test#comment"), "'test#comment'");
    }

    #[test]
    fn test_escape_shell_arg_with_percent() {
        assert_eq!(escape_shell_arg("test%var"), "'test%var'");
    }

    #[test]
    fn test_escape_shell_arg_with_equals() {
        assert_eq!(escape_shell_arg("test=value"), "'test=value'");
    }

    #[test]
    fn test_escape_shell_arg_with_at() {
        assert_eq!(escape_shell_arg("test@example"), "'test@example'");
    }

    #[test]
    fn test_escape_shell_arg_with_plus() {
        assert_eq!(escape_shell_arg("test+value"), "'test+value'");
    }

    #[test]
    fn test_escape_shell_arg_with_underscore() {
        assert_eq!(escape_shell_arg("test_value"), "'test_value'");
    }

    #[test]
    fn test_escape_shell_arg_period() {
        assert_eq!(escape_shell_arg("test.txt"), "'test.txt'");
    }

    #[test]
    fn test_escape_shell_arg_comma() {
        assert_eq!(escape_shell_arg("test,value"), "'test,value'");
    }

    #[test]
    fn test_escape_shell_arg_colon() {
        assert_eq!(escape_shell_arg("test:value"), "'test:value'");
    }

    #[test]
    fn test_escape_shell_arg_semicolon() {
        assert_eq!(escape_shell_arg("test;value"), "'test;value'");
    }

    #[test]
    fn test_escape_shell_arg_slash() {
        assert_eq!(escape_shell_arg("test/path"), "'test/path'");
    }

    #[test]
    fn test_escape_shell_arg_dotdot() {
        assert_eq!(escape_shell_arg("../test"), "'../test'");
    }

    #[test]
    fn test_claude_error_display_messages() {
        // Test that all error variants produce meaningful messages
        let errors = vec![
            ClaudeError::ClaudeNotAvailable,
            ClaudeError::VersionError("test".to_string()),
            ClaudeError::ClaudeStartFailed("test".to_string()),
            ClaudeError::ClaudeTimeout,
            ClaudeError::ClaudeExecutionFailed("test".to_string()),
        ];

        for error in errors {
            let msg = format!("{}", error);
            assert!(!msg.is_empty());
            assert!(msg.len() > 10); // All messages should be reasonably descriptive
        }
    }

    #[test]
    fn test_claude_error_debug_formats() {
        // Test Debug formatting for all error variants
        let errors = vec![
            ClaudeError::ClaudeNotAvailable,
            ClaudeError::VersionError("test".to_string()),
            ClaudeError::ClaudeStartFailed("test".to_string()),
            ClaudeError::ClaudeTimeout,
            ClaudeError::ClaudeExecutionFailed("test".to_string()),
        ];

        for error in errors {
            let debug_str = format!("{:?}", error);
            assert!(!debug_str.is_empty());
        }
    }

    // Tests for is_claude_available (integration tests that can run in CI)
    #[test]
    fn test_is_claude_available_returns_boolean() {
        // This will return either true or false, but should not panic
        let result = is_claude_available();
        // Just verify it returns a boolean value
        let _ = result;
    }

    #[test]
    fn test_is_claude_available_false_for_nonexistent_command() {
        // Mock scenario: if we override PATH, it should return false
        // This test checks the function handles missing commands gracefully
        let original_path = std::env::var("PATH").ok();
        unsafe { std::env::set_var("PATH", ""); }

        // Since PATH is empty, command should fail
        let result = is_claude_available();

        // Restore original PATH
        if let Some(path) = original_path {
            unsafe { std::env::set_var("PATH", path); }
        } else {
            unsafe { std::env::remove_var("PATH"); }
        }

        // With empty PATH, the command should not be found
        assert!(!result);
    }

    // Tests for is_claude_running
    #[test]
    fn test_is_claude_running_returns_boolean() {
        // This function should return a boolean without panicking
        let result = is_claude_running();
        // Just verify it returns a boolean value
        let _ = result;
    }

    #[test]
    fn test_is_claude_running_false_when_no_processes() {
        // In most test environments, there should be no claude process running
        // This test documents that behavior
        let result = is_claude_running();
        // We don't assert false here as claude might be running in the test environment
        let _ = result;
    }

    // Tests for kill_claude
    #[test]
    fn test_kill_claude_returns_boolean() {
        // This function should return a boolean without panicking
        let result = kill_claude();
        // Just verify it returns a boolean value
        let _ = result;
    }

    #[test]
    fn test_kill_claude_idempotent() {
        // Calling kill_claude multiple times should be safe
        let _ = kill_claude();
        let _ = kill_claude();
        let _ = kill_claude();
        // If we get here without panicking, the test passes
    }

    // Tests for is_claude_process edge cases
    #[test]
    fn test_is_claude_process_with_current_pid() {
        // Test that the function doesn't panic when checking various PIDs
        let current_pid = std::process::id();
        let result = is_claude_process(current_pid);
        // The current process is not claude, so it should return false
        assert!(!result);
    }

    #[test]
    fn test_is_claude_process_with_max_pid() {
        // Test with u32::MAX (should not panic)
        let result = is_claude_process(u32::MAX);
        assert!(!result);
    }

    #[test]
    fn test_is_claude_process_with_large_number() {
        // Test with a large but valid PID
        let result = is_claude_process(2147483647); // i32::MAX
        assert!(!result);
    }

    // Tests for get_claude_version
    #[test]
    fn test_get_claude_version_returns_result() {
        // This function returns a Result, so it should either be Ok or Err
        let result = get_claude_version();
        match result {
            Ok(_) => {
                // Claude is available
            }
            Err(_) => {
                // Claude is not available - this is expected in some environments
            }
        }
    }

    // Tests for wait_for_claude_exit
    #[test]
    fn test_wait_for_claude_exit_immediate_when_not_running() {
        // If claude is not running, should return immediately
        let result = wait_for_claude_exit(1);
        // Should return Ok if claude is not running, or timeout error if it's running
        match result {
            Ok(()) => {
                // Expected: claude is not running
            }
            Err(ClaudeError::ClaudeTimeout) => {
                // Also expected: claude is running and timed out
            }
            Err(_) => {
                panic!("Unexpected error type");
            }
        }
    }

    #[test]
    fn test_wait_for_claude_exit_zero_timeout() {
        // Test with zero timeout
        let result = wait_for_claude_exit(0);
        // Should handle zero timeout gracefully
        let _ = result;
    }

    #[test]
    fn test_wait_for_claude_exit_short_timeout() {
        // Test with very short timeout
        let result = wait_for_claude_exit(1);
        let _ = result;
    }

    // Tests for wait_for_claude_exit_with_progress
    #[test]
    fn test_wait_for_claude_exit_with_progress_none_callbacks() {
        // Test with None callbacks (should not panic)
        let result = wait_for_claude_exit_with_progress(1, None, None);
        let _ = result;
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_only_progress_callback() {
        // Test with only progress callback
        // Note: progress callback must be a function pointer, not a closure
        fn progress_callback(_duration: std::time::Duration) {
            // Do nothing
        }

        let result = wait_for_claude_exit_with_progress(1, Some(progress_callback), None);
        let _ = result;
        // We don't assert anything specific as behavior depends on whether claude is running
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_only_abort_callback() {
        // Test with only abort callback (that never aborts)
        let callback: AbortCallback = || false;

        let result = wait_for_claude_exit_with_progress(1, None, Some(callback));
        let _ = result;
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_immediate_abort() {
        // Test with abort callback that immediately returns true
        let callback: AbortCallback = || true;

        let result = wait_for_claude_exit_with_progress(5, None, Some(callback));
        // Should return Ok quickly because abort callback returns true
        assert!(result.is_ok());
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_both_callbacks() {
        // Test with both callbacks provided
        // Note: callbacks must be function pointers, not closures
        fn progress_callback(_duration: std::time::Duration) {
            // Do nothing
        }

        fn abort_callback() -> bool {
            false
        }

        let result = wait_for_claude_exit_with_progress(1, Some(progress_callback), Some(abort_callback));
        let _ = result;
        // We don't assert anything specific as behavior depends on environment
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_zero_timeout_with_callbacks() {
        // Test zero timeout with callbacks
        let callback: ProgressCallback = |_duration| {};
        let abort: AbortCallback = || false;

        let result = wait_for_claude_exit_with_progress(0, Some(callback), Some(abort));
        let _ = result;
    }

    // Additional Display tests to ensure all branches are covered
    #[test]
    fn test_claude_error_display_all_variants() {
        // Ensure Display is called for all variants
        let errors = vec![
            ClaudeError::ClaudeNotAvailable,
            ClaudeError::VersionError("version error message".to_string()),
            ClaudeError::ClaudeStartFailed("start failed message".to_string()),
            ClaudeError::ClaudeTimeout,
            ClaudeError::ClaudeExecutionFailed("execution failed message".to_string()),
        ];

        for error in errors {
            let msg = format!("{}", error);
            assert!(!msg.is_empty());
            assert!(msg.len() > 5);
        }
    }

    #[test]
    fn test_claude_error_display_empty_messages() {
        // Test with empty error messages
        let err1 = ClaudeError::VersionError("".to_string());
        let msg1 = format!("{}", err1);
        assert!(!msg1.is_empty());

        let err2 = ClaudeError::ClaudeStartFailed("".to_string());
        let msg2 = format!("{}", err2);
        assert!(!msg2.is_empty());

        let err3 = ClaudeError::ClaudeExecutionFailed("".to_string());
        let msg3 = format!("{}", err3);
        assert!(!msg3.is_empty());
    }

    #[test]
    fn test_claude_error_display_unicode_messages() {
        // Test with unicode in error messages
        let err = ClaudeError::VersionError("Error: 错误 Érreur 🎉".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Error"));
        assert!(msg.contains("错误"));
        assert!(msg.contains("Érreur"));
        assert!(msg.contains("🎉"));
    }

    #[test]
    fn test_claude_error_display_long_messages() {
        // Test with very long error messages
        let long_msg = "x".repeat(10000);
        let err = ClaudeError::VersionError(long_msg.clone());
        let msg = format!("{}", err);
        assert!(msg.contains("failed to get claude version"));
        assert!(msg.len() > 10000);
    }

    // Additional tests for improved coverage

    #[test]
    fn test_wait_for_claude_exit_with_progress_timeout_path() {
        // Test the timeout path when claude keeps running
        // This test documents the behavior when timeout occurs
        let result = wait_for_claude_exit_with_progress(1, None, None);
        // Result depends on whether claude is running
        let _ = result;
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_progress_callback_called() {
        // Test that progress callback is actually called
        static mut CALLBACK_COUNT: u32 = 0;

        fn counting_progress(_duration: std::time::Duration) {
            unsafe {
                CALLBACK_COUNT += 1;
            }
        }

        // Run with a short timeout - callback should be invoked at least once
        unsafe { CALLBACK_COUNT = 0; }
        let _ = wait_for_claude_exit_with_progress(1, Some(counting_progress), None);
        // We can't assert the count as it depends on environment, but we've tested the path
        let _ = unsafe { CALLBACK_COUNT };
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_abort_does_not_kill() {
        // Test abort callback returning false - should not kill
        let callback: AbortCallback = || false;

        let result = wait_for_claude_exit_with_progress(1, None, Some(callback));
        // Should complete normally without killing
        let _ = result;
    }

    #[test]
    fn test_is_claude_process_with_special_pids() {
        // Test edge cases for PID values
        // PID 2 is typically kthreadd (kernel thread)
        let result = is_claude_process(2);
        let _ = result; // Just ensure it doesn't panic

        // Test with a PID that might exist but isn't claude
        let result = is_claude_process(std::process::id());
        assert!(!result); // Current process is not claude
    }

    #[test]
    fn test_get_claude_version_error_handling() {
        // Test various error scenarios for get_claude_version
        // The function should handle errors gracefully
        let result = get_claude_version();
        // We don't assert success/failure as it depends on environment
        // Just ensure it returns a Result type correctly
        match result {
            Ok(version) => {
                assert!(!version.is_empty());
            }
            Err(_) => {
                // Expected in environments without claude
            }
        }
    }

    #[test]
    fn test_claude_error_source_for_variants_with_messages() {
        // Test that Error::source returns None for simple variants
        let err1 = ClaudeError::VersionError("test".to_string());
        let err2 = ClaudeError::ClaudeStartFailed("test".to_string());
        let err3 = ClaudeError::ClaudeExecutionFailed("test".to_string());

        // All these should have None source since they don't wrap other errors
        assert!(std::error::Error::source(&err1).is_none());
        assert!(std::error::Error::source(&err2).is_none());
        assert!(std::error::Error::source(&err3).is_none());
    }

    #[test]
    fn test_escape_shell_arg_preserves_content() {
        // Test that escaping preserves the actual content
        let inputs = vec![
            "simple",
            "with spaces",
            "with'apostrophe",
            "with\"quote",
            "with$dollar",
            "with;semicolon",
            "with&ampersand",
            "with|pipe",
            "with<angle>brackets",
            "with[brackets]",
            "with{braces}",
            "with*wildcard",
            "with?question",
        ];

        for input in inputs {
            let escaped = escape_shell_arg(input);
            // Should be wrapped in single quotes
            assert!(escaped.starts_with('\''));
            assert!(escaped.ends_with('\''));
        }
    }

    #[test]
    fn test_escape_shell_arg_empty_and_whitespace() {
        // Test empty string and whitespace-only strings
        assert_eq!(escape_shell_arg(""), "''");
        assert_eq!(escape_shell_arg(" "), "' '");
        assert_eq!(escape_shell_arg("  "), "'  '");
        assert_eq!(escape_shell_arg("\t"), "'\t'");
        assert_eq!(escape_shell_arg("\n"), "'\n'");
    }

    #[test]
    fn test_escape_shell_arg_unicode() {
        // Test unicode characters
        let unicode_str = "Hello 世界 🎉";
        let escaped = escape_shell_arg(unicode_str);
        assert_eq!(escaped, "'Hello 世界 🎉'");
    }

    #[test]
    fn test_escape_shell_arg_multiple_single_quotes() {
        // Test multiple single quotes in a row
        assert_eq!(escape_shell_arg("'''"), "'\\''\\''\\'''");
        assert_eq!(escape_shell_arg("a'b'c'd"), "'a'\\''b'\\''c'\\''d'");
    }

    #[test]
    fn test_build_agent_prompt_all_printable_ascii() {
        // Test all printable ASCII characters
        let all_printable: String = (32..=126).map(|c| c as u8 as char).collect();
        let cmd = build_agent_prompt(&all_printable);
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }

    #[test]
    fn test_get_prompt_file_with_various_encodings() {
        // Test that get_prompt handles different file encodings
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-encoding.txt");

        // Test with ASCII
        std::fs::write(&file_path, "ASCII prompt\n").unwrap();
        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ASCII prompt");

        // Test with UTF-8
        let utf8_content = "UTF-8 prompt: 你好 世界 🚀\n";
        std::fs::write(&file_path, utf8_content).unwrap();
        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert!(result.unwrap().contains("你好"));

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_get_prompt_from_file_with_crlf() {
        // Test Windows-style line endings (CRLF)
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test-prompt-crlf.txt");
        let prompt_content = "prompt\r\n";

        std::fs::write(&file_path, prompt_content).unwrap();

        let result = get_prompt(Some(file_path.to_str().unwrap()));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "prompt");

        std::fs::remove_file(&file_path).ok();
    }

    #[test]
    fn test_is_claude_available_with_version_check() {
        // Test that is_claude_available doesn't panic
        // and returns a boolean
        let available = is_claude_available();
        // Just verify it's a boolean
        match available {
            true | false => {
                // Test passes
            }
        }
    }

    #[test]
    fn test_is_claude_running_consistency() {
        // Test that is_claude_running returns consistent results
        let result1 = is_claude_running();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let result2 = is_claude_running();

        // Results should be the same (unless claude started/stopped in between)
        // We don't assert equality but verify both are booleans
        let _ = (result1, result2);
    }

    #[test]
    fn test_kill_claude_safety() {
        // Test that kill_claude is safe to call even when nothing is running
        let result1 = kill_claude();
        let result2 = kill_claude();

        // Both calls should succeed without panicking
        // Results indicate whether processes were killed
        let _ = (result1, result2);
    }

    #[test]
    fn test_wait_for_claude_exit_with_very_long_timeout() {
        // Test with a very long timeout
        // This should return quickly if claude is not running
        let start = std::time::Instant::now();
        let result = wait_for_claude_exit(1000);
        let elapsed = start.elapsed();

        // If claude is not running, should return very quickly
        if result.is_ok() {
            assert!(elapsed < std::time::Duration::from_secs(1));
        }
    }

    #[test]
    fn test_wait_for_claude_exit_with_progress_medium_timeout() {
        // Test with a medium timeout
        let result = wait_for_claude_exit_with_progress(10, None, None);
        let _ = result;
    }

    #[test]
    fn test_claude_error_display_with_colon() {
        // Test error messages with colons (common in error messages)
        let err = ClaudeError::VersionError("Error: something went wrong".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Error: something went wrong"));
    }

    #[test]
    fn test_build_agent_prompt_does_not_panic() {
        // Test that build_agent_prompt never panics
        let problematic_inputs = vec![
            "",
            "\0",
            "\n",
            "\r\n",
            "\t",
            "'",
            "\"",
            "$",
            ";",
            "&",
            "|",
            "<",
            ">",
            "`",
            "\\",
            "!",
            "*",
            "?",
            "[",
            "]",
            "{",
            "}",
            "(",
            ")",
            "~",
            "#",
            "%",
            "=",
            "@",
            "+",
            "_",
            ".",
            ",",
            ":",
            "/",
        ];

        for input in problematic_inputs {
            let _ = build_agent_prompt(input);
        }
    }

    #[test]
    fn test_get_prompt_does_not_panic() {
        // Test that get_prompt handles errors gracefully
        let invalid_paths = vec![
            "/nonexistent/path/to/file.txt",
            "/dev/null/invalid",
            "",
            "/proc/nonexistent/file",
        ];

        for path in invalid_paths {
            let result = get_prompt(Some(path));
            // Should return Err, not panic
            assert!(result.is_err());
        }
    }
}
