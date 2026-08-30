//! The Vitals command line companion.
//!
//! Exists so the data behind the GUI is scriptable. Every subcommand supports
//! `--json`, because the primary consumer of a CLI in this space is a script,
//! not a human reading a table.

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "vitals",
    version,
    about = "Vitals — system monitor and task manager",
    long_about = None
)]
struct Cli {
    /// Emit machine-readable JSON instead of a human table.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List processes.
    Ps {
        /// Show only processes whose name contains this string.
        #[arg(short, long)]
        filter: Option<String>,
        /// Limit output to the N highest consumers.
        #[arg(short = 'n', long, default_value_t = 0)]
        top: usize,
    },
    /// Live-updating summary, like `top`.
    Top {
        /// Refresh interval in milliseconds.
        #[arg(short, long, default_value_t = 1000)]
        interval: u64,
    },
    /// One-shot machine summary.
    Info,
    /// Capture a diagnostic report for sharing.
    Report {
        /// Seconds to record before writing the report.
        #[arg(short, long, default_value_t = 60)]
        duration: u64,
        /// Output path.
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ps { .. } | Commands::Top { .. } | Commands::Info | Commands::Report { .. } => {
            // Wired up once the platform sampler lands; parsing is validated
            // by the tests below in the meantime.
            anyhow::bail!("not yet implemented — the sampler backend is still in progress")
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        // Catches duplicate flags and bad arg definitions at test time rather
        // than on the user's first run.
        Cli::command().debug_assert();
    }

    #[test]
    fn json_flag_is_accepted_on_subcommands() {
        let cli = Cli::try_parse_from(["vitals", "ps", "--json"]).expect("should parse");
        assert!(cli.json);
        assert!(matches!(cli.command, Commands::Ps { .. }));
    }

    #[test]
    fn ps_accepts_filter_and_top() {
        let cli =
            Cli::try_parse_from(["vitals", "ps", "--filter", "chrome", "--top", "5"]).unwrap();
        match cli.command {
            Commands::Ps { filter, top } => {
                assert_eq!(filter.as_deref(), Some("chrome"));
                assert_eq!(top, 5);
            }
            other => panic!("expected Ps, got {other:?}"),
        }
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        assert!(Cli::try_parse_from(["vitals", "nonsense"]).is_err());
    }
}
