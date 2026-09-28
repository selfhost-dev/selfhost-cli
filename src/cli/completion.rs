//! `completion <shell>` — emit a completion script via `clap_complete` (design §3).

use clap::{Args, ValueEnum};
use clap_complete::Shell;

use crate::error::Result;

use super::{command, tree::write_stdout};

// Shell to generate a completion script for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CompletionShell {
    /// Bash.
    Bash,
    /// Zsh.
    Zsh,
    /// Fish.
    Fish,
}

impl CompletionShell {
    fn as_shell(self) -> Shell {
        match self {
            Self::Bash => Shell::Bash,
            Self::Zsh => Shell::Zsh,
            Self::Fish => Shell::Fish,
        }
    }
}

// Arguments for `selfhost completion`.
#[derive(Debug, Clone, Args)]
pub struct CompletionArgs {
    /// Shell to generate completions for
    pub shell: CompletionShell,
}

// Write the completion script for the requested shell to stdout.
pub fn run(args: CompletionArgs) -> Result<()> {
    let mut command = command();
    let mut script = Vec::new();
    clap_complete::generate(args.shell.as_shell(), &mut command, "selfhost", &mut script);
    let script = String::from_utf8(script).map_err(|err| crate::error::Error::Other(err.into()))?;
    write_stdout(&script)
}
