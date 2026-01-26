use clap::{Parser, Subcommand};
use otto_agent_claude::AbortCallback;
use otto_beads::{has_ready_tasks, BeadsError};
use otto_core::{launch_agent_default, start_stuck_window_monitor, AgentError};
use otto_log::color::{print_error, print_warning};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// Global shutdown flag, set by signal handlers
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Global counter for number of shutdown signals received
/// First signal: graceful shutdown (wait for agent)
/// Second signal: force kill and exit immediately
static SHUTDOWN_COUNT: AtomicU8 = AtomicU8::new(0);

/// Otto - Autonomous agent runner for beads tasks
///
/// Otto continuously checks for ready-to-work beads tasks and spawns
/// Claude Code agents to complete them.
#[derive(Parser, Debug)]
#[command(name = "otto")]
#[command(version, about, long_about = None)]
#[command(author = "Mike Kusold")]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start otto in tmux
    ///
    /// Launches otto in a tmux window named 'otto', running in watch mode.
    /// This is the recommended way to run otto persistently.
    Start,

    /// Attach to a tmux window
    ///
    /// Connects to a tmux window in the otto session. With no arguments,
    /// attaches to the 'otto' window. With an argument, attaches to the
    /// specified window.
    Attach {
        /// Window name to attach to (optional)
        ///
        /// Can be a short name like "ralph-crimson" or a full spec like "otto:ralph-crimson".
        /// If not provided, attaches to the 'otto' window.
        window: Option<String>,
    },

    /// Run the agent loop (default behavior)
    ///
    /// Continuously checks for ready-to-work beads tasks and spawns
    /// Claude Code agents to complete them.
    Ralph {
        /// Run in watch mode (loop forever, checking for ready tasks)
        ///
        /// When enabled, otto will continuously loop and spawn agents for ready tasks.
        /// When disabled, otto will stop when no ready tasks are found.
        #[arg(long, short = 'w')]
        watch: bool,

        /// Path to a custom prompt file for Claude Code agents
        ///
        /// If provided, reads the prompt from this file. Otherwise, auto-detects
        /// PROMPT_RALPH.md from the repo root, or falls back to the default
        /// OTTO_AGENT_PROMPT.
        #[arg(long, short = 'p')]
        prompt_file: Option<String>,
    },

    /// Spawn a single agent for a specific issue
    ///
    /// This command spawns a single Claude Code agent to work on a specific issue.
    /// Supports workspace isolation via git worktrees for better separation and cleanup.
    /// Workspaces are created by default for better isolation.
    Spawn {
        /// Issue ID to spawn an agent for (required)
        ///
        /// Specifies which issue the agent should work on.
        #[arg(long, short = 'i')]
        issue: String,

        /// Workspace path for isolated worktree (optional)
        ///
        /// Creates a git worktree at the specified path for isolated agent work.
        /// If not specified, defaults to ../agents/otto-<issue-id>.
        /// The workspace will be on a unique branch named agent/<workspace-name>-<issue-id>.
        #[arg(long, short = 'w')]
        workspace: Option<String>,

        /// Disable workspace isolation (optional)
        ///
        /// If specified, the agent will spawn in the main repository instead of a workspace.
        /// This flag is mutually exclusive with --workspace.
        /// Useful for quick tasks or debugging.
        #[arg(long, conflicts_with = "workspace")]
        no_workspace: bool,

        /// Path to a custom prompt file for Claude Code agents (optional)
        ///
        /// If provided, reads the prompt from this file. Otherwise, auto-detects
        /// PROMPT_RALPH.md from the repo root, or falls back to the default
        /// OTTO_AGENT_PROMPT.
        #[arg(long, short = 'p')]
        prompt_file: Option<String>,
    },
}

/// Detects if PROMPT_RALPH.md exists in the repository root.
///
/// Returns Some("PROMPT_RALPH.md") if the file exists, None otherwise.
/// This function searches upward from the current directory to find the
/// repository root (indicated by a .beads directory).
fn detect_ralph_prompt() -> Option<&'static str> {
    const PROMPT_FILE: &str = "PROMPT_RALPH.md";
    const BEADS_DIR: &str = ".beads";

    // Start from current directory and search upward
    let mut current_path = std::env::current_dir().ok()?;

    loop {
        // Check if .beads directory exists (indicates repo root)
        let beads_path = current_path.join(BEADS_DIR);
        if beads_path.is_dir() {
            // Found repo root, check for PROMPT_RALPH.md
            let prompt_path = current_path.join(PROMPT_FILE);
            if prompt_path.is_file() {
                return Some(PROMPT_FILE);
            }
            // Found repo root but no prompt file, stop searching
            return None;
        }

        // Move to parent directory
        if !current_path.pop() {
            // Reached filesystem root, not in a repo
            return None;
        }
    }
}

/// Spawns a single agent for a specific issue with optional workspace isolation.
///
/// This function:
/// 1. Validates the issue ID exists in beads
/// 2. Creates a git worktree if workspace is enabled (default behavior)
/// 3. Sets up the workspace environment (.beads config, OTTO_WORKSPACE env, .workspace-info)
/// 4. Launches the agent in a tmux window
/// 5. Returns the window name for the agent
///
/// # Arguments
/// * `issue_id` - The beads issue ID (e.g., "otto-123")
/// * `workspace_path` - Optional path for the git worktree (None = use default, Some = explicit path)
/// * `no_workspace` - If true, skip workspace creation entirely
/// * `prompt_file` - Optional path to a custom prompt file
///
/// # Returns
/// - `Ok(window_name)` if the agent was spawned successfully
/// - `Err(String)` if there was an error
fn spawn_agent_for_issue(
    issue_id: &str,
    workspace_path: Option<String>,
    no_workspace: bool,
    _prompt_file: Option<&str>,
) -> Result<String, String> {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    // Validate issue exists
    let output = Command::new("bd")
        .args(["show", issue_id])
        .output();

    match output {
        Ok(output) if output.status.success() => {
        }
        Ok(_) => {
            return Err(format!("Issue {} not found", issue_id));
        }
        Err(e) => {
            return Err(format!("Failed to validate issue: {}", e));
        }
    }

    // Determine workspace strategy
    // - If no_workspace is true: run in main repo (workspace_abs = None)
    // - If workspace_path is Some(path): use explicit path
    // - If workspace_path is None: use default path ../agents/otto-<issue-id>
    let workspace_abs = if no_workspace {
        // No workspace, run in main repo
        None
    } else if let Some(path) = workspace_path {
        // Explicit workspace path provided
        let abs = if Path::new(&path).is_absolute() {
            path.clone()
        } else {
            // Relative to current directory
            match std::env::current_dir() {
                Ok(cwd) => {
                    let p = cwd.join(&path);
                    p.to_str().unwrap_or(&path).to_string()
                }
                Err(_) => path.clone(),
            }
        };
        Some(abs)
    } else {
        // Default workspace path: ../agents/otto-<issue-id>
        let default_path = format!("../agents/otto-{}", issue_id);
        let abs = if Path::new(&default_path).is_absolute() {
            default_path.clone()
        } else {
            // Relative to current directory
            match std::env::current_dir() {
                Ok(cwd) => {
                    let p = cwd.join(&default_path);
                    p.to_str().unwrap_or(&default_path).to_string()
                }
                Err(_) => default_path.clone(),
            }
        };
        Some(abs)
    };

    // If workspace is enabled, set up the workspace
    let branch_name = if let Some(ref workspace_abs) = workspace_abs {
        // Check if workspace already exists
        if Path::new(workspace_abs).exists() {
            return Err(format!("Workspace path already exists: {}", workspace_abs));
        }

        // Create unique branch name: agent/otto-<issue-id>-<hash>
        // Use the workspace name (last component of path)
        let workspace_name = Path::new(workspace_abs)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace");
        let branch_name = format!("agent/{}-{}", workspace_name, issue_id);


        // Create git worktree
        let worktree_output = Command::new("git")
            .args(["worktree", "add", workspace_abs, "-b", &branch_name])
            .output();

        match worktree_output {
            Ok(output) if output.status.success() => {
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Failed to create worktree: {}", stderr));
            }
            Err(e) => {
                return Err(format!("Failed to run git worktree: {}", e));
            }
        }

        // Copy .beads config to workspace
        let beads_src = Path::new(".beads");
        let beads_dst = Path::new(workspace_abs).join(".beads");

        if beads_src.exists() {
            if let Err(e) = fs::create_dir_all(&beads_dst) {
                cleanup_workspace(workspace_abs);
                return Err(format!("Failed to create .beads in workspace: {}", e));
            }

            // Copy all files from .beads to workspace
            if let Err(e) = copy_dir_recursive(beads_src, &beads_dst) {
                cleanup_workspace(workspace_abs);
                return Err(format!("Failed to copy .beads to workspace: {}", e));
            }
        }

        // Create .workspace-info file with metadata
        let workspace_info = Path::new(workspace_abs).join(".workspace-info");
        let info_content = format!(
            "workspace_path={}\nbranch_name={}\nissue_id={}\noriginal_dir={}\n",
            workspace_abs,
            branch_name,
            issue_id,
            std::env::current_dir()
                .map(|p| p.to_str().unwrap_or("unknown").to_string())
                .unwrap_or("unknown".to_string())
        );

        if let Err(e) = fs::write(&workspace_info, info_content) {
            cleanup_workspace(workspace_abs);
            return Err(format!("Failed to create .workspace-info: {}", e));
        }

        // Set OTTO_WORKSPACE environment variable for the agent
        unsafe {
            std::env::set_var("OTTO_WORKSPACE", workspace_abs);
        }

        Some(branch_name)
    } else {
        // No workspace, run in main repo
        None
    };

    // Get or create an agent window
    let window_name = otto_tmux::get_or_create_agent_window(otto_tmux::OTTO_SESSION_NAME)
        .map_err(|e| format!("Failed to create tmux window: {}", e))?;

    // Construct the command to run claude
    let agent_command = if let Some(ref workspace_abs) = workspace_abs {
        // Run in workspace
        format!("cd {} && otto ralph", workspace_abs)
    } else {
        // Run in main repo
        "otto ralph".to_string()
    };

    // Send the command to the window
    otto_tmux::send_command_to_window(otto_tmux::OTTO_SESSION_NAME, &window_name, &agent_command)
        .map_err(|e| format!("Failed to send command to tmux: {}", e))?;

    // Print status message
    if let Some(ref workspace_abs) = workspace_abs {
        println!("Spawned agent for issue {} in workspace: {}", issue_id, workspace_abs);
        if let Some(ref branch) = branch_name {
            println!("Branch: {}", branch);
        }
    } else {
        println!("Spawned agent for issue {} in main repository", issue_id);
    }
    println!("Window: {}", window_name);

    Ok(window_name)
}

/// Copies a directory recursively.
///
/// # Arguments
/// * `src` - Source directory path
/// * `dst` - Destination directory path
///
/// # Returns
/// - `Ok(())` if the directory was copied successfully
/// - `Err(std::io::Error)` if there was an error
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

/// Cleans up a workspace by removing the git worktree.
///
/// # Arguments
/// * `workspace_path` - Path to the workspace to clean up
fn cleanup_workspace(workspace_path: &str) {

    // Remove git worktree
    let _ = std::process::Command::new("git")
        .args(["worktree", "remove", workspace_path])
        .output();

    // Also try to remove the directory if git worktree remove failed
    let _ = std::fs::remove_dir_all(workspace_path);
}

/// Formats a duration into a human-readable string.
///
/// Examples:
/// - "1m 23s" for 83 seconds
/// - "45s" for 45 seconds
/// - "1h 5m 30s" for 3930 seconds
fn format_duration(duration: std::time::Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    let mut parts = Vec::new();

    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds_only() {
        let duration = std::time::Duration::from_secs(45);
        assert_eq!(format_duration(duration), "45s");
    }

    #[test]
    fn test_format_duration_minutes_and_seconds() {
        let duration = std::time::Duration::from_secs(83);
        assert_eq!(format_duration(duration), "1m 23s");
    }

    #[test]
    fn test_format_duration_hours_minutes_seconds() {
        let duration = std::time::Duration::from_secs(3930);
        assert_eq!(format_duration(duration), "1h 5m 30s");
    }

    #[test]
    fn test_format_duration_zero() {
        let duration = std::time::Duration::from_secs(0);
        assert_eq!(format_duration(duration), "0s");
    }

    #[test]
    fn test_format_duration_large_hours() {
        let duration = std::time::Duration::from_secs(3661);
        assert_eq!(format_duration(duration), "1h 1m 1s");
    }

    #[test]
    fn test_format_duration_only_hours() {
        let duration = std::time::Duration::from_secs(7200);
        assert_eq!(format_duration(duration), "2h");
    }

    #[test]
    fn test_format_duration_only_minutes() {
        let duration = std::time::Duration::from_secs(300);
        assert_eq!(format_duration(duration), "5m");
    }

    #[test]
    fn test_copy_dir_recursive_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src = temp_dir.path().join("src");
        let dst = temp_dir.path().join("dst");

        std::fs::create_dir(&src).unwrap();

        assert!(copy_dir_recursive(&src, &dst).is_ok());
        assert!(dst.exists());
        assert!(dst.is_dir());
    }

    #[test]
    fn test_copy_dir_recursive_with_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src = temp_dir.path().join("src");
        let dst = temp_dir.path().join("dst");

        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("file1.txt"), "content1").unwrap();
        std::fs::write(src.join("file2.txt"), "content2").unwrap();

        assert!(copy_dir_recursive(&src, &dst).is_ok());
        assert!(dst.exists());
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("file2.txt").exists());

        assert_eq!(
            std::fs::read_to_string(dst.join("file1.txt")).unwrap(),
            "content1"
        );
        assert_eq!(
            std::fs::read_to_string(dst.join("file2.txt")).unwrap(),
            "content2"
        );
    }

    #[test]
    fn test_copy_dir_recursive_nested() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src = temp_dir.path().join("src");
        let dst = temp_dir.path().join("dst");

        std::fs::create_dir_all(src.join("subdir1/subdir2")).unwrap();
        std::fs::write(src.join("file1.txt"), "content1").unwrap();
        std::fs::write(src.join("subdir1/file2.txt"), "content2").unwrap();
        std::fs::write(src.join("subdir1/subdir2/file3.txt"), "content3").unwrap();

        assert!(copy_dir_recursive(&src, &dst).is_ok());
        assert!(dst.exists());
        assert!(dst.join("file1.txt").exists());
        assert!(dst.join("subdir1/file2.txt").exists());
        assert!(dst.join("subdir1/subdir2/file3.txt").exists());
    }

    #[test]
    fn test_detect_ralph_prompt_in_current_repo() {
        // This test assumes we're in the otto repo with .beads directory
        let result = detect_ralph_prompt();
        // We don't assert the result since it depends on whether PROMPT_RALPH.md exists
        // We just verify it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_args_parsing_no_command() {
        use clap::Parser;

        // Test that parsing an empty command list works
        let args = Args::try_parse_from(["otto"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        assert!(args.command.is_none());
    }

    #[test]
    fn test_args_parsing_start_command() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "start"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        assert!(matches!(args.command, Some(Commands::Start)));
    }

    #[test]
    fn test_args_parsing_ralph_command() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "ralph", "--watch"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Ralph { watch, prompt_file }) => {
                assert!(watch);
                assert!(prompt_file.is_none());
            }
            _ => panic!("Expected Ralph command"),
        }
    }

    #[test]
    fn test_args_parsing_ralph_with_prompt() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "ralph", "-p", "custom.md"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Ralph { watch, prompt_file }) => {
                assert!(!watch);
                assert_eq!(prompt_file, Some("custom.md".to_string()));
            }
            _ => panic!("Expected Ralph command"),
        }
    }

    #[test]
    fn test_args_parsing_spawn_command() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "spawn", "-i", "otto-123"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Spawn { issue, workspace, no_workspace, prompt_file }) => {
                assert_eq!(issue, "otto-123");
                assert!(workspace.is_none());
                assert!(!no_workspace);
                assert!(prompt_file.is_none());
            }
            _ => panic!("Expected Spawn command"),
        }
    }

    #[test]
    fn test_args_parsing_spawn_with_workspace() {
        use clap::Parser;

        let args = Args::try_parse_from([
            "otto",
            "spawn",
            "-i",
            "otto-123",
            "--workspace",
            "../agents/my-workspace",
        ]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Spawn { issue, workspace, no_workspace, .. }) => {
                assert_eq!(issue, "otto-123");
                assert_eq!(workspace, Some("../agents/my-workspace".to_string()));
                assert!(!no_workspace);
            }
            _ => panic!("Expected Spawn command"),
        }
    }

    #[test]
    fn test_args_parsing_spawn_no_workspace() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "spawn", "-i", "otto-123", "--no-workspace"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Spawn { issue, workspace, no_workspace, .. }) => {
                assert_eq!(issue, "otto-123");
                assert!(workspace.is_none());
                assert!(no_workspace);
            }
            _ => panic!("Expected Spawn command"),
        }
    }

    #[test]
    fn test_args_parsing_attach_command() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "attach"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Attach { window }) => {
                assert!(window.is_none());
            }
            _ => panic!("Expected Attach command"),
        }
    }

    #[test]
    fn test_args_parsing_attach_with_window() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "attach", "ralph-crimson"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Attach { window }) => {
                assert_eq!(window, Some("ralph-crimson".to_string()));
            }
            _ => panic!("Expected Attach command"),
        }
    }

    #[test]
    fn test_args_parsing_attach_full_spec() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "attach", "otto:ralph-willow"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Attach { window }) => {
                assert_eq!(window, Some("otto:ralph-willow".to_string()));
            }
            _ => panic!("Expected Attach command"),
        }
    }

    #[test]
    fn test_signal_handler_setup() {
        // This test just verifies that setup_signal_handlers doesn't panic
        // We can't easily test the actual signal handling behavior in a unit test
        setup_signal_handlers();
        // Give the signal handler thread a moment to start
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    #[test]
    fn test_shutdown_signaling() {
        // Test that we can set and check the shutdown flag
        assert!(!SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst));

        SHUTDOWN_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst));

        // Reset for other tests
        SHUTDOWN_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn test_shutdown_count() {
        // Test that we can increment and check the shutdown count
        SHUTDOWN_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);

        let count1 = SHUTDOWN_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count1, 0);

        let count2 = SHUTDOWN_COUNT.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(count2, 1);

        // Reset for other tests
        SHUTDOWN_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn test_cleanup_workspace() {
        // Test cleanup_workspace doesn't panic with non-existent path
        cleanup_workspace("/tmp/nonexistent-otto-workspace-12345");
    }

    #[test]
    fn test_workspace_info_content_format() {
        // Test that .workspace-info file content format is correct
        let workspace_path = "/tmp/test-workspace";
        let branch_name = "agent/test-branch";
        let issue_id = "otto-123";
        let original_dir = "/home/user/project";

        let info_content = format!(
            "workspace_path={}\nbranch_name={}\nissue_id={}\noriginal_dir={}\n",
            workspace_path, branch_name, issue_id, original_dir
        );

        assert!(info_content.contains("workspace_path=/tmp/test-workspace"));
        assert!(info_content.contains("branch_name=agent/test-branch"));
        assert!(info_content.contains("issue_id=otto-123"));
        assert!(info_content.contains("original_dir=/home/user/project"));
    }

    #[test]
    fn test_workspace_branch_name_format() {
        // Test the workspace branch naming convention
        let workspace_name = "test-workspace";
        let issue_id = "otto-123";
        let branch_name = format!("agent/{}-{}", workspace_name, issue_id);

        assert_eq!(branch_name, "agent/test-workspace-otto-123");
        assert!(branch_name.starts_with("agent/"));
        assert!(branch_name.contains('-'));
    }

    #[test]
    fn test_workspace_default_path_format() {
        // Test the default workspace path format
        let issue_id = "otto-123";
        let default_path = format!("../agents/{}", issue_id);

        assert_eq!(default_path, "../agents/otto-123");
        assert!(default_path.contains("agents/"));
        assert!(default_path.contains(&issue_id));
    }

    #[test]
    fn test_issue_validation_error_messages() {
        // Test that error messages for issue validation are properly formatted
        let issue_id = "nonexistent-issue";
        let error_msg = format!("Issue {} not found", issue_id);

        assert_eq!(error_msg, "Issue nonexistent-issue not found");
        assert!(error_msg.contains("not found"));
        assert!(error_msg.contains(&issue_id));
    }

    #[test]
    fn test_copy_dir_recursive_nonexistent_source() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src = temp_dir.path().join("nonexistent");
        let dst = temp_dir.path().join("dst");

        // Should return error when source doesn't exist
        let result = copy_dir_recursive(&src, &dst);
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_dir_recursive_creates_destination() {
        let temp_dir = tempfile::tempdir().unwrap();
        let src = temp_dir.path().join("src");
        let dst = temp_dir.path().join("dst").join("nested").join("path");

        std::fs::create_dir(&src).unwrap();
        std::fs::write(src.join("file.txt"), "content").unwrap();

        // copy_dir_recursive should create nested destination directories
        assert!(copy_dir_recursive(&src, &dst).is_ok());
        assert!(dst.exists());
        assert!(dst.join("file.txt").exists());
    }
}

/// Sets up signal handlers for SIGINT (Ctrl+C) and SIGTERM.
///
/// Signal handling behavior:
/// - First signal: Set shutdown flag, wait for agent to finish gracefully
/// - Second signal (Ctrl+C only): Kill running agent immediately and exit
///
/// SIGTERM always triggers graceful shutdown (no force kill).
fn setup_signal_handlers() {
    use signal_hook::iterator::Signals;

    // We need to fork the signal handling to a separate thread
    // to avoid restrictions on what can be done in a signal handler
    let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])
        .expect("failed to register signal handler");

    std::thread::spawn(move || {
        for sig in signals.forever() {
            match sig {
                signal_hook::consts::SIGINT => {
                    let count = SHUTDOWN_COUNT.fetch_add(1, Ordering::SeqCst);

                    if count == 0 {
                        // First Ctrl+C: graceful shutdown, kill agent if running
                        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
                        println!("\nShutdown signal received, terminating agent...");
                        println!("Agent will be killed gracefully. Press Ctrl+C again to force exit.");
                    } else {
                        // Second Ctrl+C: force exit immediately
                        println!("\nForce exit requested");
                        std::process::exit(130); // 128 + SIGINT (standard exit code for Ctrl+C)
                    }
                }
                signal_hook::consts::SIGTERM => {
                    if !SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
                        println!("\nShutdown signal received, waiting for agent to finish...");
                    }
                }
                _ => {}
            }
        }
    });
}

fn main() {
    let args = Args::parse();

    // Set up signal handlers for graceful shutdown
    setup_signal_handlers();

    match args.command {
        Some(Commands::Start) => {
            if let Err(e) = start_otto() {
                print_error(&format!("starting otto: {}", e));
                std::process::exit(1);
            }
        }
        Some(Commands::Attach { window }) => {
            if let Err(e) = attach_to_window(window) {
                print_error(&format!("attaching to window: {}", e));
                std::process::exit(1);
            }
        }
        Some(Commands::Ralph { watch, prompt_file }) => {
            // Auto-detect PROMPT_RALPH.md if no prompt file specified
            let prompt_file = if prompt_file.is_none() {
                detect_ralph_prompt()
            } else {
                prompt_file.as_deref()
            };

            if watch {
                println!("Otto running in watch mode (infinite loop)");
                println!("Press Ctrl+C to stop\n");
                run_watch_loop(prompt_file);
            } else {
                println!("Otto running in single-pass mode\n");
                run_single_pass(prompt_file);
            }
        }
        Some(Commands::Spawn { issue, workspace, no_workspace, prompt_file }) => {
            // Auto-detect PROMPT_RALPH.md if no prompt file specified
            let prompt_file = if prompt_file.is_none() {
                detect_ralph_prompt()
            } else {
                prompt_file.as_deref()
            };

            // Determine workspace: if no_workspace is true, use None (no workspace)
            // Otherwise, use Some(workspace) or None (which triggers default workspace)
            let workspace = if no_workspace {
                None
            } else {
                // If workspace is explicitly provided, use it; otherwise None (will use default)
                workspace
            };

            if let Err(e) = spawn_agent_for_issue(&issue, workspace, no_workspace, prompt_file) {
                print_error(&format!("spawning agent for issue {}: {}", issue, e));
                std::process::exit(1);
            }
        }
        None => {
            // No subcommand provided, print help
            println!("Otto - Autonomous agent runner for beads tasks\n");
            println!("Usage: otto <COMMAND>\n");
            println!("Commands:");
            println!("  start   Start otto in tmux (runs in background)");
            println!("  attach  Attach to a tmux window");
            println!("  ralph   Run the agent loop (default behavior)");
            println!("  spawn   Spawn a single agent for a specific issue");
            println!("\nFlags:");
            println!("  -h, --help     Print help");
            println!("  -V, --version  Print version");
            println!("\nExamples:");
            println!("  otto start              Start otto in tmux");
            println!("  otto attach             Attach to 'otto' window");
            println!("  otto attach ralph-willow Attach to specific window");
            println!("  otto ralph              Run in single-pass mode");
            println!("  otto ralph --watch      Run in watch mode (infinite loop)");
            println!("  otto ralph -p FILE      Use custom prompt file");
            println!("                         (auto-detects PROMPT_RALPH.md if found)");
            println!("  otto spawn -i otto-123  Spawn agent for issue otto-123");
            println!("  otto spawn -i otto-123 --workspace ../agents/feature-x");
            println!("                         Spawn agent in isolated workspace");
        }
    }
}

/// Start otto in tmux.
///
/// This function:
/// 1. Ensures tmux server is running (starts if needed)
/// 2. Ensures the 'otto' tmux session exists (creates if needed)
/// 3. Creates a window named 'otto' (if it doesn't exist)
/// 4. Runs 'otto ralph --watch' in that window
/// 5. Prints confirmation with window name
fn start_otto() -> Result<(), Box<dyn std::error::Error>> {
    use otto_tmux::{ensure_session, send_command_to_window, window_exists, OTTO_SESSION_NAME};

    const OTTO_WINDOW_NAME: &str = "otto";

    // Ensure the otto session exists
    ensure_session(OTTO_SESSION_NAME)?;

    // Check if the 'otto' window already exists
    let window_already_existed = window_exists(OTTO_SESSION_NAME, OTTO_WINDOW_NAME)?;

    if !window_already_existed {
        // Create the window named 'otto'
        otto_tmux::create_named_window(OTTO_SESSION_NAME, OTTO_WINDOW_NAME)?;
    }

    // Send 'otto ralph --watch' command to the window
    send_command_to_window(OTTO_SESSION_NAME, OTTO_WINDOW_NAME, "otto ralph --watch")?;

    if window_already_existed {
        println!("Started otto in existing window: {}", OTTO_WINDOW_NAME);
    } else {
        println!("Started otto in new window: {}", OTTO_WINDOW_NAME);
    }
    println!("Attach with: tmux attach-session -t {}:{}", OTTO_SESSION_NAME, OTTO_WINDOW_NAME);

    Ok(())
}

/// Attach to a tmux window.
///
/// This function:
/// 1. Parses the window argument to extract session and window name
/// 2. Checks if the window exists
/// 3. Attaches to the window using tmux attach-session (replaces otto process)
///
/// # Arguments
/// * `window` - Optional window specification. Can be:
///   - None: attaches to 'otto' window
///   - "ralph-xxx": attaches to 'otto:ralph-xxx' window (short form)
///   - "otto:ralph-xxx": attaches to 'otto:ralph-xxx' window (full spec)
fn attach_to_window(window: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    use otto_tmux::{attach_to_window as tmux_attach, list_windows, session_exists, OTTO_SESSION_NAME};

    const DEFAULT_WINDOW_NAME: &str = "otto";

    // Parse the window argument
    let (session, window_name) = match window {
        None => (OTTO_SESSION_NAME.to_string(), DEFAULT_WINDOW_NAME.to_string()),
        Some(w) => {
            if w.contains(':') {
                // Full spec: "session:window"
                let parts: Vec<&str> = w.split(':').collect();
                if parts.len() == 2 {
                    (parts[0].to_string(), parts[1].to_string())
                } else {
                    return Err(format!("Invalid window specification: {}", w).into());
                }
            } else {
                // Short form: just window name, use otto session
                (OTTO_SESSION_NAME.to_string(), w)
            }
        }
    };

    // Check if session exists
    match session_exists(&session) {
        Ok(true) => {}
        Ok(false) => {
            return Err(format!(
                "Session '{}' does not exist. Start otto with 'otto start'",
                session
            )
            .into());
        }
        Err(e) => {
            return Err(format!("Failed to check session: {}", e).into());
        }
    }

    // Check if window exists
    match list_windows(&session) {
        Ok(windows) => {
            if !windows.contains(&window_name) {
                print_error(&format!("Window '{}' does not exist in session '{}'", window_name, session));
                println!("\nAvailable windows:");
                for w in windows {
                    println!("  - {}", w);
                }
                println!("\nUsage:");
                println!("  otto attach              Attach to 'otto' window");
                println!("  otto attach <window>     Attach to specific window (e.g., 'ralph-willow')");
                println!("  otto attach otto:<win>   Attach with full spec");
                return Err("Window not found".into());
            }
        }
        Err(e) => {
            return Err(format!("Failed to list windows: {}", e).into());
        }
    }

    // Attach to the window (this replaces the otto process)
    tmux_attach(&session, &window_name)?;

    // This line is never reached because exec replaces the process
    Ok(())
}

fn run_single_pass(prompt_file: Option<&str>) {
    loop {
        // Check if shutdown was requested
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("Shutting down gracefully");
            return;
        }

        // Check if there are ready beads
        match has_ready_tasks() {
            Ok(true) => {
                // Ready beads exist, launch an agent
                println!("Starting agent...");
                // Create abort callback that checks SHUTDOWN_REQUESTED
                let abort_callback: AbortCallback = || {
                    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
                };
                match launch_agent_default(prompt_file, Some(abort_callback)) {
                    Ok((duration, window_name)) => {
                        println!(
                            "Agent finished in {} (duration: {})",
                            window_name,
                            format_duration(duration)
                        );
                    }
                    Err(AgentError::AgentTimeout) => {
                        print_warning("Agent timed out");
                    }
                    Err(e) => {
                        print_error(&format!("launching agent: {}", e));
                        return;
                    }
                }

                // Check for shutdown again after agent finishes
                if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                    println!("Shutting down gracefully");
                    return;
                }
            }
            Ok(false) => {
                // No ready beads, exit
                println!("No ready beads, exiting");
                return;
            }
            Err(BeadsError::NotInitialized) => {
                print_error("beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                print_error(&format!("checking for ready tasks: {}", e));
                return;
            }
        }
    }
}

fn run_watch_loop(prompt_file: Option<&str>) {
    // Start the stuck window monitoring thread
    let _monitor_handle = start_stuck_window_monitor();

    loop {
        // Check if shutdown was requested
        if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
            println!("Shutting down gracefully");
            return;
        }

        // Check if there are ready beads
        match has_ready_tasks() {
            Ok(true) => {
                // Ready beads exist, launch an agent
                println!("Starting agent...");
                // Create abort callback that checks SHUTDOWN_REQUESTED
                let abort_callback: AbortCallback = || {
                    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
                };
                match launch_agent_default(prompt_file, Some(abort_callback)) {
                    Ok((duration, window_name)) => {
                        println!(
                            "Agent finished in {} (duration: {})",
                            window_name,
                            format_duration(duration)
                        );
                    }
                    Err(AgentError::AgentTimeout) => {
                        print_warning("Agent timed out");
                    }
                    Err(e) => {
                        print_error(&format!("launching agent: {}", e));
                        // In watch mode, continue on errors
                    }
                }

                // Check for shutdown again after agent finishes
                if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                    println!("Shutting down gracefully");
                    return;
                }
            }
            Ok(false) => {
                // No ready beads, wait a bit before checking again
                println!("No ready beads, waiting...");

                // Sleep in 1-second intervals to allow shutdown checking
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        println!("Shutting down gracefully");
                        return;
                    }
                }
            }
            Err(BeadsError::NotInitialized) => {
                print_error("beads not initialized (no .beads directory)");
                return;
            }
            Err(e) => {
                print_error(&format!("checking for ready tasks: {}", e));
                // In watch mode, continue on errors

                // Sleep in 1-second intervals to allow shutdown checking
                for _ in 0..10 {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        println!("Shutting down gracefully");
                        return;
                    }
                }
            }
        }
    }
}
