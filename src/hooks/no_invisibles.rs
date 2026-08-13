//! Scans the *staged diff* for invisible Unicode characters and refuses the
//! commit if any are found on a line the commit adds. Backs the
//! `no-invisibles` builtin (see `builtins.rs`): the installed pre-commit
//! hook is a thin `sh` wrapper that execs `gitkit hooks scan-invisibles`,
//! because getting file/line/column/codepoint right needs real Unicode-aware
//! text handling, not POSIX sh pattern matching.
//!
//! Scope is deliberately narrow: only lines this commit *adds*, taken from
//! `git diff --cached`. A pre-existing invisible character on a line the
//! commit doesn't touch is left alone — see docs/hooks.md for why.

use anyhow::{Context, Result};
use std::process::Command;

/// One invisible character found on an added line.
pub(crate) struct Occurrence {
    pub file: String,
    /// 1-based line number in the new file.
    pub line: usize,
    /// 1-based column, counted in Unicode scalar values (`char`s), not bytes.
    pub column: usize,
    pub codepoint: u32,
    pub name: &'static str,
}

/// Every codepoint this hook flags, grouped by why it's here. Deliberately
/// excludes variation selectors (U+FE0F etc.), NBSP (U+00A0) and the soft
/// hyphen (U+00AD) — all have legitimate uses in this workspace (emoji
/// presentation, prose, LaTeX) and are not invisible-payload carriers.
///
/// U+200D ZERO WIDTH JOINER is flagged unconditionally, including inside
/// legitimate emoji sequences (e.g. the family emoji, which is three emoji
/// joined by ZWJ). Distinguishing "load-bearing" ZWJ from a smuggled one
/// would require an emoji-sequence table this hook doesn't have; the
/// decision here is to accept that false positive rather than silently miss
/// a real one. Strip the character from the emoji or edit around it.
pub(crate) const FLAGGED_CODEPOINTS: &[(u32, u32, &str)] = &[
    // Zero-width: no visible glyph, the classic invisible-payload carrier.
    // U+FEFF is included here too; the one-character exception for a
    // leading byte-order mark is applied by the caller, not this table.
    (0x200B, 0x200B, "ZERO WIDTH SPACE"),
    (0x200C, 0x200C, "ZERO WIDTH NON-JOINER"),
    (0x200D, 0x200D, "ZERO WIDTH JOINER"),
    (0x2060, 0x2060, "WORD JOINER"),
    (0xFEFF, 0xFEFF, "ZERO WIDTH NO-BREAK SPACE"),
    (0x180E, 0x180E, "MONGOLIAN VOWEL SEPARATOR"),
    // Bidi controls: invisible direction overrides. Beyond hiding a
    // provenance mark, these enable "Trojan Source" attacks, where the
    // order code displays in differs from the order it compiles in — reason
    // enough to flag them on their own.
    (0x200E, 0x200E, "LEFT-TO-RIGHT MARK"),
    (0x200F, 0x200F, "RIGHT-TO-LEFT MARK"),
    (0x202A, 0x202A, "LEFT-TO-RIGHT EMBEDDING"),
    (0x202B, 0x202B, "RIGHT-TO-LEFT EMBEDDING"),
    (0x202C, 0x202C, "POP DIRECTIONAL FORMATTING"),
    (0x202D, 0x202D, "LEFT-TO-RIGHT OVERRIDE"),
    (0x202E, 0x202E, "RIGHT-TO-LEFT OVERRIDE"),
    (0x2066, 0x2066, "LEFT-TO-RIGHT ISOLATE"),
    (0x2067, 0x2067, "RIGHT-TO-LEFT ISOLATE"),
    (0x2068, 0x2068, "FIRST STRONG ISOLATE"),
    (0x2069, 0x2069, "POP DIRECTIONAL ISOLATE"),
    // Unicode tag characters: no rendering at all, the classic
    // invisible-payload carrier.
    (0xE0000, 0xE007F, "UNICODE TAG CHARACTER"),
];

fn flagged_name(codepoint: u32) -> Option<&'static str> {
    FLAGGED_CODEPOINTS
        .iter()
        .find(|(lo, hi, _)| codepoint >= *lo && codepoint <= *hi)
        .map(|(_, _, name)| *name)
}

/// Runs `git diff --cached` and returns every flagged character found on a
/// line the staged change adds. Renamed files are detected (`-M`) so a pure
/// rename contributes no hunks (nothing to scan) and a rename-with-edit only
/// exposes the lines that actually changed — not the whole file. Binary
/// files and files whose diff isn't valid UTF-8 are skipped without error.
pub(crate) fn scan_staged_diff() -> Result<Vec<Occurrence>> {
    let output = Command::new("git")
        .args([
            "diff",
            "--cached",
            "-M",
            "--unified=0",
            "--no-color",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
        ])
        .output()
        .context("Failed to run 'git diff --cached'")?;
    anyhow::ensure!(
        output.status.success(),
        "git diff --cached exited with an error"
    );

    let mut occurrences = Vec::new();
    for block in split_file_blocks(&output.stdout) {
        // Non-UTF-8 (or binary-looking) diff content is skipped, not an
        // error: this hook inspects source text, not arbitrary bytes.
        if let Ok(text) = std::str::from_utf8(block) {
            scan_block(text, &mut occurrences);
        }
    }
    Ok(occurrences)
}

/// Splits raw `git diff` output into one slice per file, each starting at
/// its `diff --git ` header line.
fn split_file_blocks(raw: &[u8]) -> Vec<&[u8]> {
    const MARKER: &[u8] = b"diff --git ";
    let mut starts = Vec::new();
    let mut i = 0;
    while i + MARKER.len() <= raw.len() {
        if (i == 0 || raw[i - 1] == b'\n') && &raw[i..i + MARKER.len()] == MARKER {
            starts.push(i);
        }
        i += 1;
    }
    starts
        .iter()
        .enumerate()
        .map(|(idx, &start)| {
            let end = starts.get(idx + 1).copied().unwrap_or(raw.len());
            &raw[start..end]
        })
        .collect()
}

/// Scans one file's diff block, appending any flagged characters found on
/// added lines to `out`.
fn scan_block(block: &str, out: &mut Vec<Occurrence>) {
    if block.lines().any(|l| l.starts_with("Binary files ")) {
        return;
    }

    let Some(path) = block.lines().find_map(new_file_path) else {
        return; // deleted file (no `+` lines) or unparsable header
    };

    let mut current_line: usize = 0;
    let mut in_hunk = false;

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("@@ ") {
            in_hunk = true;
            current_line = parse_new_start(rest).unwrap_or(0);
            continue;
        }
        if !in_hunk {
            continue;
        }
        if let Some(content) = line.strip_prefix('+') {
            scan_line(&path, current_line, content, out);
            current_line += 1;
        }
        // '-' lines (removed) don't advance the new-file line counter, and
        // '\ No newline at end of file' markers carry no line to scan.
    }
}

/// Extracts the new-file path from a `+++ b/<path>` line, or `None` for
/// `+++ /dev/null` (a deleted file) or a non-matching line.
fn new_file_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("+++ ")?;
    if rest == "/dev/null" {
        return None;
    }
    Some(rest.strip_prefix("b/").unwrap_or(rest).to_string())
}

/// Parses the new-file start line number out of a hunk header remainder,
/// e.g. `-12,3 +45,6 @@ fn foo() {` -> `45`.
fn parse_new_start(rest: &str) -> Option<usize> {
    let plus_part = rest.split_whitespace().find(|tok| tok.starts_with('+'))?;
    plus_part
        .trim_start_matches('+')
        .split(',')
        .next()?
        .parse()
        .ok()
}

/// Scans one added line's content for flagged characters. `line_no` is the
/// 1-based line number this content occupies in the new file.
fn scan_line(path: &str, line_no: usize, content: &str, out: &mut Vec<Occurrence>) {
    for (idx, ch) in content.chars().enumerate() {
        let codepoint = ch as u32;
        // A leading BOM is a byte-order mark, not a watermark.
        if codepoint == 0xFEFF && line_no == 1 && idx == 0 {
            continue;
        }
        if let Some(name) = flagged_name(codepoint) {
            out.push(Occurrence {
                file: path.to_string(),
                line: line_no,
                column: idx + 1,
                codepoint,
                name,
            });
        }
    }
}

/// Formats occurrences into the human-readable report the hook prints.
pub(crate) fn format_report(occurrences: &[Occurrence]) -> Vec<String> {
    let mut lines = vec!["ERROR: invisible Unicode character(s) in staged changes:".to_string()];
    for occ in occurrences {
        lines.push(format!(
            "  {}:{}:{}: U+{:04X} {}",
            occ.file, occ.line, occ.column, occ.codepoint, occ.name
        ));
    }
    lines.push(String::new());
    lines.push("Remove the character(s) above and commit again.".to_string());
    lines.push(
        "Only lines this commit adds are scanned — a pre-existing invisible character on a line you didn't touch will not be caught.".to_string(),
    );
    lines
}

/// Entry point for `gitkit hooks scan-invisibles`, the command the
/// `no-invisibles` pre-commit hook execs. Exits non-zero via
/// `std::process::exit` when invisible characters are found, so it's kept
/// separate from the testable `scan_staged_diff`/`format_report` functions
/// above.
pub(crate) fn run() -> Result<()> {
    let occurrences = scan_staged_diff()?;
    if occurrences.is_empty() {
        return Ok(());
    }
    for line in format_report(&occurrences) {
        eprintln!("{line}");
    }
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .status()
            .expect("failed to run git");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        run_git(&["init", "-q"]);
        run_git(&["config", "user.email", "test@example.com"]);
        run_git(&["config", "user.name", "Test"]);
        std::env::set_current_dir(orig).unwrap();
        dir
    }

    /// Runs `body` with the process cwd set to `dir`, always restoring the
    /// original cwd afterwards even if `body` panics.
    fn in_dir<R>(dir: &Path, body: impl FnOnce() -> R) -> R {
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        std::env::set_current_dir(orig).unwrap();
        match result {
            Ok(r) => r,
            Err(e) => std::panic::resume_unwind(e),
        }
    }

    #[serial]
    #[test]
    fn zero_width_space_on_added_line_is_rejected() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("file.txt", "clean line\n").unwrap();
            run_git(&["add", "file.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write("file.txt", "clean line\nhidden\u{200B}space\n").unwrap();
            run_git(&["add", "file.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 1, "expected exactly one occurrence");
            assert_eq!(occurrences[0].file, "file.txt");
            assert_eq!(occurrences[0].line, 2);
            assert_eq!(occurrences[0].column, 7);
            assert_eq!(occurrences[0].codepoint, 0x200B);
            assert_eq!(occurrences[0].name, "ZERO WIDTH SPACE");
        });
    }

    #[serial]
    #[test]
    fn bidi_control_on_added_line_is_rejected() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("file.txt", "clean line\n").unwrap();
            run_git(&["add", "file.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write(
                "file.txt",
                "clean line\nlooks\u{202E}reversed\u{202C}here\n",
            )
            .unwrap();
            run_git(&["add", "file.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 2);
            assert_eq!(occurrences[0].codepoint, 0x202E);
            assert_eq!(occurrences[0].name, "RIGHT-TO-LEFT OVERRIDE");
            assert_eq!(occurrences[1].codepoint, 0x202C);
            assert_eq!(occurrences[1].name, "POP DIRECTIONAL FORMATTING");
            assert_eq!(occurrences[0].line, 2);
        });
    }

    #[serial]
    #[test]
    fn tag_character_on_added_line_is_rejected() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("file.txt", "clean line\n").unwrap();
            run_git(&["add", "file.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write("file.txt", "clean line\npayload\u{E0001}hidden\n").unwrap();
            run_git(&["add", "file.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 1);
            assert_eq!(occurrences[0].codepoint, 0xE0001);
            assert_eq!(occurrences[0].name, "UNICODE TAG CHARACTER");
            assert_eq!(occurrences[0].line, 2);
        });
    }

    /// Regression test for the scope decision: a pre-existing invisible
    /// character on a line this commit does not touch must not block it.
    #[serial]
    #[test]
    fn invisible_character_on_untouched_line_is_accepted() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("file.txt", "line one\nhas\u{200B}invisible\nline three\n").unwrap();
            run_git(&["add", "file.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            // Only line 3 changes; line 2's ZWSP is pre-existing and untouched.
            std::fs::write(
                "file.txt",
                "line one\nhas\u{200B}invisible\nline three CHANGED\n",
            )
            .unwrap();
            run_git(&["add", "file.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(
                occurrences.is_empty(),
                "pre-existing invisible character on an untouched line must not block the commit: {:?}",
                occurrences.iter().map(|o| (o.line, o.codepoint)).collect::<Vec<_>>()
            );
        });
    }

    #[serial]
    #[test]
    fn leading_bom_is_accepted() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("bom.txt", "\u{FEFF}hello world\n").unwrap();
            run_git(&["add", "bom.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(
                occurrences.is_empty(),
                "a leading BOM is not a watermark: {:?}",
                occurrences.iter().map(|o| o.line).collect::<Vec<_>>()
            );
        });
    }

    #[serial]
    #[test]
    fn feff_added_in_middle_of_file_is_rejected() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("mid.txt", "line one\n").unwrap();
            run_git(&["add", "mid.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write("mid.txt", "line one\nsecond\u{FEFF}line\n").unwrap();
            run_git(&["add", "mid.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 1);
            assert_eq!(occurrences[0].codepoint, 0xFEFF);
            assert_eq!(occurrences[0].line, 2);
            assert_eq!(occurrences[0].column, 7);
        });
    }

    /// Regression test for the false positive that would have corrupted
    /// ghscaff's `derive_key_emoji_passphrase` fixture: variation selectors
    /// following emoji base characters must never be flagged.
    #[serial]
    #[test]
    fn variation_selector_emoji_sequences_are_accepted() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("vault.rs", "// placeholder\n").unwrap();
            run_git(&["add", "vault.rs"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write(
                "vault.rs",
                "// placeholder\nlet label = \"\u{2699}\u{FE0F} \u{1F3F7}\u{FE0F}\";\nlet key = derive_key(\"\u{1F511}\u{1F6E1}\u{FE0F}\").unwrap();\n",
            )
            .unwrap();
            run_git(&["add", "vault.rs"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(
                occurrences.is_empty(),
                "variation selectors after emoji must not be flagged: {:?}",
                occurrences
                    .iter()
                    .map(|o| (o.line, o.codepoint))
                    .collect::<Vec<_>>()
            );
        });
    }

    #[serial]
    #[test]
    fn nbsp_and_soft_hyphen_are_accepted() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("prose.tex", "placeholder\n").unwrap();
            run_git(&["add", "prose.tex"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write(
                "prose.tex",
                "placeholder\nnaive\u{00AD}ty caf\u{00A0}e menu\n",
            )
            .unwrap();
            run_git(&["add", "prose.tex"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(
                occurrences.is_empty(),
                "NBSP and soft hyphen are out of scope: {:?}",
                occurrences
                    .iter()
                    .map(|o| (o.line, o.codepoint))
                    .collect::<Vec<_>>()
            );
        });
    }

    #[serial]
    #[test]
    fn binary_file_is_skipped_without_error() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("blob.dat", [0u8, 159, 146, 150, 0, 1, 2, 3, 200, 5]).unwrap();
            run_git(&["add", "blob.dat"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(occurrences.is_empty(), "binary content must be skipped");
        });
    }

    #[serial]
    #[test]
    fn multiple_occurrences_on_one_line_are_all_reported() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("multi.txt", "start\n").unwrap();
            run_git(&["add", "multi.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            std::fs::write("multi.txt", "start\na\u{200B}b\u{200C}c\u{2060}d\n").unwrap();
            run_git(&["add", "multi.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 3);
            assert_eq!(occurrences[0].codepoint, 0x200B);
            assert_eq!(occurrences[1].codepoint, 0x200C);
            assert_eq!(occurrences[2].codepoint, 0x2060);
            assert!(occurrences.iter().all(|o| o.line == 2));
        });
    }

    /// Regression test for the rename-handling decision: a pure rename
    /// (100% similarity, no content change) produces no hunks, so
    /// pre-existing content — including an invisible character — is not
    /// rescanned just because the file moved.
    #[serial]
    #[test]
    fn pure_rename_without_content_change_is_accepted() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("old.txt", "has\u{200B}invisible line\n").unwrap();
            run_git(&["add", "old.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            run_git(&["mv", "old.txt", "new.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert!(
                occurrences.is_empty(),
                "a pure rename must not re-flag pre-existing content: {:?}",
                occurrences
                    .iter()
                    .map(|o| (o.file.clone(), o.line))
                    .collect::<Vec<_>>()
            );
        });
    }

    /// A rename that also edits content only flags the lines that changed —
    /// not the whole (now "added") file.
    #[serial]
    #[test]
    fn renamed_file_with_edit_flags_only_the_changed_line() {
        let dir = init_repo();
        in_dir(dir.path(), || {
            std::fs::write("a.txt", "line one\nline two\nline three\n").unwrap();
            run_git(&["add", "a.txt"]);
            run_git(&["commit", "-q", "-m", "init"]);

            run_git(&["mv", "a.txt", "b.txt"]);
            std::fs::write("b.txt", "line one\nline two\u{200B}\nline three\n").unwrap();
            run_git(&["add", "b.txt"]);

            let occurrences = scan_staged_diff().unwrap();
            assert_eq!(occurrences.len(), 1);
            assert_eq!(occurrences[0].file, "b.txt");
            assert_eq!(occurrences[0].line, 2);
        });
    }

    #[test]
    fn format_report_names_file_line_column_and_codepoint() {
        let occurrences = vec![Occurrence {
            file: "src/foo.rs".to_string(),
            line: 12,
            column: 37,
            codepoint: 0x200B,
            name: "ZERO WIDTH SPACE",
        }];
        let report = format_report(&occurrences);
        assert!(report.iter().any(|l| l.contains("src/foo.rs:12:37")));
        assert!(report.iter().any(|l| l.contains("U+200B")));
        assert!(report.iter().any(|l| l.contains("ZERO WIDTH SPACE")));
    }

    #[test]
    fn format_report_empty_occurrences_still_has_header() {
        let report = format_report(&[]);
        assert!(!report.is_empty());
    }

    #[test]
    fn flagged_name_covers_every_group_boundary() {
        assert_eq!(flagged_name(0x200B), Some("ZERO WIDTH SPACE"));
        assert_eq!(flagged_name(0x200F), Some("RIGHT-TO-LEFT MARK"));
        assert_eq!(flagged_name(0xE0000), Some("UNICODE TAG CHARACTER"));
        assert_eq!(flagged_name(0xE007F), Some("UNICODE TAG CHARACTER"));
        assert_eq!(flagged_name(0xE0080), None);
        assert_eq!(flagged_name('a' as u32), None);
        assert_eq!(
            flagged_name(0xFE0F),
            None,
            "variation selectors are out of scope"
        );
        assert_eq!(flagged_name(0x00A0), None, "NBSP is out of scope");
        assert_eq!(flagged_name(0x00AD), None, "soft hyphen is out of scope");
    }
}
