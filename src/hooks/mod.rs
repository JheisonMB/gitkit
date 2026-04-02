use anyhow::{Context, Result};
use clap::Subcommand;
use std::{fs, path::Path};

use crate::utils::{confirm, find_repo_root};

mod builtins;

#[derive(Subcommand)]
pub enum HooksCommand {
    /// Install a built-in hook or a custom command
    ///
    /// Built-in (hook inferred automatically):
    ///   gitkit hooks add conventional-commits
    ///   gitkit hooks add no-secrets
    ///   gitkit hooks add branch-naming
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
    }
}

fn hooks_dir() -> Result<std::path::PathBuf> {
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

fn remove(hook: &str, yes: bool, dry_run: bool) -> Result<()> {
    let path = hooks_dir()?.join(hook);
    anyhow::ensure!(path.exists(), "Hook '{hook}' is not installed");

    if !confirm(&format!("Remove hook '{hook}'?"), yes) {
        println!("Aborted.");
        return Ok(());
    }

    if dry_run {
        println!("[dry-run] Would remove hook '{hook}'.");
        return Ok(());
    }

    fs::remove_file(&path).with_context(|| format!("Failed to remove hook '{hook}'"))?;
    println!("Removed hook '{hook}'.");
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
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).context("Failed to set executable permission")?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
