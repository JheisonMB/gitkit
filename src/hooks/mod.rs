use anyhow::{Context, Result};
use clap::Subcommand;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use crate::utils::{confirm, find_repo_root};

mod builtins;

#[derive(Subcommand)]
pub enum HooksCommand {
    /// Install a hook (built-in or custom command)
    Init {
        /// Git hook name (e.g. commit-msg, pre-push, pre-commit)
        hook: String,
        /// Built-in name or shell command to run
        target: String,
        #[arg(short, long)]
        yes: bool,
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// List installed hooks
    List,
    /// Remove a hook
    Remove {
        hook: String,
        #[arg(short, long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show hook content
    Show { hook: String },
}

pub fn run(cmd: HooksCommand) -> Result<()> {
    match cmd {
        HooksCommand::Init {
            hook,
            target,
            yes,
            force,
            dry_run,
        } => init(&hook, &target, yes, force, dry_run),
        HooksCommand::List => list(),
        HooksCommand::Remove { hook, yes, dry_run } => remove(&hook, yes, dry_run),
        HooksCommand::Show { hook } => show(&hook),
    }
}

fn hooks_dir() -> Result<std::path::PathBuf> {
    Ok(find_repo_root()?.join(".git").join("hooks"))
}

fn hook_script(target: &str) -> String {
    if let Some(script) = builtins::get(target) {
        return script.to_owned();
    }
    format!("#!/bin/sh\nset -e\n{target}\n")
}

fn init(hook: &str, target: &str, yes: bool, force: bool, dry_run: bool) -> Result<()> {
    let dir = hooks_dir()?;
    let path = dir.join(hook);

    if path.exists() && !force {
        if !confirm(&format!("Hook '{hook}' already exists. Overwrite?"), yes) {
            println!("Aborted.");
            return Ok(());
        }
        if !dry_run {
            let backup = dir.join(format!("{hook}.bak"));
            fs::copy(&path, &backup).with_context(|| format!("Failed to backup {hook}"))?;
            println!("Backed up to {}", backup.display());
        }
    }

    let script = hook_script(target);

    if dry_run {
        println!("[dry-run] Would write hook '{hook}':\n{script}");
        return Ok(());
    }

    fs::create_dir_all(&dir).context("Failed to create hooks directory")?;
    fs::write(&path, &script).with_context(|| format!("Failed to write hook '{hook}'"))?;
    set_executable(&path)?;
    println!("Installed hook '{hook}'.");
    Ok(())
}

fn list() -> Result<()> {
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
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).context("Failed to set executable permission")?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}
