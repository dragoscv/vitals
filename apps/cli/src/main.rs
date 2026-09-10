//! The Vitals command line companion.
//!
//! Exists so the data behind the GUI is scriptable. Every subcommand supports
//! `--json`, because the primary consumer of a CLI in this space is a script,
//! not a human reading a table.
//!
//! When the desktop app is running the CLI **attaches** to its loopback API
//! and shows the same numbers the window does; otherwise it samples the
//! machine itself. Which one happened is printed on stderr, so stdout stays
//! clean for `--json | ConvertFrom-Json`.

// A CLI's job is to print. The lint exists for library crates.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod client;
mod commands;
mod discovery;
mod fold;
mod render;
mod source;
mod sse;

use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::source::{Preference, Source};

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

    /// Attach to a Vitals local API at this address (e.g. `http://127.0.0.1:7330`)
    /// instead of discovering the running app.
    #[arg(long, global = true, value_name = "URL", conflicts_with = "no_attach")]
    attach: Option<String>,

    /// Never attach to a running app; always sample directly.
    #[arg(long, global = true)]
    no_attach: bool,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn preference(&self) -> Preference {
        if self.no_attach {
            Preference::Direct
        } else if let Some(url) = &self.attach {
            Preference::Attach(url.clone())
        } else {
            Preference::Auto
        }
    }
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
    /// Serve metrics to the LAN without the desktop app (headless).
    Serve {
        /// TCP port to bind on every interface.
        #[arg(short, long, default_value_t = 7331)]
        port: u16,
        /// Bearer token clients must present. Generated and printed once if
        /// omitted.
        #[arg(long)]
        token: Option<String>,
        /// Give the token control scope (end/suspend/resume/priority) rather
        /// than read-only.
        #[arg(long)]
        control: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Commands::Serve {
        port,
        token,
        control,
    } = &cli.command
    {
        // `serve` never attaches: an instance that proxied the desktop
        // would go dark when the window closed, silently.
        return commands::serve::run(&commands::serve::Options {
            port: *port,
            token: token.clone(),
            control: *control,
        });
    }

    let mut source = Source::discover(&cli.preference())?;
    eprintln!("{}", source.describe());

    match cli.command {
        Commands::Ps { filter, top } => {
            commands::ps::run(&mut source, filter.as_deref(), top, cli.json)
        }
        Commands::Top { interval } => {
            commands::top::run(&mut source, Duration::from_millis(interval), cli.json)
        }
        Commands::Info => commands::info::run(&mut source, cli.json),
        Commands::Report { duration, output } => commands::report::run(
            &mut source,
            Duration::from_secs(duration),
            output.as_deref(),
            cli.json,
        ),
        Commands::Serve { .. } => unreachable!("handled above"),
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

    #[test]
    fn attach_and_no_attach_are_mutually_exclusive() {
        assert!(
            Cli::try_parse_from(["vitals", "--attach", "http://x:1", "--no-attach", "ps"]).is_err()
        );
        let cli = Cli::try_parse_from(["vitals", "ps", "--no-attach"]).unwrap();
        assert!(matches!(cli.preference(), Preference::Direct));
        let cli = Cli::try_parse_from(["vitals", "--attach", "http://x:1", "ps"]).unwrap();
        assert!(matches!(cli.preference(), Preference::Attach(u) if u == "http://x:1"));
    }

    #[test]
    fn serve_defaults_to_the_lan_port_and_read_only() {
        let cli = Cli::try_parse_from(["vitals", "serve"]).unwrap();
        match cli.command {
            Commands::Serve {
                port,
                token,
                control,
            } => {
                assert_eq!(port, 7331);
                assert_eq!(token, None);
                assert!(!control);
            }
            other => panic!("expected Serve, got {other:?}"),
        }
    }
}
