//! `config` — stored defaults (design §3, §5).
//!
//! The CLI-level `config` group is separate from [`crate::config`], which owns the
//! `~/.selfhost/config.json` profile-store types.

use clap::Args;

use super::*;

// `config set <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct ConfigSetArgs {
    /// Setting to write (org, provider, format, timeout)
    pub key: String,

    /// New value
    pub value: String,
}

stub_group!(
    /// Read and write your saved default settings
    ConfigCommand, "config",
    leaves {
        /// List every saved setting
        List(NoArgs) => "list",
        /// Show one saved setting
        Get(NameArgs) => "get",
        /// Change one saved setting
        Set(ConfigSetArgs) => "set",
        /// Remove a saved setting
        Unset(NameArgs) => "unset",
        /// Print where settings are stored
        Path(NoArgs) => "path",
    }
    groups {}
);
