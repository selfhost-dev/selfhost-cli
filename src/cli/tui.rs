//! `tui` — the interactive terminal UI (design §3, §4, §7 Slice 7).
//!
//! `selfhost tui` (and bare `selfhost` on an interactive terminal) renders the
//! welcome-screen scaffold in [`crate::tui`], built on `ratatui`/`crossterm` —
//! both already direct dependencies of this crate. The full multi-view UI
//! (overview/clusters/projects/deploys/alerts/wallet) over the same
//! client/auth/profiles as the CLI is Slice 7.
//!
//! Bare `selfhost` on an interactive terminal also lands here (k9s-style): see
//! [`should_launch_tui`] for the guard that keeps scripts and CI out.

use clap::{Args, ValueEnum};

use crate::error::{Error, Result};

// Seconds between automatic refreshes when `--refresh` is not given.
const DEFAULT_REFRESH_SECS: u64 = 5;

// Screen the TUI opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum TuiView {
    /// Health and wallet runway
    #[default]
    Overview,
    /// Database clusters with status, metrics and log tails
    Clusters,
    /// Projects and their databases, services and domains
    Projects,
    /// Deploy runs and live build logs
    Deploys,
    /// Alert rules, fired alerts and notification channels
    Alerts,
    /// Wallet balance, top-ups and auto-recharge
    Wallet,
}

// Arguments for `selfhost tui`.
#[derive(Debug, Clone, Args)]
pub struct TuiArgs {
    /// Screen to open on startup
    #[arg(long, value_enum, default_value = "overview", value_name = "VIEW")]
    pub view: TuiView,

    /// Disable actions that change anything
    #[arg(long)]
    pub read_only: bool,

    /// Seconds between automatic refreshes
    #[arg(long, default_value_t = DEFAULT_REFRESH_SECS, value_name = "SECS")]
    pub refresh: u64,
}

impl Default for TuiArgs {
    fn default() -> Self {
        Self {
            view: TuiView::Overview,
            read_only: false,
            refresh: DEFAULT_REFRESH_SECS,
        }
    }
}

// The explicit `selfhost tui` command: require a real terminal, then render.
//
// The parsed arguments are pinned here (and read) so the surface cannot drift
// while Slice 7 implements the views.
pub fn run(args: TuiArgs) -> Result<()> {
    use std::io::IsTerminal;

    let TuiArgs {
        view,
        read_only,
        refresh,
    } = args;
    let _ = (view, read_only, refresh);

    if !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err(Error::Usage(
            "selfhost tui requires an interactive terminal".into(),
        ));
    }
    crate::tui::run()
}

// Whether a bare `selfhost` (no subcommand) should open the TUI.
//
// All four guards must hold:
// * stdin and stdout are terminals — a pipe or redirect means a script;
// * `TERM` is not `dumb` — a dumb terminal cannot drive a full-screen UI;
// * `SELFHOSTDEV_NO_TUI` is unset or empty — the explicit opt-out for scripts,
//   editors and CI that happen to run on a TTY.
//
// An unset `TERM` is allowed: the TTY check has already established an
// interactive terminal, and `TERM` is absent in some minimal environments.
pub fn should_launch_tui(
    stdin_tty: bool,
    stdout_tty: bool,
    term: Option<&str>,
    no_tui_env: Option<&str>,
) -> bool {
    stdin_tty && stdout_tty && term != Some("dumb") && no_tui_env.is_none_or(str::is_empty)
}

#[cfg(test)]
mod tests {
    use super::should_launch_tui;

    // Convenience: the predicate with everything in the "launch" position.
    fn launch(stdin_tty: bool, stdout_tty: bool, term: Option<&str>, no_tui: Option<&str>) -> bool {
        should_launch_tui(stdin_tty, stdout_tty, term, no_tui)
    }

    #[test]
    fn interactive_terminal_launches_the_tui() {
        assert!(launch(true, true, Some("xterm-256color"), None));
        // `TERM` unset is still allowed once both streams are TTYs.
        assert!(launch(true, true, None, None));
        // An empty SELFHOSTDEV_NO_TUI is not a set value.
        assert!(launch(true, true, Some("xterm"), Some("")));
    }

    #[test]
    fn non_tty_streams_stay_on_the_cli() {
        assert!(!launch(false, true, Some("xterm"), None));
        assert!(!launch(true, false, Some("xterm"), None));
        assert!(!launch(false, false, None, None));
    }

    #[test]
    fn dumb_terminal_stays_on_the_cli() {
        assert!(!launch(true, true, Some("dumb"), None));
    }

    #[test]
    fn no_tui_env_opts_out() {
        assert!(!launch(true, true, Some("xterm"), Some("1")));
        assert!(!launch(true, true, Some("xterm"), Some("anything")));
    }
}
