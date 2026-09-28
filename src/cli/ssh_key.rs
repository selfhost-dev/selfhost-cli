//! `ssh-key` — organization and project keys, project SSH access (design §4).

use super::*;

stub_group!(
    /// `ssh-key org` — organization keys.
    SshKeyOrgCommand, "ssh-key org",
    leaves {
        List(NoArgs) => "list",
        Add(SshKeyAddArgs) => "add",
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// `ssh-key project` — project keys.
    SshKeyProjectCommand, "ssh-key project",
    leaves {
        List(OptionalTargetArgs) => "list",
        Add(SshKeyAddArgs) => "add",
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// `ssh-key access` — project SSH access.
    SshKeyAccessCommand, "ssh-key access",
    leaves {
        Set(AccessSetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// Organization and project SSH keys, project SSH access.
    SshKeyCommand, "ssh-key",
    leaves {
    }
    groups {
        Org(SshKeyOrgCommand) => "org",
        Project(SshKeyProjectCommand) => "project",
        Access(SshKeyAccessCommand) => "access",
    }
);
