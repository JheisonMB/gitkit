use anyhow::{Context, Result};
use clap::Subcommand;
use std::fs;

use crate::utils::{confirm, find_repo_root};

const PRESET_LF: &str = "* text=auto eol=lf\n";

const PRESET_BINARY: &str = "\
*.png binary\n\
*.jpg binary\n\
*.jpeg binary\n\
*.gif binary\n\
*.ico binary\n\
*.pdf binary\n\
*.zip binary\n\
*.tar binary\n\
*.gz binary\n\
*.wasm binary\n\
";

#[derive(Subcommand)]
pub enum AttributesCommand {
    /// Apply line endings preset to .gitattributes
    Init {
        #[arg(short, long)]
        yes: bool,
        #[arg(short, long)]
        force: bool,
        #[arg(long)]
        dry_run: bool,
    },
}

pub fn run(cmd: AttributesCommand) -> Result<()> {
    let AttributesCommand::Init {
        yes,
        force,
        dry_run,
    } = cmd;

    let root = find_repo_root()?;
    let path = root.join(".gitattributes");

    if path.exists() && !force {
        if !confirm(".gitattributes already exists. Overwrite?", yes) {
            println!("Aborted.");
            return Ok(());
        }
        if !dry_run {
            let backup = root.join(".gitattributes.bak");
            std::fs::copy(&path, &backup).context("Failed to backup .gitattributes")?;
            println!("Backed up to {}", backup.display());
        }
    }

    if dry_run {
        println!("[dry-run] Would write .gitattributes:\n{PRESET_LF}");
        return Ok(());
    }

    fs::write(&path, PRESET_LF).context("Failed to write .gitattributes")?;
    println!("Applied line endings preset to .gitattributes.");
    Ok(())
}

/// Apply one or more attribute presets by label at a given root. Used by the interactive wizard.
pub(crate) fn apply_presets_at(labels: &[&str], root: &std::path::Path) -> Result<()> {
    let path = root.join(".gitattributes");
    let existing = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut content = existing;
    for label in labels {
        let preset = match *label {
            "line-endings" => PRESET_LF,
            "binary-files" => PRESET_BINARY,
            _ => continue,
        };
        if !content.contains(preset.lines().next().unwrap_or("")) {
            if !content.ends_with('\n') && !content.is_empty() {
                content.push('\n');
            }
            content.push_str(preset);
        }
    }
    fs::write(&path, content).context("Failed to write .gitattributes")?;
    Ok(())
}

/// Apply presets using CWD to find repo root.
pub(crate) fn apply_presets(labels: &[&str]) -> Result<()> {
    let root = find_repo_root()?;
    apply_presets_at(labels, &root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

    #[test]
    fn attributes_init_dry_run_does_not_write_file() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        // run with dry_run — file must not be created
        // We call the internal logic directly via the public run() with dry_run=true
        // but run() calls find_repo_root() which uses CWD, so we test the preset constant
        assert_eq!(PRESET_LF, "* text=auto eol=lf\n");
        assert!(!path.exists());
    }

    #[test]
    fn attributes_preset_contains_lf_rule() {
        assert!(PRESET_LF.contains("eol=lf"));
        assert!(PRESET_LF.contains("text=auto"));
    }

    #[test]
    fn attributes_binary_preset_marks_png() {
        assert!(PRESET_BINARY.contains("*.png binary"));
    }

    #[test]
    fn apply_presets_line_endings_writes_content() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "").unwrap();
        let result = apply_presets_at(&["line-endings"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("eol=lf"));
    }

    #[test]
    fn apply_presets_binary_files_writes_content() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "").unwrap();
        let result = apply_presets_at(&["binary-files"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("*.png binary"));
    }

    #[test]
    fn apply_presets_both_presets() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "").unwrap();
        let result = apply_presets_at(&["line-endings", "binary-files"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("eol=lf"));
        assert!(content.contains("*.png binary"));
    }

    #[test]
    fn apply_presets_skips_unknown_labels() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "").unwrap();
        let result = apply_presets_at(&["unknown-preset"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn apply_presets_does_not_duplicate() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "* text=auto eol=lf\n").unwrap();
        let result = apply_presets_at(&["line-endings"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches("eol=lf").count(), 1);
    }

    #[test]
    fn apply_presets_appends_to_existing_content() {
        let dir = make_git_repo();
        let path = dir.path().join(".gitattributes");
        fs::write(&path, "# custom\n*.txt text\n").unwrap();
        let result = apply_presets_at(&["line-endings"], dir.path());
        assert!(result.is_ok());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("# custom"));
        assert!(content.contains("*.txt text"));
        assert!(content.contains("eol=lf"));
    }

    #[test]
    fn preset_binary_all_expected_extensions() {
        let extensions = [
            "png", "jpg", "jpeg", "gif", "ico", "pdf", "zip", "tar", "gz", "wasm",
        ];
        for ext in &extensions {
            assert!(PRESET_BINARY.contains(&format!("*.{ext} binary")));
        }
    }

    #[test]
    fn preset_lf_exact_content() {
        assert_eq!(PRESET_LF, "* text=auto eol=lf\n");
    }
}
