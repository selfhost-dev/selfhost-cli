//! `config` — stored defaults (design §3, §5).
//!
//! The CLI-level `config` group is separate from [`crate::config`], which owns the
//! `~/.selfhost/config.json` profile-store types.

use clap::Args;

use super::*;

// `config set <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct ConfigSetArgs {
    /// Default to write (`org`, `provider`, `format`, `timeout`)
    pub key: String,

    /// New value
    pub value: String,
}

stub_group!(
    /// Read/write defaults (org, provider, format, timeout).
    ConfigCommand, "config",
    leaves {
        List(NoArgs) => "list",
        Get(NameArgs) => "get",
        Set(ConfigSetArgs) => "set",
        Unset(NameArgs) => "unset",
        Path(NoArgs) => "path",
    }
    groups {}
);
