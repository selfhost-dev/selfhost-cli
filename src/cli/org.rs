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
    /// Member email
    pub email: Option<String>,

    /// Role to grant (`owner`, `admin`, `member`, `viewer`)
    #[arg(long)]
    pub role: Option<String>,
}

stub_group!(
    /// `org members` — membership management.
    OrgMembersCommand, "org members",
    leaves {
        List(OptionalTargetArgs) => "list",
        Add(MemberArgs) => "add",
        Remove(MemberArgs) => "remove",
        UpdateRole(MemberArgs) => "update-role",
    }
    groups {}
);

stub_group!(
    /// `org roles` — available roles.
    OrgRolesCommand, "org roles",
    leaves {
        List(NoArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// `org invites` — pending invitations.
    OrgInvitesCommand, "org invites",
    leaves {
        List(NoArgs) => "list",
        Create(MemberArgs) => "create",
        Revoke(MemberArgs) => "revoke",
    }
    groups {}
);

stub_group!(
    /// `org activity` — organization activity log.
    OrgActivityCommand, "org activity",
    leaves {
        List(ActivityListArgs) => "list",
    }
    groups {}
);

stub_group!(
    /// Organizations, members, invitations, activity log.
    OrgCommand, "org",
    leaves {
        List(NoArgs) => "list",
        Create(OrgCreateArgs) => "create",
        Show(TargetArgs) => "show",
        Update(TargetArgs) => "update",
        Delete(TargetArgs) => "delete",
        Use(NameArgs) => "use",
    }
    groups {
        Members(OrgMembersCommand) => "members",
        Roles(OrgRolesCommand) => "roles",
        Invites(OrgInvitesCommand) => "invites",
        Activity(OrgActivityCommand) => "activity",
    }
);
