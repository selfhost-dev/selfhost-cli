//! `scaling` — policies and capacity ladders (design §4).

use clap::Args;

use super::*;

// `scaling create|update`.
#[derive(Debug, Clone, Args)]
pub struct PolicyArgs {
    /// Policy name
    pub name: Option<String>,

    /// Smallest replica count to keep
    #[arg(long)]
    pub min: Option<u32>,

    /// Largest replica count to allow
    #[arg(long)]
    pub max: Option<u32>,

    /// Metric that drives the policy
    #[arg(long)]
    pub metric: Option<String>,
}

// `scaling capacity …`.
#[derive(Debug, Clone, Args)]
pub struct CapacityArgs {
    /// Database or group to plan for
    pub target: Option<String>,

    /// Region
    #[arg(long)]
    pub region: Option<String>,

    /// Cloud to plan on
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
}

stub_group!(
    /// How far a database can grow
    ScalingCapacityCommand, "scaling capacity",
    leaves {
        /// Show the available instance sizes
        Ladder(CapacityArgs) => "ladder",
        /// Show the capacity settings
        Config(CapacityArgs) => "config",
        /// Show the current scale plan
        Plan(CapacityArgs) => "plan",
    }
    groups {}
);

stub_group!(
    /// Scaling policies, capacity ladders and scale plans
    ScalingCommand, "scaling",
    leaves {
        /// List policies
        List(NoArgs) => "list",
        /// Show one policy
        Show(TargetArgs) => "show",
        /// Create a policy
        Create(PolicyArgs) => "create",
        /// Change a policy
        Update(PolicyArgs) => "update",
        /// Delete a policy
        Delete(TargetArgs) => "delete",
    }
    groups {
        Capacity(ScalingCapacityCommand) => "capacity",
    }
);
