use anyhow::Result;
use std::{fs, process::Command};

use crate::hooks::builtins;
use crate::utils::find_repo_root;

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

        let builtin_match = builtins::ALL
            .iter()
            .find(|b| b.hook == hook_name && content.contains(&b.script[..80.min(b.script.len())]));

        match builtin_match {
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

    let configs = [
        ("push.autoSetupRemote", "true"),
        ("help.autocorrect", "prompt"),
        ("diff.algorithm", "histogram"),
        ("merge.conflictstyle", "zdiff3"),
        ("rerere.enabled", "true"),
    ];

    let mut any = false;
    for (key, _expected) in &configs {
        if let Some(value) = git_config_get(key, scope_flag) {
            println!("  ✓ {key} = {value}");
            any = true;
        }
    }

    if !any {
        println!("  (none)");
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_config_get_returns_none_for_missing_key() {
        let result = git_config_get("nonexistent.key.xyz", "--global");
        assert!(result.is_none());
    }
}
