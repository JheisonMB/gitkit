use anyhow::{Context, Result};
use clap::Subcommand;
use std::fs;

use crate::utils::{confirm, find_repo_root};

const PRESET: &str = "* text=auto eol=lf\n";

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
        println!("[dry-run] Would write .gitattributes:\n{PRESET}");
        return Ok(());
    }

    fs::write(&path, PRESET).context("Failed to write .gitattributes")?;
    println!("Applied line endings preset to .gitattributes.");
    Ok(())
}
