//! `profile` — named API/console endpoints (design §5).

use clap::Args;

use super::*;

// `profile add <name>` — the base URL comes from global `--base-url`/`--org`.
#[derive(Debug, Clone, Args)]
pub struct ProfileAddArgs {
    /// Name for the new profile
    pub name: String,

    /// Console URL used for the browser sign-in step
    #[arg(long = "console-url")]
    pub console_url: Option<String>,
}

// `profile set <name> <key> <value>`.
#[derive(Debug, Clone, Args)]
pub struct ProfileSetArgs {
    /// Profile to change
    pub name: String,

    /// Setting to write (base_url, console_url, org, provider)
    pub key: String,

    /// New value
    pub value: String,
}

stub_group!(
    /// Manage saved profiles: API and console endpoints, default org
    ProfileCommand, "profile",
    leaves {
        /// List your profiles
        List(NoArgs) => "list",
        /// Show one profile
        Show(NameArgs) => "show",
        /// Add a profile
        Add(ProfileAddArgs) => "add",
        /// Delete a profile
        Remove(NameArgs) => "remove",
        /// Make a profile the default
        Use(NameArgs) => "use",
        /// Change one setting in a profile
        Set(ProfileSetArgs) => "set",
    }
    groups {}
);
