//! `webhook` — organization webhook endpoints (design §4).

use clap::Args;

use super::*;

// `webhook create|update`.
#[derive(Debug, Clone, Args)]
pub struct WebhookArgs {
    /// Where to send the events
    #[arg(long)]
    pub url: Option<String>,

    /// Event to subscribe to (repeatable)
    #[arg(long)]
    pub event: Vec<String>,
}

stub_group!(
    /// Webhook endpoints for your organization
    WebhookCommand, "webhook",
    leaves {
        /// List endpoints
        List(NoArgs) => "list",
        /// Show one endpoint
        Show(TargetArgs) => "show",
        /// Add an endpoint
        Create(WebhookArgs) => "create",
        /// Change an endpoint
        Update(WebhookArgs) => "update",
        /// Delete an endpoint
        Delete(TargetArgs) => "delete",
    }
    groups {}
);
