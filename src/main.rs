use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use harness_blast_radius::claude;

#[derive(Parser)]
#[command(
    version,
    about = "Audit what your AI coding harness configuration allows"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the effective Claude Code configuration and where each value comes from
    Show {
        /// Project directory (defaults to the current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Directory with managed-settings.json and managed-mcp.json
        #[arg(long)]
        managed_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Show { path, managed_dir } => {
            let paths = claude::Paths::detect(&path, managed_dir)?;
            print!("{}", claude::show::render(&claude::resolve(paths)));
        }
    }
    Ok(())
}
