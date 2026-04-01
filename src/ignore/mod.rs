use anyhow::{Context, Result};
use clap::Subcommand;
use std::fs;

use crate::utils::{confirm, find_repo_root};

const API_BASE: &str = "https://www.toptal.com/developers/gitignore/api";

#[derive(Subcommand)]
pub enum IgnoreCommand {
    /// Generate .gitignore for the given templates
    Add {
        /// Comma-separated list of templates (e.g. rust,vscode)
        templates: String,
        #[arg(short, long)]
        yes: bool,
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// List available templates, optionally filtered
    List { filter: Option<String> },
}

pub fn run(cmd: IgnoreCommand) -> Result<()> {
    match cmd {
        IgnoreCommand::Add {
            templates,
            yes,
            force,
            dry_run,
        } => add(&templates, yes, force, dry_run),
        IgnoreCommand::List { filter } => list(filter.as_deref()),
    }
}

fn add(templates: &str, yes: bool, force: bool, dry_run: bool) -> Result<()> {
    let root = find_repo_root()?;
    let path = root.join(".gitignore");

    if path.exists() && !force && !confirm(".gitignore already exists. Overwrite?", yes) {
        println!("Aborted.");
        return Ok(());
    }

    let url = format!("{API_BASE}/{templates}");
    let content = ureq::get(&url)
        .call()
        .context("Failed to fetch gitignore templates")?
        .into_string()
        .context("Failed to read response")?;

    if dry_run {
        println!("[dry-run] Would write .gitignore:\n{content}");
        return Ok(());
    }

    fs::write(&path, content).context("Failed to write .gitignore")?;
    println!("Generated .gitignore for: {templates}");
    Ok(())
}

fn list(filter: Option<&str>) -> Result<()> {
    let url = format!("{API_BASE}/list?format=lines");
    let content = ureq::get(&url)
        .call()
        .context("Failed to fetch template list")?
        .into_string()
        .context("Failed to read response")?;

    for line in content.lines() {
        if filter.is_none_or(|f| line.contains(f)) {
            println!("{line}");
        }
    }
    Ok(())
}
