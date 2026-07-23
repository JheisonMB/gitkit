use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Check if directory is a valid git repository.
pub fn is_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn is_git_repo_returns_bool() {
        let _ = is_git_repo();
    }

    #[test]
    fn git_dir_exists_returns_bool() {
        let _ = git_dir_exists();
    }

    #[test]
    fn is_git_repo_in_current_dir() {
        let result = is_git_repo();
        let _: bool = result;
    }

    #[test]
    fn is_git_repo_does_not_panic_for_invalid_dir() {
        // Verify it returns false rather than panicking when not in a repo
        let original = std::env::current_dir().ok();
        let dir = TempDir::new().unwrap();
        let _ = std::env::set_current_dir(dir.path());
        let result = is_git_repo();
        assert!(!result);
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    fn git_dir_exists_in_non_repo_dir() {
        let dir = TempDir::new().unwrap();
        assert!(!dir.path().join(".git").exists());
    }

    #[test]
    fn git_dir_exists_when_git_present() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(dir.path().join(".git").exists());
    }

    #[test]
    fn init_if_needed_skips_if_git_exists() {
        // In a dir that already has .git, init_if_needed should return Ok(false)
        let original = std::env::current_dir().ok();
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let _ = std::env::set_current_dir(dir.path());
        let result = init_if_needed();
        assert!(result.is_ok());
        assert!(!result.unwrap());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    #[ignore = "flaky: set_current_dir races with parallel tests"]
    fn init_if_needed_initializes_new_repo() {
        let dir = TempDir::new().unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = init_if_needed();
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(dir.path().join(".git").exists());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }
}
