//! `alert` — rules, triggered instances, notification channels (design §4).

use clap::Args;

use super::*;

// `alert rules create|update`.
#[derive(Debug, Clone, Args)]
pub struct AlertRuleArgs {
    /// Rule name
    pub name: Option<String>,

    /// Metric to watch
    #[arg(long)]
    pub metric: Option<String>,

    /// Value that triggers the alert
    #[arg(long)]
    pub threshold: Option<String>,

    /// How serious it is (info, warning, critical)
    #[arg(long)]
    pub severity: Option<String>,
}

// `alert channels create|update`.
#[derive(Debug, Clone, Args)]
pub struct AlertChannelArgs {
    /// Channel name
    pub name: Option<String>,

    /// Where to send alerts (slack, email, webhook, pagerduty)
    #[arg(long)]
    pub kind: Option<String>,

    /// Destination URL
    #[arg(long)]
    pub url: Option<String>,
}

stub_group!(
    /// When SelfHost should alert you
    AlertRulesCommand, "alert rules",
    leaves {
        /// List rules
        List(NoArgs) => "list",
        /// Show one rule
        Show(TargetArgs) => "show",
        /// Create a rule
        Create(AlertRuleArgs) => "create",
        /// Change a rule
        Update(AlertRuleArgs) => "update",
        /// Delete a rule
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Alerts that fired
    AlertInstancesCommand, "alert instances",
    leaves {
        /// List fired alerts
        List(NoArgs) => "list",
        /// Acknowledge an alert
        Ack(TargetArgs) => "ack",
        /// Resolve an alert
        Resolve(TargetArgs) => "resolve",
    }
    groups {}
);

stub_group!(
    /// Where alerts are delivered
    AlertChannelsCommand, "alert channels",
    leaves {
        /// List channels
        List(NoArgs) => "list",
        /// Show one channel
        Show(TargetArgs) => "show",
        /// Add a channel
        Create(AlertChannelArgs) => "create",
        /// Change a channel
        Update(AlertChannelArgs) => "update",
        /// Delete a channel
        Delete(TargetArgs) => "delete",
        /// Send a test message
        Test(TargetArgs) => "test",
    }
    groups {}
);

stub_group!(
    /// Alert rules, fired alerts and notification channels
    AlertCommand, "alert",
    leaves {
    }
    groups {
        Rules(AlertRulesCommand) => "rules",
        Instances(AlertInstancesCommand) => "instances",
        Channels(AlertChannelsCommand) => "channels",
    }
);
