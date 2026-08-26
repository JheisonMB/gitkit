use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

/// The ledger: which repositories gitkit has ever applied something to, and
/// what. It only ever records *where to look* — `gitkit status --global`
/// re-reads every fact from disk at query time, so a hand-edited or deleted
/// hook is never misreported just because the ledger still lists it.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct Registry {
    #[serde(default)]
    pub repos: BTreeMap<String, RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct RegistryEntry {
    pub path: String,
    pub applied_at: String,
    #[serde(default)]
    pub applied: Vec<String>,
}

#[cfg(test)]
static TEST_REGISTRY_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(test)]
fn ensure_test_registry_dir() -> PathBuf {
    TEST_REGISTRY_DIR
        .get_or_init(|| {
            let dir = tempfile::TempDir::new().unwrap();
            dir.keep()
        })
        .clone()
}

pub(crate) fn registry_path() -> Result<PathBuf> {
    if let Ok(gitkit_home) = std::env::var("GITKIT_HOME") {
        return Ok(PathBuf::from(gitkit_home).join("registry.toml"));
    }
    #[cfg(test)]
    {
        Ok(ensure_test_registry_dir().join("registry.toml"))
    }
    #[cfg(not(test))]
    {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .context("Neither HOME nor USERPROFILE environment variable is set")?;
        Ok(PathBuf::from(home).join(".gitkit").join("registry.toml"))
    }
}

/// Loads the ledger. A missing, empty, or unparseable file yields an empty
/// registry rather than an error — the ledger is an optimization that
/// supplies paths to check, never a precondition for gitkit to run.
pub(crate) fn load() -> Registry {
    let Ok(path) = registry_path() else {
        return Registry::default();
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return Registry::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

pub(crate) fn save(registry: &Registry) -> Result<()> {
    let path = registry_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create gitkit registry directory")?;
    }
    let content = toml::to_string_pretty(registry).context("Failed to serialize registry")?;
    fs::write(&path, content).context("Failed to write registry")?;
    Ok(())
}

/// Records that `items` were applied to `repo_root`, merging into any
/// existing entry for that path (deduped) instead of duplicating it.
pub(crate) fn record(repo_root: &Path, items: &[String]) -> Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let key = repo_root.to_string_lossy().to_string();
    let mut registry = load();
    let entry = registry
        .repos
        .entry(key.clone())
        .or_insert_with(|| RegistryEntry {
            path: key.clone(),
            applied_at: String::new(),
            applied: Vec::new(),
        });
    entry.applied_at = now_timestamp();
    for item in items {
        if !entry.applied.contains(item) {
            entry.applied.push(item.clone());
        }
    }
    save(&registry)
}

/// Best-effort wrapper for the apply entry points: the ledger must never
/// fail the caller's actual work (installing a hook, applying config, ...).
/// A write failure is warned about and otherwise swallowed.
pub(crate) fn record_best_effort(repo_root: &Path, items: &[String]) {
    if let Err(e) = record(repo_root, items) {
        eprintln!("Warning: could not update gitkit registry: {e}");
    }
}

fn now_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant's days-since-epoch -> (year, month, day) algorithm.
/// Self-contained so a single formatted timestamp doesn't need a date crate.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

const SKIP_DIR_NAMES: &[&str] = &["node_modules", "target", ".cargo", ".cache", "vendor"];

pub(crate) struct ScanResult {
    /// Repositories found, each with the builtin hook names detected inside.
    pub found: Vec<(PathBuf, Vec<String>)>,
    pub dirs_visited: usize,
    pub max_depth: usize,
}

/// Walks `root` looking for repositories with a gitkit-installed hook. Never
/// follows symlinks (so it can never escape `root`), and skips a fixed list
/// of noisy directory names instead of doing full `.gitignore` parsing.
pub(crate) fn scan(root: &Path) -> ScanResult {
    let mut found = Vec::new();
    let mut dirs_visited = 0usize;
    let mut max_depth = 0usize;
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        dirs_visited += 1;
        max_depth = max_depth.max(depth);
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == ".git" {
                let builtins = detect_installed_builtins(&entry.path());
                if !builtins.is_empty() {
                    found.push((dir.clone(), builtins));
                }
                continue;
            }
            if SKIP_DIR_NAMES.contains(&name_str.as_ref()) {
                continue;
            }
            stack.push((entry.path(), depth + 1));
        }
    }

    ScanResult {
        found,
        dirs_visited,
        max_depth,
    }
}

/// Names of builtins whose content is installed under `git_dir/hooks`.
fn detect_installed_builtins(git_dir: &Path) -> Vec<String> {
    let hooks_dir = git_dir.join("hooks");
    let Ok(entries) = fs::read_dir(&hooks_dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".bak") || name.ends_with(".sample") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };

        if crate::hooks::is_dispatcher(&content, &name) {
            for part in crate::hooks::list_parts(&hooks_dir, &name) {
                if crate::hooks::builtins::get(&part).is_some() {
                    found.push(part);
                }
            }
            continue;
        }

        if let Some(b) = crate::hooks::detect_builtin(&name, &content) {
            found.push(b.name.to_string());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Provides a clean, isolated registry for tests that need an empty starting state.
    /// Sets GITKIT_HOME to a unique temp dir, ensuring no interference from other tests.
    fn with_clean_registry<F: FnOnce()>(f: F) {
        let dir = tempfile::TempDir::new().unwrap();
        let orig = std::env::var("GITKIT_HOME").ok();
        unsafe {
            std::env::set_var("GITKIT_HOME", dir.path());
        }
        f();
        unsafe {
            match &orig {
                Some(h) => std::env::set_var("GITKIT_HOME", h),
                None => std::env::remove_var("GITKIT_HOME"),
            }
        }
    }

    #[test]
    fn registry_path_returns_registry_toml_filename() {
        let path = registry_path().unwrap();
        assert_eq!(path.file_name().unwrap(), "registry.toml");
    }

    #[test]
    fn registry_path_resolves_under_test_dir_not_real_home() {
        let path = registry_path().unwrap();
        let real_home = std::env::var("HOME").unwrap_or_default();
        let real_gitkit = PathBuf::from(&real_home)
            .join(".gitkit")
            .join("registry.toml");
        assert_ne!(
            path, real_gitkit,
            "registry_path() in tests must NOT resolve to the real ~/.gitkit/registry.toml"
        );
    }

    #[serial]
    #[test]
    fn registry_path_honors_gitkit_home_over_test_default() {
        let gitkit_home = tempfile::TempDir::new().unwrap();
        let orig_gitkit_home = std::env::var("GITKIT_HOME").ok();
        unsafe {
            std::env::set_var("GITKIT_HOME", gitkit_home.path());
        }
        let path = registry_path().unwrap();
        assert!(
            path.starts_with(gitkit_home.path()),
            "GITKIT_HOME must take precedence: got {}",
            path.display()
        );
        assert_eq!(path.file_name().unwrap(), "registry.toml");
        unsafe {
            match &orig_gitkit_home {
                Some(h) => std::env::set_var("GITKIT_HOME", h),
                None => std::env::remove_var("GITKIT_HOME"),
            }
        }
    }

    #[serial]
    #[test]
    fn load_missing_registry_returns_default() {
        with_clean_registry(|| {
            let reg = load();
            assert!(reg.repos.is_empty());
        });
    }

    #[serial]
    #[test]
    fn load_corrupt_registry_returns_default_not_panic() {
        with_clean_registry(|| {
            let dir = ensure_test_registry_dir();
            fs::write(dir.join("registry.toml"), "not valid toml {{{").unwrap();
            let reg = load();
            assert!(reg.repos.is_empty());
        });
    }

    #[serial]
    #[test]
    fn load_empty_registry_returns_default() {
        with_clean_registry(|| {
            let dir = ensure_test_registry_dir();
            fs::write(dir.join("registry.toml"), "").unwrap();
            let reg = load();
            assert!(reg.repos.is_empty());
        });
    }

    #[serial]
    #[test]
    fn record_writes_entry_with_absolute_path() {
        with_clean_registry(|| {
            let repo = tempfile::TempDir::new().unwrap();
            record(repo.path(), &["hook:no-secrets".to_string()]).unwrap();
            let reg = load();
            let key = repo.path().to_string_lossy().to_string();
            let entry = reg.repos.get(&key).unwrap();
            assert_eq!(entry.path, key);
            assert!(entry.applied.contains(&"hook:no-secrets".to_string()));
            assert!(!entry.applied_at.is_empty());
        });
    }

    #[serial]
    #[test]
    fn record_twice_updates_single_entry_not_duplicate() {
        with_clean_registry(|| {
            let repo = tempfile::TempDir::new().unwrap();
            record(repo.path(), &["hook:no-secrets".to_string()]).unwrap();
            record(repo.path(), &["hook:conventional-commits".to_string()]).unwrap();
            let reg = load();
            assert_eq!(reg.repos.len(), 1);
            let key = repo.path().to_string_lossy().to_string();
            let entry = reg.repos.get(&key).unwrap();
            assert!(entry.applied.contains(&"hook:no-secrets".to_string()));
            assert!(entry
                .applied
                .contains(&"hook:conventional-commits".to_string()));
        });
    }

    #[serial]
    #[test]
    fn record_same_item_twice_does_not_duplicate_in_list() {
        with_clean_registry(|| {
            let repo = tempfile::TempDir::new().unwrap();
            record(repo.path(), &["hook:no-secrets".to_string()]).unwrap();
            record(repo.path(), &["hook:no-secrets".to_string()]).unwrap();
            let reg = load();
            let key = repo.path().to_string_lossy().to_string();
            let entry = reg.repos.get(&key).unwrap();
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
    fn record_empty_items_is_noop() {
        with_clean_registry(|| {
            let repo = tempfile::TempDir::new().unwrap();
            record(repo.path(), &[]).unwrap();
            let reg = load();
            assert!(reg.repos.is_empty());
        });
    }

    #[serial]
    #[test]
    fn record_best_effort_swallows_write_failure_without_panic() {
        let blocker = tempfile::TempDir::new().unwrap();
        let blocker_file = blocker.path().join("blocker");
        fs::write(&blocker_file, "not a directory").unwrap();

        let orig = std::env::var("GITKIT_HOME").ok();
        unsafe {
            std::env::set_var("GITKIT_HOME", &blocker_file);
        }

        let repo = tempfile::TempDir::new().unwrap();
        let result = record(repo.path(), &["hook:no-secrets".to_string()]);
        assert!(
            result.is_err(),
            "record must fail when GITKIT_HOME is a file"
        );

        record_best_effort(repo.path(), &["hook:no-secrets".to_string()]);

        unsafe {
            match &orig {
                Some(h) => std::env::set_var("GITKIT_HOME", h),
                None => std::env::remove_var("GITKIT_HOME"),
            }
        }
    }

    #[serial]
    #[test]
    fn save_and_load_roundtrip() {
        with_clean_registry(|| {
            let mut reg = Registry::default();
            reg.repos.insert(
                "/tmp/example".to_string(),
                RegistryEntry {
                    path: "/tmp/example".to_string(),
                    applied_at: "2026-01-01T00:00:00Z".to_string(),
                    applied: vec!["hook:no-secrets".to_string()],
                },
            );
            save(&reg).unwrap();
            let loaded = load();
            assert_eq!(loaded, reg);
        });
    }

    #[serial]
    #[test]
    fn test_suite_does_not_write_to_real_registry() {
        let real_home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let real_registry = PathBuf::from(&real_home)
            .join(".gitkit")
            .join("registry.toml");
        let real_existed = real_registry.exists();
        let real_mtime_before = fs::metadata(&real_registry)
            .ok()
            .and_then(|m| m.modified().ok());

        let repo = tempfile::TempDir::new().unwrap();
        crate::registry::record_best_effort(
            repo.path(),
            &["hook:test-isolation-check".to_string()],
        );

        if real_existed {
            let real_mtime_after = fs::metadata(&real_registry)
                .ok()
                .and_then(|m| m.modified().ok());
            assert_eq!(
                real_mtime_before, real_mtime_after,
                "the real ~/.gitkit/registry.toml must not be modified by tests"
            );
        } else {
            assert!(
                !real_registry.exists(),
                "the real ~/.gitkit/registry.toml must not be created by tests"
            );
        }
    }

    // ── civil_from_days / now_timestamp ─────────────────────────────────────

    #[test]
    fn civil_from_days_epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn civil_from_days_known_date() {
        // 2026-08-12 is 20677 days after 1970-01-01.
        assert_eq!(civil_from_days(20_677), (2026, 8, 12));
    }

    #[test]
    fn now_timestamp_has_expected_shape() {
        let ts = now_timestamp();
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.chars().nth(4), Some('-'));
    }

    // ── scan ─────────────────────────────────────────────────────────────

    #[test]
    fn scan_finds_repos_with_gitkit_hooks_only() {
        let root = tempfile::TempDir::new().unwrap();

        let repo_a = root.path().join("repo-a");
        let hooks_a = repo_a.join(".git").join("hooks");
        fs::create_dir_all(&hooks_a).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        fs::write(hooks_a.join("pre-commit"), no_secrets.script).unwrap();

        let repo_b = root.path().join("nested").join("repo-b");
        let hooks_b = repo_b.join(".git").join("hooks");
        fs::create_dir_all(&hooks_b).unwrap();
        let cc = crate::hooks::builtins::get("conventional-commits").unwrap();
        fs::write(hooks_b.join("commit-msg"), cc.script).unwrap();

        let repo_c = root.path().join("repo-c-no-hooks");
        fs::create_dir_all(repo_c.join(".git").join("hooks")).unwrap();

        let result = scan(root.path());
        assert_eq!(result.found.len(), 2);
        let found_paths: Vec<_> = result.found.iter().map(|(p, _)| p.clone()).collect();
        assert!(found_paths.contains(&repo_a));
        assert!(found_paths.contains(&repo_b));
    }

    #[test]
    fn scan_skips_noise_directories() {
        let root = tempfile::TempDir::new().unwrap();
        let hooks = root
            .path()
            .join("node_modules")
            .join("some-pkg")
            .join(".git")
            .join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        fs::write(hooks.join("pre-commit"), no_secrets.script).unwrap();

        let result = scan(root.path());
        assert!(result.found.is_empty());
    }

    #[test]
    fn scan_reports_builtin_names_found() {
        let root = tempfile::TempDir::new().unwrap();
        let hooks = root.path().join("repo").join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        fs::write(hooks.join("pre-commit"), no_secrets.script).unwrap();

        let result = scan(root.path());
        assert_eq!(result.found.len(), 1);
        assert_eq!(result.found[0].1, vec!["no-secrets".to_string()]);
    }

    #[test]
    fn scan_empty_tree_finds_nothing() {
        let root = tempfile::TempDir::new().unwrap();
        let result = scan(root.path());
        assert!(result.found.is_empty());
        assert_eq!(result.dirs_visited, 1);
    }

    #[test]
    fn scan_does_not_follow_symlinked_directories() {
        let root = tempfile::TempDir::new().unwrap();
        let real_repo = root.path().join("real-repo");
        let hooks = real_repo.join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let no_secrets = crate::hooks::builtins::get("no-secrets").unwrap();
        fs::write(hooks.join("pre-commit"), no_secrets.script).unwrap();

        let outside = tempfile::TempDir::new().unwrap();
        let escape_target = outside.path().join("escape-repo");
        let escape_hooks = escape_target.join(".git").join("hooks");
        fs::create_dir_all(&escape_hooks).unwrap();
        fs::write(escape_hooks.join("pre-commit"), no_secrets.script).unwrap();

        #[cfg(unix)]
        {
            let link = root.path().join("escaped");
            let _ = std::os::unix::fs::symlink(outside.path(), &link);
        }

        let result = scan(root.path());
        // Only the real repo inside `root` should be found; the symlinked
        // escape target must never be traversed into.
        assert_eq!(result.found.len(), 1);
        assert_eq!(result.found[0].0, real_repo);
    }
}
