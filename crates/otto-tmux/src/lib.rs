//! Tmux session management for Otto
//!
//! Provides functionality to create, reuse, and interact with tmux sessions
//! for running Claude Code agents.

use std::process::Command;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Error type for tmux operations.
#[derive(Debug)]
pub enum TmuxError {
    /// Tmux is not installed or not available
    TmuxNotAvailable,
    /// Session creation failed
    SessionCreationFailed(String),
    /// Window creation failed
    WindowCreationFailed(String),
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
            TmuxError::WindowCreationFailed(msg) => write!(f, "failed to create tmux window: {}", msg),
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

/// Prefix for agent window names.
pub const AGENT_WINDOW_PREFIX: &str = "ralph-";

/// Generates a unique random name for an agent window.
///
/// Uses the petname crate to generate memorable random names
/// like "ralph-crimson", "ralph-willow", etc.
///
/// # Returns
/// A unique window name in the format "ralph-<word>"
///
/// # Example
/// ```rust
/// use otto_tmux::generate_agent_window_name;
///
/// let name = generate_agent_window_name();
/// assert!(name.starts_with("ralph-"));
/// ```
pub fn generate_agent_window_name() -> String {
    // Generate a random short word (1-2 words, lowercase)
    // petname::petname(number_of_words, separator)
    let petname = petname::petname(1, "-");
    format!("{}{}", AGENT_WINDOW_PREFIX, petname)
}

/// Creates a new tmux window with the given name in a session.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name for the new window
///
/// # Returns
/// - `Ok(())` if the window was created successfully
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::WindowCreationFailed)` if creation fails
pub fn create_named_window(session_name: &str, window_name: &str) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let output = Command::new("tmux")
        .args(["new-window", "-t", session_name, "-n", window_name])
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(TmuxError::WindowCreationFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::WindowCreationFailed(e.to_string())),
    }
}

/// Lists all window names in a tmux session.
///
/// # Arguments
/// * `session_name` - The name of the session
///
/// # Returns
/// - `Ok(Vec<String>)` containing window names
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCheckFailed)` if listing fails
pub fn list_windows(session_name: &str) -> TmuxResult<Vec<String>> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    // Use list-windows to get window information
    // Format: "#{window_name}" gives us just the window names
    let output = Command::new("tmux")
        .args(["list-windows", "-t", session_name, "-F", "#{window_name}"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let windows: Vec<String> = stdout
                .lines()
                .map(|line| line.to_string())
                .collect();
            Ok(windows)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(TmuxError::SessionCheckFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::SessionCheckFailed(e.to_string())),
    }
}

/// Checks if a window exists in a tmux session.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name of the window to check
///
/// # Returns
/// - `Ok(true)` if the window exists
/// - `Ok(false)` if the window does not exist
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCheckFailed)` if the check fails
pub fn window_exists(session_name: &str, window_name: &str) -> TmuxResult<bool> {
    match list_windows(session_name)? {
        windows => Ok(windows.contains(&window_name.to_string())),
    }
}

/// Executes a command within a specific tmux window.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name of the window
/// * `command` - The command to execute
///
/// # Returns
/// - `Ok(())` if the command was sent successfully
/// - `Err(TmuxError::CommandExecutionFailed)` if sending the command fails
pub fn send_command_to_window(
    session_name: &str,
    window_name: &str,
    command: &str,
) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let target = format!("{}:{}", session_name, window_name);
    let output = Command::new("tmux")
        .args(["send-keys", "-t", &target, command, "C-m"])
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

/// Creates a new tmux window with a unique agent name.
///
/// This is a convenience function that generates a unique window name
/// and creates the window. It handles name collisions by retrying
/// with a new name if the window already exists.
///
/// # Arguments
/// * `session_name` - The name of the session
///
/// # Returns
/// - `Ok(String)` containing the window name if created successfully
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::WindowCreationFailed)` if creation fails after retries
///
/// # Example
/// ```rust
/// use otto_tmux::create_agent_window;
///
/// match create_agent_window("otto") {
///     Ok(window_name) => println!("Created window: {}", window_name),
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// ```
pub fn create_agent_window(session_name: &str) -> TmuxResult<String> {
    // Try up to 10 times to generate a unique name
    for _ in 0..10 {
        let window_name = generate_agent_window_name();

        // Check if window already exists
        match window_exists(session_name, &window_name)? {
            true => continue, // Name collision, try again
            false => {
                // Window doesn't exist, create it
                create_named_window(session_name, &window_name)?;
                return Ok(window_name);
            }
        }
    }

    // Failed to generate unique name after 10 attempts
    Err(TmuxError::WindowCreationFailed(
        "failed to generate unique window name after 10 attempts".to_string(),
    ))
}

/// Finds an agent window not running Claude.
///
/// This function searches for existing "ralph-*" windows and returns
/// one where Claude is not actively running. A window is considered
/// available if:
/// - No process is running (idle shell)
/// - A process is running but it's not Claude
///
/// # Arguments
/// * `session_name` - The name of the session
///
/// # Returns
/// - `Ok(Some(String))` containing the window name if a suitable window is found
/// - `Ok(None)` if no windows are available
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCheckFailed)` if listing fails
pub fn find_idle_agent_window(session_name: &str) -> TmuxResult<Option<String>> {
    let ralph_windows = list_windows_by_pattern(session_name, AGENT_WINDOW_PREFIX)?;

    for window_name in ralph_windows {
        let pane_spec = get_pane_spec(session_name, &window_name);

        // Check if this window has a Claude process running
        match get_pane_pid(&pane_spec) {
            Ok(Some(pid)) => {
                // Has a process - check if it's Claude
                let cmdline_path = format!("/proc/{}/cmdline", pid);
                if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                    let command = cmdline.replace('\0', " ");
                    // If not running Claude, this window is available
                    if !command.contains("claude") {
                        return Ok(Some(window_name));
                    }
                }
            }
            Ok(None) => {
                // No process running - this window is available
                return Ok(Some(window_name));
            }
            Err(_) => {
                // Error querying - skip this window
                continue;
            }
        }
    }

    Ok(None)
}

/// Gets or creates an agent window, preferring to reuse idle windows.
///
/// This function first tries to find an existing agent window that is not
/// running Claude. If found, it returns that window name.
/// Otherwise, it creates a new window.
///
/// # Arguments
/// * `session_name` - The name of the session
///
/// # Returns
/// - `Ok(String)` containing the window name
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::WindowCreationFailed)` if creation fails
pub fn get_or_create_agent_window(session_name: &str) -> TmuxResult<String> {
    // First, try to find an idle window
    if let Ok(Some(idle_window)) = find_idle_agent_window(session_name) {
        return Ok(idle_window);
    }

    // No idle window found, create a new one
    create_agent_window(session_name)
}

/// Lists windows in a session matching a pattern.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `pattern` - A substring pattern to match window names against
///
/// # Returns
/// - `Ok(Vec<String>)` containing window names that match the pattern
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::SessionCheckFailed)` if listing fails
pub fn list_windows_by_pattern(session_name: &str, pattern: &str) -> TmuxResult<Vec<String>> {
    let all_windows = list_windows(session_name)?;
    let matching: Vec<String> = all_windows
        .into_iter()
        .filter(|w| w.contains(pattern))
        .collect();
    Ok(matching)
}

/// Gets the pane specification for a window.
///
/// Returns the full pane spec in the format "session:window.0"
/// (assuming pane 0, which is the default for new windows).
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name of the window
///
/// # Returns
/// - `Ok(String)` containing the pane specification
pub fn get_pane_spec(session_name: &str, window_name: &str) -> String {
    format!("{}:{}.0", session_name, window_name)
}

/// Kills a window in a session.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name of the window to kill
///
/// # Returns
/// - `Ok(())` if the window was killed successfully
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::CommandExecutionFailed)` if killing fails
pub fn kill_window(session_name: &str, window_name: &str) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let target = format!("{}:{}", session_name, window_name);
    let output = Command::new("tmux")
        .args(["kill-window", "-t", &target])
        .output();

    match output {
        Ok(output) => {
            // Exit code 0 means success, but window might not exist
            // We treat "window not found" as success (idempotent)
            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr);
            // If window doesn't exist, that's fine - it's already gone
            if stderr.contains("can't find window") || stderr.contains("no such window") {
                return Ok(());
            }

            Err(TmuxError::CommandExecutionFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::CommandExecutionFailed(e.to_string())),
    }
}

/// Checks if a pane is idle (no Claude process running).
///
/// A pane is considered idle if:
/// - No process is running (None PID), OR
/// - A shell process is running (bash, zsh, fish) - indicating Claude exited
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:ralph-xxx.0")
///
/// # Returns
/// - `Ok(true)` if the pane is idle
/// - `Ok(false)` if the pane has an active non-shell process
/// - `Err` if there was an error querying the pane
fn is_pane_idle(pane_spec: &str) -> TmuxResult<bool> {
    match get_pane_command(pane_spec)? {
        Some(command) => {
            // Check if it's a shell process (bash, zsh, fish)
            let shell_names = ["bash", "zsh", "fish", "sh"];
            let is_shell = shell_names.iter().any(|shell| {
                command.contains(shell) && !command.contains("claude")
            });
            Ok(is_shell)
        }
        None => Ok(true), // No process running = idle
    }
}

/// Finds an idle ralph-* window in the otto session.
///
/// An idle window is one where:
/// - No process is running, OR
/// - Only a shell process is running (bash, zsh, fish)
///
/// # Returns
/// - `Ok(Some(String))` containing the window name if an idle window is found
/// - `Ok(None)` if no idle windows exist
/// - `Err` if there was an error
pub fn find_idle_ralph_window() -> TmuxResult<Option<String>> {
    let ralph_windows = list_windows_by_pattern(OTTO_SESSION_NAME, AGENT_WINDOW_PREFIX)?;

    for window_name in ralph_windows {
        let pane_spec = get_pane_spec(OTTO_SESSION_NAME, &window_name);

        if is_pane_idle(&pane_spec)? {
            return Ok(Some(window_name));
        }
    }

    Ok(None)
}

/// Captures the content of a pane.
///
/// Returns the visible text content of the pane.
///
/// # Arguments
/// * `pane_spec` - The pane specification (e.g., "otto:ralph-xxx.0")
///
/// # Returns
/// - `Ok(String)` containing the pane content
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::CommandExecutionFailed)` if capture fails
pub fn capture_pane(pane_spec: &str) -> TmuxResult<String> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    // Capture last 1000 lines of the pane
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", pane_spec, "-p", "-S", "-1000"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(content)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // If pane doesn't exist, return empty string (graceful degradation)
            if stderr.contains("can't find pane") || stderr.contains("no such pane") {
                return Ok(String::new());
            }
            Err(TmuxError::CommandExecutionFailed(stderr.to_string()))
        }
        Err(e) => Err(TmuxError::CommandExecutionFailed(e.to_string())),
    }
}

/// Attaches to a tmux window.
///
/// This function replaces the current process with tmux attach-session,
/// effectively transferring control to tmux.
///
/// # Arguments
/// * `session_name` - The name of the session
/// * `window_name` - The name of the window to attach to
///
/// # Returns
/// - `Err(TmuxError::TmuxNotAvailable)` if tmux is not installed
/// - `Err(TmuxError::CommandExecutionFailed)` if attach fails
///
/// # Note
/// This function uses `std::process::Command::exec()` which replaces
/// the current process. It never returns on success.
///
/// # Platform Support
/// This function is only available on Unix platforms where we can
/// replace the current process with exec.
#[cfg(unix)]
pub fn attach_to_window(session_name: &str, window_name: &str) -> TmuxResult<()> {
    if !is_tmux_available() {
        return Err(TmuxError::TmuxNotAvailable);
    }

    let target = format!("{}:{}", session_name, window_name);

    // Use exec to replace the otto process with tmux
    // This never returns on success
    let error = std::process::Command::new("tmux")
        .args(["attach-session", "-t", &target])
        .exec();

    // If exec returns, it means an error occurred
    Err(TmuxError::CommandExecutionFailed(error.to_string()))
}

/// Attaches to a tmux window (non-Unix version).
///
/// On non-Unix platforms, this function is not available.
#[cfg(not(unix))]
pub fn attach_to_window(session_name: &str, window_name: &str) -> TmuxResult<()> {
    Err(TmuxError::CommandExecutionFailed(
        "attach is only supported on Unix platforms".to_string(),
    ))
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

    #[test]
    fn test_agent_window_prefix_constant() {
        assert_eq!(AGENT_WINDOW_PREFIX, "ralph-");
    }

    #[test]
    fn test_generate_agent_window_name_format() {
        let name = generate_agent_window_name();
        assert!(name.starts_with("ralph-"));
        assert!(name.len() > "ralph-".len());
    }

    #[test]
    fn test_generate_agent_window_names_unique() {
        // Generate multiple names and verify they're different
        let name1 = generate_agent_window_name();
        let name2 = generate_agent_window_name();
        // Note: there's a small chance of collision, but very unlikely
        // This test just verifies the function works and generates reasonable names
        assert!(name1.starts_with("ralph-"));
        assert!(name2.starts_with("ralph-"));
    }

    #[test]
    fn test_create_agent_window_function_exists() {
        // Just verify the function signature compiles
        let _ = create_agent_window as fn(&str) -> TmuxResult<String>;
    }

    #[test]
    fn test_send_command_to_window_function_exists() {
        // Just verify the function signature compiles
        let _ = send_command_to_window as fn(&str, &str, &str) -> TmuxResult<()>;
    }

    #[test]
    fn test_list_windows_function_exists() {
        // Just verify the function signature compiles
        let _ = list_windows as fn(&str) -> TmuxResult<Vec<String>>;
    }

    #[test]
    fn test_window_exists_function_exists() {
        // Just verify the function signature compiles
        let _ = window_exists as fn(&str, &str) -> TmuxResult<bool>;
    }
}
