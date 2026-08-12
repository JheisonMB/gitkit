use anyhow::Result;
use clap::Args;
use std::fs;

use crate::hooks::HookHealth;
use crate::utils::{find_repo_root, git_config_get};

#[derive(Args, Default)]
pub struct StatusArgs {
    /// Set the executable bit on every dormant hook. Content is untouched —
    /// a dormant hook's content already matches a builtin verbatim.
    #[arg(long)]
    pub repair: bool,
    /// Exit non-zero if any hook is dormant (git is silently ignoring it)
    #[arg(long)]
    pub strict: bool,
}

pub fn run(args: StatusArgs) -> Result<()> {
    let dormant_found = run_report(&args)?;
    if args.strict && dormant_found {
        std::process::exit(1);
    }
    Ok(())
}

/// Runs the actual report and returns whether any hook is (still) dormant
/// once any requested repair has been applied. Kept separate from `run` so
/// the `--strict` exit-code decision can be exercised without triggering
/// `process::exit` in-process (which would kill the test binary).
fn run_report(args: &StatusArgs) -> Result<bool> {
    let in_repo = find_repo_root().is_ok();
    let mut dormant_found = false;

    if in_repo {
        dormant_found = print_hooks(args.repair)?;
        println!();
        print_gitignore()?;
        println!();
        print_gitattributes()?;
        println!();
        print_config("local")?;
        println!();
    }

    print_config("global")?;

    Ok(dormant_found)
}

/// Prints each installed hook's health and, if `repair` is set, fixes
/// dormant ones in place. Returns whether any hook is still dormant when
/// this returns (i.e. `false` for any hook this call successfully repaired).
fn print_hooks(repair: bool) -> Result<bool> {
    println!("Hooks:");

    let root = find_repo_root()?;
    let hooks_dir = root.join(".git").join("hooks");

    if !hooks_dir.exists() {
        println!("  (none)");
        return Ok(false);
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
        return Ok(false);
    }

    let mut dormant_found = false;

    for entry in installed {
        let hook_name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let health = crate::hooks::classify_hook(&hook_name, &path)?;

        if repair && health == HookHealth::Dormant {
            crate::hooks::set_executable(&path)?;
            let label = builtin_label(&hook_name, &path);
            println!("  ✓ {label} ({hook_name}) — repaired: set executable, git will now run it");
            continue;
        }

        match health {
            HookHealth::Active => {
                let label = builtin_label(&hook_name, &path);
                println!("  ✓ {label} ({hook_name}) — active");
            }
            HookHealth::Dormant => {
                dormant_found = true;
                let label = builtin_label(&hook_name, &path);
                println!(
                    "  ✗ {label} ({hook_name}) — dormant: not executable, so git ignores it and never runs it (fix with `gitkit status --repair`)"
                );
            }
            HookHealth::Modified => {
                let content = fs::read_to_string(&path).unwrap_or_default();
                let first_cmd = content
                    .lines()
                    .find(|l| !l.starts_with('#') && !l.starts_with("set ") && !l.trim().is_empty())
                    .unwrap_or("(custom)")
                    .trim();
                let skip_note = if repair { " (repair skipped)" } else { "" };
                println!("  ~ {hook_name}{skip_note} — modified: {first_cmd:?}");
            }
            HookHealth::Absent => {
                // Unreachable in practice: we're iterating files that exist.
                // Defensive only, in case the file vanished mid-scan.
                println!("  ? {hook_name} — vanished during scan");
            }
        }
    }

    Ok(dormant_found)
}

/// Best-effort display label for an installed hook: the builtin's name if
/// its content matches one, otherwise the raw hook filename.
fn builtin_label(hook_name: &str, path: &std::path::Path) -> String {
    let content = fs::read_to_string(path).unwrap_or_default();
    crate::hooks::detect_builtin(hook_name, &content)
        .map(|b| b.name.to_string())
        .unwrap_or_else(|| hook_name.to_string())
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
        let result = print_hooks(false);
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
        let result = print_hooks(false);
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
        let result = print_hooks(false);
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── GK-D: hook health (active / dormant / modified / absent) ────────────

    #[serial]
    #[test]
    fn print_hooks_active_hook_matching_builtin_reports_no_dormant() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        let path = hooks_dir.join("pre-commit");
        std::fs::write(&path, no_secrets.script).unwrap();
        crate::hooks::set_executable(&path).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(false).unwrap();
        assert!(!dormant_found);
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_dormant_hook_with_executable_bit_removed_is_detected() {
        // Regression test for GK-D: an installed hook whose content matches
        // a builtin but whose executable bit was never set (or was lost) is
        // the exact bug this spec exists to catch. `status` must flag it.
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        let path = hooks_dir.join("pre-commit");
        std::fs::write(&path, no_secrets.script).unwrap();
        crate::hooks::set_executable(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&path, perms).unwrap();
        }

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(false).unwrap();
        #[cfg(unix)]
        assert!(
            dormant_found,
            "a non-executable builtin match must be reported dormant"
        );
        #[cfg(not(unix))]
        assert!(
            !dormant_found,
            "the executable bit does not apply on non-Unix targets"
        );
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_hand_edited_hook_is_modified_not_dormant() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        let path = hooks_dir.join("pre-commit");
        std::fs::write(&path, format!("{}\necho 'extra'\n", no_secrets.script)).unwrap();
        crate::hooks::set_executable(&path).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(false).unwrap();
        assert!(
            !dormant_found,
            "hand-edited content is `modified`, never `dormant`"
        );
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_no_hook_file_is_absent_and_not_reported_dormant() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(false).unwrap();
        assert!(!dormant_found);
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_bak_file_not_executable_does_not_affect_result() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        let active_path = hooks_dir.join("pre-commit");
        std::fs::write(&active_path, no_secrets.script).unwrap();
        crate::hooks::set_executable(&active_path).unwrap();
        // A stray, non-executable .bak file must be ignored entirely.
        std::fs::write(hooks_dir.join("pre-commit.bak"), no_secrets.script).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(false).unwrap();
        assert!(!dormant_found);
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn print_hooks_non_utf8_hook_content_does_not_panic() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let path = hooks_dir.join("pre-commit");
        std::fs::write(&path, [0x23, 0x21, 0xff, 0xfe, 0x00, 0x01]).unwrap();
        crate::hooks::set_executable(&path).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks(false);
        assert!(result.is_ok());
        assert!(!result.unwrap());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── --repair ─────────────────────────────────────────────────────────────

    #[serial]
    #[test]
    fn repair_turns_dormant_hook_into_active_with_byte_identical_content() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        let path = hooks_dir.join("pre-commit");
        std::fs::write(&path, no_secrets.script).unwrap();
        crate::hooks::set_executable(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o644);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        let before = std::fs::read(&path).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let dormant_found = print_hooks(true).unwrap();
        assert!(!dormant_found, "repair should leave nothing dormant");
        let after = std::fs::read(&path).unwrap();
        assert_eq!(before, after, "repair must not rewrite hook content");
        assert_eq!(
            crate::hooks::classify_hook("pre-commit", &path).unwrap(),
            HookHealth::Active
        );
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn repair_does_not_touch_modified_hook() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        let path = hooks_dir.join("pre-push");
        std::fs::write(&path, "#!/bin/sh\ncargo test\n").unwrap();
        crate::hooks::set_executable(&path).unwrap();
        let before = std::fs::read(&path).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks(true);
        assert!(result.is_ok());
        let after = std::fs::read(&path).unwrap();
        assert_eq!(before, after, "repair must never touch a modified hook");
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn repair_does_not_create_a_file_for_an_absent_hook() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());
        let result = print_hooks(true);
        assert!(result.is_ok());
        assert!(
            std::fs::read_dir(&hooks_dir).unwrap().next().is_none(),
            "repair must never install an absent hook"
        );
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
        // We're in the gitkit repo, so run() should work. strict=false, so
        // this never hits the process::exit path.
        let result = run(StatusArgs::default());
        assert!(result.is_ok());
        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }
}
