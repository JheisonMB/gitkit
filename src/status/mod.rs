use anyhow::Result;
use std::fs;

use crate::utils::{find_repo_root, git_config_get};

pub fn run() -> Result<()> {
    let in_repo = find_repo_root().is_ok();

    if in_repo {
        print_hooks()?;
        println!();
        print_gitignore()?;
        println!();
        print_gitattributes()?;
        println!();
        print_config("local")?;
        println!();
    }

    print_config("global")?;

    Ok(())
}

fn print_hooks() -> Result<()> {
    println!("Hooks:");

    let root = find_repo_root()?;
    let hooks_dir = root.join(".git").join("hooks");

    if !hooks_dir.exists() {
        println!("  (none)");
        return Ok(());
    }

    let installed: Vec<_> = fs::read_dir(&hooks_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            !s.ends_with(".bak") && !s.ends_with(".sample")
        })
        .collect();

    if installed.is_empty() {
        println!("  (none)");
        return Ok(());
    }

    for entry in installed {
        let hook_name = entry.file_name().to_string_lossy().to_string();
        let content = fs::read_to_string(entry.path()).unwrap_or_default();

        match crate::hooks::detect_builtin(&hook_name, &content) {
            Some(b) => println!("  ✓ {} ({})", b.name, b.hook),
            None => {
                let first_cmd = content
                    .lines()
                    .find(|l| !l.starts_with('#') && !l.starts_with("set ") && !l.trim().is_empty())
                    .unwrap_or("(custom)")
                    .trim();
                println!("  ✓ custom: {} → {:?}", hook_name, first_cmd);
            }
        }
    }

    Ok(())
}

fn print_gitignore() -> Result<()> {
    println!(".gitignore:");

    let root = find_repo_root()?;
    let path = root.join(".gitignore");

    if !path.exists() {
        println!("  (none)");
        return Ok(());
    }

    let content = fs::read_to_string(&path)?;
    let patterns: Vec<_> = content
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();

    println!("  ✓ {} patterns", patterns.len());
    Ok(())
}

fn print_gitattributes() -> Result<()> {
    println!(".gitattributes:");

    let root = find_repo_root()?;
    let path = root.join(".gitattributes");

    if !path.exists() {
        println!("  (none)");
        return Ok(());
    }

    let content = fs::read_to_string(&path)?;
    let mut presets = Vec::new();

    if content.contains("eol=lf") {
        presets.push("line-endings (eol=lf)");
    }
    if content.contains("binary") {
        presets.push("binary-files");
    }

    if presets.is_empty() {
        println!("  ✓ custom");
    } else {
        println!("  ✓ {}", presets.join(", "));
    }

    Ok(())
}

fn print_config(scope: &str) -> Result<()> {
    let label = if scope == "global" {
        "Git config (global)"
    } else {
        "Git config (local)"
    };
    println!("{label}:");

    let scope_flag = if scope == "global" {
        "--global"
    } else {
        "--local"
    };

    let mut any = false;
    for option in crate::config::CONFIG_OPTIONS {
        if let Some(value) = git_config_get(option.key, scope_flag) {
            println!("  ✓ {} = {value}", option.key);
            any = true;
        }
    }

    if !any {
        println!("  (none)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn git_config_get_returns_none_for_missing_key() {
        let result = git_config_get("nonexistent.key.xyz", "--global");
        assert!(result.is_none());
    }

    #[test]
    fn git_config_get_accepts_global_scope() {
        let _ = git_config_get("user.name", "--global");
    }

    #[test]
    fn git_config_get_accepts_local_scope() {
        let _ = git_config_get("user.name", "--local");
    }

    #[test]
    fn git_config_get_returns_string_when_found() {
        let result = git_config_get("user.name", "--global");
        if let Some(val) = result {
            assert!(!val.is_empty());
        }
    }

    // ── print_hooks ─────────────────────────────────────────────────────────

    #[serial]
    #[test]
    fn print_hooks_in_repo_with_no_hooks() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::create_dir(dir.path().join(".git").join("hooks")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_in_repo_without_hooks_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_with_sample_file_ignored() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit.sample"), "#!/bin/sh\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── print_gitignore ─────────────────────────────────────────────────────

    #[serial]
    #[test]
    fn print_gitignore_when_file_missing() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitignore();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_gitignore_with_patterns() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "target/\n*.log\n\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitignore();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_gitignore_with_only_comments() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitignore"), "# comment\n# another\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitignore();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── print_gitattributes ─────────────────────────────────────────────────

    #[serial]
    #[test]
    fn print_gitattributes_when_file_missing() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitattributes();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_gitattributes_with_line_endings() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitattributes"), "* text=auto eol=lf\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitattributes();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_gitattributes_with_binary() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitattributes"), "*.png binary\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitattributes();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_gitattributes_with_custom_only() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".gitattributes"), "*.txt text\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_gitattributes();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── print_config ────────────────────────────────────────────────────────

    #[test]
    fn print_config_global_does_not_panic() {
        let result = print_config("global");
        assert!(result.is_ok());
    }

    #[test]
    fn print_config_local_does_not_panic() {
        let result = print_config("local");
        assert!(result.is_ok());
    }

    // ── run (integration) ──────────────────────────────────────────────────

    #[serial]
    #[test]
    fn run_in_repo_does_not_panic() {
        let original = std::env::current_dir().ok();
        // We're in the gitkit repo, so run() should work
        let result = run();
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }
}
