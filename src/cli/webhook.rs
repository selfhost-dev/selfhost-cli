//! `webhook` — organization webhook endpoints (design §4).

use clap::Args;

use super::*;

// `webhook create|update`.
#[derive(Debug, Clone, Args)]
pub struct WebhookArgs {
    /// Destination URL
    #[arg(long)]
    pub url: Option<String>,

    /// Event to subscribe to (repeatable)
    #[arg(long)]
    pub event: Vec<String>,
}

stub_group!(
    /// Organization webhook endpoints.
    WebhookCommand, "webhook",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(WebhookArgs) => "create",
        Update(WebhookArgs) => "update",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);
