use anyhow::{Context, Result};
use clap::Subcommand;
use std::fs;

use crate::utils::find_repo_root;

const API_BASE: &str = "https://www.toptal.com/developers/gitignore/api";

#[derive(Subcommand)]
pub enum IgnoreCommand {
    /// Generate .gitignore for the given templates
    Add {
        /// Comma-separated list of templates (e.g. rust,vscode,agentic)
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

fn add(templates: &str, _yes: bool, force: bool, dry_run: bool) -> Result<()> {
    let root = find_repo_root()?;
    let path = root.join(".gitignore");

    let new_content = resolve_templates(templates)?;
    let merged = if force {
        new_content.clone()
    } else {
        merge_gitignore(&path, &new_content)
    };

    if dry_run {
        println!("[dry-run] Would write .gitignore:\n{merged}");
        return Ok(());
    }

    fs::write(&path, merged).context("Failed to write .gitignore")?;
    println!("Updated .gitignore for: {templates}");
    Ok(())
}

/// Split templates, resolve built-ins locally, fetch the rest from the API.
/// Combines both into a single output.
fn resolve_templates(templates: &str) -> Result<String> {
    let mut builtin_parts: Vec<&str> = Vec::new();
    let mut api_templates: Vec<&str> = Vec::new();

    for t in templates.split(',').map(str::trim) {
        if builtins::get(t).is_some() {
            builtin_parts.push(t);
        } else {
            api_templates.push(t);
        }
    }

    let mut output = String::new();

    for name in &builtin_parts {
        output.push_str(builtins::get(name).unwrap());
    }

    if !api_templates.is_empty() {
        let joined = api_templates.join(",");
        let url = format!("{API_BASE}/{joined}");
        let fetched = ureq::get(&url)
            .call()
            .context("Failed to fetch gitignore templates")?
            .into_string()
            .context("Failed to read response")?;
        if fetched.trim().is_empty() {
            anyhow::bail!(
                "No templates found for: {}. Run 'gitkit ignore list' to see available templates.",
                joined
            );
        }
        output.push_str(&fetched);
    }

    Ok(output)
}

fn list(filter: Option<&str>) -> Result<()> {
    // Always show built-ins first
    for name in builtins::NAMES {
        if filter.is_none_or(|f| name.contains(f)) {
            println!("{name} (built-in)");
        }
    }

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

/// Merge new gitignore content into existing file, skipping lines already present.
/// Preserves existing content and appends only new non-duplicate lines.
fn merge_gitignore(path: &std::path::Path, new_content: &str) -> String {
    let existing = if path.exists() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    let existing_lines: std::collections::HashSet<&str> = existing.lines().collect();

    let to_append: String = new_content
        .lines()
        .filter(|line| !existing_lines.contains(line))
        .fold(String::new(), |mut acc, line| {
            acc.push_str(line);
            acc.push('\n');
            acc
        });

    if to_append.trim().is_empty() {
        return existing;
    }

    let mut result = existing;
    if !result.ends_with('\n') && !result.is_empty() {
        result.push('\n');
    }
    result.push_str(&to_append);
    result
}

mod builtins {
    pub(super) const NAMES: &[&str] = &["agentic"];

    pub(super) fn get(name: &str) -> Option<&'static str> {
        match name {
            "agentic" => Some(AGENTIC),
            _ => None,
        }
    }

    const AGENTIC: &str = "\
# Kiro
.kiro/
skills-lock.json

# Agent specs / project context
.agents/

# Cursor
.cursor/

# GitHub Copilot
.copilot/

# Continue
.continue/
";
}
