use clap::{Parser, Subcommand};
use otto_agent_claude::AbortCallback;
use otto_beads::{has_ready_tasks, BeadsError};
use otto_core::{launch_agent_default, start_stuck_window_monitor, AgentError};
use otto_log::color::{print_error, print_warning};
use std::io::Write;
use std::path::Path;
use std::str;
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

    /// Agent self-termination command
    ///
    /// Orchestrates clean agent exit with validation, cleanup, and Claude shutdown.
    /// This is the critical "land the plane" command that all agents must run when complete.
    Done {
        /// Exit mode: completed or escalated (default: completed)
        ///
        /// completed: Validate git state, push changes, sync beads, close hook, exit
        /// escalated: Skip validation, preserve hook bead for recovery, exit
        #[arg(long)]
        mode: Option<String>,

        /// Git state observation for escalated mode (optional)
        ///
        /// Records the observed git state when escalating:
        ///   clean       - Working tree clean, all pushed
        ///   uncommitted - Uncommitted changes present
        ///   unpushed    - Committed but not pushed
        #[arg(long)]
        status: Option<String>,

        /// Explicit issue ID (optional)
        ///
        /// If not provided, will attempt auto-detection from environment or beads state.
        #[arg(long, short = 'i')]
        issue: Option<String>,

        /// Delete workspace after completion (completed mode only)
        ///
        /// Requires clean git state and confirmation (unless --yes flag).
        #[arg(long)]
        nuke: bool,

        /// Skip confirmation prompts (for --nuke flag)
        #[arg(long)]
        yes: bool,

        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
    },

    /// Pre-flight check validation
    ///
    /// Validates the environment is properly configured for agents to work.
    /// This should be called before starting work to ensure everything is ready.
    PreFlightCheck,

    /// Workspace management commands
    ///
    /// Manage git worktrees used as agent workspaces for isolated work.
    Workspace {
        #[command(subcommand)]
        workspace_command: WorkspaceCommands,
    },
}

#[derive(Subcommand, Debug)]
enum WorkspaceCommands {
    /// List all workspaces
    ///
    /// Shows all git worktrees with their status information.
    List,

    /// Show workspace metadata
    ///
    /// Display detailed information about a specific workspace from its .workspace-info file.
    Show {
        /// Path to the workspace
        path: String,
    },

    /// Remove a workspace
    ///
    /// Remove a specific workspace directory using git worktree remove.
    Remove {
        /// Path to the workspace to remove
        path: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Clean up all workspaces
    ///
    /// Remove all agent workspaces in the ../agents directory.
    Clean {
        /// Skip confirmation prompts
        #[arg(long)]
        force: bool,
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

    #[test]
    fn test_args_parsing_done_command_completed() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "done"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { mode, status, issue, nuke, yes, dry_run }) => {
                assert_eq!(mode, None);
                assert_eq!(status, None);
                assert_eq!(issue, None);
                assert!(!nuke);
                assert!(!yes);
                assert!(!dry_run);
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_done_command_escalated() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "done", "--mode", "escalated"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { mode, status, .. }) => {
                assert_eq!(mode, Some("escalated".to_string()));
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_done_command_with_status() {
        use clap::Parser;

        let args = Args::try_parse_from([
            "otto", "done", "--mode", "escalated", "--status", "uncommitted",
        ]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { mode, status, .. }) => {
                assert_eq!(mode, Some("escalated".to_string()));
                assert_eq!(status, Some("uncommitted".to_string()));
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_done_command_with_issue() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "done", "--issue", "otto-123"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { issue, .. }) => {
                assert_eq!(issue, Some("otto-123".to_string()));
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_done_command_nuke() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "done", "--nuke", "--yes"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { nuke, yes, .. }) => {
                assert!(nuke);
                assert!(yes);
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_done_command_dry_run() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "done", "--dry-run"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Done { dry_run, .. }) => {
                assert!(dry_run);
            }
            _ => panic!("Expected Done command"),
        }
    }

    #[test]
    fn test_args_parsing_pre_flight_check() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "pre-flight-check"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::PreFlightCheck) => {}
            _ => panic!("Expected PreFlightCheck command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_list() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "list"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::List => {}
                    _ => panic!("Expected Workspace::List command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_show() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "show", "../agents/test"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::Show { path } => {
                        assert_eq!(path, "../agents/test");
                    }
                    _ => panic!("Expected Workspace::Show command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_remove() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "remove", "../agents/test"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::Remove { path, force } => {
                        assert_eq!(path, "../agents/test");
                        assert!(!force);
                    }
                    _ => panic!("Expected Workspace::Remove command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_remove_force() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "remove", "../agents/test", "--force"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::Remove { path, force } => {
                        assert_eq!(path, "../agents/test");
                        assert!(force);
                    }
                    _ => panic!("Expected Workspace::Remove command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_clean() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "clean"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::Clean { force } => {
                        assert!(!force);
                    }
                    _ => panic!("Expected Workspace::Clean command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_args_parsing_workspace_clean_force() {
        use clap::Parser;

        let args = Args::try_parse_from(["otto", "workspace", "clean", "--force"]);
        assert!(args.is_ok());
        let args = args.unwrap();
        match args.command {
            Some(Commands::Workspace { workspace_command }) => {
                match workspace_command {
                    WorkspaceCommands::Clean { force } => {
                        assert!(force);
                    }
                    _ => panic!("Expected Workspace::Clean command"),
                }
            }
            _ => panic!("Expected Workspace command"),
        }
    }

    #[test]
    fn test_exit_claude_function_exists() {
        // Test that exit_claude function signature compiles
        let _ = exit_claude as fn(&str, u64) -> Result<(), String>;
    }

    #[test]
    fn test_exit_claude_no_tmux_no_process() {
        // When not in tmux and no Claude processes, should return Err
        let result = exit_claude("test", 1);
        // Should fail because no Claude processes are running
        assert!(result.is_err());
    }

    #[test]
    fn test_exit_claude_with_tmux_pane_env() {
        // When TMUX_PANE is set, should try to use pane-specific kill
        // This test sets a fake TMUX_PANE env var and verifies the function
        // doesn't panic (it will fail to find the pane, but shouldn't crash)
        std::env::set_var("TMUX_PANE", "%0");
        let result = exit_claude("test", 1);
        // Should either succeed or fail gracefully (not panic)
        let _ = result;
        std::env::remove_var("TMUX_PANE");
    }
}

/// Run the otto done command for agent self-termination.
///
/// This function orchestrates clean agent exit with validation, cleanup, and Claude shutdown.
/// It supports two modes:
/// - completed: Full validation, close bead, clear state, exit
/// - escalated: Skip validation, leave bead open, preserve state, exit
///
/// # Arguments
/// * `mode` - Exit mode (Some("completed") or Some("escalated"))
/// * `status_observation` - Git state observation for escalated mode
/// * `issue_id` - Explicit issue ID (optional, will auto-detect if None)
/// * `nuke_workspace` - Whether to delete workspace after completion
/// * `yes_flag` - Skip confirmation prompts
/// * `dry_run` - Show what would happen without executing
///
/// # Returns
/// - `Ok(())` if termination sequence completes successfully
/// - `Err(String)` if there was an error
fn run_done_command(
    mode: Option<String>,
    status_observation: Option<String>,
    issue_id: Option<String>,
    nuke_workspace: bool,
    yes_flag: bool,
    dry_run: bool,
) -> Result<(), String> {
    use std::fs;
    use std::process::Command;

    // Parse and validate mode (default to "completed")
    let mode = mode.as_deref().unwrap_or("completed");
    if mode != "completed" && mode != "escalated" {
        return Err(format!(
            "Invalid mode: '{}' (must be 'completed' or 'escalated')",
            mode
        ));
    }

    // Validate: --status only allowed with --mode escalated
    if status_observation.is_some() && mode != "escalated" {
        return Err("--status option can only be used with --mode escalated".to_string());
    }

    // Validate: --nuke only allowed with --mode completed
    if nuke_workspace && mode != "completed" {
        return Err("--nuke option can only be used with --mode completed".to_string());
    }

    // Validate status observation value if provided
    if let Some(ref status) = status_observation {
        if status != "clean" && status != "uncommitted" && status != "unpushed" {
            return Err(format!(
                "Invalid status: '{}' (must be 'clean', 'uncommitted', or 'unpushed')",
                status
            ));
        }
    }

    // Detect issue ID if not explicitly provided
    let issue_id = if let Some(id) = issue_id {
        id
    } else {
        detect_issue_id()?
    };

    // Log configuration
    if dry_run {
        println!("DRY RUN MODE - No changes will be made\n");
    }
    println!("Otto termination initiated");
    println!("Mode: {}", mode);
    if !issue_id.is_empty() {
        println!("Issue: {}", issue_id);
    }
    println!();

    // Get project root
    let project_root = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;

    // Paths
    let hook_file = project_root.join(".beads").join("hook");

    if mode == "completed" {
        // Completed mode workflow
        println!("Step 1: Validating git state...");

        if dry_run {
            println!("  [DRY RUN] Would validate git state (skipped in dry-run)");
        } else {
            // Validate git state
            if let Err(e) = validate_git_state() {
                println!();
                return Err(format!("Git state validation failed: {}", e));
            }
            println!("✓ Git state validation passed");
        }

        // Step 2: Sync beads
        println!("Step 2: Syncing beads...");
        if dry_run {
            println!("  [DRY RUN] Would run: bd sync");
        } else {
            let output = Command::new("bd")
                .arg("sync")
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    println!("✓ Beads synced");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(format!("bd sync failed: {}", stderr));
                }
                Err(e) => {
                    return Err(format!("Failed to run bd sync: {}", e));
                }
            }
        }

        // Step 3: Close hooked bead
        println!("Step 3: Closing hooked bead (if any)...");
        let hooked_bead_id = detect_hooked_bead(&hook_file)?;
        if let Some(bead_id) = hooked_bead_id {
            if dry_run {
                println!("  [DRY RUN] Would run: bd close {}", bead_id);
            } else {
                let output = Command::new("bd")
                    .args(["close", &bead_id, "--reason=Issue completed via otto done"])
                    .output();
                match output {
                    Ok(output) if output.status.success() => {
                        println!("✓ Closed hooked bead: {}", bead_id);
                    }
                    Ok(_) => {
                        println!("⚠ Warning: Failed to close bead {} (continuing)", bead_id);
                    }
                    Err(_) => {
                        println!("⚠ Warning: Failed to close bead {} (continuing)", bead_id);
                    }
                }
            }
        } else {
            println!("  No hooked bead to close");
        }

        // Step 4: Clear hook bead state
        println!("Step 4: Clearing hook bead state...");
        if dry_run {
            println!("  [DRY RUN] Would clear hook bead state");
        } else {
            if hook_file.exists() {
                if fs::remove_file(&hook_file).is_ok() {
                    println!("✓ Hook bead state cleared");
                } else {
                    println!("⚠ Warning: Failed to clear hook bead state (continuing)");
                }
            } else {
                println!("  Hook file does not exist (nothing to clear)");
            }
        }

        // Step 5: Nuke workspace (if --nuke flag)
        if nuke_workspace {
            println!("Step 5: Cleaning up workspace...");
            if let Err(e) = nuke_workspace_helper(&project_root, yes_flag, dry_run) {
                println!();
                println!("⚠ Warning: Workspace cleanup failed: {}", e);
                println!("Workspace cleanup is optional, but termination will continue");
            }
            println!();
        }

        // Step 6: Exit Claude cleanly
        if dry_run {
            println!("Step 6: [DRY RUN] Would exit Claude cleanly");
        } else {
            println!("Step 6: Exiting Claude cleanly...");
            if let Err(e) = exit_claude("completed", 5) {
                println!("⚠ Warning: Claude exit encountered issues: {}", e);
            } else {
                println!("✓ Claude exit initiated");
            }
        }
    } else {
        // Escalated mode workflow
        println!("Escalated mode workflow:");
        println!("  ✓ Skip validation (escalated mode)");
        println!(
            "  ✓ Git state observation: {}",
            status_observation.as_deref().unwrap_or("unknown")
        );

        // Step 2: Attempt bd sync (best effort)
        println!("Step 2: Attempting bd sync (best effort)...");
        if dry_run {
            println!("  [DRY RUN] Would run: bd sync");
        } else {
            let output = Command::new("bd")
                .arg("sync")
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    println!("✓ Beads synced (best effort)");
                }
                _ => {
                    println!("⚠ Warning: Beads sync failed (continuing anyway - escalated mode)");
                }
            }
        }

        // Step 3: Detect hooked bead for recovery
        println!("Step 3: Detecting hooked bead for recovery...");
        let hooked_bead_id = detect_hooked_bead(&hook_file)?;
        if let Some(ref bead_id) = hooked_bead_id {
            println!("  ✓ Leaving hooked bead open: {}", bead_id);
            println!("  ✓ Run 'otto ralph {}' to resume work", bead_id);
        } else {
            println!("  No hooked bead detected");
        }

        // Step 4: Exit Claude cleanly
        if dry_run {
            println!("Step 4: [DRY RUN] Would exit Claude cleanly");
        } else {
            println!("Step 4: Exiting Claude cleanly...");
            if let Err(e) = exit_claude("escalated", 5) {
                println!("⚠ Warning: Claude exit encountered issues: {}", e);
            } else {
                println!("✓ Claude exit initiated");
            }
        }
    }

    println!();
    println!("✓ Termination sequence complete");

    if dry_run {
        println!();
        println!("Dry run complete - no changes made");
        println!("Exit mechanism would have been triggered");
    }

    Ok(())
}

/// Detect the currently hooked bead ID.
///
/// Checks OTTO_CURRENT_BEAD env var, then .beads/hook file.
///
/// # Arguments
/// * `hook_file` - Path to the .beads/hook file
///
/// # Returns
/// - `Ok(Some(bead_id))` if a hooked bead is found
/// - `Ok(None)` if no hooked bead is found
/// - `Err(String)` if there was an error reading the hook file
fn detect_hooked_bead(hook_file: &Path) -> Result<Option<String>, String> {
    use std::fs;

    // Try OTTO_CURRENT_BEAD environment variable first
    if let Ok(bead_id) = std::env::var("OTTO_CURRENT_BEAD") {
        if !bead_id.is_empty() {
            return Ok(Some(bead_id));
        }
    }

    // Try reading from .beads/hook file
    if hook_file.exists() {
        let content = fs::read_to_string(hook_file)
            .map_err(|e| format!("Failed to read hook file: {}", e))?;
        let bead_id = content.trim();
        if !bead_id.is_empty() {
            return Ok(Some(bead_id.to_string()));
        }
    }

    Ok(None)
}

/// Auto-detect issue ID from environment or beads state.
///
/// # Returns
/// - `Ok(issue_id)` - The detected issue ID (or empty string if none detected)
/// - `Err(String)` if there was an error
fn detect_issue_id() -> Result<String, String> {
    use std::fs;

    // Try BEAD_ID environment variable (set by agents)
    if let Ok(bead_id) = std::env::var("BEAD_ID") {
        if !bead_id.is_empty() {
            return Ok(bead_id);
        }
    }

    // Try to read from .beads issues.jsonl (most recent bead)
    let project_root = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;
    let issues_file = project_root.join(".beads").join("issues.jsonl");

    if issues_file.exists() {
        // Read the last line (most recent issue)
        let content = fs::read_to_string(&issues_file)
            .map_err(|e| format!("Failed to read issues.jsonl: {}", e))?;

        if let Some(last_line) = content.lines().last() {
            // Parse JSON to extract the issue ID
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(last_line) {
                if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                    return Ok(id.to_string());
                }
            }
        }
    }

    // No issue ID detected - return empty string (not an error)
    Ok(String::new())
}

/// Validate git working directory state.
///
/// Checks:
/// 1. Working tree is clean (no uncommitted changes)
/// 2. All commits are pushed to remote
/// 3. No git stashes
///
/// # Returns
/// - `Ok(())` if all validations pass
/// - `Err(String)` with error message if validation fails
fn validate_git_state() -> Result<(), String> {
    use std::process::Command;

    // Check 1: Working tree clean
    let output = Command::new("git")
        .args(["diff", "--quiet"])
        .output();
    match output {
        Ok(output) if output.status.success() => {}
        _ => {
            return Err("Working tree has uncommitted changes (run git status)".to_string());
        }
    }

    let output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .output();
    match output {
        Ok(output) if output.status.success() => {}
        _ => {
            return Err("Working tree has staged changes (run git status)".to_string());
        }
    }

    // Check 2: All commits pushed
    let branch = get_current_branch()?;
    let main_branch = get_main_branch()?;

    // Check for unpushed commits
    let output = Command::new("git")
        .args(["log", &format!("origin/{}..{}", main_branch, branch)])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                return Err("There are unpushed commits (run git push)".to_string());
            }
        }
        Err(e) => {
            // If we can't check, log a warning but don't fail
            eprintln!("Warning: Could not check for unpushed commits: {}", e);
        }
    }

    // Check 3: No stashes
    let output = Command::new("git")
        .args(["stash", "list"])
        .output();
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                return Err("You have git stashes (run git stash list)".to_string());
            }
        }
        Err(_) => {
            // Don't fail if we can't check stashes
        }
    }

    Ok(())
}

/// Get the current git branch name.
///
/// # Returns
/// - `Ok(branch_name)` - The current branch name
/// - `Err(String)` if there was an error
fn get_current_branch() -> Result<String, String> {
    use std::process::Command;

    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| format!("Failed to get current branch: {}", e))?;

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        return Err("Could not determine current branch".to_string());
    }

    Ok(branch)
}

/// Get the main branch name (main or master).
///
/// # Returns
/// - `Ok(branch_name)` - The main branch name ("main" or "master")
fn get_main_branch() -> Result<String, String> {
    use std::process::Command;

    // First check if there's a remote origin with a refs/remotes/origin/HEAD
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(branch) = stdout.strip_prefix("refs/remotes/origin/") {
                return Ok(branch.trim().to_string());
            }
        }
    }

    // Check if main branch exists
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "main"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            return Ok("main".to_string());
        }
    }

    // Check if master branch exists
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "master"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            return Ok("master".to_string());
        }
    }

    // Fallback to "main" as default
    Ok("main".to_string())
}

/// Remove workspace after completion.
///
/// # Arguments
/// * `project_root` - Path to the project root
/// * `yes_flag` - Skip confirmation prompts
/// * `dry_run` - Show what would happen without executing
///
/// # Returns
/// - `Ok(())` if workspace nuke succeeds
/// - `Err(String)` if there was an error
fn nuke_workspace_helper(
    project_root: &Path,
    yes_flag: bool,
    dry_run: bool,
) -> Result<(), String> {
    use std::fs;
    use std::process::Command;

    // Get workspace path from OTTO_WORKSPACE environment variable
    let workspace_path = std::env::var("OTTO_WORKSPACE")
        .map_err(|_| "No workspace to clean up (OTTO_WORKSPACE not set)".to_string())?;

    let workspace_path = Path::new(&workspace_path);

    // Verify workspace exists
    if !workspace_path.exists() {
        return Err(format!("Workspace path does not exist: {:?}", workspace_path));
    }

    // Get relative path for display
    let rel_path = workspace_path
        .strip_prefix(project_root)
        .unwrap_or(workspace_path);
    println!("Workspace: {}", rel_path.display());

    // Get branch name
    let output = Command::new("git")
        .args(["-C", workspace_path.to_str().unwrap(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout);
            println!("Branch: {}", branch.trim());
        }
    }

    // Get bead ID from .workspace-info
    let workspace_info = workspace_path.join(".workspace-info");
    if workspace_info.exists() {
        if let Ok(content) = fs::read_to_string(&workspace_info) {
            for line in content.lines() {
                if line.starts_with("issue_id=") {
                    let bead_id = line.strip_prefix("issue_id=").unwrap_or("");
                    if !bead_id.is_empty() {
                        println!("Bead: {}", bead_id);
                    }
                    break;
                }
            }
        }
    }

    println!();

    // Safety check: verify workspace is clean
    let output = Command::new("git")
        .args(["-C", workspace_path.to_str().unwrap(), "status", "--porcelain"])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                return Err("Workspace has uncommitted changes or untracked files".to_string());
            }
        }
        Err(e) => {
            return Err(format!("Failed to check workspace status: {}", e));
        }
    }

    // Confirm removal (unless --yes flag)
    if !yes_flag {
        if dry_run {
            println!("  [DRY RUN] Would prompt: Remove workspace '{}'? [y/N]", rel_path.display());
        } else {
            print!("Remove workspace '{}'? [y/N] ", rel_path.display());
            use std::io::Write;
            std::io::stdout().flush().unwrap();

            let mut input = String::new();
            std::io::stdin().read_line(&mut input).map_err(|e| format!("Failed to read input: {}", e))?;

            if !input.trim().to_lowercase().starts_with('y') {
                println!("Workspace cleanup cancelled");
                return Ok(());
            }
        }
    } else {
        println!("Skipping confirmation (--yes flag set)");
    }

    // Remove workspace
    if dry_run {
        println!("  [DRY RUN] Would run: git worktree remove --force {}", workspace_path.display());
        println!("  [DRY RUN] Would run: git worktree prune");
    } else {
        let output = Command::new("git")
            .args(["worktree", "remove", "--force", workspace_path.to_str().unwrap()])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                println!("✓ Removed workspace {}", rel_path.display());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Failed to remove workspace: {}", stderr));
            }
            Err(e) => {
                return Err(format!("Failed to run git worktree remove: {}", e));
            }
        }

        // Prune orphaned worktrees metadata
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .output();
    }

    println!("✓ Workspace cleanup complete");
    Ok(())
}

/// Exit Claude Code by sending SIGTERM to the current Claude process.
///
/// This function attempts to kill only the Claude process in the current tmux pane,
/// not all Claude processes globally. If not in a tmux session or if the pane
/// cannot be detected, it falls back to the old behavior of killing all Claude processes.
///
/// # Arguments
/// * `mode` - Exit mode (for logging purposes)
/// * `timeout` - Timeout in seconds before force kill
///
/// # Returns
/// - `Ok(())` if Claude exit was initiated successfully
/// - `Err(String)` if there was an error
fn exit_claude(_mode: &str, timeout: u64) -> Result<(), String> {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    // Try to get the current tmux pane PID
    // Check if we're in a tmux session by looking for TMUX_PANE environment variable
    if let Ok(pane_id) = std::env::var("TMUX_PANE") {
        // We're in a tmux session, try to kill only the Claude in this pane
        // Use pane_current_command to check if claude is running
        let output = Command::new("tmux")
            .args(["display-message", "-p", "-t", &pane_id, "#{pane_pid}"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(pid) = pid_str.parse::<u32>() {
                    // Check if this is a Claude process by examining /proc
                    let cmdline_path = format!("/proc/{}/cmdline", pid);
                    if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                        if cmdline.contains("claude") {
                            // This is a Claude process, kill it
                            let _ = Command::new("kill")
                                .args(["-TERM", &pid.to_string()])
                                .output();

                            // Wait for process to terminate
                            for _ in 0..timeout {
                                let _ = Command::new("kill")
                                    .args(["-0", &pid.to_string()])
                                    .output();

                                thread::sleep(Duration::from_secs(1));
                            }

                            // Force kill if still running
                            let _ = Command::new("kill")
                                .args(["-KILL", &pid.to_string()])
                                .output();

                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    // Fallback: Not in tmux or couldn't detect pane, use old global method
    // This is the legacy behavior that kills all Claude processes
    // Note: This should rarely be used in practice with otto ralph workflows
    let output = Command::new("pgrep")
        .args(["-f", "claude"])
        .output();

    let claude_pids: Vec<u32> = match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout
                .lines()
                .filter_map(|line| line.trim().parse::<u32>().ok())
                .collect()
        }
        _ => {
            return Err("No Claude processes found".to_string());
        }
    };

    if claude_pids.is_empty() {
        return Err("No Claude processes found".to_string());
    }

    // Send SIGTERM to all Claude processes for graceful shutdown
    for pid in &claude_pids {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }

    // Wait for processes to terminate with timeout
    for _ in 0..timeout {
        let output = Command::new("pgrep")
            .args(["-f", "claude"])
            .output();

        match output {
            Ok(output) if !output.status.success() => {
                // All Claude processes terminated
                return Ok(());
            }
            _ => {
                thread::sleep(Duration::from_secs(1));
            }
        }
    }

    // Timeout - force kill with SIGKILL
    let output = Command::new("pgrep")
        .args(["-f", "claude"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    let _ = Command::new("kill")
                        .args(["-KILL", &pid.to_string()])
                        .output();
                }
            }
        }
    }

    thread::sleep(Duration::from_secs(1));

    // Final check
    let output = Command::new("pgrep")
        .args(["-f", "claude"])
        .output();

    match output {
        Ok(output) if !output.status.success() => Ok(()),
        Ok(_) => Err("Failed to terminate all Claude processes".to_string()),
        Err(_) => Ok(()),
    }
}

/// Run pre-flight check validation.
///
/// Validates the environment is properly configured for agents to work.
/// Checks:
/// 1. Git repository status
/// 2. Beads initialization
/// 3. Beads sync status
/// 4. Uncommitted changes
/// 5. Unpushed commits
///
/// # Returns
/// - `Ok(())` if all checks pass
/// - `Err(String)` with error message if validation fails
fn run_pre_flight_check() -> Result<(), String> {
    use std::process::Command;

    println!("Running otto pre-flight checks...");
    println!();

    let mut all_passed = true;

    // Check 1: Git repository
    println!("Check 1: Git repository...");
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("  ✓ Git repository detected");
        }
        _ => {
            println!("  ✗ Not a git repository");
            all_passed = false;
        }
    }
    println!();

    // Get project root for .beads check
    let project_root = std::env::current_dir()
        .map_err(|e| format!("Failed to get current directory: {}", e))?;

    // Check 2: Beads initialized
    println!("Check 2: Beads initialization...");
    let beads_dir = project_root.join(".beads");
    if beads_dir.is_dir() {
        println!("  ✓ Beads initialized");
    } else {
        println!("  ✗ Beads not initialized (no .beads directory)");
        println!("  Run 'bd init' to initialize beads");
        all_passed = false;
    }
    println!();

    // Check 3: Beads sync status (warning only, don't fail)
    println!("Check 3: Beads sync status...");
    let output = Command::new("bd")
        .args(["sync", "--status"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("  ✓ Beads sync status OK");
        }
        _ => {
            println!("  ⚠ Beads sync may be needed");
            println!("  Run 'bd sync' to synchronize with remote");
            // Don't fail on this, just warn
        }
    }
    println!();

    // Check 4: Working tree clean
    println!("Check 4: Working tree status...");
    let output = Command::new("git")
        .args(["diff", "--quiet"])
        .output();

    match output {
        Ok(output) if output.status.success() => {}
        _ => {
            println!("  ✗ Working tree has uncommitted changes");
            println!("  Run 'git status' to see changes");
            println!("  Commit or stash changes before starting work");
            all_passed = false;
        }
    }

    let output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            println!("  ✓ Working tree is clean");
        }
        _ => {
            println!("  ✗ Working tree has staged changes");
            println!("  Run 'git status' to see changes");
            println!("  Commit or stash changes before starting work");
            all_passed = false;
        }
    }
    println!();

    // Check 5: Commits pushed
    println!("Check 5: Commit push status...");
    let branch = get_current_branch()?;
    let main_branch = get_main_branch()?;

    // Check for unpushed commits
    let output = Command::new("git")
        .args(["log", &format!("origin/{}..{}", main_branch, branch)])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if !stdout.trim().is_empty() {
                println!("  ✗ There are unpushed commits");
                println!("  Run 'git push' to push commits");
                all_passed = false;
            } else {
                println!("  ✓ All commits are pushed");
            }
        }
        Err(e) => {
            // If we can't check, log a warning but don't fail
            println!("  ⚠ Could not check for unpushed commits: {}", e);
        }
    }
    println!();

    // Summary
    if all_passed {
        println!("✓ All pre-flight checks passed!");
        println!();
        println!("Environment is ready for agent work");
        Ok(())
    } else {
        println!("✗ Some pre-flight checks failed");
        println!();
        println!("Please fix the issues above before starting work");
        Err("Pre-flight checks failed".to_string())
    }
}

/// Run workspace management commands.
///
/// # Arguments
/// * `command` - The workspace subcommand to execute
///
/// # Returns
/// - `Ok(())` if the command executes successfully
/// - `Err(String)` if there was an error
fn run_workspace_command(command: WorkspaceCommands) -> Result<(), String> {
    use std::process::Command;

    match command {
        WorkspaceCommands::List => {
            // Get project root
            let project_root = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;

            // Get list of worktrees
            let output = Command::new("git")
                .args(["worktree", "list"])
                .output()
                .map_err(|e| format!("Failed to list worktrees: {}", e))?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!("Failed to list worktrees: {}", stderr));
            }

            let worktrees_output = String::from_utf8_lossy(&output.stdout);

            // Parse worktree list
            let mut workspaces = Vec::new();
            for line in worktrees_output.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                let path = parts[0];

                // Skip main worktree (project root)
                if path == project_root.to_str().unwrap_or("") {
                    continue;
                }

                // Get branch name (in brackets, e.g., [branch-name])
                let branch = if parts.len() > 1 {
                    let branch_part = parts[1..].join(" ");
                    if branch_part.starts_with('[') && branch_part.ends_with(']') {
                        branch_part[1..branch_part.len()-1].to_string()
                    } else if branch_part.contains("detached") {
                        // Get commit SHA for detached HEAD
                        Command::new("git")
                            .args(["-C", path, "rev-parse", "--short", "HEAD"])
                            .output()
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                            .unwrap_or_else(|_| "unknown".to_string())
                    } else {
                        "unknown".to_string()
                    }
                } else {
                    "unknown".to_string()
                };

                // Check if workspace is clean
                let status = if is_workspace_clean(path) {
                    "clean"
                } else {
                    "dirty"
                };

                // Get bead ID from .workspace-info
                let bead_id = std::fs::read_to_string(format!("{}/.workspace-info", path))
                    .ok()
                    .and_then(|content| {
                        content.lines()
                            .find(|l| l.starts_with("issue_id="))
                            .and_then(|l| l.strip_prefix("issue_id="))
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();

                workspaces.push((path, branch, status, bead_id));
            }

            // Print results
            if workspaces.is_empty() {
                println!("No agent workspaces found");
                println!();
                println!("Workspaces are created when using 'otto spawn --workspace <path>'");
                return Ok(());
            }

            // Print header
            println!("{:<22} {:<26} {:<9} {:<11} {}", "WORKSPACE", "BRANCH", "STATUS", "BEAD", "AGE");
            println!("{}", str::repeat("-", 80));

            // Print each workspace
            for (path, branch, status, bead_id) in workspaces {
                // Get relative path
                let path_buf = Path::new(&path);
                let rel_path = if let Ok(rel) = path_buf.strip_prefix(&project_root) {
                    rel.to_str().unwrap_or(&path)
                } else {
                    &path
                };

                // Get workspace age
                let age = get_workspace_age(&path);

                println!("{:<22} {:<26} {:<9} {:<11} {}", rel_path, branch, status, bead_id, age);
            }

            Ok(())
        }

        WorkspaceCommands::Show { path } => {
            // Resolve path
            let workspace_path = if Path::new(&path).is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&path).to_str().unwrap_or(path.as_str()).to_string())
                    .unwrap_or(path.clone())
            };

            // Check if workspace exists
            if !Path::new(&workspace_path).exists() {
                return Err(format!("Workspace path does not exist: {}", workspace_path));
            }

            // Check if it's a git worktree
            let output = Command::new("git")
                .args(["worktree", "list"])
                .output()
                .map_err(|e| format!("Failed to list worktrees: {}", e))?;

            let worktrees_output = String::from_utf8_lossy(&output.stdout);
            let is_worktree = worktrees_output.lines().any(|line| {
                line.starts_with(&workspace_path)
            });

            if !is_worktree {
                println!("Warning: Path is not a known git worktree");
            }

            // Read .workspace-info file
            let info_path = format!("{}/.workspace-info", workspace_path);
            let info_content = std::fs::read_to_string(&info_path);

            match info_content {
                Ok(content) => {
                    println!("Workspace: {}", workspace_path);
                    println!();
                    println!("Metadata:");

                    for line in content.lines() {
                        if line.starts_with('#') || line.trim().is_empty() {
                            continue;
                        }

                        if let Some((key, value)) = line.split_once('=') {
                            println!("  {}: {}", key, value);
                        }
                    }

                    // Get branch
                    let output = Command::new("git")
                        .args(["-C", &workspace_path, "rev-parse", "--abbrev-ref", "HEAD"])
                        .output();

                    if let Ok(output) = output {
                        if output.status.success() {
                            let branch = String::from_utf8_lossy(&output.stdout);
                            println!("  current_branch: {}", branch.trim());
                        }
                    }

                    // Check status
                    let status = if is_workspace_clean(&workspace_path) {
                        "clean"
                    } else {
                        "dirty"
                    };
                    println!("  status: {}", status);
                }
                Err(_) => {
                    println!("Workspace: {}", workspace_path);
                    println!();
                    println!("No .workspace-info file found");
                    println!();
                    println!("This workspace may have been created manually or the metadata file is missing.");
                }
            }

            Ok(())
        }

        WorkspaceCommands::Remove { path, force } => {
            // Resolve path
            let workspace_path = if Path::new(&path).is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()
                    .map(|cwd| cwd.join(&path).to_str().unwrap_or(path.as_str()).to_string())
                    .unwrap_or(path.clone())
            };

            // Check if workspace exists
            if !Path::new(&workspace_path).exists() {
                return Err(format!("Workspace path does not exist: {}", workspace_path));
            }

            // Check if it's a git worktree
            let output = Command::new("git")
                .args(["worktree", "list"])
                .output()
                .map_err(|e| format!("Failed to list worktrees: {}", e))?;

            let worktrees_output = String::from_utf8_lossy(&output.stdout);
            let is_worktree = worktrees_output.lines().any(|line| {
                line.starts_with(&workspace_path)
            });

            if !is_worktree {
                println!("Warning: Path is not a known git worktree");
            }

            // Check if workspace is clean (unless force flag)
            if !force && !is_workspace_clean(&workspace_path) {
                return Err("Workspace has uncommitted changes. Commit or stash changes before removing workspace. Use --force to remove anyway.".to_string());
            }

            // Get workspace info for display
            let project_root = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;

            let rel_path = Path::new(&workspace_path).strip_prefix(&project_root)
                .unwrap_or(Path::new(&workspace_path));

            let output = Command::new("git")
                .args(["-C", &workspace_path, "rev-parse", "--abbrev-ref", "HEAD"])
                .output();

            let branch = if let Ok(output) = output {
                if output.status.success() {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                } else {
                    "unknown".to_string()
                }
            } else {
                "unknown".to_string()
            };

            // Show workspace info
            println!("Workspace: {}", rel_path.display());
            println!("Branch: {}", branch);

            // Get bead ID
            let info_path = format!("{}/.workspace-info", workspace_path);
            if let Ok(content) = std::fs::read_to_string(&info_path) {
                for line in content.lines() {
                    if line.starts_with("issue_id=") {
                        if let Some(bead_id) = line.strip_prefix("issue_id=") {
                            println!("Bead: {}", bead_id);
                        }
                        break;
                    }
                }
            }

            println!();

            // Confirm removal
            if !force {
                print!("Remove workspace '{}'? [y/N] ", rel_path.display());
                use std::io::Write;
                std::io::stdout().flush().unwrap();

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)
                    .map_err(|e| format!("Failed to read input: {}", e))?;

                if !input.trim().to_lowercase().starts_with('y') {
                    println!("Cancelled");
                    return Ok(());
                }
            }

            // Remove worktree
            let output = Command::new("git")
                .args(["worktree", "remove", "--force", &workspace_path])
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    println!("✓ Removed workspace {}", rel_path.display());
                    Ok(())
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(format!("Failed to remove workspace: {}", stderr))
                }
                Err(e) => {
                    Err(format!("Failed to run git worktree remove: {}", e))
                }
            }
        }

        WorkspaceCommands::Clean { force } => {
            // Get project root
            let project_root = std::env::current_dir()
                .map_err(|e| format!("Failed to get current directory: {}", e))?;

            // Get agents directory
            let agents_dir = project_root.join("../agents");

            if !agents_dir.exists() {
                println!("No agents directory found");
                return Ok(());
            }

            // List all directories in agents
            let mut workspaces = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&agents_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        workspaces.push(path);
                    }
                }
            }

            if workspaces.is_empty() {
                println!("No workspaces found in {}", agents_dir.display());
                return Ok(());
            }

            println!("Found {} workspace(s):", workspaces.len());
            println!();

            for workspace in &workspaces {
                let rel_path = workspace.strip_prefix(&project_root).unwrap_or(workspace);
                println!("  - {}", rel_path.display());
            }

            println!();

            // Confirm removal
            if !force {
                print!("Remove all workspaces? [y/N] ");
                use std::io::Write;
                std::io::stdout().flush().unwrap();

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)
                    .map_err(|e| format!("Failed to read input: {}", e))?;

                if !input.trim().to_lowercase().starts_with('y') {
                    println!("Cancelled");
                    return Ok(());
                }
            }

            // Remove each workspace
            let mut removed = 0;
            let mut failed = 0;

            for workspace in &workspaces {
                let output = Command::new("git")
                    .args(["worktree", "remove", "--force", workspace.to_str().unwrap()])
                    .output();

                match output {
                    Ok(output) if output.status.success() => {
                        let rel_path = workspace.strip_prefix(&project_root).unwrap_or(workspace);
                        println!("✓ Removed {}", rel_path.display());
                        removed += 1;
                    }
                    _ => {
                        let rel_path = workspace.strip_prefix(&project_root).unwrap_or(workspace);
                        println!("✗ Failed to remove {}", rel_path.display());
                        failed += 1;
                    }
                }
            }

            // Prune orphaned worktrees
            println!();
            let _ = Command::new("git")
                .args(["worktree", "prune"])
                .output();

            println!();
            println!("Removed: {} workspace(s)", removed);
            if failed > 0 {
                println!("Failed: {} workspace(s)", failed);
            }

            Ok(())
        }
    }
}

/// Check if a workspace is clean (no uncommitted changes).
///
/// # Arguments
/// * `workspace_path` - Path to the workspace
///
/// # Returns
/// - `true` if the workspace is clean
/// - `false` if the workspace has uncommitted changes
fn is_workspace_clean(workspace_path: &str) -> bool {
    use std::process::Command;

    // Check for unstaged changes
    let output = Command::new("git")
        .args(["-C", workspace_path, "diff", "--quiet"])
        .output();

    let unstaged_ok = matches!(output, Ok(output) if output.status.success());

    // Check for staged changes
    let output = Command::new("git")
        .args(["-C", workspace_path, "diff", "--cached", "--quiet"])
        .output();

    let staged_ok = matches!(output, Ok(output) if output.status.success());

    // Check for untracked files (excluding .workspace-info and .beads)
    let output = Command::new("git")
        .args(["-C", workspace_path, "ls-files", "--others", "--exclude-standard"])
        .output();

    let untracked_ok = if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.lines()
            .all(|line| line == ".workspace-info" || line.starts_with(".beads/"))
    } else {
        false
    };

    unstaged_ok && staged_ok && untracked_ok
}

/// Get workspace age in human-readable format.
///
/// # Arguments
/// * `workspace_path` - Path to the workspace
///
/// # Returns
/// - Human-readable age string (e.g., "1h", "2d", "30m")
fn get_workspace_age(workspace_path: &str) -> String {
    use std::process::Command;

    // Try to get the first commit timestamp on the branch
    let output = Command::new("git")
        .args(["-C", workspace_path, "rev-list", "--max-parents=0", "HEAD"])
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !commit.is_empty() {
                let output = Command::new("git")
                    .args(["-C", workspace_path, "show", "-s", "--format=%ct", &commit])
                    .output();

                if let Ok(output) = output {
                    if output.status.success() {
                        let timestamp = String::from_utf8_lossy(&output.stdout).trim().parse().unwrap_or(0);

                        use std::time::{SystemTime, UNIX_EPOCH};
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        let age = now.saturating_sub(timestamp);
                        let days = age / 86400;
                        let hours = age / 3600;
                        let minutes = age / 60;

                        if days > 0 {
                            return format!("{}d", days);
                        } else if hours > 0 {
                            return format!("{}h", hours);
                        } else {
                            return format!("{}m", minutes);
                        }
                    }
                }
            }
        }
    }

    // Fallback: check directory modification time
    if let Ok(metadata) = std::fs::metadata(workspace_path) {
        if let Ok(modified) = metadata.modified() {
            use std::time::{SystemTime, UNIX_EPOCH};
            let modified_time = modified
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let age = now.saturating_sub(modified_time);
            let days = age / 86400;
            let hours = age / 3600;
            let minutes = age / 60;

            if days > 0 {
                return format!("{}d", days);
            } else if hours > 0 {
                return format!("{}h", hours);
            } else {
                return format!("{}m", minutes);
            }
        }
    }

    "unknown".to_string()
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
        Some(Commands::Done { mode, status, issue, nuke, yes, dry_run }) => {
            if let Err(e) = run_done_command(mode, status, issue, nuke, yes, dry_run) {
                print_error(&format!("otto done: {}", e));
                std::process::exit(1);
            }
            // Exit with special code to signal Claude to terminate
            std::process::exit(144);
        }
        Some(Commands::PreFlightCheck) => {
            if let Err(e) = run_pre_flight_check() {
                print_error(&format!("otto pre-flight-check: {}", e));
                std::process::exit(1);
            }
        }
        Some(Commands::Workspace { workspace_command }) => {
            if let Err(e) = run_workspace_command(workspace_command) {
                print_error(&format!("otto workspace: {}", e));
                std::process::exit(1);
            }
        }
        None => {
            // No subcommand provided, print help
            println!("Otto - Autonomous agent runner for beads tasks\n");
            println!("Usage: otto <COMMAND>\n");
            println!("Commands:");
            println!("  start            Start otto in tmux (runs in background)");
            println!("  attach           Attach to a tmux window");
            println!("  ralph            Run the agent loop (default behavior)");
            println!("  spawn            Spawn a single agent for a specific issue");
            println!("  done             Agent self-termination with cleanup");
            println!("  pre-flight-check Validate environment before agent work");
            println!("  workspace        Manage git worktrees for agent workspaces");
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
            println!("  otto done               Terminate with completed mode");
            println!("  otto done --mode escalated  Escalate (skip validation)");
            println!("  otto pre-flight-check  Validate environment before starting work");
            println!("  otto workspace list    List all workspaces");
            println!("  otto workspace clean   Remove all workspaces");
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
                // No ready beads, wait a bit before checking again with animation
                // Animate growing dots: ... -> .... -> ..... -> .......... (10 dots max)
                // Total animation time: 10 seconds (0.5s per frame, 20 frames)
                let base_dots = 3;
                let max_dots = 10;
                let frames = 20; // 10 seconds / 0.5 seconds per frame

                for frame in 0..frames {
                    // Calculate dots: cycle from 3 to 10, then back to 3
                    // frame 0: 3 dots, frame 1: 4 dots, ..., frame 7: 10 dots, frame 8: 3 dots, ...
                    let dot_count = base_dots + (frame % (max_dots - base_dots + 1));
                    let dots = ".".repeat(dot_count);
                    // Calculate trailing spaces needed to clear leftover characters from longer messages
                    // Max line length = "No ready beads, waiting" (23 chars) + max_dots (10) = 33 chars
                    // Current line length = 23 + dot_count
                    // Trailing spaces = 33 - (23 + dot_count) = max_dots - dot_count
                    let trailing_spaces = " ".repeat(max_dots - dot_count);
                    print!("\rNo ready beads, waiting{}{}", dots, trailing_spaces);
                    std::io::stdout().flush().unwrap();

                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if SHUTDOWN_REQUESTED.load(Ordering::SeqCst) {
                        println!(); // End the animation line
                        println!("Shutting down gracefully");
                        return;
                    }
                }
                // Carriage return moves to start of same line for next iteration
                print!("\r");
                std::io::stdout().flush().unwrap();
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
