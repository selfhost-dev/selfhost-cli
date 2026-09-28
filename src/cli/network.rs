//! `network` — VPCs, subnets, security groups (design §4).

use clap::Args;

use super::*;

// `create` for the three network resources.
#[derive(Debug, Clone, Args)]
pub struct NetworkCreateArgs {
    /// Name for the new resource
    pub name: Option<String>,

    /// CIDR block (VPCs and subnets)
    #[arg(long)]
    pub cidr: Option<String>,
}

stub_group!(
    /// Your virtual private clouds
    NetworkVpcCommand, "network vpc",
    leaves {
        /// List VPCs
        List(NoArgs) => "list",
        /// Show one VPC
        Show(TargetArgs) => "show",
        /// Create a VPC
        Create(NetworkCreateArgs) => "create",
        /// Delete a VPC
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Subnets inside your VPCs
    NetworkSubnetCommand, "network subnet",
    leaves {
        /// List subnets
        List(NoArgs) => "list",
        /// Show one subnet
        Show(TargetArgs) => "show",
        /// Create a subnet
        Create(NetworkCreateArgs) => "create",
        /// Delete a subnet
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Firewall rules for your databases
    NetworkSecurityGroupCommand, "network security-group",
    leaves {
        /// List security groups
        List(NoArgs) => "list",
        /// Show one security group
        Show(TargetArgs) => "show",
        /// Create a security group
        Create(NetworkCreateArgs) => "create",
        /// Delete a security group
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// VPCs, subnets and security groups
    NetworkCommand, "network",
    leaves {
    }
    groups {
        Vpc(NetworkVpcCommand) => "vpc",
        Subnet(NetworkSubnetCommand) => "subnet",
        SecurityGroup(NetworkSecurityGroupCommand) => "security-group",
    }
);
