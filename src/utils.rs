use anyhow::{Context, Result};
use std::path::PathBuf;

/// Walk up from CWD until we find a `.git` directory, like git itself does.
pub(crate) fn find_repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("Failed to get current directory")?;
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        let Some(parent) = dir.parent() else {
            anyhow::bail!("Not inside a git repository");
        };
        dir = parent.to_path_buf();
    }
}

/// Prompt the user for confirmation. Returns true if --yes or user types y/Y.
pub(crate) fn confirm(prompt: &str, yes: bool) -> bool {
    if yes {
        return true;
    }
    eprint!("{} [y/N] ", prompt);
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap_or(0);
    matches!(input.trim(), "y" | "Y")
}
