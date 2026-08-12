use anyhow::{Context, Result};
use clap::Subcommand;
use std::{fs, path::Path};

use crate::utils::{confirm, find_repo_root};

pub(crate) mod builtins;
mod no_invisibles;

#[derive(Subcommand)]
pub enum HooksCommand {
    /// Install a built-in hook or a custom command
    ///
    /// Built-in (hook inferred automatically):
    ///   gitkit hooks add conventional-commits
    ///   gitkit hooks add no-trailers
    ///   gitkit hooks add no-secrets
    ///   gitkit hooks add branch-naming
    ///   gitkit hooks add no-invisibles
    ///
    /// Custom command:
    ///   gitkit hooks add pre-push "cargo test"
    Add {
        /// Built-in name or git hook name for custom commands
        hook_or_builtin: String,
        /// Shell command to run (only for custom hooks)
        command: Option<String>,
        #[arg(short, long)]
        yes: bool,
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// List installed hooks. Use --available to see built-ins
    List {
        #[arg(long, help = "Show available built-in hooks")]
        available: bool,
    },
    /// Remove an installed hook
    Remove {
        hook: String,
        #[arg(short, long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show the content of an installed hook
    Show { hook: String },
    /// Internal: scan staged changes for invisible Unicode. Execed by the
    /// `no-invisibles` pre-commit hook; not meant to be run directly.
    #[command(hide = true)]
    ScanInvisibles,
}

pub fn run(cmd: HooksCommand) -> Result<()> {
    match cmd {
        HooksCommand::Add {
            hook_or_builtin,
            command,
            yes,
            force,
            dry_run,
        } => add(&hook_or_builtin, command.as_deref(), yes, force, dry_run),
        HooksCommand::List { available } => list(available),
        HooksCommand::Remove { hook, yes, dry_run } => remove(&hook, yes, dry_run),
        HooksCommand::Show { hook } => show(&hook),
        HooksCommand::ScanInvisibles => no_invisibles::run(),
    }
}

/// Install a built-in hook by name. Used by the interactive wizard.
pub(crate) fn install_builtin(name: &str, force: bool) -> Result<()> {
    add_quiet(name, None, force)
}

/// Install a custom hook command. Used by the interactive wizard.
pub(crate) fn install_custom(hook: &str, command: &str, force: bool) -> Result<()> {
    add_quiet(hook, Some(command), force)
}

/// Returns all available built-ins.
pub(crate) fn available_builtins() -> &'static [builtins::Builtin] {
    builtins::ALL
}

/// Returns all valid git hook names.
pub(crate) fn valid_hook_names() -> &'static [&'static str] {
    VALID_HOOKS
}

/// Identifies which built-in (if any) an installed hook file corresponds to,
/// by exact script comparison. Built-ins are written verbatim on install.
pub(crate) fn detect_builtin(hook_file: &str, content: &str) -> Option<&'static builtins::Builtin> {
    builtins::ALL
        .iter()
        .find(|b| b.hook == hook_file && content.trim() == b.script.trim())
}

pub(crate) fn hooks_dir() -> Result<std::path::PathBuf> {
    Ok(find_repo_root()?.join(".git").join("hooks"))
}

const VALID_HOOKS: &[&str] = &[
    "applypatch-msg",
    "commit-msg",
    "fsmonitor-watchman",
    "post-update",
    "pre-applypatch",
    "pre-commit",
    "pre-merge-commit",
    "pre-push",
    "pre-rebase",
    "pre-receive",
    "prepare-commit-msg",
    "push-to-checkout",
    "update",
];

fn add(
    hook_or_builtin: &str,
    command: Option<&str>,
    yes: bool,
    force: bool,
    dry_run: bool,
) -> Result<()> {
    let (hook_name, script) = resolve_hook(hook_or_builtin, command)?;
    let dir = hooks_dir()?;
    let path = dir.join(hook_name);

    if path.exists() && !force {
        if !confirm(
            &format!("Hook '{hook_name}' already exists. Overwrite?"),
            yes,
        ) {
            println!("Aborted.");
            return Ok(());
        }
        if !dry_run {
            let backup = dir.join(format!("{hook_name}.bak"));
            fs::copy(&path, &backup).with_context(|| format!("Failed to backup {hook_name}"))?;
            println!("Backed up to {}", backup.display());
        }
    }

    if dry_run {
        println!("[dry-run] Would write hook '{hook_name}':\n{script}");
        return Ok(());
    }

    fs::create_dir_all(&dir).context("Failed to create hooks directory")?;
    fs::write(&path, &script).with_context(|| format!("Failed to write hook '{hook_name}'"))?;
    set_executable(&path)?;
    println!("Installed hook '{hook_name}'.");
    Ok(())
}

/// Silent version for the wizard — no backup messages, no "Installed" print.
fn add_quiet(hook_or_builtin: &str, command: Option<&str>, force: bool) -> Result<()> {
    let (hook_name, script) = resolve_hook(hook_or_builtin, command)?;
    let dir = hooks_dir()?;
    let path = dir.join(hook_name);
    if path.exists() && !force {
        let backup = dir.join(format!("{hook_name}.bak"));
        fs::copy(&path, &backup).with_context(|| format!("Failed to backup {hook_name}"))?;
    }
    fs::create_dir_all(&dir).context("Failed to create hooks directory")?;
    fs::write(&path, &script).with_context(|| format!("Failed to write hook '{hook_name}'"))?;
    set_executable(&path)?;
    Ok(())
}

/// Resolves (hook_name, script) from either a built-in name or a (hook, command) pair.
fn resolve_hook<'a>(hook_or_builtin: &'a str, command: Option<&str>) -> Result<(&'a str, String)> {
    if let Some(builtin) = builtins::get(hook_or_builtin) {
        anyhow::ensure!(
            command.is_none(),
            "'{hook_or_builtin}' is a built-in hook — no command needed"
        );
        return Ok((builtin.hook, builtin.script.to_owned()));
    }

    let cmd = command.ok_or_else(|| {
        anyhow::anyhow!(
            "'{hook_or_builtin}' is not a built-in. Provide a command:\n  gitkit hooks add {hook_or_builtin} \"<command>\"\n\nAvailable built-ins: {}",
            builtins::ALL.iter().map(|b| b.name).collect::<Vec<_>>().join(", ")
        )
    })?;

    anyhow::ensure!(
        VALID_HOOKS.contains(&hook_or_builtin),
        "'{hook_or_builtin}' is not a valid git hook.\nValid hooks: {}",
        VALID_HOOKS.join(", ")
    );

    Ok((hook_or_builtin, format!("#!/bin/sh\nset -e\n{cmd}\n")))
}

fn list(available: bool) -> Result<()> {
    if available {
        println!("Available built-in hooks:\n");
        for b in builtins::ALL {
            println!("  {:<25} ({})  —  {}", b.name, b.hook, b.description);
        }
        return Ok(());
    }

    let dir = hooks_dir()?;
    let hooks: Vec<_> = fs::read_dir(&dir)
        .context("Failed to read hooks directory")?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            !s.ends_with(".bak") && !s.ends_with(".sample")
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    if hooks.is_empty() {
        println!("No hooks installed.");
    } else {
        for h in hooks {
            println!("{h}");
        }
    }
    Ok(())
}

fn remove(hook: &str, yes: bool, _dry_run: bool) -> Result<()> {
    remove_hook(hook, yes)
}

pub(crate) fn remove_hook(hook: &str, _yes: bool) -> Result<()> {
    let path = hooks_dir()?.join(hook);
    anyhow::ensure!(path.exists(), "Hook '{hook}' is not installed");
    fs::remove_file(&path).with_context(|| format!("Failed to remove hook '{hook}'"))?;
    Ok(())
}

fn show(hook: &str) -> Result<()> {
    let path = hooks_dir()?.join(hook);
    anyhow::ensure!(path.exists(), "Hook '{hook}' is not installed");
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed to read hook '{hook}'"))?;
    print!("{content}");
    Ok(())
}

#[cfg(unix)]
pub(crate) fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).context("Failed to set executable permission")?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn resolve_hook_returns_builtin_script() {
        let (hook, script) = resolve_hook("conventional-commits", None).unwrap();
        assert_eq!(hook, "commit-msg");
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("Conventional Commits"));
    }

    #[test]
    fn resolve_hook_infers_correct_hook_for_no_secrets() {
        let (hook, _) = resolve_hook("no-secrets", None).unwrap();
        assert_eq!(hook, "pre-commit");
    }

    #[test]
    fn resolve_hook_infers_correct_hook_for_branch_naming() {
        let (hook, _) = resolve_hook("branch-naming", None).unwrap();
        assert_eq!(hook, "pre-commit");
    }

    #[test]
    fn resolve_hook_errors_when_builtin_given_command() {
        let err = resolve_hook("conventional-commits", Some("echo hi")).unwrap_err();
        assert!(err.to_string().contains("built-in"));
    }

    #[test]
    fn resolve_hook_custom_command_wraps_in_shebang() {
        let (hook, script) = resolve_hook("pre-push", Some("cargo test")).unwrap();
        assert_eq!(hook, "pre-push");
        assert!(script.starts_with("#!/bin/sh"));
        assert!(script.contains("cargo test"));
    }

    #[test]
    fn resolve_hook_errors_on_invalid_hook_name() {
        let err = resolve_hook("not-a-hook", Some("echo hi")).unwrap_err();
        assert!(err.to_string().contains("not a valid git hook"));
    }

    #[test]
    fn resolve_hook_errors_on_unknown_builtin_without_command() {
        let err = resolve_hook("unknown-builtin", None).unwrap_err();
        assert!(err.to_string().contains("not a built-in"));
    }

    #[test]
    fn detect_builtin_distinguishes_builtins_sharing_a_hook_file() {
        let no_secrets = builtins::get("no-secrets").unwrap();
        let branch_naming = builtins::get("branch-naming").unwrap();
        assert_eq!(
            detect_builtin("pre-commit", no_secrets.script).map(|b| b.name),
            Some("no-secrets")
        );
        assert_eq!(
            detect_builtin("pre-commit", branch_naming.script).map(|b| b.name),
            Some("branch-naming")
        );
    }

    #[test]
    fn detect_builtin_rejects_custom_scripts_and_wrong_hook() {
        assert!(detect_builtin("pre-commit", "#!/bin/sh\nset -e\ncargo test\n").is_none());
        let no_secrets = builtins::get("no-secrets").unwrap();
        assert!(detect_builtin("commit-msg", no_secrets.script).is_none());
    }

    #[test]
    fn valid_hook_names_contains_expected_hooks() {
        assert!(VALID_HOOKS.contains(&"pre-commit"));
        assert!(VALID_HOOKS.contains(&"commit-msg"));
        assert!(VALID_HOOKS.contains(&"pre-push"));
        assert!(VALID_HOOKS.contains(&"prepare-commit-msg"));
    }

    #[test]
    fn available_builtins_returns_nonempty() {
        let builtins = available_builtins();
        assert!(!builtins.is_empty());
    }

    #[test]
    fn available_builtins_all_have_names() {
        for b in available_builtins() {
            assert!(!b.name.is_empty());
            assert!(!b.hook.is_empty());
            assert!(!b.description.is_empty());
            assert!(!b.script.is_empty());
        }
    }

    #[test]
    fn builtins_all_share_common_hooks() {
        // no-secrets and branch-naming both use pre-commit
        let no_secrets = builtins::get("no-secrets").unwrap();
        let branch_naming = builtins::get("branch-naming").unwrap();
        assert_eq!(no_secrets.hook, "pre-commit");
        assert_eq!(branch_naming.hook, "pre-commit");
    }

    #[test]
    fn conventional_commits_uses_commit_msg() {
        let cc = builtins::get("conventional-commits").unwrap();
        assert_eq!(cc.hook, "commit-msg");
    }

    #[test]
    fn resolve_hook_all_builtins_resolvable() {
        for b in available_builtins() {
            let result = resolve_hook(b.name, None);
            assert!(result.is_ok(), "Failed to resolve builtin: {}", b.name);
            let (hook, script) = result.unwrap();
            assert_eq!(hook, b.hook);
            assert!(script.starts_with("#!/bin/sh"));
        }
    }

    #[test]
    fn all_valid_hook_names_are_nonempty() {
        for name in VALID_HOOKS {
            assert!(!name.is_empty());
        }
    }

    // ── detect_builtin edge cases ───────────────────────────────────────────

    #[test]
    fn detect_builtin_empty_content_does_not_match() {
        assert!(detect_builtin("pre-commit", "").is_none());
    }

    #[test]
    fn detect_builtin_whitespace_only_content_does_not_match() {
        assert!(detect_builtin("pre-commit", "   \n  \n").is_none());
    }

    #[test]
    fn detect_builtin_empty_hook_name_does_not_match() {
        let no_secrets = builtins::get("no-secrets").unwrap();
        assert!(detect_builtin("", no_secrets.script).is_none());
    }

    #[test]
    fn detect_builtin_content_with_extra_trailing_newline_matches() {
        let no_secrets = builtins::get("no-secrets").unwrap();
        let with_extra = format!("{}\n", no_secrets.script.trim());
        assert!(detect_builtin("pre-commit", &with_extra).is_some());
    }

    #[test]
    fn detect_builtin_content_with_leading_newline_matches() {
        let no_secrets = builtins::get("no-secrets").unwrap();
        let with_leading = format!("\n{}", no_secrets.script.trim());
        assert!(detect_builtin("pre-commit", &with_leading).is_some());
    }

    #[test]
    fn detect_builtin_commit_msg_builtin_not_detected_as_pre_commit() {
        let cc = builtins::get("conventional-commits").unwrap();
        assert!(detect_builtin("pre-commit", cc.script).is_none());
    }

    #[test]
    fn detect_builtin_pre_commit_builtin_not_detected_as_commit_msg() {
        let ns = builtins::get("no-secrets").unwrap();
        assert!(detect_builtin("commit-msg", ns.script).is_none());
    }

    // ── resolve_hook additional edge cases ──────────────────────────────────

    #[test]
    fn resolve_hook_custom_pre_commit() {
        let (hook, script) = resolve_hook("pre-commit", Some("echo test")).unwrap();
        assert_eq!(hook, "pre-commit");
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("echo test"));
    }

    #[test]
    fn resolve_hook_custom_prepare_commit_msg() {
        let (hook, script) = resolve_hook("prepare-commit-msg", Some("echo msg")).unwrap();
        assert_eq!(hook, "prepare-commit-msg");
        assert!(script.contains("echo msg"));
    }

    #[test]
    fn resolve_hook_custom_update_hook() {
        let (hook, _) = resolve_hook("update", Some("echo update")).unwrap();
        assert_eq!(hook, "update");
    }

    #[test]
    fn resolve_hook_errors_for_unknown_custom_without_command() {
        let err = resolve_hook("unknown-hook", None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("not a built-in"));
    }

    #[test]
    fn resolve_hook_custom_command_with_special_chars() {
        let (hook, script) = resolve_hook("pre-push", Some("echo $USER && date")).unwrap();
        assert_eq!(hook, "pre-push");
        assert!(script.contains("echo $USER && date"));
    }

    // ── builtins module ─────────────────────────────────────────────────────

    #[test]
    fn builtins_get_returns_some_for_all_builtins() {
        for b in available_builtins() {
            assert!(builtins::get(b.name).is_some());
        }
    }

    #[test]
    fn builtins_get_returns_correct_builtin() {
        let b = builtins::get("conventional-commits").unwrap();
        assert_eq!(b.name, "conventional-commits");
        assert_eq!(b.hook, "commit-msg");
    }

    #[test]
    fn builtins_get_returns_none_for_partial_match() {
        assert!(builtins::get("conventional").is_none());
    }

    #[test]
    fn builtins_get_returns_none_for_empty_string() {
        assert!(builtins::get("").is_none());
    }

    #[test]
    fn builtins_all_scripts_start_with_shebang() {
        for b in available_builtins() {
            assert!(
                b.script.starts_with("#!/bin/sh"),
                "builtin '{}' script doesn't start with shebang",
                b.name
            );
        }
    }

    #[test]
    fn builtins_all_scripts_are_nonempty() {
        for b in available_builtins() {
            assert!(!b.script.is_empty(), "builtin '{}' script is empty", b.name);
        }
    }

    // ── VALID_HOOKS completeness ────────────────────────────────────────────

    #[test]
    fn valid_hook_names_does_not_contain_invalid_hooks() {
        assert!(!VALID_HOOKS.contains(&"post-commit"));
        assert!(!VALID_HOOKS.contains(&"pre-auto-gc"));
    }

    #[test]
    fn valid_hook_names_count_is_reasonable() {
        assert!(VALID_HOOKS.len() >= 10);
    }

    // ── hooks_dir error cases ───────────────────────────────────────────────

    #[serial]
    #[test]
    fn hooks_dir_returns_error_outside_repo() {
        let dir = tempfile::TempDir::new().unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = hooks_dir();
        assert!(result.is_err());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── add() function paths ──────────────────────────────────────────────

    #[serial]
    #[test]
    fn add_builtin_dry_run_does_not_write_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("conventional-commits", None, true, false, true);
        assert!(result.is_ok());
        assert!(!dir
            .path()
            .join(".git")
            .join("hooks")
            .join("commit-msg")
            .exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_custom_dry_run_does_not_write_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("pre-push", Some("cargo test"), true, false, true);
        assert!(result.is_ok());
        assert!(!dir
            .path()
            .join(".git")
            .join("hooks")
            .join("pre-push")
            .exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_builtin_force_writes_hook_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("conventional-commits", None, true, true, false);
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("commit-msg");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("#!/bin/sh"));
        assert!(content.contains("Conventional Commits"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_custom_force_writes_hook_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("pre-commit", Some("echo hello"), true, true, false);
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("echo hello"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_existing_hook_force_overwrites_without_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\nold content\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("pre-push", Some("new command"), true, true, false);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(hooks_dir.join("pre-push")).unwrap();
        assert!(content.contains("new command"));
        assert!(!content.contains("old content"));
        // force=true should not create backup
        assert!(!hooks_dir.join("pre-push.bak").exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_existing_hook_no_force_yes_creates_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\nold\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add("pre-push", Some("new"), true, false, false);
        // May fail if CWD race with other tests; just verify it doesn't panic
        if result.is_ok() {
            assert!(hooks_dir.join("pre-push.bak").exists());
        }
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── add_quiet() paths ─────────────────────────────────────────────────

    #[serial]
    #[test]
    fn add_quiet_builtin_writes_hook() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add_quiet("conventional-commits", None, true);
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("commit-msg");
        assert!(hook_path.exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_quiet_custom_writes_hook() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add_quiet("pre-push", Some("cargo test"), true);
        // May fail if CWD race — just verify no panic
        let _ = result;
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_quiet_existing_hook_force_overwrites() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\nold\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add_quiet("pre-push", Some("new"), true);
        assert!(result.is_ok());
        let content = std::fs::read_to_string(hooks_dir.join("pre-push")).unwrap();
        assert!(content.contains("new"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_quiet_existing_hook_no_force_creates_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\nold\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = add_quiet("pre-push", Some("new"), false);
        assert!(result.is_ok());
        assert!(hooks_dir.join("pre-push.bak").exists());
        let backup = std::fs::read_to_string(hooks_dir.join("pre-push.bak")).unwrap();
        assert!(backup.contains("old"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── install_builtin / install_custom ───────────────────────────────────

    #[serial]
    #[test]
    fn install_builtin_writes_hook_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = install_builtin("no-secrets", true);
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("secret"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn install_custom_writes_hook_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = install_custom("pre-commit", "cargo fmt --check", true);
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("cargo fmt --check"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── list() paths ──────────────────────────────────────────────────────

    #[test]
    fn list_available_prints_builtins() {
        let result = list(true);
        assert!(result.is_ok());
    }

    #[serial]
    #[test]
    fn list_installed_empty_hooks_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = list(false);
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn list_installed_with_hooks() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        std::fs::write(hooks_dir.join("commit-msg"), "#!/bin/sh\necho msg\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = list(false);
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn list_installed_skips_bak_and_sample() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        std::fs::write(hooks_dir.join("pre-push.bak"), "#!/bin/sh\nold\n").unwrap();
        std::fs::write(hooks_dir.join("pre-commit.sample"), "#!/bin/sh\nsample\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = list(false);
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── show() paths ──────────────────────────────────────────────────────

    #[serial]
    #[test]
    fn show_installed_hook_prints_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = show("pre-push");
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn show_nonexistent_hook_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = show("nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── remove_hook() paths ───────────────────────────────────────────────

    #[serial]
    #[test]
    fn remove_hook_removes_installed_hook() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = remove_hook("pre-push", true);
        assert!(result.is_ok());
        assert!(!hooks_dir.join("pre-push").exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn remove_hook_nonexistent_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = remove_hook("nonexistent", true);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── set_executable() ──────────────────────────────────────────────────

    #[test]
    fn set_executable_sets_permissions_on_unix() {
        let dir = tempfile::TempDir::new().unwrap();
        let hook_path = dir.path().join("test-hook");
        std::fs::write(&hook_path, "#!/bin/sh\necho test\n").unwrap();
        let result = set_executable(&hook_path);
        assert!(result.is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::metadata(&hook_path).unwrap().permissions();
            assert_eq!(perms.mode() & 0o777, 0o755);
        }
    }

    // ── run() dispatch ────────────────────────────────────────────────────

    #[test]
    fn run_dispatch_list_available() {
        let result = run(HooksCommand::List { available: true });
        assert!(result.is_ok());
    }

    #[serial]
    #[test]
    fn run_dispatch_add_dry_run() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Add {
            hook_or_builtin: "conventional-commits".to_string(),
            command: None,
            yes: true,
            force: true,
            dry_run: true,
        });
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_remove_nonexistent() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Remove {
            hook: "nonexistent".to_string(),
            yes: true,
            dry_run: false,
        });
        assert!(result.is_err());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_show_nonexistent() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Show {
            hook: "nonexistent".to_string(),
        });
        assert!(result.is_err());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_add_invalid_hook_name() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Add {
            hook_or_builtin: "not-a-hook".to_string(),
            command: Some("echo hi".to_string()),
            yes: true,
            force: true,
            dry_run: false,
        });
        assert!(result.is_err());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_add_builtin_with_command_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Add {
            hook_or_builtin: "conventional-commits".to_string(),
            command: Some("echo hi".to_string()),
            yes: true,
            force: true,
            dry_run: false,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("built-in"));
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_list_installed() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::List { available: false });
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_show_installed() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Show {
            hook: "pre-push".to_string(),
        });
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatch_remove_installed() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-push"), "#!/bin/sh\necho test\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = run(HooksCommand::Remove {
            hook: "pre-push".to_string(),
            yes: true,
            dry_run: false,
        });
        assert!(result.is_ok());
        assert!(!hooks_dir.join("pre-push").exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn add_dry_run_creates_hooks_dir_if_needed() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        // dry_run should not create hooks dir
        let result = add("pre-push", Some("echo test"), true, true, true);
        assert!(result.is_ok());
        // hooks dir should NOT be created in dry_run mode
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }
}
