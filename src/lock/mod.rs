use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::hooks;
use crate::utils::find_repo_root;

const LOCK_FILE_NAME: &str = "gitkit.lock";
const HOOK_NAME: &str = "pre-commit";
const BACKUP_SUFFIX: &str = "gitkit-orig";
const DEFAULT_REASON: &str = "Agent session active";

/// The pre-commit hook gitkit installs. Pure POSIX `sh` — no dependency on the
/// `gitkit` binary or any JSON tooling, so it stays fast and has nothing new
/// to fail on the hot path. Reads `gitkit.lock` next to it, fails open on any
/// missing/malformed/expired lock, and chains to a backed-up user hook if any.
const LOCK_HOOK_SCRIPT: &str = r#"#!/bin/sh
# Installed by `gitkit lock`. Blocks commits while a lock is active.
# See `gitkit lock status` / `gitkit unlock`. Bypass with `git commit --no-verify`.

git_dir=$(git rev-parse --git-dir 2>/dev/null) || exit 0
lock_file="$git_dir/gitkit.lock"
orig_hook="$git_dir/hooks/pre-commit.gitkit-orig"

if [ -f "$lock_file" ]; then
  ops=$(sed -n 's/.*"operations":\[\([^]]*\)\].*/\1/p' "$lock_file" 2>/dev/null)
  if printf '%s' "$ops" | grep -qF '"commit"'; then
    expires=$(sed -n 's/.*"expires_at":"\([^"]*\)".*/\1/p' "$lock_file" 2>/dev/null)
    blocked=1
    if [ -n "$expires" ]; then
      now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
      if [ "$now" \> "$expires" ]; then
        blocked=0
      fi
    fi
    if [ "$blocked" -eq 1 ]; then
      reason=$(sed -n 's/.*"reason":"\([^"]*\)".*/\1/p' "$lock_file" 2>/dev/null)
      echo "gitkit: commit blocked - ${reason:-Agent session active}" >&2
      echo "gitkit: run 'gitkit unlock' to remove the lock, or 'git commit --no-verify' to bypass it" >&2
      exit 1
    fi
  fi
fi

if [ -x "$orig_hook" ]; then
  exec "$orig_hook" "$@"
fi

exit 0
"#;

#[derive(Args)]
pub struct LockArgs {
    #[command(subcommand)]
    action: Option<LockAction>,
    /// Auto-expire the lock after a duration, e.g. 30m, 2h, 1d
    #[arg(long)]
    timeout: Option<String>,
    /// Message shown when a blocked commit is attempted
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Subcommand)]
enum LockAction {
    /// Show whether a lock is currently active
    Status,
}

pub fn run(args: LockArgs) -> Result<()> {
    match args.action {
        Some(LockAction::Status) => status(),
        None => lock(args.timeout.as_deref(), args.reason.as_deref()),
    }
}

pub fn unlock() -> Result<()> {
    let root = find_repo_root()?;
    let path = lock_file_path(&root);
    if path.exists() {
        fs::remove_file(&path).context("Failed to remove lock file")?;
    }
    uninstall_hook()?;
    println!("Unlocked. Commits are no longer blocked by gitkit.");
    Ok(())
}

fn lock(timeout: Option<&str>, reason: Option<&str>) -> Result<()> {
    let root = find_repo_root()?;

    let expires_at = timeout
        .map(parse_duration)
        .transpose()?
        .map(|secs| format_rfc3339(unix_now() + secs));

    let lf = LockFile {
        locked_at: format_rfc3339(unix_now()),
        expires_at,
        reason: reason.unwrap_or(DEFAULT_REASON).to_string(),
        operations: vec!["commit".to_string()],
    };

    fs::write(lock_file_path(&root), lf.to_json()).context("Failed to write lock file")?;
    install_hook().context("Failed to install pre-commit hook")?;

    println!("Locked: {}", lf.reason);
    match &lf.expires_at {
        Some(exp) => println!("Expires at: {exp}"),
        None => println!("Expires at: never (until `gitkit unlock`)"),
    }
    Ok(())
}

fn status() -> Result<()> {
    let root = find_repo_root()?;
    let path = lock_file_path(&root);

    if !path.exists() {
        println!("No lock active.");
        return Ok(());
    }

    let content = fs::read_to_string(&path).context("Failed to read lock file")?;
    let Some(lf) = LockFile::parse(&content) else {
        println!("Lock file is malformed - treated as unlocked (commits are not blocked).");
        return Ok(());
    };

    if !lf.operations.iter().any(|op| op == "commit") {
        println!("No commit lock active.");
        return Ok(());
    }

    let now = format_rfc3339(unix_now());
    println!("Locked: {}", lf.reason);
    println!("Locked at: {}", lf.locked_at);
    match &lf.expires_at {
        Some(exp) if now.as_str() > exp.as_str() => {
            println!("Expires at: {exp} (expired - commits are not blocked)");
        }
        Some(exp) => println!("Expires at: {exp}"),
        None => println!("Expires at: never"),
    }
    Ok(())
}

fn lock_file_path(root: &Path) -> PathBuf {
    root.join(".git").join(LOCK_FILE_NAME)
}

fn install_hook() -> Result<()> {
    let dir = hooks::hooks_dir()?;
    fs::create_dir_all(&dir).context("Failed to create hooks directory")?;
    let hook_path = dir.join(HOOK_NAME);
    let backup_path = dir.join(format!("{HOOK_NAME}.{BACKUP_SUFFIX}"));

    if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path).unwrap_or_default();
        if existing.trim() != LOCK_HOOK_SCRIPT.trim() {
            fs::copy(&hook_path, &backup_path)
                .context("Failed to back up existing pre-commit hook")?;
        }
    }

    fs::write(&hook_path, LOCK_HOOK_SCRIPT).context("Failed to write pre-commit hook")?;
    hooks::set_executable(&hook_path)?;
    Ok(())
}

/// Only touches the hook file if it's still the one gitkit installed —
/// never clobbers a hook that was manually replaced after locking.
fn uninstall_hook() -> Result<()> {
    let dir = hooks::hooks_dir()?;
    let hook_path = dir.join(HOOK_NAME);
    let backup_path = dir.join(format!("{HOOK_NAME}.{BACKUP_SUFFIX}"));

    if !hook_path.exists() {
        return Ok(());
    }
    let existing = fs::read_to_string(&hook_path).unwrap_or_default();
    if existing.trim() != LOCK_HOOK_SCRIPT.trim() {
        return Ok(());
    }

    if backup_path.exists() {
        fs::rename(&backup_path, &hook_path)
            .context("Failed to restore original pre-commit hook")?;
        hooks::set_executable(&hook_path)?;
    } else {
        fs::remove_file(&hook_path).context("Failed to remove pre-commit hook")?;
    }
    Ok(())
}

// ── lock file model ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
struct LockFile {
    locked_at: String,
    expires_at: Option<String>,
    reason: String,
    operations: Vec<String>,
}

impl LockFile {
    /// Compact single-line JSON by design: keeps the pre-commit hook's shell
    /// parsing (one `sed` pass per field) simple and fast.
    fn to_json(&self) -> String {
        let expires = match &self.expires_at {
            Some(e) => format!("\"{}\"", escape(e)),
            None => "null".to_string(),
        };
        let ops = self
            .operations
            .iter()
            .map(|o| format!("\"{}\"", escape(o)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"locked_at\":\"{}\",\"expires_at\":{},\"reason\":\"{}\",\"operations\":[{}]}}\n",
            escape(&self.locked_at),
            expires,
            escape(&self.reason),
            ops
        )
    }

    fn parse(content: &str) -> Option<Self> {
        let locked_at = extract_string(content, "locked_at")?;
        let reason = extract_string(content, "reason").unwrap_or_default();
        let expires_at = extract_string(content, "expires_at");
        let operations = extract_array(content, "operations")?;
        Some(LockFile {
            locked_at,
            expires_at,
            reason,
            operations,
        })
    }
}

fn extract_string(content: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = content.find(&needle)? + needle.len();
    let rest = &content[start..];
    let mut end = None;
    let mut escaped = false;
    for (i, c) in rest.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => {
                end = Some(i);
                break;
            }
            _ => {}
        }
    }
    Some(unescape(&rest[..end?]))
}

fn extract_array(content: &str, key: &str) -> Option<Vec<String>> {
    let needle = format!("\"{key}\":[");
    let start = content.find(&needle)? + needle.len();
    let rest = &content[start..];
    let end = rest.find(']')?;
    let inner = &rest[..end];
    if inner.trim().is_empty() {
        return Some(Vec::new());
    }
    inner
        .split(',')
        .map(|s| {
            let s = s.trim();
            let s = s.strip_prefix('"')?.strip_suffix('"')?;
            Some(unescape(s))
        })
        .collect()
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

// ── duration & time helpers ──────────────────────────────────────────────

fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    anyhow::ensure!(!s.is_empty(), "Duration cannot be empty");

    let mut total: u64 = 0;
    let mut num = String::new();
    let mut any_unit = false;

    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        anyhow::ensure!(
            !num.is_empty(),
            "Invalid duration '{s}': expected a number before '{c}'"
        );
        let n: u64 = num
            .parse()
            .with_context(|| format!("Invalid duration '{s}'"))?;
        let mult: u64 = match c {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86400,
            _ => anyhow::bail!("Invalid duration unit '{c}' in '{s}' (use s, m, h, or d)"),
        };
        total += n * mult;
        num.clear();
        any_unit = true;
    }

    anyhow::ensure!(
        num.is_empty(),
        "Invalid duration '{s}': trailing number with no unit"
    );
    anyhow::ensure!(
        any_unit,
        "Invalid duration '{s}': missing unit (use s, m, h, or d)"
    );
    Ok(total)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Howard Hinnant's `civil_from_days`: converts days since 1970-01-01 into a
/// proleptic-Gregorian (year, month, day). See
/// http://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn format_rfc3339(secs: u64) -> String {
    let secs = secs as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    // ── format_rfc3339 / civil_from_days ────────────────────────────────────

    #[test]
    fn format_rfc3339_epoch_zero() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_rfc3339_known_timestamps() {
        assert_eq!(format_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_rfc3339(1_754_812_800), "2025-08-10T08:00:00Z");
        assert_eq!(format_rfc3339(1_893_456_000), "2030-01-01T00:00:00Z");
    }

    #[test]
    fn format_rfc3339_leap_day() {
        assert_eq!(format_rfc3339(951_782_400), "2000-02-29T00:00:00Z");
    }

    // ── parse_duration ───────────────────────────────────────────────────────

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("30m").unwrap(), 30 * 60);
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("2h").unwrap(), 2 * 3600);
    }

    #[test]
    fn parse_duration_days() {
        assert_eq!(parse_duration("1d").unwrap(), 86400);
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("45s").unwrap(), 45);
    }

    #[test]
    fn parse_duration_combined_units() {
        assert_eq!(parse_duration("1h30m").unwrap(), 3600 + 30 * 60);
    }

    #[test]
    fn parse_duration_rejects_empty() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn parse_duration_rejects_missing_unit() {
        assert!(parse_duration("30").is_err());
    }

    #[test]
    fn parse_duration_rejects_unknown_unit() {
        assert!(parse_duration("30x").is_err());
    }

    #[test]
    fn parse_duration_rejects_unit_without_number() {
        assert!(parse_duration("m").is_err());
    }

    #[test]
    fn parse_duration_rejects_trailing_number() {
        assert!(parse_duration("30m5").is_err());
    }

    // ── escape / unescape ────────────────────────────────────────────────────

    #[test]
    fn escape_unescape_round_trip() {
        let s = "quotes \" and \\ and\nnewlines\tand\rtabs";
        assert_eq!(unescape(&escape(s)), s);
    }

    #[test]
    fn escape_produces_single_line_output() {
        let s = "line one\nline two";
        assert!(!escape(s).contains('\n'));
    }

    // ── LockFile to_json / parse ─────────────────────────────────────────────

    #[test]
    fn lock_file_round_trips_through_json() {
        let lf = LockFile {
            locked_at: "2026-07-31T10:00:00Z".to_string(),
            expires_at: Some("2026-07-31T10:30:00Z".to_string()),
            reason: "Agent session active".to_string(),
            operations: vec!["commit".to_string()],
        };
        let json = lf.to_json();
        let parsed = LockFile::parse(&json).unwrap();
        assert_eq!(parsed, lf);
    }

    #[test]
    fn lock_file_round_trips_with_null_expiry() {
        let lf = LockFile {
            locked_at: "2026-07-31T10:00:00Z".to_string(),
            expires_at: None,
            reason: "no timeout".to_string(),
            operations: vec!["commit".to_string()],
        };
        let json = lf.to_json();
        assert!(json.contains("\"expires_at\":null"));
        let parsed = LockFile::parse(&json).unwrap();
        assert_eq!(parsed, lf);
    }

    #[test]
    fn lock_file_json_is_single_line() {
        let lf = LockFile {
            locked_at: "2026-07-31T10:00:00Z".to_string(),
            expires_at: None,
            reason: "multi\nline\nreason".to_string(),
            operations: vec!["commit".to_string()],
        };
        let json = lf.to_json();
        assert_eq!(json.trim().lines().count(), 1);
    }

    #[test]
    fn lock_file_reason_containing_word_commit_does_not_confuse_operations() {
        let lf = LockFile {
            locked_at: "2026-07-31T10:00:00Z".to_string(),
            expires_at: None,
            reason: "no commits during agent session".to_string(),
            operations: vec!["push".to_string()],
        };
        let json = lf.to_json();
        let parsed = LockFile::parse(&json).unwrap();
        assert_eq!(parsed.operations, vec!["push".to_string()]);
        assert!(!parsed.operations.iter().any(|o| o == "commit"));
    }

    #[test]
    fn lock_file_parse_rejects_malformed_content() {
        assert!(LockFile::parse("not json at all").is_none());
        assert!(LockFile::parse("").is_none());
        assert!(LockFile::parse("{{{garbage").is_none());
    }

    #[test]
    fn lock_file_parse_missing_locked_at_fails() {
        assert!(LockFile::parse(r#"{"reason":"x","operations":["commit"]}"#).is_none());
    }

    #[test]
    fn lock_file_parse_missing_operations_fails() {
        assert!(LockFile::parse(r#"{"locked_at":"2026-01-01T00:00:00Z","reason":"x"}"#).is_none());
    }

    #[test]
    fn lock_file_parse_empty_operations_array() {
        let parsed =
            LockFile::parse(r#"{"locked_at":"2026-01-01T00:00:00Z","reason":"x","operations":[]}"#)
                .unwrap();
        assert!(parsed.operations.is_empty());
    }

    // ── lock hook script sanity ──────────────────────────────────────────────

    #[test]
    fn lock_hook_script_is_valid_shell_shebang() {
        assert!(LOCK_HOOK_SCRIPT.starts_with("#!/bin/sh"));
    }

    #[test]
    fn lock_hook_script_references_lock_file_and_backup() {
        assert!(LOCK_HOOK_SCRIPT.contains("gitkit.lock"));
        assert!(LOCK_HOOK_SCRIPT.contains("pre-commit.gitkit-orig"));
        assert!(LOCK_HOOK_SCRIPT.contains("--no-verify"));
    }

    // ── install_hook / uninstall_hook ────────────────────────────────────────

    #[serial]
    #[test]
    fn install_hook_writes_executable_pre_commit_hook() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = install_hook();
        assert!(result.is_ok());
        let hook_path = dir.path().join(".git").join("hooks").join("pre-commit");
        assert!(hook_path.exists());
        let content = std::fs::read_to_string(&hook_path).unwrap();
        assert_eq!(content, LOCK_HOOK_SCRIPT);

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn install_hook_backs_up_existing_user_hook() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\necho user hook\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = install_hook();
        assert!(result.is_ok());
        let backup = hooks_dir.join("pre-commit.gitkit-orig");
        assert!(backup.exists());
        assert!(std::fs::read_to_string(&backup)
            .unwrap()
            .contains("echo user hook"));
        let content = std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap();
        assert_eq!(content, LOCK_HOOK_SCRIPT);

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn install_hook_twice_does_not_overwrite_backup_with_own_script() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\necho user hook\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        install_hook().unwrap();
        install_hook().unwrap();

        let backup = std::fs::read_to_string(hooks_dir.join("pre-commit.gitkit-orig")).unwrap();
        assert!(backup.contains("echo user hook"));

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn uninstall_hook_restores_backed_up_user_hook() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\necho user hook\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        install_hook().unwrap();
        uninstall_hook().unwrap();

        let content = std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap();
        assert!(content.contains("echo user hook"));
        assert!(!hooks_dir.join("pre-commit.gitkit-orig").exists());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn uninstall_hook_removes_hook_when_no_backup_existed() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        install_hook().unwrap();
        uninstall_hook().unwrap();

        assert!(!dir
            .path()
            .join(".git")
            .join("hooks")
            .join("pre-commit")
            .exists());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn uninstall_hook_leaves_non_gitkit_hook_untouched() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(
            hooks_dir.join("pre-commit"),
            "#!/bin/sh\necho someone else\n",
        )
        .unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        uninstall_hook().unwrap();

        let content = std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap();
        assert!(content.contains("echo someone else"));

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn uninstall_hook_noop_when_nothing_installed() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        assert!(uninstall_hook().is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    // ── lock() / unlock() / status() integration ────────────────────────────

    #[serial]
    #[test]
    fn lock_then_status_reports_active_lock() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        lock(None, Some("testing")).unwrap();
        let lock_path = dir.path().join(".git").join(LOCK_FILE_NAME);
        assert!(lock_path.exists());
        let content = std::fs::read_to_string(&lock_path).unwrap();
        assert!(content.contains("\"reason\":\"testing\""));
        assert!(content.contains("\"operations\":[\"commit\"]"));
        assert!(status().is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn lock_with_timeout_sets_expires_at() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        lock(Some("30m"), None).unwrap();
        let content =
            std::fs::read_to_string(dir.path().join(".git").join(LOCK_FILE_NAME)).unwrap();
        assert!(content.contains("\"expires_at\":\""));
        assert!(!content.contains("\"expires_at\":null"));

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn lock_rejects_invalid_timeout() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = lock(Some("nonsense"), None);
        assert!(result.is_err());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn lock_twice_is_idempotent_and_updates_reason() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        lock(None, Some("first")).unwrap();
        lock(None, Some("second")).unwrap();

        let content =
            std::fs::read_to_string(dir.path().join(".git").join(LOCK_FILE_NAME)).unwrap();
        assert!(content.contains("\"reason\":\"second\""));
        assert!(!content.contains("\"reason\":\"first\""));

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn unlock_removes_lock_file_and_restores_hook() {
        let dir = TempDir::new().unwrap();
        let hooks_dir = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\necho user hook\n").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        lock(None, None).unwrap();
        assert!(dir.path().join(".git").join(LOCK_FILE_NAME).exists());

        unlock().unwrap();
        assert!(!dir.path().join(".git").join(LOCK_FILE_NAME).exists());
        let content = std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap();
        assert!(content.contains("echo user hook"));

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn status_reports_no_lock_when_file_missing() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        assert!(status().is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn status_reports_malformed_lock_file_as_unlocked() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git").join(LOCK_FILE_NAME), "not json").unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        assert!(status().is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn status_reports_expired_lock() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let lf = LockFile {
            locked_at: "2000-01-01T00:00:00Z".to_string(),
            expires_at: Some("2000-01-01T00:01:00Z".to_string()),
            reason: "long expired".to_string(),
            operations: vec!["commit".to_string()],
        };
        std::fs::write(dir.path().join(".git").join(LOCK_FILE_NAME), lf.to_json()).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        assert!(status().is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatches_status_action() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = run(LockArgs {
            action: Some(LockAction::Status),
            timeout: None,
            reason: None,
        });
        assert!(result.is_ok());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[serial]
    #[test]
    fn run_dispatches_lock_action() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = run(LockArgs {
            action: None,
            timeout: Some("1h".to_string()),
            reason: Some("ci".to_string()),
        });
        assert!(result.is_ok());
        assert!(dir.path().join(".git").join(LOCK_FILE_NAME).exists());

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }
}
