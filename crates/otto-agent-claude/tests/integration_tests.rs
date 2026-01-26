//! Integration tests for otto-agent-claude
//!
//! These tests interact with external system commands and may have
//! different results depending on the environment.

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use otto_agent_claude::{
    get_claude_version, is_claude_available, is_claude_process, is_claude_running,
    kill_claude, wait_for_claude_exit, wait_for_claude_exit_with_progress, ClaudeError,
};

/// Helper to get a temp file path
fn temp_file(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(name);
    path
}

/// Helper to create a fake /proc/<pid>/cmdline for testing
fn create_fake_cmdline(pid: u32, content: &str) -> PathBuf {
    let path = format!("/tmp/test-proc-{}/cmdline", pid);
    let dir = format!("/tmp/test-proc-{}", pid);

    fs::create_dir_all(&dir).unwrap();
    File::create(&path).unwrap().write_all(content.as_bytes()).unwrap();

    PathBuf::from(path)
}

/// Helper to cleanup fake cmdline
fn cleanup_fake_cmdline(pid: u32) {
    let dir = format!("/tmp/test-proc-{}", pid);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_is_claude_available() {
    // This test just checks that the function runs without panicking
    // The result depends on whether claude is installed in the environment
    let available = is_claude_available();
    // Just ensure it returns a boolean
    let _ = available;
}

#[test]
fn test_get_claude_version() {
    match get_claude_version() {
        Ok(version) => {
            // If claude is available, version should be non-empty
            assert!(!version.is_empty());
            println!("Claude version: {}", version);
        }
        Err(ClaudeError::VersionError(_)) => {
            // Expected if claude is not available
        }
        Err(_) => {
            panic!("Unexpected error type");
        }
    }
}

#[test]
fn test_is_claude_running() {
    // Just check the function runs without panicking
    let running = is_claude_running();
    let _ = running;
}

#[test]
fn test_wait_for_claude_exit_immediate() {
    // If claude is not running, should return Ok immediately
    match wait_for_claude_exit(1) {
        Ok(()) => {
            // Expected if claude is not running
        }
        Err(ClaudeError::ClaudeTimeout) => {
            // claude is running and didn't exit in 1 second
            // This is ok for the test
        }
        Err(_) => {
            panic!("Unexpected error type");
        }
    }
}

#[test]
fn test_wait_for_claude_exit_short_timeout() {
    let result = wait_for_claude_exit(2);
    // Either Ok (not running) or Timeout (running) are acceptable
    match result {
        Ok(()) => {}
        Err(ClaudeError::ClaudeTimeout) => {}
        Err(_) => panic!("Unexpected error type"),
    }
}

#[test]
fn test_wait_for_claude_exit_with_progress_no_callback() {
    // Test with a short timeout and no callbacks
    let result = wait_for_claude_exit_with_progress(1, None, None);
    match result {
        Ok(()) => {}
        Err(ClaudeError::ClaudeTimeout) => {}
        Err(_) => panic!("Unexpected error type"),
    }
}

#[test]
fn test_wait_for_claude_exit_with_progress_callback() {
    // Use a static callback that doesn't capture
    fn progress_callback(_duration: Duration) {
        // Just a no-op callback for testing
    }

    // Test with progress callback
    let result = wait_for_claude_exit_with_progress(1, Some(progress_callback), None);
    match result {
        Ok(()) => {}
        Err(ClaudeError::ClaudeTimeout) => {}
        Err(_) => panic!("Unexpected error type"),
    }
}

#[test]
fn test_wait_for_claude_exit_with_abort_callback() {
    // Abort callback that returns false (don't abort)
    fn no_abort() -> bool {
        false
    }

    let result = wait_for_claude_exit_with_progress(1, None, Some(no_abort));
    match result {
        Ok(()) => {}
        Err(ClaudeError::ClaudeTimeout) => {}
        Err(_) => panic!("Unexpected error type"),
    }
}

#[test]
fn test_wait_for_claude_exit_with_immediate_abort() {
    // Abort callback that returns true immediately
    fn immediate_abort() -> bool {
        true
    }

    // This should return Ok immediately (after killing claude if running)
    let result = wait_for_claude_exit_with_progress(5, None, Some(immediate_abort));
    assert!(result.is_ok(), "Should return Ok when abort callback returns true");
}

#[test]
fn test_kill_claude() {
    // This test is tricky because we don't want to kill actual claude processes
    // Just test that the function runs without panicking
    // In a test environment, there should be no claude processes running
    let killed = kill_claude();
    let _ = killed;
}

#[test]
fn test_is_claude_process_with_fake_cmdline() {
    // Note: is_claude_process reads from /proc/{pid}/cmdline
    // We can't actually test this without modifying /proc, which requires root
    // So we just test with PIDs that shouldn't exist or shouldn't be claude

    // Test with PID that shouldn't exist
    assert!(!is_claude_process(999999999));

    // Test with PID 1 (should be init/systemd, not claude)
    // This might return false or true depending on the system
    // Just ensure it doesn't panic
    let _ = is_claude_process(1);
}

#[test]
fn test_get_prompt_from_temp_file() {
    use otto_agent_claude::get_prompt;

    let temp_path = temp_file("otto-test-prompt-integration.txt");
    let content = "integration test prompt\n";

    File::create(&temp_path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();

    let result = get_prompt(Some(temp_path.to_str().unwrap()));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "integration test prompt");

    fs::remove_file(&temp_path).ok();
}

#[test]
fn test_get_prompt_from_file_with_special_chars() {
    use otto_agent_claude::get_prompt;

    let temp_path = temp_file("otto-test-prompt-special.txt");
    let content = "prompt with $pecial chars and 'quotes'\n";

    File::create(&temp_path)
        .unwrap()
        .write_all(content.as_bytes())
        .unwrap();

    let result = get_prompt(Some(temp_path.to_str().unwrap()));
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "prompt with $pecial chars and 'quotes'");

    fs::remove_file(&temp_path).ok();
}

#[test]
fn test_build_agent_prompt_integration() {
    use otto_agent_claude::build_agent_prompt;

    let prompts = vec![
        "simple prompt",
        "prompt with 'quotes'",
        "prompt with $pecial chars",
        "prompt with\nnewlines",
        "",
        "prompt with multiple\n\nnewlines",
    ];

    for prompt in prompts {
        let cmd = build_agent_prompt(prompt);
        assert!(cmd.contains("claude --dangerously-skip-permissions"));
    }
}

#[test]
fn test_claude_error_display() {
    use otto_agent_claude::ClaudeError;
    use std::fmt::Display;

    let errors = vec![
        ClaudeError::ClaudeNotAvailable,
        ClaudeError::VersionError("test version error".to_string()),
        ClaudeError::ClaudeStartFailed("test start error".to_string()),
        ClaudeError::ClaudeTimeout,
        ClaudeError::ClaudeExecutionFailed("test exec error".to_string()),
    ];

    for error in errors {
        let msg = format!("{}", error);
        assert!(!msg.is_empty());
    }
}

#[test]
fn test_claude_error_debug() {
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
