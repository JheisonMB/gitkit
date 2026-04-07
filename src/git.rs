use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Check if directory is a valid git repository.
pub fn is_git_repo() -> bool {
    Command::new("git")
        .args(&["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if .git directory exists.
fn git_dir_exists() -> bool {
    Path::new(".git").exists()
}

/// Initialize git repository if not already a git repo.
pub fn init_if_needed() -> Result<bool> {
    if git_dir_exists() {
        return Ok(false);
    }

    Command::new("git")
        .arg("init")
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run 'git init': {}", e))?;

    if !is_git_repo() {
        anyhow::bail!("git init succeeded but .git not found");
    }

    Ok(true)
}
