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

    /// Threshold expression
    #[arg(long)]
    pub threshold: Option<String>,

    /// Severity (`info`, `warning`, `critical`)
    #[arg(long)]
    pub severity: Option<String>,
}

// `alert channels create|update`.
#[derive(Debug, Clone, Args)]
pub struct AlertChannelArgs {
    /// Channel name
    pub name: Option<String>,

    /// Channel kind (`slack`, `email`, `webhook`, `pagerduty`)
    #[arg(long)]
    pub kind: Option<String>,

    /// Destination URL
    #[arg(long)]
    pub url: Option<String>,
}

stub_group!(
    /// `alert rules` — threshold rules.
    AlertRulesCommand, "alert rules",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(AlertRuleArgs) => "create",
        Update(AlertRuleArgs) => "update",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `alert instances` — fired alerts.
    AlertInstancesCommand, "alert instances",
    leaves {
        List(NoArgs) => "list",
        Ack(TargetArgs) => "ack",
        Resolve(TargetArgs) => "resolve",
    }
    groups {}
);

stub_group!(
    /// `alert channels` — notification channels.
    AlertChannelsCommand, "alert channels",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(AlertChannelArgs) => "create",
        Update(AlertChannelArgs) => "update",
        Delete(TargetArgs) => "delete",
        Test(TargetArgs) => "test",
    }
    groups {}
);

stub_group!(
    /// Alert rules, triggered instances, notification channels.
    AlertCommand, "alert",
    leaves {
    }
    groups {
        Rules(AlertRulesCommand) => "rules",
        Instances(AlertInstancesCommand) => "instances",
        Channels(AlertChannelsCommand) => "channels",
    }
);
