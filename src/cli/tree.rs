//! `tree` — print the full command tree (design §3, §6).
//!
//! Ruby's `tree` was documented but absent; this one ships for real because agent
//! workflows depend on enumerating every command path.

use std::io::Write;

use clap::CommandFactory;

use crate::error::{Error, Result};

use super::Cli;

// Print `selfhost <group> <sub>` for every registered command, one per line,
// by walking the clap tree instead of a hand-maintained list.
pub fn run() -> Result<()> {
    let command = Cli::command();
    let mut buffer = Vec::new();
    write_tree(&command, "selfhost", &mut buffer).expect("writing into memory cannot fail");
    let tree = String::from_utf8(buffer).map_err(|err| Error::Other(err.into()))?;
    write_stdout(&tree)
}

/// Write to stdout, treating a closed pipe (`| head`) as a clean exit.
pub(crate) fn write_stdout(text: &str) -> Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(Error::Other(err.into())),
    }
}

fn write_tree(command: &clap::Command, prefix: &str, out: &mut impl Write) -> std::io::Result<()> {
    for subcommand in command.get_subcommands() {
        if subcommand.is_hide_set() {
            continue;
        }
        let path = format!("{prefix} {}", subcommand.get_name());
        writeln!(out, "{path}")?;
        write_tree(subcommand, &path, out)?;
    }
    Ok(())
}
