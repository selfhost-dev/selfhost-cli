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
mod watch;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            err.code()
        }
    }
}
