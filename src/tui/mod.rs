//! Interactive terminal UI (design §4 "TUI requirements", §7 Slice 7).
//!
//! This is the welcome-screen scaffold: alternate screen + raw mode, rendered
//! with ratatui, and nothing else — no API calls, no auth, no profile reads.
//! The full views (overview, clusters, projects, deploys, alerts, wallet) land
//! in Slice 7 over the same `ApiClient`/auth/profiles the CLI uses.

use std::io::{self, Stdout};
use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Alignment;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::error::{Error, Result};

/// Terminal to drive the TUI on.
type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Guards [`restore_terminal`]: the panic hook, the process exit and a failed
/// setup could otherwise emit the restore sequences more than once.
static TERMINAL_RESTORED: AtomicBool = AtomicBool::new(false);

/// Run the TUI until the user quits (exit 0).
///
/// The terminal is restored on every path: normal quit, an error return, and
/// panic (through the installed hook).
pub fn run() -> Result<()> {
    install_panic_hook();
    let mut terminal = enter_terminal()?;
    let result = event_loop(&mut terminal);
    restore_terminal();
    result
}

/// Enter the alternate screen and raw mode. Restores on a partial failure.
fn enter_terminal() -> Result<TuiTerminal> {
    enable_raw_mode().map_err(io_error)?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen, Hide) {
        restore_terminal();
        return Err(io_error(err));
    }
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(err) => {
            restore_terminal();
            Err(io_error(err))
        }
    }
}

/// Draw until a quit key arrives. Idle here is fine: the welcome screen has no
/// data to refresh, and Slice 7 replaces the blocking read with
/// `tokio::select!` over `EventStream` + API channels.
fn event_loop(terminal: &mut TuiTerminal) -> Result<()> {
    loop {
        terminal.draw(render).map_err(io_error)?;
        match event::read().map_err(io_error)? {
            Event::Key(key) if key.kind == KeyEventKind::Press && is_quit(key) => return Ok(()),
            _ => {}
        }
    }
}

/// `q`, `Esc` and `Ctrl-C` all quit with exit 0.
fn is_quit(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
        _ => false,
    }
}

/// Leave the alternate screen, disable raw mode and show the cursor again.
/// Idempotent, and best-effort on failure (a broken terminal must still exit).
fn restore_terminal() {
    if TERMINAL_RESTORED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

/// Restore the terminal before the default hook prints the panic message.
fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

/// The welcome screen: product name and version, what is still to come, and how
/// to leave.
fn render(frame: &mut Frame) {
    let lines = vec![
        Line::from("SelfHost"),
        Line::from(format!("selfhost {}", env!("CARGO_PKG_VERSION"))),
        Line::from(""),
        Line::from("The interactive UI is under construction; this is the welcome screen."),
        Line::from(""),
        Line::from("Press q to quit"),
    ];
    let panel = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title("selfhost"));
    frame.render_widget(panel, frame.area());
}

/// Map an I/O failure into the CLI error type (exit code 1).
fn io_error(err: io::Error) -> Error {
    Error::Other(err.into())
}

#[cfg(test)]
mod tests {
    use super::{is_quit, render};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Render the welcome screen into an in-memory buffer and flatten it to text.
    fn rendered() -> String {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("TestBackend::new cannot fail");
        terminal
            .draw(render)
            .expect("drawing into a buffer cannot fail");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn welcome_screen_shows_the_title_version_and_quit_hint() {
        let text = rendered();
        assert!(text.contains("SelfHost"), "missing product name:\n{text}");
        assert!(
            text.contains(env!("CARGO_PKG_VERSION")),
            "missing version:\n{text}"
        );
        assert!(
            text.contains("Press q to quit"),
            "missing quit hint:\n{text}"
        );
    }

    #[test]
    fn quit_keys() {
        let press = |code, modifiers| KeyEvent::new(code, modifiers);
        assert!(is_quit(press(KeyCode::Char('q'), KeyModifiers::NONE)));
        assert!(is_quit(press(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(is_quit(press(KeyCode::Char('c'), KeyModifiers::CONTROL)));
        assert!(!is_quit(press(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!is_quit(press(KeyCode::Char('j'), KeyModifiers::NONE)));
    }
}
