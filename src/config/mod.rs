use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use std::process::Command;

use crate::utils::{confirm, find_repo_root};

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Apply a curated git config preset
    Apply {
        preset: Preset,
        #[arg(short, long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, conflicts_with = "local")]
        global: bool,
        #[arg(long, conflicts_with = "global")]
        local: bool,
    },
    /// Show current git config values
    Show,
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
    match cmd {
        ConfigCommand::Apply {
            preset,
            yes,
            dry_run,
            global,
            local,
        } => {
            let scope = determine_scope(global, local);
            match preset {
                Preset::Defaults => apply_defaults(dry_run, scope),
                Preset::Advanced => apply_advanced(dry_run, scope),
                Preset::Delta => apply_delta(yes, dry_run, scope),
            }
        }
        ConfigCommand::Show => show_config(),
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ConfigScope {
    Global,
    Local,
}

fn determine_scope(global: bool, local: bool) -> ConfigScope {
    if global {
        ConfigScope::Global
    } else if local || find_repo_root().is_ok() {
        ConfigScope::Local
    } else {
        ConfigScope::Global
    }
}

fn scope_flag(scope: ConfigScope) -> &'static str {
    match scope {
        ConfigScope::Global => "--global",
        ConfigScope::Local => "--local",
    }
}

fn show_config() -> Result<()> {
    println!("Git config (global):");
    show_scope_config("--global");
    println!();
    println!("Git config (local):");
    show_scope_config("--local");
    Ok(())
}

fn show_scope_config(scope: &str) {
    let configs = [
        "push.autoSetupRemote",
        "help.autocorrect",
        "diff.algorithm",
        "merge.conflictstyle",
        "rerere.enabled",
        "core.pager",
    ];

    let mut any = false;
    for key in &configs {
        if let Some(value) = git_config_get(key, scope) {
            println!("  {key} = {value}");
            any = true;
        }
    }

    if !any {
        println!("  (none)");
    }
}

fn git_config_get(key: &str, scope: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["config", scope, "--get", key])
        .output()
        .ok()?;

    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Individual config options exposed for the interactive wizard.
pub(crate) struct ConfigOption {
    pub key: &'static str,
    pub value: Option<&'static str>, // None for multi-key options like delta
    pub label: &'static str,
    pub recommended: bool,
}

pub(crate) const CONFIG_OPTIONS: &[ConfigOption] = &[
    ConfigOption {
        key: "push.autoSetupRemote",
        value: Some("true"),
        label: "push.autoSetupRemote = true  —  auto-set upstream on first push",
        recommended: true,
    },
    ConfigOption {
        key: "help.autocorrect",
        value: Some("prompt"),
        label: "help.autocorrect = prompt  —  suggest corrections for mistyped commands",
        recommended: true,
    },
    ConfigOption {
        key: "diff.algorithm",
        value: Some("histogram"),
        label: "diff.algorithm = histogram  —  cleaner diffs for moved code",
        recommended: true,
    },
    ConfigOption {
        key: "merge.conflictstyle",
        value: Some("zdiff3"),
        label: "merge.conflictstyle = zdiff3  —  show base in conflict markers",
        recommended: false,
    },
    ConfigOption {
        key: "rerere.enabled",
        value: Some("true"),
        label: "rerere.enabled = true  —  remember and reuse conflict resolutions",
        recommended: false,
    },
    ConfigOption {
        key: "core.pager",
        value: None, // handled separately — installs git-delta via cargo
        label: "core.pager = delta  —  beautiful syntax-highlighted diffs (requires cargo)",
        recommended: false,
    },
];

/// Apply selected config option keys. Used by the interactive wizard.
pub(crate) fn apply_config_keys(
    keys: &[&str],
    cargo_available: bool,
    scope: ConfigScope,
) -> Result<()> {
    for key in keys {
        match *key {
            "core.pager" => {
                anyhow::ensure!(
                    cargo_available,
                    "cargo not found — cannot install git-delta"
                );
                if !delta_installed() {
                    install_delta()?;
                }
                for (k, v) in DELTA_CONFIGS {
                    git_config_set(k, v, scope)?;
                }
            }
            _ => {
                let value = CONFIG_OPTIONS
                    .iter()
                    .find(|o| o.key == *key)
                    .and_then(|o| o.value)
                    .ok_or_else(|| anyhow::anyhow!("Unknown config key: {key}"))?;
                git_config_set(key, value, scope)?;
            }
        }
    }
    Ok(())
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

fn apply_defaults(dry_run: bool, scope: ConfigScope) -> Result<()> {
    apply_configs(DEFAULTS, dry_run, scope)
}

fn apply_advanced(dry_run: bool, scope: ConfigScope) -> Result<()> {
    println!(
        "Warning: merge.conflictstyle=zdiff3 may cause issues with GitHub Desktop and GUI merge tools."
    );
    apply_configs(ADVANCED, dry_run, scope)
}

fn apply_delta(yes: bool, dry_run: bool, scope: ConfigScope) -> Result<()> {
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
        Disable with: git config {} delta.side-by-side false",
        scope_flag(scope)
    );
    apply_configs(DELTA_CONFIGS, dry_run, scope)
}

fn apply_configs(configs: GitConfigs, dry_run: bool, scope: ConfigScope) -> Result<()> {
    let flag = scope_flag(scope);
    let mut already_set = 0;

    for (key, value) in configs {
        let current = git_config_get(key, flag);

        if current.as_deref() == Some(value) {
            println!("✓ {key} = {value} (already set)");
            already_set += 1;
        } else if dry_run {
            println!("[dry-run] git config {flag} {key} {value}");
        } else {
            git_config_set(key, value, scope)?;
            println!("✓ Set {key} = {value}");
        }
    }

    if already_set == configs.len() {
        println!("\nAll configs already applied.");
    }

    Ok(())
}

fn git_config_set(key: &str, value: &str, scope: ConfigScope) -> Result<()> {
    let flag = scope_flag(scope);
    let status = Command::new("git")
        .args(["config", flag, key, value])
        .status()
        .with_context(|| format!("Failed to run git config for '{key}'"))?;
    anyhow::ensure!(status.success(), "git config {flag} {key} {value} failed");
    Ok(())
}

pub(crate) fn remove_config_key(key: &str, scope: ConfigScope) -> Result<()> {
    let flag = scope_flag(scope);
    let status = Command::new("git")
        .args(["config", flag, "--unset", key])
        .status()
        .with_context(|| format!("Failed to unset git config for '{key}'"))?;
    anyhow::ensure!(status.success(), "git config {flag} --unset {key} failed");
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
        let result = apply_configs(DEFAULTS, true, ConfigScope::Global);
        assert!(result.is_ok());
    }

    #[test]
    fn apply_configs_dry_run_covers_advanced_preset() {
        assert!(apply_configs(ADVANCED, true, ConfigScope::Global).is_ok());
    }

    #[test]
    fn apply_configs_dry_run_covers_delta_preset() {
        assert!(apply_configs(DELTA_CONFIGS, true, ConfigScope::Global).is_ok());
    }

    #[test]
    fn determine_scope_defaults_to_global_outside_repo() {
        let scope = determine_scope(false, false);
        assert!(matches!(scope, ConfigScope::Global | ConfigScope::Local));
    }

    #[test]
    fn determine_scope_respects_explicit_flags() {
        assert!(matches!(determine_scope(true, false), ConfigScope::Global));
        assert!(matches!(determine_scope(false, true), ConfigScope::Local));
    }

    #[test]
    fn scope_flag_returns_correct_values() {
        assert_eq!(scope_flag(ConfigScope::Global), "--global");
        assert_eq!(scope_flag(ConfigScope::Local), "--local");
    }
}
