//! `ssh-key` — organization and project keys, project SSH access (design §4).

use super::*;

stub_group!(
    /// SSH keys shared across your organization
    SshKeyOrgCommand, "ssh-key org",
    leaves {
        /// List organization keys
        List(NoArgs) => "list",
        /// Add an organization key
        Add(SshKeyAddArgs) => "add",
        /// Remove an organization key
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// SSH keys that belong to one project
    SshKeyProjectCommand, "ssh-key project",
    leaves {
        /// List project keys
        List(OptionalTargetArgs) => "list",
        /// Add a project key
        Add(SshKeyAddArgs) => "add",
        /// Remove a project key
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// Who may SSH into a project
    SshKeyAccessCommand, "ssh-key access",
    leaves {
        /// Choose the access mode
        Set(AccessSetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// SSH keys for your organization and projects
    SshKeyCommand, "ssh-key",
    leaves {
    }
    groups {
        Org(SshKeyOrgCommand) => "org",
        Project(SshKeyProjectCommand) => "project",
        Access(SshKeyAccessCommand) => "access",
    }
);
