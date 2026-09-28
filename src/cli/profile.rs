//! `profile` — named API/console endpoints (design §5).

use clap::Args;

use super::*;

// `profile add <name>` — the base URL comes from global `--base-url`/`--org`.
#[derive(Debug, Clone, Args)]
pub struct ProfileAddArgs {
    /// Profile name
    pub name: String,

    /// Console URL used for the browser-login hop
    #[arg(long = "console-url")]
    pub console_url: Option<String>,
}

// `profile set <name> <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct ProfileSetArgs {
    /// Profile name
    pub name: String,

    /// Field to set (`base_url`, `console_url`, `org`, `provider`)
    pub key: String,

    /// New value
    pub value: String,
}

stub_group!(
    /// Manage profiles — name, API base URL, console URL.
    ProfileCommand, "profile",
    leaves {
        List(NoArgs) => "list",
        Show(NameArgs) => "show",
        Add(ProfileAddArgs) => "add",
        Remove(NameArgs) => "remove",
        Use(NameArgs) => "use",
        Set(ProfileSetArgs) => "set",
    }
    groups {}
);
