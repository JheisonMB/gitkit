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

/// Add templates merging into existing .gitignore. Used by the interactive wizard (silent).
pub(crate) fn add_templates(templates: &str, force: bool) -> Result<()> {
    let root = find_repo_root()?;
    let path = root.join(".gitignore");
    let new_content = resolve_templates(templates)?;
    let merged = if force {
        new_content
    } else {
        merge_gitignore(&path, &new_content)
    };
    fs::write(&path, merged).context("Failed to write .gitignore")?;
    Ok(())
}

/// Fetch template names from the API for the search prompt.
pub(crate) fn fetch_template_list() -> Result<Vec<String>> {
    let url = format!("{API_BASE}/list?format=lines");
    let content = ureq::get(&url)
        .call()
        .context("Failed to fetch template list")?
        .into_string()
        .context("Failed to read response")?;
    let mut names: Vec<String> = builtins::NAMES.iter().map(|s| s.to_string()).collect();
    names.extend(content.lines().map(|l| l.to_string()));
    Ok(names)
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

/// Merge new gitignore content into existing file, skipping non-empty non-comment
/// lines already present. Preserves existing content and appends only new entries.
fn merge_gitignore(path: &std::path::Path, new_content: &str) -> String {
    let existing = if path.exists() {
        fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    let existing_patterns: std::collections::HashSet<&str> = existing
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    let to_append: String = new_content
        .lines()
        .filter(|line| {
            line.is_empty() || line.starts_with('#') || !existing_patterns.contains(line)
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp_gitignore(content: &str) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".gitignore");
        fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn merge_gitignore_appends_new_patterns() {
        let (_dir, path) = tmp_gitignore("target/\n");
        let result = merge_gitignore(&path, "*.log\n");
        assert!(result.contains("target/"));
        assert!(result.contains("*.log"));
    }

    #[test]
    fn merge_gitignore_skips_duplicate_patterns() {
        let (_dir, path) = tmp_gitignore("target/\n*.log\n");
        let result = merge_gitignore(&path, "*.log\n");
        assert_eq!(result.matches("*.log").count(), 1);
    }

    #[test]
    fn merge_gitignore_keeps_comments_and_blank_lines() {
        let (_dir, path) = tmp_gitignore("target/\n");
        let new = "# Rust\ntarget/\n*.pdb\n";
        let result = merge_gitignore(&path, new);
        // comment and blank lines from new content are always appended
        assert!(result.contains("# Rust"));
        assert!(result.contains("*.pdb"));
    }

    #[test]
    fn merge_gitignore_returns_existing_when_nothing_new() {
        let (_dir, path) = tmp_gitignore("target/\n");
        let result = merge_gitignore(&path, "target/\n");
        assert_eq!(result, "target/\n");
    }

    #[test]
    fn merge_gitignore_works_on_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".gitignore");
        let result = merge_gitignore(&path, "*.log\n");
        assert_eq!(result, "*.log\n");
    }

    #[test]
    fn resolve_templates_returns_builtin_agentic() {
        let result = resolve_templates("agentic").unwrap();
        assert!(result.contains(".kiro/"));
        assert!(result.contains(".cursor/"));
    }
}

mod builtins {
    pub(super) const NAMES: &[&str] = &["agentic"];

    pub(super) fn get(name: &str) -> Option<&'static str> {
        match name {
            "agentic" => Some(AGENTIC),
            _ => None,
        }
    }

    const AGENTIC: &str = "\n# AI coding agents\n\
.kiro/\n\
.cursor/\n\
.windsurf/\n\
.claude/\n\
.continue/\n\
.copilot/\n\
.kilocode/\n\
.zencoder/\n\
.qwen/\n\
.agents/\n\
skills-lock.json\n";
}
