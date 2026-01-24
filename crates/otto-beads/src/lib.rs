//! Beads integration for Otto
//!
//! Provides functionality to check for ready-to-work beads tasks.

use std::process::Command;

/// Error type for beads operations.
#[derive(Debug)]
pub enum BeadsError {
    /// Beads is not available (not installed or not in PATH)
    BeadsNotAvailable,
    /// Beads is not initialized (no .beads directory)
    NotInitialized,
    /// Command execution failed
    ExecutionFailed(String),
}

impl std::fmt::Display for BeadsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BeadsError::BeadsNotAvailable => write!(f, "beads command not found"),
            BeadsError::NotInitialized => write!(f, "beads not initialized (no .beads directory)"),
            BeadsError::ExecutionFailed(msg) => write!(f, "beads execution failed: {}", msg),
        }
    }
}

impl std::error::Error for BeadsError {}

/// Result type for beads operations.
pub type BeadsResult<T> = Result<T, BeadsError>;

/// Checks if there are ready-to-work beads tasks.
///
/// Runs `bd ready` and parses the output to determine if there are any
/// tasks with no blockers that can be worked on.
///
/// # Returns
/// - `Ok(true)` if ready beads exist
/// - `Ok(false)` if no ready beads exist
/// - `Err(BeadsError::NotInitialized)` if beads is not initialized
/// - `Err(BeadsError::BeadsNotAvailable)` if beads command is not found
/// - `Err(BeadsError::ExecutionFailed)` if command execution fails
///
/// # Examples
/// ```
/// use otto_beads::has_ready_tasks;
///
/// match has_ready_tasks() {
///     Ok(true) => println!("There are ready tasks to work on"),
///     Ok(false) => println!("No ready tasks found"),
///     Err(e) => println!("Error checking for ready tasks: {}", e),
/// }
/// ```
pub fn has_ready_tasks() -> BeadsResult<bool> {
    // Check if beads is available by running `bd --version`
    let check_result = Command::new("bd")
        .arg("--version")
        .output();

    match check_result {
        Ok(output) if output.status.success() => {
            // beads is available, proceed to check for ready tasks
        }
        Ok(_) => {
            return Err(BeadsError::BeadsNotAvailable);
        }
        Err(_) => {
            return Err(BeadsError::BeadsNotAvailable);
        }
    }

    // Run `bd ready` to get list of ready tasks
    let output = Command::new("bd")
        .arg("ready")
        .output()
        .map_err(|e| BeadsError::ExecutionFailed(e.to_string()))?;

    // Check if command succeeded
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check if it's a "not initialized" error
        if stderr.contains("not initialized") || stderr.contains(".beads") {
            return Err(BeadsError::NotInitialized);
        }
        return Err(BeadsError::ExecutionFailed(stderr.to_string()));
    }

    // Parse output: ready tasks are shown as "📋 Ready work (N issues...)" or listed
    // If output contains task listings (lines with issue IDs), there are ready tasks
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for lines that contain task patterns like "ralph-xxx" or similar issue IDs
    // The output format shows ready tasks with bullets and issue IDs
    let has_tasks = stdout.lines()
        .any(|line| {
            // Skip empty lines and header lines
            let line = line.trim();
            // Look for lines that contain task indicators (bullets, brackets with issue IDs)
            // Format: "1. [● P1] [task] ralph-xxx: Title"
            line.contains('[') && line.contains(']')
        });

    Ok(has_tasks)
}
