//! `selfhost` — the SelfHost platform CLI.
//!
//! Slice 0 ships the command surface, the output/exit-code layer and the
//! profile-store types. Parsing happens here; dispatching happens in
//! [`cli::Cli::run`]. Commands whose behaviour a later slice implements answer
//! with `not implemented yet: <full command path>` on stderr and exit 1.

mod api;
mod auth;
mod cli;
mod config;
mod error;
mod output;
mod tui;
mod watch;

use std::process::ExitCode;

use clap::FromArgMatches;
use clap::error::ErrorKind;

fn main() -> ExitCode {
    // [`cli::command`] is the derive-generated tree plus the generated help
    // trailer and the unified options heading, so parsing goes through it
    // rather than through `Cli::parse`.
    let matches = match cli::command().try_get_matches() {
        Ok(matches) => matches,
        Err(err) => return handle_parse_error(err),
    };
    let cli = match cli::Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(err) => err.exit(),
    };

    report(cli.run())
}

/// A bare `selfhost` on an interactive terminal opens the TUI (k9s-style,
/// design §3/§7); every other parse error — including `--help`/`--version`,
/// which stay errors of their own kind — keeps clap's normal output and exit.
fn handle_parse_error(err: clap::Error) -> ExitCode {
    if is_missing_subcommand(err.kind()) {
        if let Some(refusal) = dry_run_refusal() {
            return report(Err(refusal));
        }
        if tui_enabled() {
            // Same code path as `selfhost tui`: the welcome-screen scaffold today.
            return report(cli::tui::run(cli::tui::TuiArgs::default()));
        }
    }
    err.exit()
}

/// The refusal for a bare invocation with `--dry-run`, if the flag is set.
///
/// A missing subcommand never reaches [`cli::Cli::run`], where the dry-run gate
/// lives, so the flag is read from a tolerant parse: clap fills in the global
/// options it saw before the subcommand went missing. `None` means the flag was
/// absent and the caller keeps its ordinary path — the terminal UI on a
/// terminal, clap's own usage error everywhere else.
fn dry_run_refusal() -> Option<error::Error> {
    let matches = cli::command().ignore_errors(true).get_matches();
    let global = cli::GlobalArgs::from_arg_matches(&matches).ok()?;
    if !global.dry_run {
        return None;
    }
    Some(cli::dry_run_refused(cli::dry_run_subject_of_group(
        matches.subcommand_name(),
    )))
}

/// Whether a parse error means "no subcommand was given".
///
/// clap reports a completely bare invocation as
/// `DisplayHelpOnMissingArgumentOrSubcommand` and an invocation with only
/// global flags (e.g. `selfhost --profile qa`) as `MissingSubcommand`; both are
/// "no subcommand". `--help`/`-h`/`--version`/`-V` are their own kinds and are
/// never treated as this.
fn is_missing_subcommand(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    )
}

/// Gather the environment for [`cli::tui::should_launch_tui`] and ask it.
fn tui_enabled() -> bool {
    use std::io::IsTerminal;

    let term = std::env::var("TERM").ok();
    let no_tui = std::env::var("SELFHOSTDEV_NO_TUI").ok();
    cli::tui::should_launch_tui(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
        term.as_deref(),
        no_tui.as_deref(),
    )
}

/// Print the outcome of a dispatch the way the process should exit.
fn report(result: error::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            err.code()
        }
    }
}
