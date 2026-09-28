//! `scaling` — policies and capacity ladders (design §4).

use clap::Args;

use super::*;

// `scaling create|update`.
#[derive(Debug, Clone, Args)]
pub struct PolicyArgs {
    /// Policy name
    pub name: Option<String>,

    /// Minimum replica count
    #[arg(long)]
    pub min: Option<u32>,

    /// Maximum replica count
    #[arg(long)]
    pub max: Option<u32>,

    /// Metric driving the policy
    #[arg(long)]
    pub metric: Option<String>,
}

// `scaling capacity …`.
#[derive(Debug, Clone, Args)]
pub struct CapacityArgs {
    /// Instance or group to plan for
    pub target: Option<String>,

    /// Region
    #[arg(long)]
    pub region: Option<String>,

    /// Cloud to plan on
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
}

stub_group!(
    /// `scaling capacity` — ladders and scale plans.
    ScalingCapacityCommand, "scaling capacity",
    leaves {
        Ladder(CapacityArgs) => "ladder",
        Config(CapacityArgs) => "config",
        Plan(CapacityArgs) => "plan",
    }
    groups {}
);

stub_group!(
    /// Scaling policies; capacity ladders and scale plans.
    ScalingCommand, "scaling",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(PolicyArgs) => "create",
        Update(PolicyArgs) => "update",
        Delete(TargetArgs) => "delete",
    }
    groups {
        Capacity(ScalingCapacityCommand) => "capacity",
    }
);
