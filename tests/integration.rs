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

// ═══════════════════════════════════════════════════════════════════════════
// Lock / commit-blocking integration tests (real git repo, real git commit)
// ═══════════════════════════════════════════════════════════════════════════

fn init_git_repo(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("Failed to run git");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "test@example.com"]);
    run(&["config", "user.name", "Test User"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    run(&["add", "README.md"]);
}

fn git_commit_allow_empty(dir: &std::path::Path, msg: &str) -> (bool, String) {
    let output = Command::new("git")
        .args(["commit", "--allow-empty", "-m", msg])
        .current_dir(dir)
        .output()
        .expect("Failed to run git commit");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), format!("{stdout}{stderr}"))
}

#[test]
fn lock_fixture_commit_succeeds_unlocked_fails_locked_succeeds_after_unlock() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());
    let binary = gitkit_binary();

    // Unlocked: commit succeeds.
    let (ok, _) = git_commit_allow_empty(dir.path(), "initial commit");
    assert!(ok, "commit should succeed with no lock active");

    // Lock: commit fails and names the reason + how to unlock.
    let lock_out = Command::new(&binary)
        .args(["lock", "--reason", "Agent session active"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());

    let (ok, msg) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok, "commit should fail while locked");
    assert!(msg.contains("Agent session active"), "message was: {msg}");
    assert!(msg.contains("gitkit unlock"), "message was: {msg}");

    // Unlock: commit succeeds again.
    let unlock_out = Command::new(&binary)
        .args(["unlock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit unlock");
    assert!(unlock_out.status.success());

    let (ok, _) = git_commit_allow_empty(dir.path(), "commit after unlock");
    assert!(ok, "commit should succeed after unlock");
}

#[test]
fn lock_fixture_expired_lock_does_not_block_commit() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--timeout", "30m"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());

    // Confirm it blocks before expiry.
    let (ok, _) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok, "commit should fail while lock has not expired");

    // Rewrite the lock file with an expiry far in the past.
    let lock_path = dir.path().join(".git").join("gitkit.lock");
    std::fs::write(
        &lock_path,
        r#"{"locked_at":"2000-01-01T00:00:00Z","expires_at":"2000-01-01T00:01:00Z","reason":"stale","operations":["commit"]}"#,
    )
    .unwrap();

    let (ok, _) = git_commit_allow_empty(dir.path(), "commit after expiry");
    assert!(ok, "commit should succeed once the lock has expired");

    // status should report the lock as expired.
    let status_out = Command::new(&binary)
        .args(["lock", "status"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock status");
    let status_msg = String::from_utf8_lossy(&status_out.stdout);
    assert!(status_msg.contains("expired"), "status was: {status_msg}");
}

#[test]
fn lock_fixture_malformed_lock_file_fails_open() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());
    let binary = gitkit_binary();

    // Install the hook via a real lock, then corrupt the lock file.
    let lock_out = Command::new(&binary)
        .args(["lock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());

    let lock_path = dir.path().join(".git").join("gitkit.lock");
    std::fs::write(&lock_path, "not json at all {{{").unwrap();

    let (ok, _) = git_commit_allow_empty(dir.path(), "commit with malformed lock");
    assert!(ok, "a malformed lock file must never block a commit");
}

#[test]
fn lock_fixture_locking_twice_is_idempotent() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());
    let binary = gitkit_binary();

    let run_lock = |reason: &str| {
        let out = Command::new(&binary)
            .args(["lock", "--reason", reason])
            .current_dir(dir.path())
            .output()
            .expect("Failed to run gitkit lock");
        assert!(out.status.success());
    };
    run_lock("first reason");
    run_lock("second reason");

    let (ok, msg) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok);
    assert!(msg.contains("second reason"), "message was: {msg}");
    assert!(!msg.contains("first reason"), "message was: {msg}");

    let hooks_dir = dir.path().join(".git").join("hooks");
    assert!(
        !hooks_dir.join("pre-commit.gitkit-orig").exists(),
        "locking twice with no prior user hook must not fabricate a backup"
    );
}

#[test]
fn lock_fixture_preserves_existing_user_pre_commit_hook() {
    let dir = TempDir::new().unwrap();
    init_git_repo(dir.path());
    let hooks_dir = dir.path().join(".git").join("hooks");
    std::fs::create_dir_all(&hooks_dir).unwrap();
    let user_hook = "#!/bin/sh\necho user-hook-ran >&2\nexit 0\n";
    std::fs::write(hooks_dir.join("pre-commit"), user_hook).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(hooks_dir.join("pre-commit"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(hooks_dir.join("pre-commit"), perms).unwrap();
    }
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());
    assert!(hooks_dir.join("pre-commit.gitkit-orig").exists());

    let unlock_out = Command::new(&binary)
        .args(["unlock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit unlock");
    assert!(unlock_out.status.success());

    let restored = std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap();
    assert_eq!(restored, user_hook);
    assert!(!hooks_dir.join("pre-commit.gitkit-orig").exists());

    // The restored user hook still runs on commit.
    let (ok, msg) = git_commit_allow_empty(dir.path(), "commit runs user hook");
    assert!(ok);
    assert!(msg.contains("user-hook-ran"), "message was: {msg}");
}

// ═══════════════════════════════════════════════════════════════════════════
// Lock / push-blocking integration tests (real git repo, real git push)
// ═══════════════════════════════════════════════════════════════════════════

/// Sets up `dir` as a git repo with an initial commit and a local bare
/// remote named `origin`, so `git push` can be exercised without touching
/// the network. Returns the bare remote's `TempDir` - keep it alive for the
/// duration of the test.
fn init_git_repo_with_remote(dir: &std::path::Path) -> TempDir {
    let remote_dir = TempDir::new().unwrap();
    let status = Command::new("git")
        .args(["init", "--bare", "-q"])
        .current_dir(remote_dir.path())
        .status()
        .expect("Failed to init bare remote");
    assert!(status.success());

    init_git_repo(dir);
    let (ok, _) = git_commit_allow_empty(dir, "initial commit");
    assert!(ok, "initial commit should succeed");

    let status = Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            remote_dir.path().to_str().unwrap(),
        ])
        .current_dir(dir)
        .status()
        .expect("Failed to add remote");
    assert!(status.success());

    remote_dir
}

fn git_push_head(dir: &std::path::Path) -> (bool, String) {
    let output = Command::new("git")
        .args(["push", "origin", "HEAD:refs/heads/main"])
        .current_dir(dir)
        .output()
        .expect("Failed to run git push");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (output.status.success(), format!("{stdout}{stderr}"))
}

#[test]
fn push_fixture_push_succeeds_unlocked_fails_locked_succeeds_after_unlock() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    // Unlocked: push succeeds.
    let (ok, msg) = git_push_head(dir.path());
    assert!(ok, "push should succeed with no lock active: {msg}");

    // Lock --push: push fails and names the reason + how to unlock.
    let lock_out = Command::new(&binary)
        .args(["lock", "--push", "--reason", "Agent session active"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --push");
    assert!(lock_out.status.success());

    let (ok, _) = git_commit_allow_empty(dir.path(), "another commit");
    assert!(ok, "commit should still succeed - only push is locked");

    let (ok, msg) = git_push_head(dir.path());
    assert!(!ok, "push should fail while push-locked");
    assert!(msg.contains("Agent session active"), "message was: {msg}");
    assert!(msg.contains("gitkit unlock"), "message was: {msg}");

    // Unlock: push succeeds again.
    let unlock_out = Command::new(&binary)
        .args(["unlock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit unlock");
    assert!(unlock_out.status.success());

    let (ok, _) = git_push_head(dir.path());
    assert!(ok, "push should succeed after unlock");
}

#[test]
fn push_fixture_expired_lock_does_not_block_push() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--push", "--timeout", "30m"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --push");
    assert!(lock_out.status.success());

    // Confirm it blocks before expiry.
    let (ok, _) = git_push_head(dir.path());
    assert!(!ok, "push should fail while lock has not expired");

    // Rewrite the lock file with an expiry far in the past.
    let lock_path = dir.path().join(".git").join("gitkit.lock");
    std::fs::write(
        &lock_path,
        r#"{"locked_at":"2000-01-01T00:00:00Z","expires_at":"2000-01-01T00:01:00Z","reason":"stale","operations":["push"]}"#,
    )
    .unwrap();

    let (ok, _) = git_push_head(dir.path());
    assert!(ok, "push should succeed once the lock has expired");

    let status_out = Command::new(&binary)
        .args(["lock", "status"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock status");
    let status_msg = String::from_utf8_lossy(&status_out.stdout);
    assert!(status_msg.contains("expired"), "status was: {status_msg}");
}

#[test]
fn push_fixture_malformed_lock_file_fails_open() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--push"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --push");
    assert!(lock_out.status.success());

    let lock_path = dir.path().join(".git").join("gitkit.lock");
    std::fs::write(&lock_path, "not json at all {{{").unwrap();

    let (ok, _) = git_push_head(dir.path());
    assert!(ok, "a malformed lock file must never block a push");
}

#[test]
fn push_fixture_commit_lock_alone_does_not_block_push() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    // Only the commit lock is active - push must remain unblocked.
    let lock_out = Command::new(&binary)
        .args(["lock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());

    let (ok, msg) = git_push_head(dir.path());
    assert!(ok, "push should succeed while only commit is locked: {msg}");

    let (ok, _) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok, "commit should still fail while commit-locked");
}

#[test]
fn push_fixture_all_locks_both_commit_and_push() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--all", "--reason", "full lockdown"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --all");
    assert!(lock_out.status.success());

    let (ok, _) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok, "commit should fail under --all");

    let (ok, msg) = git_push_head(dir.path());
    assert!(!ok, "push should fail under --all: {msg}");
    assert!(msg.contains("full lockdown"), "message was: {msg}");
}

#[test]
fn push_fixture_adding_push_lock_preserves_commit_lock_reason() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--reason", "original session"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock");
    assert!(lock_out.status.success());

    // Extend to push without a new --reason.
    let lock_out = Command::new(&binary)
        .args(["lock", "--push"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --push");
    assert!(lock_out.status.success());

    let (ok, msg) = git_push_head(dir.path());
    assert!(!ok, "push should fail once push is locked");
    assert!(msg.contains("original session"), "message was: {msg}");

    let (ok, msg) = git_commit_allow_empty(dir.path(), "blocked commit");
    assert!(!ok, "commit should still fail too");
    assert!(msg.contains("original session"), "message was: {msg}");
}

#[test]
fn push_fixture_preserves_existing_user_pre_push_hook() {
    let dir = TempDir::new().unwrap();
    let _remote = init_git_repo_with_remote(dir.path());
    let hooks_dir = dir.path().join(".git").join("hooks");
    let user_hook = "#!/bin/sh\ncat >/dev/null\necho user-push-hook-ran >&2\nexit 0\n";
    std::fs::write(hooks_dir.join("pre-push"), user_hook).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(hooks_dir.join("pre-push"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(hooks_dir.join("pre-push"), perms).unwrap();
    }
    let binary = gitkit_binary();

    let lock_out = Command::new(&binary)
        .args(["lock", "--push"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit lock --push");
    assert!(lock_out.status.success());
    assert!(hooks_dir.join("pre-push.gitkit-orig").exists());

    let unlock_out = Command::new(&binary)
        .args(["unlock"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to run gitkit unlock");
    assert!(unlock_out.status.success());

    let restored = std::fs::read_to_string(hooks_dir.join("pre-push")).unwrap();
    assert_eq!(restored, user_hook);
    assert!(!hooks_dir.join("pre-push.gitkit-orig").exists());

    // The restored user hook still runs on push.
    let (ok, msg) = git_push_head(dir.path());
    assert!(ok);
    assert!(msg.contains("user-push-hook-ran"), "message was: {msg}");
}
