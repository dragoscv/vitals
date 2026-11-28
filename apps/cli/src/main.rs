//! The Vitals command line companion.
//!
//! Exists so the data behind the GUI is scriptable. Every subcommand supports
//! `--json`, because the primary consumer of a CLI in this space is a script,
//! not a human reading a table.
//!
//! When the desktop app is running the CLI **attaches** to it — over its
//! per-user named pipe, or failing that its loopback API — and shows the same
//! numbers the window does; otherwise it samples the machine itself. Which
//! one happened is printed on stderr, so stdout stays clean for
//! `--json | ConvertFrom-Json`.

// A CLI's job is to print. The lint exists for library crates.
#![allow(clippy::print_stdout, clippy::print_stderr)]

mod client;
mod commands;
mod discovery;
mod exit;
mod fold;
mod render;
mod source;
mod sse;

use std::time::Duration;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use crate::source::{Preference, Source};

/// Where `--source` says the numbers should come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum SourceArg {
    /// Attach to the running app if there is one, else sample directly.
    #[default]
    Auto,
    /// Attach to the running app over its pipe, or fail.
    App,
    /// Always sample directly, even when the app is running.
    Local,
}

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

    /// Where to read from: the running app (`app`), this process's own
    /// sampler (`local`), or whichever is available (`auto`).
    #[arg(long, global = true, value_enum, default_value_t = SourceArg::Auto, conflicts_with_all = ["attach", "no_attach"])]
    source: SourceArg,

    /// Attach to a Vitals local API at this address (e.g. `http://127.0.0.1:7330`)
    /// instead of discovering the running app.
    #[arg(long, global = true, value_name = "URL", conflicts_with = "no_attach")]
    attach: Option<String>,

    /// Never attach to a running app; always sample directly. Same as
    /// `--source local`.
    #[arg(long, global = true)]
    no_attach: bool,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn preference(&self) -> Preference {
        if self.no_attach || self.source == SourceArg::Local {
            Preference::Direct
        } else if self.source == SourceArg::App {
            Preference::App
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
    /// Turn Windows 11 efficiency mode on (default) or off for one process.
    ///
    /// The PID is matched against a fresh sample and refused if absent, so
    /// a recycled PID is never acted on. Prints the state read back after.
    Eco {
        /// The process ID, as `vitals ps` shows it.
        pid: u32,
        /// Restore normal scheduling instead of enabling efficiency mode.
        #[arg(long)]
        off: bool,
    },
    /// End a process.
    ///
    /// The PID is matched against a fresh sample, so a recycled PID is never
    /// hit. Critical and protected processes are refused: a script cannot see
    /// the confirmation the desktop shows for them.
    Kill {
        /// The process ID, as `vitals ps` shows it.
        pid: u32,
    },
    /// Freeze a process until `vitals resume`. Critical processes are refused.
    Suspend {
        /// The process ID, as `vitals ps` shows it.
        pid: u32,
    },
    /// Let a suspended process run again.
    Resume {
        /// The process ID, as `vitals ps` shows it.
        pid: u32,
    },
    /// Change a process's scheduling priority.
    Priority {
        /// The process ID, as `vitals ps` shows it.
        pid: u32,
        /// The new priority class.
        #[arg(value_enum)]
        level: commands::act::Level,
    },
    /// Capture a diagnostic report for sharing.
    Report {
        /// Seconds to record before writing the report.
        #[arg(short, long, default_value_t = 60)]
        duration: u64,
        /// Output path.
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
    /// Serve this machine to an AI agent over the Model Context Protocol.
    ///
    /// Reads one JSON message per line on stdin and answers on stdout, with
    /// the tools `summary`, `processes`, `sensors`, `alerts`, `history` and
    /// `diagnosis`. Read-only: nothing here can end, freeze or re-prioritise
    /// a process, and there is no tool that tries to.
    ///
    /// Point Claude, Copilot or Cursor at `vitals mcp`; see
    /// docs/integrations/mcp.md for the configuration.
    Mcp,
    /// Serve metrics to the LAN without the desktop app (headless).
    ///
    /// The same routes the desktop serves, including `POST /mcp` — the
    /// read-only Model Context Protocol server, for an AI agent running on
    /// another machine. Like every other read route it needs a token, and
    /// like them it confers no ability to change anything.
    Serve {
        /// TCP port to bind on every interface.
        #[arg(short, long, default_value_t = 7331)]
        port: u16,
        /// Bearer token clients must present. Generated and printed once if
        /// omitted.
        #[arg(long)]
        token: Option<String>,
        /// Refused. A headless server cannot act on processes — there is no
        /// one to confirm ending a critical one — so this exits 4 with an
        /// explanation instead of minting a token it cannot honour. Pair the
        /// phone with the desktop app for that.
        #[arg(long)]
        control: bool,
        /// Publish to an MQTT broker with Home Assistant discovery, e.g.
        /// `mqtt://user:pass@broker.local:1883` or `mqtts://broker.local`.
        /// Off when omitted, and nothing is published until it is given.
        #[arg(long)]
        mqtt: Option<String>,
        /// Seconds between MQTT publishes. Clamped to 1..=3600.
        #[arg(long)]
        mqtt_interval: Option<u64>,
    },
    /// Print a shell completion script for this CLI.
    ///
    /// Redirect it to the file your shell reads, for example
    /// `vitals completions bash > /etc/bash_completion.d/vitals`.
    Completions {
        /// The shell to generate for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Every sensor reading this machine can make.
    ///
    /// A sensor that could not be measured is absent from the list, never a
    /// zero reading. `--json` prints the same array `/api/v1/sensors` serves.
    Sensors,
    /// Machine-wide samples from the desktop app's history store.
    ///
    /// History belongs to the desktop, so a direct sampler has none to give
    /// and says so rather than inventing a flat line (exit code 5).
    History {
        /// How many seconds back to include.
        #[arg(short, long, default_value_t = 300)]
        seconds: u32,
    },
    /// Alerts currently raised, from the same engine the desktop evaluates.
    Alerts,
    /// List, open or close widgets on the running app.
    ///
    /// Widgets are windows, so only the app that owns them can do this: the
    /// command always asks the running Vitals over its attach pipe, and there
    /// is no direct-sampler equivalent. An open names the reading, its size
    /// (`small` by default) and its layout (`stack` by default); a position
    /// is optional and the app cascades to a free spot without one.
    ///
    /// Exactly one of `--kind`, `--close` and `--list` is required. An
    /// unknown name for any of them is a usage error (exit 2). When no app
    /// answers the exit code is 3; when an app answers and will not do it —
    /// a build older than this message, or one with no widget handler — it is
    /// 6, so a script can tell "start the app" from "upgrade it".
    Widget(commands::widget::Widget),
}

fn main() {
    let code = match run() {
        Ok(()) => exit::OK,
        Err(error) => {
            // `{:?}` keeps anyhow's whole chain, the same shape the standard
            // `Termination` impl printed before this function existed, so a
            // reader still sees the cause behind the context.
            eprintln!("Error: {error:?}");
            exit::code_for(&error)
        }
    };
    std::process::exit(code);
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Serve {
            port,
            token,
            control,
            mqtt,
            mqtt_interval,
        } => {
            // `serve` never attaches: an instance that proxied the desktop
            // would go dark when the window closed, silently.
            return commands::serve::run(&commands::serve::Options {
                port: *port,
                token: token.clone(),
                control: *control,
                mqtt: mqtt.clone(),
                mqtt_interval: *mqtt_interval,
                json: cli.json,
            });
        }
        // Printed from the definition alone, so it must not wait on a source
        // that this machine may not have.
        Commands::Completions { shell } => {
            print_completions(*shell);
            return Ok(());
        }
        _ => {}
    }

    let mut source = Source::discover(&cli.preference()).map_err(exit::link_failure)?;
    eprintln!("{}", source.describe());

    match cli.command {
        Commands::Ps { filter, top } => {
            commands::ps::run(&mut source, filter.as_deref(), top, cli.json)
        }
        Commands::Top { interval } => {
            commands::top::run(&mut source, Duration::from_millis(interval), cli.json)
        }
        Commands::Info => commands::info::run(&mut source, cli.json),
        Commands::Sensors => commands::sensors::run(&source, cli.json),
        Commands::History { seconds } => commands::history::run(&source, seconds, cli.json),
        Commands::Alerts => commands::alerts::run(&mut source, cli.json),
        Commands::Widget(args) => commands::widget::run(&mut source, &args, cli.json),
        Commands::Eco { pid, off } => commands::eco::run(&mut source, pid, !off, cli.json),
        Commands::Kill { pid } => {
            commands::act::run(&mut source, pid, commands::act::Action::Kill, cli.json)
        }
        Commands::Suspend { pid } => {
            commands::act::run(&mut source, pid, commands::act::Action::Suspend, cli.json)
        }
        Commands::Resume { pid } => {
            commands::act::run(&mut source, pid, commands::act::Action::Resume, cli.json)
        }
        Commands::Priority { pid, level } => commands::act::run(
            &mut source,
            pid,
            commands::act::Action::Priority(level),
            cli.json,
        ),
        Commands::Report { duration, output } => commands::report::run(
            &mut source,
            Duration::from_secs(duration),
            output.as_deref(),
            cli.json,
        ),
        // Takes the source rather than borrowing it: the MCP server owns it
        // for the life of the session.
        Commands::Mcp => commands::mcp::run(source),
        // Both return before a source is discovered, above.
        Commands::Serve { .. } | Commands::Completions { .. } => {
            unreachable!("handled before the source was discovered")
        }
    }
}

/// Writes a completion script for `shell` to stdout.
fn print_completions(shell: clap_complete::Shell) {
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "vitals", &mut std::io::stdout());
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
    fn eco_takes_a_pid_and_defaults_to_on() {
        let cli = Cli::try_parse_from(["vitals", "eco", "4242"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Eco {
                pid: 4242,
                off: false
            }
        ));
        let cli = Cli::try_parse_from(["vitals", "eco", "4242", "--off", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Commands::Eco {
                pid: 4242,
                off: true
            }
        ));
    }

    #[test]
    fn eco_refuses_a_missing_or_non_numeric_pid() {
        assert!(Cli::try_parse_from(["vitals", "eco"]).is_err());
        assert!(Cli::try_parse_from(["vitals", "eco", "notepad"]).is_err());
    }

    #[test]
    fn kill_suspend_resume_take_a_numeric_pid_and_nothing_else() {
        for verb in ["kill", "suspend", "resume"] {
            assert!(
                Cli::try_parse_from(["vitals", verb, "4242"]).is_ok(),
                "{verb}"
            );
            assert!(Cli::try_parse_from(["vitals", verb]).is_err(), "{verb}");
            assert!(
                Cli::try_parse_from(["vitals", verb, "notepad"]).is_err(),
                "{verb}"
            );
        }
    }

    #[test]
    fn priority_takes_a_pid_and_a_kebab_case_level() {
        let cli = Cli::try_parse_from(["vitals", "priority", "4242", "below-normal"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Priority {
                pid: 4242,
                level: commands::act::Level::BelowNormal
            }
        ));
        assert!(Cli::try_parse_from(["vitals", "priority", "4242", "turbo"]).is_err());
        assert!(Cli::try_parse_from(["vitals", "priority", "4242"]).is_err());
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
    fn source_flag_selects_the_preference_and_excludes_the_older_flags() {
        let cli = Cli::try_parse_from(["vitals", "ps", "--source", "local"]).unwrap();
        assert!(matches!(cli.preference(), Preference::Direct));
        let cli = Cli::try_parse_from(["vitals", "--source", "app", "top"]).unwrap();
        assert!(matches!(cli.preference(), Preference::App));
        let cli = Cli::try_parse_from(["vitals", "ps"]).unwrap();
        assert!(matches!(cli.preference(), Preference::Auto));
        assert!(Cli::try_parse_from(["vitals", "ps", "--source", "app", "--no-attach"]).is_err());
        assert!(Cli::try_parse_from(["vitals", "ps", "--source", "pipe"]).is_err());
    }

    #[test]
    fn serve_defaults_to_the_lan_port_and_read_only() {
        let cli = Cli::try_parse_from(["vitals", "serve"]).unwrap();
        match cli.command {
            Commands::Serve {
                port,
                token,
                control,
                mqtt,
                mqtt_interval,
            } => {
                assert_eq!(port, 7331);
                assert_eq!(token, None);
                assert!(!control);
                assert_eq!(mqtt, None, "MQTT is off unless a broker is given");
                assert_eq!(mqtt_interval, None);
            }
            other => panic!("expected Serve, got {other:?}"),
        }
    }

    #[test]
    fn widget_parses_the_three_actions_and_the_open_options() {
        let cli = Cli::try_parse_from([
            "vitals", "widget", "--kind", "cpu", "--size", "large", "--layout", "ring", "--x",
            "10", "--y", "20",
        ])
        .unwrap();
        match cli.command {
            Commands::Widget(args) => {
                assert_eq!(args.kind, Some(commands::widget::Kind::Cpu));
                assert_eq!(args.size, Some(commands::widget::Size::Large));
                assert_eq!(args.layout, Some(commands::widget::Layout::Ring));
                assert_eq!(args.x, Some(10.0));
                assert_eq!(args.y, Some(20.0));
                assert!(!args.list);
            }
            other => panic!("expected Widget, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["vitals", "widget", "--close", "w3"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Widget(commands::widget::Widget {
                close: Some(ref id),
                ..
            }) if id == "w3"
        ));

        let cli = Cli::try_parse_from(["vitals", "widget", "--list", "--json"]).unwrap();
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Commands::Widget(commands::widget::Widget { list: true, .. })
        ));
    }

    #[test]
    fn widget_refuses_a_missing_action_a_second_action_and_an_unknown_name() {
        for args in [
            vec!["vitals", "widget"],
            vec!["vitals", "widget", "--kind", "cpu", "--close", "w1"],
            vec!["vitals", "widget", "--list", "--kind", "cpu"],
            vec!["vitals", "widget", "--kind", "gpu"],
            vec!["vitals", "widget", "--kind", "cpu", "--size", "huge"],
            vec!["vitals", "widget", "--kind", "cpu", "--layout", "grid"],
            vec!["vitals", "widget", "--list", "--size", "large"],
        ] {
            assert!(
                Cli::try_parse_from(&args).is_err(),
                "{args:?} should be refused"
            );
        }
    }
}
