use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use harness_blast_radius::claude;
use harness_blast_radius::finding::Persona;
use harness_blast_radius::report::{self, Format};
use harness_blast_radius::style::{ColorChoice, Style};

#[derive(Parser)]
#[command(
    name = "harness-blast",
    version,
    about = "Audit what Claude Code's configuration lets it do without asking"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// When to color output
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
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
    /// Explain whether Claude Code would allow, ask or deny a tool call, and which rule decides
    Why {
        /// A shell command, or a tool call such as "Read(~/.ssh/id_rsa)" or "mcp__github__create_issue"
        call: String,
        /// Project directory (defaults to the current directory)
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Directory with managed-settings.json and managed-mcp.json
        #[arg(long)]
        managed_dir: Option<PathBuf>,
    },
    /// Report risky settings in the effective Claude Code configuration
    Audit {
        /// Project directory (defaults to the current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Directory with managed-settings.json and managed-mcp.json
        #[arg(long)]
        managed_dir: Option<PathBuf>,
        /// How much to report
        #[arg(long, value_enum, default_value_t = Persona::Regular)]
        persona: Persona,
        /// Output format
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// Exit 0 even when findings are reported
        #[arg(long)]
        no_exit_codes: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let style = Style::new(cli.color);
    match cli.command {
        Command::Show { path, managed_dir } => {
            let paths = claude::Paths::detect(&path, managed_dir)?;
            print!("{}", claude::show::render(&claude::resolve(paths), style));
        }
        Command::Why {
            call,
            path,
            managed_dir,
        } => {
            let paths = claude::Paths::detect(&path, managed_dir)?;
            print!(
                "{}",
                claude::why::render(&claude::resolve(paths), &call, style)
            );
        }
        Command::Audit {
            path,
            managed_dir,
            persona,
            format,
            no_exit_codes,
        } => {
            let paths = claude::Paths::detect(&path, managed_dir)?;
            let resolved = claude::resolve(paths);
            let findings = claude::rules::check(&resolved);
            let shown: Vec<_> = findings.iter().filter(|f| f.visible(persona)).collect();
            let project = &resolved.paths.project;
            match format {
                Format::Text => print!(
                    "{}",
                    claude::audit::render(&resolved.paths, &findings, persona, style)
                ),
                Format::Json => print!(
                    "{}",
                    report::json(project, &shown, findings.len() - shown.len())
                ),
                Format::Sarif => print!("{}", report::sarif(project, &shown)),
            }
            // Exit codes encode the highest visible severity: 11 low, 12 medium, 13 high.
            let worst = shown.iter().map(|f| f.rule.severity).max();
            if let (Some(sev), false) = (worst, no_exit_codes) {
                std::process::exit(sev.exit_code());
            }
        }
    }
    Ok(())
}
