use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use std::process::Command;

use crate::utils::confirm;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Apply a curated git config preset
    Apply {
        preset: Preset,
        #[arg(short, long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(ValueEnum, Clone)]
pub enum Preset {
    /// push.autoSetupRemote, help.autocorrect, diff.algorithm
    Defaults,
    /// merge.conflictstyle zdiff3, rerere.enabled (terminal-focused)
    Advanced,
    /// core.pager delta (installs git-delta if needed)
    Delta,
}

pub fn run(cmd: ConfigCommand) -> Result<()> {
    let ConfigCommand::Apply {
        preset,
        yes,
        dry_run,
    } = cmd;
    match preset {
        Preset::Defaults => apply_defaults(dry_run),
        Preset::Advanced => apply_advanced(dry_run),
        Preset::Delta => apply_delta(yes, dry_run),
    }
}

type GitConfigs = &'static [(&'static str, &'static str)];

const DEFAULTS: GitConfigs = &[
    ("push.autoSetupRemote", "true"),
    ("help.autocorrect", "prompt"),
    ("diff.algorithm", "histogram"),
];

const ADVANCED: GitConfigs = &[
    ("merge.conflictstyle", "zdiff3"),
    ("rerere.enabled", "true"),
];

const DELTA_CONFIGS: GitConfigs = &[
    ("core.pager", "delta"),
    ("interactive.diffFilter", "delta --color-only"),
    ("delta.navigate", "true"),
    ("delta.side-by-side", "true"),
];

fn apply_defaults(dry_run: bool) -> Result<()> {
    apply_configs(DEFAULTS, dry_run)
}

fn apply_advanced(dry_run: bool) -> Result<()> {
    println!(
        "Warning: merge.conflictstyle=zdiff3 may cause issues with GitHub Desktop and GUI merge tools."
    );
    apply_configs(ADVANCED, dry_run)
}

fn apply_delta(yes: bool, dry_run: bool) -> Result<()> {
    if !delta_installed() {
        if !confirm(
            "git-delta is not installed. Install via `cargo install git-delta`?",
            yes,
        ) {
            println!("Aborted.");
            return Ok(());
        }
        if !dry_run {
            install_delta()?;
        } else {
            println!("[dry-run] Would run: cargo install git-delta");
        }
    }
    println!(
        "Note: delta.side-by-side=true may look wrong in narrow terminals. \
        Disable with: git config --global delta.side-by-side false"
    );
    apply_configs(DELTA_CONFIGS, dry_run)
}

fn apply_configs(configs: GitConfigs, dry_run: bool) -> Result<()> {
    for (key, value) in configs {
        if dry_run {
            println!("[dry-run] git config --global {key} {value}");
        } else {
            git_config_set(key, value)?;
            println!("Set {key} = {value}");
        }
    }
    Ok(())
}

fn git_config_set(key: &str, value: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["config", "--global", key, value])
        .status()
        .with_context(|| format!("Failed to run git config for '{key}'"))?;
    anyhow::ensure!(status.success(), "git config --global {key} {value} failed");
    Ok(())
}

fn delta_installed() -> bool {
    Command::new("delta")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn install_delta() -> Result<()> {
    anyhow::ensure!(
        Command::new("cargo")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false),
        "cargo not found in PATH — install Rust from https://rustup.rs to install git-delta"
    );
    println!("Installing git-delta...");
    let status = Command::new("cargo")
        .args(["install", "git-delta"])
        .status()
        .context("Failed to run cargo install git-delta")?;
    anyhow::ensure!(status.success(), "cargo install git-delta failed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_configs_dry_run_prints_without_running_git() {
        // dry_run=true must not invoke git; if it did it would fail in CI without a repo
        let result = apply_configs(DEFAULTS, true);
        assert!(result.is_ok());
    }

    #[test]
    fn apply_configs_dry_run_covers_advanced_preset() {
        assert!(apply_configs(ADVANCED, true).is_ok());
    }

    #[test]
    fn apply_configs_dry_run_covers_delta_preset() {
        assert!(apply_configs(DELTA_CONFIGS, true).is_ok());
    }
}
