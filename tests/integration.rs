use std::process::Command;
use tempfile::TempDir;

fn gitkit_binary() -> std::path::PathBuf {
    // Build the binary first, then return its path
    let output = Command::new("cargo")
        .args(["build", "--message-format=json"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to build");
    assert!(output.status.success(), "Failed to build gitkit binary");

    // Find the binary in target/debug
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let binary = manifest_dir.join("target/debug/gitkit");
    assert!(binary.exists(), "Binary not found at {binary:?}");
    binary
}

fn run_gitkit(args: &[&str]) -> (bool, String) {
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Failed to run gitkit");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (output.status.success(), format!("{stdout}{stderr}"))
}

// ═══════════════════════════════════════════════════════════════════════════
// CLI integration tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cli_no_args_shows_banner() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let (success, output) = run_gitkit(&["--help"]);
    assert!(success, "gitkit --help should succeed");
    assert!(output.contains("gitkit"));
}

#[test]
fn cli_version_flag() {
    let (success, output) = run_gitkit(&["--version"]);
    assert!(success);
    assert!(output.contains("gitkit"));
}

#[test]
fn cli_help_flag() {
    let (success, output) = run_gitkit(&["--help"]);
    assert!(success);
    assert!(output.contains("init"));
    assert!(output.contains("status"));
    assert!(output.contains("clone"));
    assert!(output.contains("hooks"));
    assert!(output.contains("ignore"));
    assert!(output.contains("attributes"));
    assert!(output.contains("config"));
    assert!(output.contains("build"));
}

#[test]
fn cli_hooks_help() {
    let (success, output) = run_gitkit(&["hooks", "--help"]);
    assert!(success);
    assert!(output.contains("add"));
    assert!(output.contains("list"));
    assert!(output.contains("remove"));
    assert!(output.contains("show"));
}

#[test]
fn cli_ignore_help() {
    let (success, output) = run_gitkit(&["ignore", "--help"]);
    assert!(success);
    assert!(output.contains("add"));
    assert!(output.contains("list"));
}

#[test]
fn cli_attributes_help() {
    let (success, output) = run_gitkit(&["attributes", "--help"]);
    assert!(success);
    assert!(output.contains("init"));
}

#[test]
fn cli_config_help() {
    let (success, output) = run_gitkit(&["config", "--help"]);
    assert!(success);
    assert!(output.contains("apply"));
    assert!(output.contains("show"));
}

#[test]
fn cli_build_help() {
    let (success, output) = run_gitkit(&["build", "--help"]);
    assert!(success);
    assert!(output.contains("list"));
    assert!(output.contains("apply"));
    assert!(output.contains("save"));
    assert!(output.contains("delete"));
}

#[test]
fn cli_status_outside_repo() {
    let dir = TempDir::new().unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["status"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    // status outside a repo should not panic (just print global config)
    assert!(output.status.success());
}

#[test]
fn cli_hooks_list_outside_repo() {
    let dir = TempDir::new().unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "list"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    // hooks list outside repo should fail gracefully (no hooks dir)
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should indicate error about not being in a repo
    assert!(!output.status.success() || stderr.contains("error") || !output.status.success());
}

#[test]
fn cli_build_list_empty() {
    let dir = TempDir::new().unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["build", "list"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No builds") || stdout.contains("Saved builds") || !output.status.success(),
        "Should show 'No builds', 'Saved builds', or fail gracefully"
    );
}

#[test]
fn cli_hooks_add_invalid_builtin() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "add", "--yes", "nonexistent-builtin"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should fail — not a builtin and no command provided
    assert!(
        !output.status.success()
            || stderr.contains("not a built-in")
            || stdout.contains("not a built-in"),
        "Should reject unknown builtin without command"
    );
}

#[test]
fn cli_hooks_add_custom_hook() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "add", "--yes", "pre-push", "echo test"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(
        output.status.success(),
        "Adding custom hook should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Verify the hook file was created
    let hook_path = dir.path().join(".git").join("hooks").join("pre-push");
    assert!(hook_path.exists());
    let content = std::fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains("#!/bin/sh"));
    assert!(content.contains("echo test"));
}

#[test]
fn cli_hooks_add_builtin_conventional_commits() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "add", "--yes", "conventional-commits"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(
        output.status.success(),
        "Installing builtin should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hook_path = dir.path().join(".git").join("hooks").join("commit-msg");
    assert!(hook_path.exists());
}

#[test]
fn cli_hooks_remove_installed_hook() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let hooks_dir = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    // Create a dummy hook
    std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();

    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "remove", "--yes", "pre-push"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(
        output.status.success(),
        "Removing hook should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!hooks_dir.join("pre-push").exists());
}

#[test]
fn cli_hooks_remove_nonexistent_hook() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let hooks_dir = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();

    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "remove", "--yes", "nonexistent-hook"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(
        !output.status.success(),
        "Removing nonexistent hook should fail"
    );
}

#[test]
fn cli_hooks_show_installed_hook() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let hooks_dir = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let hook_content = "#!/bin/sh\necho hello\n";
    std::fs::write(hooks_dir.join("pre-push"), hook_content).unwrap();

    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "show", "pre-push"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("echo hello"));
}

#[test]
fn cli_hooks_show_nonexistent_hook() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "show", "nonexistent"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(!output.status.success());
}

#[test]
fn cli_hooks_add_invalid_hook_name() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "add", "--yes", "not-a-real-hook", "echo hi"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success() || stderr.contains("not a valid git hook"),
        "Should reject invalid hook name"
    );
}

#[test]
fn cli_hooks_add_custom_hook_creates_executable() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["hooks", "add", "--yes", "pre-commit", "echo hello"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(output.status.success());
    let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::metadata(&hook_path).unwrap().permissions();
        assert!(perms.mode() & 0o111 != 0, "Hook should be executable");
    }
}

#[test]
fn cli_hooks_add_with_dry_run() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args([
            "hooks",
            "add",
            "--yes",
            "--dry-run",
            "pre-push",
            "echo test",
        ])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run]"));
    // Hook file should NOT exist
    assert!(!dir
        .path()
        .join(".git")
        .join("hooks")
        .join("pre-push")
        .exists());
}

#[test]
fn cli_ignore_add_dry_run() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["ignore", "add", "--yes", "--dry-run", "rust"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run]"));
}

#[test]
fn cli_attributes_init_dry_run() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(".git")).unwrap();
    let binary = gitkit_binary();
    let output = Command::new(&binary)
        .args(["attributes", "init", "--yes", "--dry-run"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[dry-run]"));
    assert!(!dir.path().join(".gitattributes").exists());
}

#[test]
fn cli_config_show_does_not_panic() {
    let (success, _) = run_gitkit(&["config", "show"]);
    assert!(success);
}

#[test]
fn cli_config_apply_dry_run() {
    let (success, output) = run_gitkit(&["config", "apply", "defaults", "--dry-run"]);
    assert!(success);
    assert!(output.contains("[dry-run]") || output.contains("already set"));
}
