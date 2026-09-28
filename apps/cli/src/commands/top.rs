//! `vitals top` — a live, redrawing summary.
//!
//! Alternate screen + raw mode, restored on every exit path including a
//! panic in the render, because a terminal left in raw mode after `top` dies
//! is the single most annoying thing a CLI can do to a user.

use std::io::{Write, stdout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::{cursor, execute, terminal};
use serde::Serialize;

use crate::fold::View;
use crate::render;
use crate::source::Source;

/// How many processes fit in a reasonable terminal beneath the header.
const ROWS: usize = 20;

/// Whether a key event means "stop".
#[must_use]
pub fn is_quit(key: &KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Tick<'a> {
    t: u64,
    system: &'a vitals_core::metrics::SystemMetrics,
    /// `null` when the alert list could not be read this tick. A failed read
    /// is not "no alerts"; printing 0 there is the one thing this project
    /// exists not to do.
    alerts: Option<usize>,
    top_processes: Vec<vitals_core::process::Process>,
}

/// # Errors
/// The source failed, or the terminal could not be put into raw mode.
pub fn run(source: &mut Source, interval: Duration, json: bool) -> Result<()> {
    if json {
        return run_json(source, interval);
    }

    let mut guard = TerminalGuard::enter()?;
    let quit = spawn_key_listener();
    let mut out = stdout();

    loop {
        if quit.load(Ordering::Relaxed) {
            break;
        }
        // Alerts come from a separate route in attached mode; one request
        // per tick is cheap on loopback and keeps the count honest.
        let alerts = source.alerts().ok().map(|a| a.len());
        let Some(view) = source.next_frame(interval)? else {
            break;
        };
        draw(&mut out, view, alerts)?;
    }

    guard.leave()?;
    Ok(())
}

fn draw(out: &mut impl Write, view: &View, alerts: Option<usize>) -> Result<()> {
    let list = render::top_by_cpu(view.processes(), ROWS);
    execute!(
        out,
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )?;
    // Raw mode disables the terminal's own `\n` → `\r\n` translation, so
    // every line must carry the carriage return itself.
    write!(out, "{}\r\n", render::summary_line(&view.system, alerts))?;
    write!(out, "press q to quit\r\n\r\n")?;
    for line in render::processes(&list).lines() {
        write!(out, "{line}\r\n")?;
    }
    out.flush()?;
    Ok(())
}

fn run_json(source: &mut Source, interval: Duration) -> Result<()> {
    let quit = spawn_ctrl_c();
    let mut out = stdout().lock();
    while !quit.load(Ordering::Relaxed) {
        let alerts = source.alerts().ok().map(|a| a.len());
        let Some(view) = source.next_frame(interval)? else {
            break;
        };
        let tick = Tick {
            t: view.timestamp_ms,
            system: &view.system,
            alerts,
            top_processes: render::top_by_cpu(view.processes(), ROWS),
        };
        serde_json::to_writer(&mut out, &tick)?;
        writeln!(out)?;
        out.flush()?;
    }
    Ok(())
}

/// Reads keys on its own thread so the sampling loop never blocks on input.
fn spawn_key_listener() -> Arc<AtomicBool> {
    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(Event::Key(key)) if is_quit(&key) => {
                    flag.store(true, Ordering::Relaxed);
                    return;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    });
    quit
}

/// In `--json` mode stdin is not ours to read, so only Ctrl-C ends the loop.
fn spawn_ctrl_c() -> Arc<AtomicBool> {
    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    // tokio is already a dependency for `serve`; its signal handler is the
    // portable way to catch Ctrl-C without a fourth crate.
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async {
            let _ = tokio::signal::ctrl_c().await;
        });
        flag.store(true, Ordering::Relaxed);
    });
    quit
}

/// Raw mode + alternate screen, undone on drop.
struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self { active: true })
    }

    fn leave(&mut self) -> Result<()> {
        if self.active {
            self.active = false;
            execute!(stdout(), cursor::Show, terminal::LeaveAlternateScreen)?;
            terminal::disable_raw_mode()?;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // Errors on the unwind path have nowhere to go; the terminal is
        // restored as far as it can be.
        let _ = self.leave();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_escape_and_ctrl_c_quit_but_a_plain_c_does_not() {
        let plain = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert!(is_quit(&plain(KeyCode::Char('q'))));
        assert!(is_quit(&plain(KeyCode::Esc)));
        assert!(is_quit(&KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!is_quit(&plain(KeyCode::Char('c'))));
    }
}
