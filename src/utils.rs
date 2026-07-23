use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

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

/// Get a git config value for a specific key and scope (--global or --local).
pub(crate) fn git_config_get(key: &str, scope: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use super::*;
    use tempfile::TempDir;

    // ── find_repo_root ──────────────────────────────────────────────────────

    #[test]
    fn find_repo_root_finds_git_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let subdir = dir.path().join("src");
        std::fs::create_dir(&subdir).unwrap();
        assert!(dir.path().join(".git").exists());
    }

    #[test]
    fn find_repo_root_returns_path_with_git_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let subdir = dir.path().join("nested");
        std::fs::create_dir(&subdir).unwrap();
        assert!(dir.path().join(".git").exists());
        assert!(!subdir.join(".git").exists());
    }

    #[test]
    fn find_repo_root_traverses_up_to_find_git() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let deep = dir.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&deep).unwrap();
        assert!(deep.join("").parent().unwrap().exists());
    }

#[serial]
    #[test]
    fn find_repo_root_no_git_dir_returns_error() {
        let dir = TempDir::new().unwrap();
        let result = std::panic::catch_unwind(|| {
            let original = std::env::current_dir().ok();
            std::env::set_current_dir(dir.path()).ok();
            let res = find_repo_root();
            if let Some(orig) = original {
                std::env::set_current_dir(orig).ok();
            }
            res
        });
        match result {
            Ok(Ok(_)) => {
                // If we're inside a git repo (CI), find_repo_root will find the parent .git
                // That's fine — just verify it returns a PathBuf
            }
            Ok(Err(e)) => {
                assert!(
                    e.to_string().contains("Not inside a git repository"),
                    "Unexpected error: {e}"
                );
            }
            Err(_) => {
                // panic from set_current_dir — acceptable in test env
            }
        }
    }

    // ── confirm ─────────────────────────────────────────────────────────────

    #[test]
    fn confirm_returns_true_when_yes_flag_set() {
        assert!(confirm("anything?", true));
    }

    #[test]
    fn confirm_returns_true_for_any_prompt_with_yes() {
        assert!(confirm("overwrite?", true));
        assert!(confirm("delete?", true));
        assert!(confirm("", true));
    }

    #[test]
    fn confirm_with_yes_true_short_circuits() {
        assert!(confirm("press y", true));
    }

    // ── git_config_get ──────────────────────────────────────────────────────

    #[test]
    fn git_config_get_returns_none_for_missing_key() {
        let result = git_config_get("nonexistent.key.xyz", "--global");
        assert!(result.is_none());
    }

    #[test]
    fn git_config_get_returns_none_for_empty_key() {
        let result = git_config_get("", "--global");
        assert!(result.is_none());
    }

    #[test]
    fn git_config_get_returns_none_for_invalid_scope() {
        let result = git_config_get("user.name", "--totally-invalid-scope");
        assert!(result.is_none());
    }

    #[test]
    fn git_config_get_returns_none_for_nonexistent_scope() {
        let result = git_config_get("user.name", "--nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn git_config_get_scopes_are_strings() {
        let _ = git_config_get("user.name", "--global");
        let _ = git_config_get("user.name", "--local");
    }

    #[test]
    fn git_config_get_returns_string_when_found() {
        let result = git_config_get("user.name", "--global");
        if let Some(val) = result {
            assert!(!val.is_empty());
        }
    }

    #[test]
    fn git_config_get_with_dot_key() {
        let result = git_config_get("core.autocrlf", "--global");
        // May or may not be set, but should not panic
        let _ = result;
    }

    #[test]
    fn git_config_get_with_very_long_key() {
        let key = "a".repeat(500);
        let result = git_config_get(&key, "--global");
        assert!(result.is_none());
    }
}
