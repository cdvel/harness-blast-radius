use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use harness_blast_radius::baseline::{self, Baseline};
use harness_blast_radius::claude;
use harness_blast_radius::finding::Persona;
use harness_blast_radius::report::{self, Counts, Format};
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
        /// Leave out findings accepted in this file; it is never read unless passed
        #[arg(long, value_name = "FILE", conflicts_with = "write_baseline")]
        baseline: Option<PathBuf>,
        /// Write the current findings to FILE as accepted, then exit 0
        #[arg(long, value_name = "FILE")]
        write_baseline: Option<PathBuf>,
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
            baseline,
            write_baseline,
            no_exit_codes,
        } => {
            let paths = claude::Paths::detect(&path, managed_dir)?;
            let resolved = claude::resolve(paths);
            let findings = claude::rules::check(&resolved);
            let shown: Vec<_> = findings.iter().filter(|f| f.visible(persona)).collect();
            let display = |p: &std::path::Path| resolved.paths.display(p);
            if let Some(file) = write_baseline {
                std::fs::write(&file, baseline::write(&shown, &display))
                    .with_context(|| format!("writing {}", file.display()))?;
                eprintln!(
                    "wrote {} finding{} to {}; add a reason to each",
                    shown.len(),
                    if shown.len() == 1 { "" } else { "s" },
                    file.display()
                );
                return Ok(());
            }
            let mut counts = Counts {
                hidden: findings.len() - shown.len(),
                ..Counts::default()
            };
            let shown = match baseline {
                Some(file) => {
                    let applied = Baseline::load(&file)?.apply(shown, &display);
                    counts.accepted = applied.accepted;
                    counts.stale = applied.stale;
                    applied.kept
                }
                None => shown,
            };
            let project = &resolved.paths.project;
            match format {
                Format::Text => print!(
                    "{}",
                    claude::audit::render(&resolved.paths, &shown, counts, style)
                ),
                Format::Json => print!("{}", report::json(project, &shown, counts)),
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
