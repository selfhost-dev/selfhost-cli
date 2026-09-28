//! `network` — VPCs, subnets, security groups (design §4).

use clap::Args;

use super::*;

// `create` for the three network resources.
#[derive(Debug, Clone, Args)]
pub struct NetworkCreateArgs {
    /// Resource name
    pub name: Option<String>,

    /// CIDR block (`vpc`/`subnet`)
    #[arg(long)]
    pub cidr: Option<String>,
}

stub_group!(
    /// `network vpc` — VPCs.
    NetworkVpcCommand, "network vpc",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(NetworkCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `network subnet` — subnets.
    NetworkSubnetCommand, "network subnet",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(NetworkCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `network security-group` — security groups.
    NetworkSecurityGroupCommand, "network security-group",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(NetworkCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// VPCs, subnets, security groups.
    NetworkCommand, "network",
    leaves {
    }
    groups {
        Vpc(NetworkVpcCommand) => "vpc",
        Subnet(NetworkSubnetCommand) => "subnet",
        SecurityGroup(NetworkSecurityGroupCommand) => "security-group",
    }
);
