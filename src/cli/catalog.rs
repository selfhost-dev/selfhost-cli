//! `catalog` — regions, instance types, storage types, cost estimates (design §4).

use clap::Args;

use super::*;

// `catalog instance-types --region`.
#[derive(Debug, Clone, Args)]
pub struct InstanceTypesArgs {
    /// Region to list for
    #[arg(long)]
    pub region: Option<String>,

    /// Cloud to list from
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,
}

// `catalog estimate` — monthly cost of a prospective instance.
#[derive(Debug, Clone, Args)]
pub struct EstimateArgs {
    /// Instance type to price
    #[arg(long = "instance-type")]
    pub instance_type: Option<String>,

    /// Storage size, e.g. `100gb`
    #[arg(long)]
    pub size: Option<String>,

    /// Region
    #[arg(long)]
    pub region: Option<String>,

    /// Cloud to price on
    #[arg(long, value_enum)]
    pub provider: Option<Provider>,

    /// Hours per month to assume
    #[arg(long)]
    pub hours: Option<u32>,
}

stub_group!(
    /// Regions, instance types, storage types, cost estimates.
    CatalogCommand, "catalog",
    leaves {
        Regions(NoArgs) => "regions",
        InstanceTypes(InstanceTypesArgs) => "instance-types",
        StorageTypes(NoArgs) => "storage-types",
        Estimate(EstimateArgs) => "estimate",
    }
    groups {}
);
