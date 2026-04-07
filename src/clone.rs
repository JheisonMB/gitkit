use anyhow::Result;
use clap::Parser;
use std::process::Command;

#[derive(Parser)]
pub struct CloneArgs {
    /// Repository URL or path to clone
    repository: String,

    /// Directory to clone into (defaults to repository name)
    directory: Option<String>,

    /// Clone specific branch
    #[arg(short, long)]
    branch: Option<String>,
}

pub fn run(args: CloneArgs) -> Result<()> {
    let repository = &args.repository;
    let directory = &args.directory;
    let branch = &args.branch;

    println!();
    println!("  ◇ cloning repository...");

    let mut cmd = Command::new("git");
    cmd.arg("clone");

    if let Some(b) = branch {
        cmd.arg("--branch").arg(b);
    }

    cmd.arg(repository);
    if let Some(d) = directory {
        cmd.arg(d);
    }

    let status = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run 'git clone': {}", e))?
        .status;

    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    println!("  ◇ clone completed  ✓");

    let target_dir = if let Some(d) = directory {
        d.to_string()
    } else {
        repository
            .split('/')
            .last()
            .unwrap_or(repository)
            .trim_end_matches(".git")
            .to_string()
    };

    std::env::set_current_dir(&target_dir)
        .map_err(|e| anyhow::anyhow!("Failed to enter directory '{}': {}", target_dir, e))?;

    println!();
    println!("  ◇ running gitkit init...");
    println!();

    crate::init::run()?;

    Ok(())
}
