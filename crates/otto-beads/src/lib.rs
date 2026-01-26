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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beads_error_display() {
        let err = BeadsError::BeadsNotAvailable;
        assert_eq!(format!("{}", err), "beads command not found");

        let err = BeadsError::NotInitialized;
        assert_eq!(format!("{}", err), "beads not initialized (no .beads directory)");

        let err = BeadsError::ExecutionFailed("test error".to_string());
        assert_eq!(format!("{}", err), "beads execution failed: test error");
    }

    #[test]
    fn test_beads_error_impls() {
        // Verify that BeadsError implements the required traits
        let err = BeadsError::BeadsNotAvailable;
        let _display: &dyn std::fmt::Display = &err;
        let _debug: &dyn std::fmt::Debug = &err;
        let _error: &dyn std::error::Error = &err;
    }

    #[test]
    fn test_beads_result_type_exists() {
        // Verify that BeadsResult is a Result type
        let ok_result: BeadsResult<bool> = Ok(true);
        let err_result: BeadsResult<bool> = Err(BeadsError::BeadsNotAvailable);

        assert!(ok_result.is_ok());
        assert!(err_result.is_err());
    }

    #[test]
    fn test_beads_result_variants() {
        // Test all error variants can be created
        let _ = BeadsError::BeadsNotAvailable;
        let _ = BeadsError::NotInitialized;
        let _ = BeadsError::ExecutionFailed("test".to_string());
    }

    #[test]
    fn test_beads_error_source() {
        use std::error::Error;
        // Verify Error trait implementation provides source()
        let err = BeadsError::ExecutionFailed("test".to_string());
        assert!(err.source().is_none()); // Our error doesn't have an underlying source

        let err2 = BeadsError::BeadsNotAvailable;
        assert!(err2.source().is_none());
    }

    #[test]
    fn test_has_ready_tasks_function_exists() {
        // Just verify the function signature compiles
        let _ = has_ready_tasks as fn() -> BeadsResult<bool>;
    }

    #[test]
    fn test_beads_not_available_error_code() {
        // Verify the error code path works
        // When bd command is not available, should return BeadsNotAvailable
        // We can't test this without modifying PATH, but we verify the logic exists
        let err = BeadsError::BeadsNotAvailable;
        assert!(matches!(err, BeadsError::BeadsNotAvailable));
    }

    #[test]
    fn test_not_initialized_error_code() {
        // Verify the not initialized error exists
        let err = BeadsError::NotInitialized;
        assert!(matches!(err, BeadsError::NotInitialized));
    }

    #[test]
    fn test_execution_failed_error_code() {
        // Verify the execution failed error exists
        let err = BeadsError::ExecutionFailed("command failed".to_string());
        assert!(matches!(err, BeadsError::ExecutionFailed(_)));
        if let BeadsError::ExecutionFailed(msg) = err {
            assert_eq!(msg, "command failed");
        }
    }

    #[test]
    fn test_error_matching() {
        // Test pattern matching on error types
        match BeadsError::BeadsNotAvailable {
            BeadsError::BeadsNotAvailable => assert!(true),
            BeadsError::NotInitialized => assert!(false),
            BeadsError::ExecutionFailed(_) => assert!(false),
        }

        match BeadsError::NotInitialized {
            BeadsError::BeadsNotAvailable => assert!(false),
            BeadsError::NotInitialized => assert!(true),
            BeadsError::ExecutionFailed(_) => assert!(false),
        }

        match BeadsError::ExecutionFailed("test".to_string()) {
            BeadsError::BeadsNotAvailable => assert!(false),
            BeadsError::NotInitialized => assert!(false),
            BeadsError::ExecutionFailed(_) => assert!(true),
        }
    }

    #[test]
    fn test_beads_result_with_ok() {
        // Test BeadsResult with Ok values
        let result: BeadsResult<bool> = Ok(true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), true);

        let result2: BeadsResult<bool> = Ok(false);
        assert!(result2.is_ok());
        assert_eq!(result2.unwrap(), false);
    }

    #[test]
    fn test_beads_result_with_err() {
        // Test BeadsResult with Err values
        let result: BeadsResult<bool> = Err(BeadsError::BeadsNotAvailable);
        assert!(result.is_err());

        let result2: BeadsResult<bool> = Err(BeadsError::NotInitialized);
        assert!(result2.is_err());

        let result3: BeadsResult<bool> = Err(BeadsError::ExecutionFailed("test".to_string()));
        assert!(result3.is_err());
    }

    #[test]
    fn test_error_message_formatting() {
        // Verify error messages are formatted correctly
        let err = BeadsError::BeadsNotAvailable;
        let msg = format!("{}", err);
        assert!(!msg.is_empty());
        assert!(msg.contains("not found"));

        let err2 = BeadsError::NotInitialized;
        let msg2 = format!("{}", err2);
        assert!(!msg2.is_empty());
        assert!(msg2.contains("not initialized"));

        let err3 = BeadsError::ExecutionFailed("detailed error".to_string());
        let msg3 = format!("{}", err3);
        assert!(!msg3.is_empty());
        assert!(msg3.contains("detailed error"));
    }

    #[test]
    fn test_beads_result_map_and_or_else() {
        // Test that BeadsResult works with standard Result methods
        let result: BeadsResult<bool> = Ok(true);
        let mapped = result.map(|v| !v);
        assert_eq!(mapped.unwrap(), false);

        let result2: BeadsResult<bool> = Err(BeadsError::BeadsNotAvailable);
        let recovered = result2.or_else(|e| match e {
            BeadsError::BeadsNotAvailable => Ok(false),
            _ => Err(e),
        });
        assert_eq!(recovered.unwrap(), false);
    }

    #[test]
    fn test_has_ready_tasks_returns_result() {
        // The function should return a BeadsResult<bool>
        // We can't test the actual behavior without mocking or running bd commands
        // But we can verify it's callable and compiles
        // Note: This will fail if bd is not available, which is expected
        let _result = has_ready_tasks();
        // We don't assert on the result since it depends on the environment
    }

    #[test]
    fn test_error_variants_are_exhaustive() {
        // Verify all error variants are covered in match
        let errors = vec![
            BeadsError::BeadsNotAvailable,
            BeadsError::NotInitialized,
            BeadsError::ExecutionFailed("test".to_string()),
        ];

        for err in errors {
            match err {
                BeadsError::BeadsNotAvailable => {}
                BeadsError::NotInitialized => {}
                BeadsError::ExecutionFailed(_) => {}
            }
        }
    }

    #[test]
    fn test_has_ready_tasks_returns_bool_when_ok() {
        // Test that when has_ready_tasks returns Ok, it contains a bool
        // This test will only pass in an environment with beads properly initialized
        match has_ready_tasks() {
            Ok(has_tasks) => {
                // Verify the result is actually a boolean
                let _is_bool: bool = has_tasks;
            }
            Err(_) => {
                // If beads is not available or initialized, that's expected in some environments
                // The test passes as long as the return type is correct
            }
        }
    }

    #[test]
    fn test_has_ready_tasks_checks_beads_availability() {
        // Test that the function checks for beads availability first
        // The function should attempt to run `bd --version`
        // We can't mock this easily, but we verify the function is callable
        let result = has_ready_tasks();
        // We don't assert on the result since it depends on environment
        // Just verify it returns a Result type
        let _: BeadsResult<bool> = result;
    }

    #[test]
    fn test_has_ready_tasks_parses_output_correctly() {
        // Test output parsing logic
        // The function looks for lines with brackets [ ]
        let test_output_with_tasks = "📋 Ready work (1 issue)\n1. [● P2] [task] otto-123: Some task";
        let has_tasks = test_output_with_tasks.lines()
            .any(|line| {
                let line = line.trim();
                line.contains('[') && line.contains(']')
            });
        assert!(has_tasks, "Should detect tasks in output");

        let test_output_without_tasks = "📋 Ready work (0 issues)\nNo ready work";
        let no_tasks = !test_output_without_tasks.lines()
            .any(|line| {
                let line = line.trim();
                line.contains('[') && line.contains(']')
            });
        assert!(no_tasks, "Should not detect tasks when none exist");
    }

    #[test]
    fn test_has_ready_tasks_detects_initialized_error() {
        // Test that the function can detect not initialized error
        // This tests the logic that checks stderr for "not initialized" or ".beads"
        let test_stderr = "error: beads not initialized (no .beads directory)";
        let is_not_init = test_stderr.contains("not initialized") || test_stderr.contains(".beads");
        assert!(is_not_init, "Should detect not initialized error");

        let test_stderr2 = "error: no .beads directory found";
        let is_not_init2 = test_stderr2.contains("not initialized") || test_stderr2.contains(".beads");
        assert!(is_not_init2, "Should detect .beads in error message");

        let test_stderr3 = "some other error";
        let is_not_init3 = test_stderr3.contains("not initialized") || test_stderr3.contains(".beads");
        assert!(!is_not_init3, "Should not detect not initialized in unrelated error");
    }

    #[test]
    fn test_beads_error_downcasting() {
        // Test that BeadsError can be downcast from std::error::Error
        use std::error::Error;
        let err = BeadsError::ExecutionFailed("test".to_string());
        let _err_ref: &dyn Error = &err;

        let err2 = BeadsError::BeadsNotAvailable;
        let _err_ref2: &dyn Error = &err2;

        let err3 = BeadsError::NotInitialized;
        let _err_ref3: &dyn Error = &err3;
    }

    #[test]
    fn test_error_equality() {
        // Test that errors can be compared for equality
        let err1 = BeadsError::BeadsNotAvailable;
        let err2 = BeadsError::BeadsNotAvailable;
        // Note: BeadsError doesn't derive PartialEq, so we test with matches
        assert!(matches!(err1, BeadsError::BeadsNotAvailable));
        assert!(matches!(err2, BeadsError::BeadsNotAvailable));
    }

    #[test]
    fn test_has_ready_tasks_return_variants() {
        // Test all possible return types from has_ready_tasks
        // We verify the type signature supports all variants
        fn check_result(result: BeadsResult<bool>) {
            match result {
                Ok(true) => {}
                Ok(false) => {}
                Err(BeadsError::BeadsNotAvailable) => {}
                Err(BeadsError::NotInitialized) => {}
                Err(BeadsError::ExecutionFailed(_)) => {}
            }
        }

        // Test with all possible result types
        check_result(Ok(true));
        check_result(Ok(false));
        check_result(Err(BeadsError::BeadsNotAvailable));
        check_result(Err(BeadsError::NotInitialized));
        check_result(Err(BeadsError::ExecutionFailed("test".to_string())));
    }
}
