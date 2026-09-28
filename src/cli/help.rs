//! `help [COMMAND…]` — same as `<command> --help` (design §3, §6).

use clap::{Args, Command};

use crate::error::{Error, Result};

use super::command;

// Arguments for `selfhost help`.
#[derive(Debug, Clone, Args)]
pub struct HelpArgs {
    /// Command path to print help for, e.g. `postgres users`
    #[arg(value_name = "COMMAND", num_args = 0..)]
    pub command: Vec<String>,
}

// Print the help of the addressed command (the root when no path is given).
//
// The banner keeps the full invocation path (`selfhost postgres users`) rather than
// clap's bare subcommand name: the Ruby CLI's "nested help drops the parent
// namespace" drift is explicitly not inherited (design §2).
pub fn run(args: HelpArgs) -> Result<()> {
    let mut target = find(command(), &args.command).ok_or_else(|| {
        Error::Usage(format!(
            "unknown command: selfhost {}",
            args.command.join(" ")
        ))
    })?;

    if !args.command.is_empty() {
        target = target.bin_name(format!("selfhost {}", args.command.join(" ")));
    }

    // A closed pipe (`selfhost help postgres users | head`) is a clean exit, not an
    // error — same contract as `tree` (see `tree::write_stdout`).
    match target.print_help() {
        Ok(()) => println!(),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(err) => return Err(Error::Other(err.into())),
    }
    Ok(())
}

// Walk the command tree by name, cloning as we descend (borrows nothing from the root).
fn find(command: Command, path: &[String]) -> Option<Command> {
    let Some((head, rest)) = path.split_first() else {
        return Some(command);
    };
    let next = command.find_subcommand(head)?.clone();
    find(next, rest)
}
