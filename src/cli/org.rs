//! `org` — organizations, members, invitations, activity log (design §4).

use clap::Args;

use super::*;

// `org create <name> [--slug]`.
#[derive(Debug, Clone, Args)]
pub struct OrgCreateArgs {
    /// Organization name
    pub name: String,

    /// Slug to claim
    #[arg(long)]
    pub slug: Option<String>,
}

// Members and invitations share this shape.
#[derive(Debug, Clone, Args)]
pub struct MemberArgs {
    /// Email address of the person
    pub email: Option<String>,

    /// Role to grant (owner, admin, member, viewer)
    #[arg(long)]
    pub role: Option<String>,
}

stub_group!(
    /// People in your organization and their roles
    OrgMembersCommand, "org members",
    leaves {
        /// List members
        List(OptionalTargetArgs) => "list",
        /// Add a member
        Add(MemberArgs) => "add",
        /// Remove a member
        Remove(MemberArgs) => "remove",
        /// Change a member's role
        UpdateRole(MemberArgs) => "update-role",
    }
    groups {}
);

stub_group!(
    /// Roles you can grant
    OrgRolesCommand, "org roles",
    leaves {
        /// List roles
        List(NoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Pending invitations
    OrgInvitesCommand, "org invites",
    leaves {
        /// List pending invitations
        List(NoArgs) => "list",
        /// Invite someone
        Create(MemberArgs) => "create",
        /// Cancel an invitation
        Revoke(MemberArgs) => "revoke",
    }
    groups {}
);

stub_group!(
    /// What happened in your organization
    OrgActivityCommand, "org activity",
    leaves {
        /// List recent activity
        List(ActivityListArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Organizations, members, invitations and activity
    OrgCommand, "org",
    leaves {
        /// List your organizations
        List(NoArgs) => "list",
        /// Create an organization
        Create(OrgCreateArgs) => "create",
        /// Show one organization
        Show(TargetArgs) => "show",
        /// Change an organization's details
        Update(TargetArgs) => "update",
        /// Delete an organization
        Delete(TargetArgs) => "delete",
        /// Make an organization the default
        Use(NameArgs) => "use",
    }
    groups {
        Members(OrgMembersCommand) => "members",
        Roles(OrgRolesCommand) => "roles",
        Invites(OrgInvitesCommand) => "invites",
        Activity(OrgActivityCommand) => "activity",
    }
);
