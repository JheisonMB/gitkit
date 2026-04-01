use anyhow::Result;
use clap::{Parser, Subcommand};

mod attributes;
mod config;
mod hooks;
mod ignore;
mod utils;

#[derive(Parser)]
#[command(
    name = "gitkit",
    version,
    about = "Standalone CLI for configuring git repos"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage git hooks
    Hooks {
        #[command(subcommand)]
        action: hooks::HooksCommand,
    },
    /// Generate .gitignore via gitignore.io
    Ignore {
        #[command(subcommand)]
        action: ignore::IgnoreCommand,
    },
    /// Configure .gitattributes
    Attributes {
        #[command(subcommand)]
        action: attributes::AttributesCommand,
    },
    /// Apply curated git config presets
    Config {
        #[command(subcommand)]
        action: config::ConfigCommand,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Hooks { action } => hooks::run(action),
        Command::Ignore { action } => ignore::run(action),
        Command::Attributes { action } => attributes::run(action),
        Command::Config { action } => config::run(action),
    }
}
