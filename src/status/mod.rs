use anyhow::Result;
use clap::Args;
use std::fs;
use std::path::{Path, PathBuf};

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
    /// Report every repository gitkit has ever been applied to, machine-wide
    #[arg(long)]
    pub global: bool,
    /// With --global, remove registry entries whose repository no longer exists
    #[arg(long)]
    pub prune: bool,
    /// Discover repositories with gitkit hooks under DIR and register them
    #[arg(long, value_name = "DIR")]
    pub scan: Option<PathBuf>,
}

pub fn run(args: StatusArgs) -> Result<()> {
    if let Some(dir) = &args.scan {
        run_scan(dir)?;
        if !args.global {
            return Ok(());
        }
    }

    if args.global {
        return run_global(args.prune);
    }

    let dormant_found = run_report(&args)?;
    if args.strict && dormant_found {
        std::process::exit(1);
    }
    Ok(())
}

/// `gitkit status --scan <DIR>`: discovers repositories under `dir` with a
/// gitkit-installed hook that the registry doesn't yet know about, and
/// registers them. This is the adoption path for repositories configured
/// before the registry existed — it never runs implicitly.
fn run_scan(dir: &Path) -> Result<()> {
    println!(
        "Scanning {} for gitkit-managed repositories...",
        dir.display()
    );
    let result = crate::registry::scan(dir);

    if result.found.is_empty() {
        println!(
            "  (none found — visited {} director{}, depth {})",
            result.dirs_visited,
            if result.dirs_visited == 1 { "y" } else { "ies" },
            result.max_depth
        );
        return Ok(());
    }

    for (path, builtins) in &result.found {
        let items: Vec<String> = builtins.iter().map(|b| format!("hook:{b}")).collect();
        crate::registry::record_best_effort(path, &items);
        println!("  ✓ {} — {}", path.display(), builtins.join(", "));
    }

    println!(
        "\nFound and registered {} repositor{} (visited {} director{}, depth {}).",
        result.found.len(),
        if result.found.len() == 1 { "y" } else { "ies" },
        result.dirs_visited,
        if result.dirs_visited == 1 { "y" } else { "ies" },
        result.max_depth
    );
    Ok(())
}

/// `gitkit status --global`: reports every registered repository's hook
/// health, read from disk at query time — the registry only supplies the
/// list of paths to check. Never writes anything unless `prune` is set.
fn run_global(prune: bool) -> Result<()> {
    let mut registry = crate::registry::load();

    if registry.repos.is_empty() {
        println!("No repositories registered with gitkit.");
        println!("Run `gitkit status --scan <DIR>` to discover existing installs.");
        return Ok(());
    }

    let total = registry.repos.len();
    let mut gone: Vec<String> = Vec::new();
    let mut healthy = 0usize;
    let mut any_dormant = false;

    for (path_str, entry) in &registry.repos {
        let path = Path::new(path_str);

        if !path.exists() {
            gone.push(path_str.clone());
            println!("{path_str}");
            println!(
                "  ✗ gone — repository no longer exists (last applied {})",
                entry.applied_at
            );
            println!();
            continue;
        }

        let hooks_dir = path.join(".git").join("hooks");
        let mut problems = Vec::new();

        for item in &entry.applied {
            let Some(name) = item.strip_prefix("hook:") else {
                continue;
            };
            let hook_file = crate::hooks::builtins::get(name)
                .map(|b| b.hook.to_string())
                .unwrap_or_else(|| name.to_string());

            let health = if let Some(builtin) = crate::hooks::builtins::get(name) {
                let part_path =
                    crate::hooks::parts_dir(&hooks_dir, builtin.hook).join(builtin.name);
                if part_path.exists() {
                    crate::hooks::classify_part(&hooks_dir, builtin.hook, builtin.name)?
                } else {
                    crate::hooks::classify_hook(&hook_file, &hooks_dir.join(&hook_file))?
                }
            } else {
                crate::hooks::classify_hook(&hook_file, &hooks_dir.join(&hook_file))?
            };

            match health {
                HookHealth::Dormant => {
                    any_dormant = true;
                    problems.push(format!(
                        "  ✗ {name} ({hook_file}) — dormant: not executable, git ignores it"
                    ));
                }
                HookHealth::Absent => {
                    problems.push(format!(
                        "  ✗ {name} ({hook_file}) — absent: registered but no longer installed"
                    ));
                }
                HookHealth::Modified => {
                    problems.push(format!("  ~ {name} ({hook_file}) — modified since install"));
                }
                HookHealth::Active => {}
            }
        }

        if problems.is_empty() {
            healthy += 1;
        } else {
            println!("{path_str}");
            for p in &problems {
                println!("{p}");
            }
            println!();
        }
    }

    if prune && !gone.is_empty() {
        for g in &gone {
            registry.repos.remove(g);
        }
        crate::registry::save(&registry)?;
        println!("Pruned {} repositories that no longer exist.", gone.len());
    } else if !gone.is_empty() {
        println!(
            "{} repositories are gone. Re-run with `gitkit status --global --prune` to remove them from the registry.",
            gone.len()
        );
    }

    println!("\n{healthy}/{total} repositories healthy.");
    if any_dormant {
        println!(
            "⚠ dormant hooks found — run `gitkit status --repair` inside the affected repository."
        );
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
        .filter(|e| e.path().is_file())
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
        let content = fs::read_to_string(&path).unwrap_or_default();
        let is_dispatcher = crate::hooks::is_dispatcher(&content, &hook_name);
        let has_parts = !crate::hooks::list_parts(&hooks_dir, &hook_name).is_empty();

        if is_dispatcher || has_parts {
            if !is_dispatcher {
                // Content no longer matches what gitkit installed, but parts
                // are still present — report the mismatch without hiding the
                // builtins still recorded underneath it.
                let skip_note = if repair { " (repair skipped)" } else { "" };
                println!(
                    "  ~ {hook_name}{skip_note} — modified: dispatcher content doesn't match what gitkit installed; builtins below only run if this file still invokes them"
                );
            }
            let hook_dormant = print_dispatcher_parts(&hooks_dir, &hook_name, repair)?;
            dormant_found |= hook_dormant;
            continue;
        }

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

/// Prints the health of every part composed under a dispatcher hook and, if
/// `repair` is set, sets the executable bit on the dispatcher itself and on
/// any dormant part. Returns whether anything is still dormant afterward.
fn print_dispatcher_parts(hooks_dir: &Path, hook_name: &str, repair: bool) -> Result<bool> {
    let dispatcher_path = hooks_dir.join(hook_name);
    let mut dormant_found = false;

    if repair && !crate::hooks::is_executable(&dispatcher_path)? {
        crate::hooks::set_executable(&dispatcher_path)?;
    }

    for part_name in crate::hooks::list_parts(hooks_dir, hook_name) {
        let part_path = crate::hooks::parts_dir(hooks_dir, hook_name).join(&part_name);
        let health = crate::hooks::classify_part(hooks_dir, hook_name, &part_name)?;

        if repair && health == HookHealth::Dormant {
            crate::hooks::set_executable(&part_path)?;
            println!(
                "  ✓ {part_name} ({hook_name}) — repaired: set executable, git will now run it"
            );
            continue;
        }

        match health {
            HookHealth::Active => println!("  ✓ {part_name} ({hook_name}) — active"),
            HookHealth::Dormant => {
                dormant_found = true;
                println!(
                    "  ✗ {part_name} ({hook_name}) — dormant: not executable, so git ignores it and never runs it (fix with `gitkit status --repair`)"
                );
            }
            HookHealth::Modified => {
                println!("  ~ {part_name} ({hook_name}) — modified since install");
            }
            HookHealth::Absent => {}
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
    fn repair_sets_executable_bit_on_dormant_part_and_on_dispatcher() {
        // GK-A: two builtins composed on one git hook via the gitkit.d
        // dispatcher; both the dispatcher and one of its parts lose their
        // executable bit, and `--repair` must fix both.
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        crate::hooks::install_builtin("conventional-commits", true).unwrap();
        crate::hooks::install_builtin("no-body", true).unwrap();

        let hooks_dir = dir.path().join(".git").join("hooks");
        let dispatcher_path = hooks_dir.join("commit-msg");
        let dormant_part_path = crate::hooks::parts_dir(&hooks_dir, "commit-msg").join("no-body");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&dispatcher_path, &dormant_part_path] {
                let mut perms = std::fs::metadata(path).unwrap().permissions();
                perms.set_mode(0o644);
                std::fs::set_permissions(path, perms).unwrap();
            }
        }

        let dormant_found = print_hooks(true).unwrap();
        assert!(!dormant_found, "repair should leave nothing dormant");

        #[cfg(unix)]
        {
            assert!(crate::hooks::is_executable(&dispatcher_path).unwrap());
            assert!(crate::hooks::is_executable(&dormant_part_path).unwrap());
        }
        assert_eq!(
            crate::hooks::classify_part(&hooks_dir, "commit-msg", "no-body").unwrap(),
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

    // ── GK-E: registry-backed `gitkit status --global` ─────────────────────

    /// Points HOME at a fresh temp dir and CWD at a fresh temp repo for the
    /// duration of `f`, restoring both afterward. Never touches the real
    /// ~/.gitkit or the caller's working directory.
    fn with_temp_home_and_repo<F: FnOnce(&std::path::Path, &std::path::Path)>(f: F) {
        let home = TempDir::new().unwrap();
        let repo = TempDir::new().unwrap();
        fs::create_dir_all(repo.path().join(".git").join("hooks")).unwrap();

        let orig_home = std::env::var("HOME").ok();
        let orig_cwd = std::env::current_dir().ok();
        unsafe {
            std::env::set_var("HOME", home.path());
        }
        let _ = std::env::set_current_dir(repo.path());

        f(home.path(), repo.path());

        if let Some(cwd) = orig_cwd {
            let _ = std::env::set_current_dir(cwd);
        }
        unsafe {
            match &orig_home {
                Some(h) => std::env::set_var("HOME", h),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[serial]
    #[test]
    fn installing_a_hook_writes_a_ledger_entry_with_absolute_path() {
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let reg = crate::registry::load();
            let key = repo.to_string_lossy().to_string();
            let entry = reg.repos.get(&key).expect("repo should be registered");
            assert!(entry.applied.contains(&"hook:no-secrets".to_string()));
        });
    }

    #[serial]
    #[test]
    fn installing_a_hook_twice_updates_one_entry_not_two() {
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let reg = crate::registry::load();
            assert_eq!(reg.repos.len(), 1);
            let key = repo.to_string_lossy().to_string();
            let entry = &reg.repos[&key];
            assert_eq!(
                entry
                    .applied
                    .iter()
                    .filter(|i| *i == "hook:no-secrets")
                    .count(),
                1
            );
        });
    }

    #[serial]
    #[test]
    fn global_reports_dormant_hook_read_from_disk() {
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let path = repo.join(".git").join("hooks").join("pre-commit");
                let mut perms = fs::metadata(&path).unwrap().permissions();
                perms.set_mode(0o644);
                fs::set_permissions(&path, perms).unwrap();
            }
            let result = run_global(false);
            assert!(result.is_ok());
        });
    }

    #[serial]
    #[test]
    fn global_reports_deleted_hook_as_absent_not_installed_ledger_lies_regression() {
        // Regression test for the ledger-lies failure mode this spec exists
        // to prevent: the registry still lists the hook as applied, but the
        // file has been deleted from disk entirely. `status --global` must
        // read the current filesystem state and report `absent`, never
        // silently trust the ledger's stale claim that it's installed.
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let hook_path = repo.join(".git").join("hooks").join("pre-commit");
            fs::remove_file(&hook_path).unwrap();

            let reg = crate::registry::load();
            let key = repo.to_string_lossy().to_string();
            let entry = &reg.repos[&key];
            let hook_file = crate::hooks::builtins::get("no-secrets").unwrap().hook;
            let health =
                crate::hooks::classify_hook(hook_file, &hook_path).expect("health read ok");
            assert_eq!(health, HookHealth::Absent);
            assert!(entry.applied.contains(&"hook:no-secrets".to_string()));

            let result = run_global(false);
            assert!(result.is_ok());
        });
    }

    #[serial]
    #[test]
    fn global_reports_gone_repository_that_no_longer_exists() {
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let path_buf = repo.to_path_buf();
            let _ = std::env::set_current_dir(std::env::temp_dir());
            fs::remove_dir_all(&path_buf).unwrap();

            let reg = crate::registry::load();
            let key = path_buf.to_string_lossy().to_string();
            assert!(!path_buf.exists());
            assert!(reg.repos.contains_key(&key));

            let result = run_global(false);
            assert!(result.is_ok());
            let reg_after = crate::registry::load();
            assert!(
                reg_after.repos.contains_key(&key),
                "without --prune the registry must be unchanged"
            );
        });
    }

    #[serial]
    #[test]
    fn prune_removes_gone_entry_and_leaves_it_unchanged_without_prune() {
        with_temp_home_and_repo(|_home, repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let path_buf = repo.to_path_buf();
            let key = path_buf.to_string_lossy().to_string();
            let _ = std::env::set_current_dir(std::env::temp_dir());
            fs::remove_dir_all(&path_buf).unwrap();

            // Without --prune: unchanged.
            run_global(false).unwrap();
            let reg = crate::registry::load();
            assert!(reg.repos.contains_key(&key));

            // With --prune: removed.
            run_global(true).unwrap();
            let reg = crate::registry::load();
            assert!(!reg.repos.contains_key(&key));
        });
    }

    #[serial]
    #[test]
    fn global_with_missing_ledger_produces_a_message_not_an_error() {
        with_temp_home_and_repo(|_home, _repo| {
            let result = run_global(false);
            assert!(result.is_ok());
        });
    }

    #[serial]
    #[test]
    fn global_with_corrupt_ledger_produces_a_message_not_a_panic() {
        with_temp_home_and_repo(|home, _repo| {
            let dir = home.join(".gitkit");
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("registry.toml"), "not valid toml {{{").unwrap();
            let result = run_global(false);
            assert!(result.is_ok());
        });
    }

    #[serial]
    #[test]
    fn scan_on_temp_tree_with_two_gitkit_repos_finds_exactly_two() {
        with_temp_home_and_repo(|_home, _cwd| {
            let root = TempDir::new().unwrap();

            let repo_a = root.path().join("repo-a");
            let hooks_a = repo_a.join(".git").join("hooks");
            fs::create_dir_all(&hooks_a).unwrap();
            let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
            fs::write(hooks_a.join("pre-commit"), no_secrets.script).unwrap();

            let repo_b = root.path().join("repo-b");
            let hooks_b = repo_b.join(".git").join("hooks");
            fs::create_dir_all(&hooks_b).unwrap();
            let cc = crate::hooks::builtins::get("conventional-commits").unwrap();
            fs::write(hooks_b.join("commit-msg"), cc.script).unwrap();

            let repo_c = root.path().join("repo-c-no-hooks");
            fs::create_dir_all(repo_c.join(".git").join("hooks")).unwrap();

            let result = run_scan(root.path());
            assert!(result.is_ok());

            let reg = crate::registry::load();
            assert!(reg
                .repos
                .contains_key(&repo_a.to_string_lossy().to_string()));
            assert!(reg
                .repos
                .contains_key(&repo_b.to_string_lossy().to_string()));
            assert!(!reg
                .repos
                .contains_key(&repo_c.to_string_lossy().to_string()));
            assert_eq!(reg.repos.len(), 2);
        });
    }

    #[serial]
    #[test]
    fn global_alone_never_writes_the_registry() {
        with_temp_home_and_repo(|home, _repo| {
            crate::hooks::install_builtin("no-secrets", true).unwrap();
            let registry_path = home.join(".gitkit").join("registry.toml");
            let before = fs::read_to_string(&registry_path).unwrap();
            run_global(false).unwrap();
            let after = fs::read_to_string(&registry_path).unwrap();
            assert_eq!(before, after);
        });
    }

    #[serial]
    #[test]
    fn ledger_write_failure_does_not_fail_hook_installation() {
        with_temp_home_and_repo(|home, repo| {
            // Block ~/.gitkit from being created, so the registry write
            // inside install_builtin fails. The hook install itself must
            // still succeed.
            fs::write(home.join(".gitkit"), "not a directory").unwrap();
            let result = crate::hooks::install_builtin("no-secrets", true);
            assert!(result.is_ok());
            let hook_path = repo.join(".git").join("hooks").join("pre-commit");
            assert!(hook_path.exists());
        });
    }
}
