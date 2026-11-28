//! `vitals widget` — list, open and close widgets on the running app.
//!
//! A widget is a window, and only the desktop app owns windows. There is no
//! honest way to "open a widget" against a direct sampler — there would be
//! nothing drawing it — so every action here is a request to the running app
//! over the attach pipe, and the app's answer is what the user is shown.
//!
//! Two failures are deliberately different exit codes:
//!
//! * **3** — no app answered (nothing to ask);
//! * **6** — an app answered and would not do it: a build older than this
//!   message, a host that never registered a widget handler, or a source that
//!   is not the app's own pipe.
//!
//! The app's own refusal text is printed verbatim rather than reworded, so a
//! user sees the reason the app gave.

use anyhow::Result;
use clap::{ArgGroup, Args, ValueEnum};
use serde::Serialize;
use vitals_ipc::attach::{WidgetAction, WidgetInfo, WidgetKind, WidgetLayout, WidgetSize};

use crate::exit;
use crate::render;
use crate::source::Source;

/// What a widget shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Kind {
    Cpu,
    Memory,
    Disk,
    Network,
    Thermal,
    Overview,
}

impl From<Kind> for WidgetKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Cpu => Self::Cpu,
            Kind::Memory => Self::Memory,
            Kind::Disk => Self::Disk,
            Kind::Network => Self::Network,
            Kind::Thermal => Self::Thermal,
            Kind::Overview => Self::Overview,
        }
    }
}

/// How large the widget window is. Defaults to `small`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Size {
    Small,
    Medium,
    Large,
}

impl From<Size> for WidgetSize {
    fn from(size: Size) -> Self {
        match size {
            Size::Small => Self::Small,
            Size::Medium => Self::Medium,
            Size::Large => Self::Large,
        }
    }
}

/// How the widget lays its reading out. Defaults to `stack`; `ring` is only
/// drawn for `cpu` and `memory`, and the app downgrades it for any other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Layout {
    Stack,
    Split,
    Ring,
}

impl From<Layout> for WidgetLayout {
    fn from(layout: Layout) -> Self {
        match layout {
            Layout::Stack => Self::Stack,
            Layout::Split => Self::Split,
            Layout::Ring => Self::Ring,
        }
    }
}

/// Lists, opens or closes a widget on the running app.
#[derive(Args, Debug)]
#[command(group(
    ArgGroup::new("action")
        .required(true)
        .args(["kind", "close", "list"])
))]
pub struct Widget {
    /// Which reading the new widget shows.
    #[arg(long, value_name = "KIND", value_enum, conflicts_with_all = ["close", "list"])]
    pub kind: Option<Kind>,
    /// How large the new widget is: small (default), medium or large.
    #[arg(long, value_enum, requires = "kind", conflicts_with_all = ["close", "list"])]
    pub size: Option<Size>,
    /// How the new widget lays out: stack (default), split or ring.
    #[arg(long, value_enum, requires = "kind", conflicts_with_all = ["close", "list"])]
    pub layout: Option<Layout>,
    /// Logical left edge of the new widget, in pixels from the screen's left.
    ///
    /// Omitted, the app cascades the widget to a free spot.
    #[arg(
        long,
        value_name = "N",
        allow_negative_numbers = true,
        requires = "kind",
        conflicts_with_all = ["close", "list"]
    )]
    pub x: Option<f64>,
    /// Logical top edge of the new widget, in pixels from the screen's top.
    ///
    /// Omitted, the app cascades the widget to a free spot.
    #[arg(
        long,
        value_name = "N",
        allow_negative_numbers = true,
        requires = "kind",
        conflicts_with_all = ["close", "list"]
    )]
    pub y: Option<f64>,
    /// Close the widget with this id, as `--list` reports it.
    #[arg(long, value_name = "ID", conflicts_with_all = ["kind", "list"])]
    pub close: Option<String>,
    /// List the widgets that are open.
    #[arg(long, conflicts_with_all = ["kind", "close"])]
    pub list: bool,
}

impl Widget {
    /// The action these arguments describe.
    ///
    /// # Errors
    /// No action was named. clap's arg group makes that a usage error before
    /// this runs, but the group is not visible to the type system and a
    /// library path may not assume it did its job.
    pub fn action(&self) -> Result<WidgetAction> {
        if self.list {
            return Ok(WidgetAction::List);
        }
        if let Some(id) = &self.close {
            return Ok(WidgetAction::Close { id: id.clone() });
        }
        let Some(kind) = self.kind else {
            anyhow::bail!("name an action: --list, --close <id>, or --kind <KIND>");
        };
        Ok(WidgetAction::Open {
            kind: kind.into(),
            size: self.size.unwrap_or(Size::Small).into(),
            layout: self.layout.unwrap_or(Layout::Stack).into(),
            x: self.x,
            y: self.y,
        })
    }
}

/// What `--json` prints on success.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// The verb that was sent, as the wire spells it.
    pub action: &'static str,
    /// Every widget the app has open afterwards — the same list whatever the
    /// action was, so a caller can replace its state rather than patch it.
    pub widgets: Vec<WidgetInfo>,
    /// The widget an `open` created or a `close` removed; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// # Errors
/// No app answered (exit 3); an app answered and would not do it (exit 6);
/// the reply could not be parsed.
pub fn run(source: &mut Source, args: &Widget, json: bool) -> Result<()> {
    let action = args.action()?;
    let reply = source.widget(action.clone())?;
    if reply.status != "ok" {
        return Err(exit::unsupported(anyhow::anyhow!(
            "the app answered {} for a widget action it understood",
            reply.status
        )));
    }

    let outcome = Outcome {
        action: action.verb(),
        widgets: reply.widgets,
        id: reply.id,
    };
    if json {
        println!("{}", serde_json::to_string(&outcome)?);
        return Ok(());
    }

    match &action {
        WidgetAction::List => {
            println!("{}", render::widgets(&outcome.widgets));
            if outcome.widgets.is_empty() {
                eprintln!("no widgets are open");
            }
        }
        WidgetAction::Open {
            kind, size, layout, ..
        } => match &outcome.id {
            Some(id) => println!(
                "opened widget {id} ({}, {}, {})",
                kind.as_str(),
                size.as_str(),
                layout.as_str()
            ),
            // A host that honours the action without naming the widget is
            // not an error — the list is still in `--json` — but saying so
            // beats inventing an id to close later.
            None => println!(
                "opened a widget ({}, {}, {}); the app did not report its id",
                kind.as_str(),
                size.as_str(),
                layout.as_str()
            ),
        },
        WidgetAction::Close { id } => {
            if outcome.id.is_some() {
                println!("closed widget {id}");
            } else {
                // Not a failure: closing an id that is already gone is the
                // state the caller asked for.
                eprintln!("no widget is open with id {id}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::FromArgMatches as _;

    /// Parses the way clap's own path does for an `Args` struct: the group
    /// and the `requires` edges only exist once they are augmented in.
    fn parsed(args: &[&str]) -> Widget {
        match try_parse(args) {
            Ok(widget) => widget,
            Err(error) => panic!("{args:?} should parse: {error}"),
        }
    }

    fn try_parse(args: &[&str]) -> Result<Widget, clap::Error> {
        // clap reads the first element as the program name. Cases below spell
        // it or leave it out, so strip one here and prepend exactly one: a
        // bare `["--list", "--close", "w1"]` would otherwise be parsed as
        // `--close w1` alone and the first action would be dropped silently,
        // which is the very mistake these tests exist to catch.
        let args = args.strip_prefix(&["widget"][..]).unwrap_or(args);
        let command = Widget::augment_args(clap::Command::new("widget"));
        let matches =
            command.try_get_matches_from(std::iter::once("widget").chain(args.iter().copied()))?;
        Widget::from_arg_matches(&matches)
    }

    #[test]
    fn cli_definition_is_valid() {
        // The arg group and the `requires = "kind"` edges are only checked by
        // clap's own debug assertions.
        Widget::augment_args(clap::Command::new("widget")).debug_assert();
    }

    #[test]
    fn the_spelling_clap_accepts_is_the_spelling_the_app_parses() {
        // A mismatch is a CLI that accepts a value from `--help` and sends
        // one the app refuses. Pinned against the wire accessors, not against
        // a copy of the literals.
        for kind in Kind::value_variants() {
            let shown = kind.to_possible_value().unwrap();
            assert_eq!(shown.get_name(), WidgetKind::from(*kind).as_str());
        }
        for size in Size::value_variants() {
            let shown = size.to_possible_value().unwrap();
            assert_eq!(shown.get_name(), WidgetSize::from(*size).as_str());
        }
        for layout in Layout::value_variants() {
            let shown = layout.to_possible_value().unwrap();
            assert_eq!(shown.get_name(), WidgetLayout::from(*layout).as_str());
        }
    }

    #[test]
    fn kind_builds_an_open_with_the_documented_defaults() {
        let action = parsed(&["widget", "--kind", "cpu"]).action().unwrap();
        assert_eq!(
            action,
            WidgetAction::Open {
                kind: WidgetKind::Cpu,
                size: WidgetSize::Small,
                layout: WidgetLayout::Stack,
                x: None,
                y: None,
            }
        );
    }

    #[test]
    fn size_layout_and_position_are_carried_through_when_given() {
        let action = parsed(&[
            "widget", "--kind", "memory", "--size", "medium", "--layout", "ring", "--x", "120",
            "--y", "-40",
        ])
        .action()
        .unwrap();
        assert_eq!(
            action,
            WidgetAction::Open {
                kind: WidgetKind::Memory,
                size: WidgetSize::Medium,
                layout: WidgetLayout::Ring,
                x: Some(120.0),
                y: Some(-40.0),
            }
        );
    }

    #[test]
    fn close_and_list_are_actions_of_their_own() {
        assert_eq!(
            parsed(&["widget", "--close", "w3"]).action().unwrap(),
            WidgetAction::Close {
                id: "w3".to_owned()
            }
        );
        assert_eq!(
            parsed(&["widget", "--list"]).action().unwrap(),
            WidgetAction::List
        );
    }

    #[test]
    fn naming_two_actions_at_once_is_a_usage_error() {
        // `--kind` with `--close` is the mistake a script makes when it edits
        // one flag and forgets the other; it must not silently open.
        let cases: [&[&str]; 3] = [
            &["--kind", "cpu", "--close", "w1"],
            &["--list", "--close", "w1"],
            &["--list", "--kind", "cpu"],
        ];
        for args in cases {
            assert!(try_parse(args).is_err(), "{args:?} should be refused");
        }
    }

    #[test]
    fn no_action_at_all_is_a_usage_error() {
        assert!(try_parse(&[]).is_err());
    }

    #[test]
    fn an_unknown_kind_size_or_layout_is_a_usage_error() {
        assert!(try_parse(&["--kind", "gpu"]).is_err());
        assert!(try_parse(&["--kind", "cpu", "--size", "huge"]).is_err());
        assert!(try_parse(&["--kind", "cpu", "--layout", "grid"]).is_err());
    }

    #[test]
    fn position_and_size_without_a_kind_are_refused() {
        // They only mean something for an `open`; accepting them beside
        // `--list` would silently ignore what the user typed.
        let cases: [&[&str]; 4] = [
            &["--list", "--size", "large"],
            &["--list", "--layout", "ring"],
            &["--close", "w1", "--x", "10"],
            &["--close", "w1", "--y", "10"],
        ];
        for args in cases {
            assert!(try_parse(args).is_err(), "{args:?} should be refused");
        }
    }

    #[test]
    fn json_output_names_the_action_the_widgets_and_the_id() {
        let outcome = Outcome {
            action: WidgetAction::Open {
                kind: WidgetKind::Cpu,
                size: WidgetSize::Small,
                layout: WidgetLayout::Stack,
                x: None,
                y: None,
            }
            .verb(),
            widgets: vec![WidgetInfo {
                id: "w3".to_owned(),
                kind: WidgetKind::Cpu,
                size: WidgetSize::Small,
                layout: WidgetLayout::Stack,
            }],
            id: Some("w3".to_owned()),
        };
        assert_eq!(
            serde_json::to_string(&outcome).unwrap(),
            r#"{"action":"open","widgets":[{"id":"w3","kind":"cpu","size":"small","layout":"stack"}],"id":"w3"}"#
        );

        // A list names no widget, so the field is absent rather than null.
        let list = Outcome {
            action: WidgetAction::List.verb(),
            widgets: Vec::new(),
            id: None,
        };
        assert_eq!(
            serde_json::to_string(&list).unwrap(),
            r#"{"action":"list","widgets":[]}"#
        );
    }

    #[test]
    fn the_verb_printed_is_the_verb_sent() {
        for (args, verb) in [
            (vec!["widget", "--list"], "list"),
            (vec!["widget", "--close", "w1"], "close"),
            (vec!["widget", "--kind", "disk"], "open"),
        ] {
            let action = parsed(&args).action().unwrap();
            assert_eq!(action.verb(), verb, "{args:?}");
        }
    }
}
