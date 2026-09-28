//! `project` — projects and their databases, services, backups, snapshots, SSH
//! (design §4).

use clap::Args;

use super::*;

// `project db create <engine>` (design §3 example).
#[derive(Debug, Clone, Args)]
pub struct ProjectDbCreateArgs {
    /// Database engine (`postgresql`, `mysql`, `redis`, `mongodb`)
    pub engine: String,

    /// Owning project
    #[arg(long)]
    pub project: Option<String>,

    /// Database name
    #[arg(long)]
    pub name: Option<String>,
}

// `project service create` — one-click templates.
#[derive(Debug, Clone, Args)]
pub struct ServiceCreateArgs {
    /// Owning project
    #[arg(long)]
    pub project: Option<String>,

    /// One-click service template
    #[arg(long)]
    pub template: Option<String>,

    /// Service name
    #[arg(long)]
    pub name: Option<String>,
}

stub_group!(
    /// `project db` — in-project databases.
    ProjectDbCommand, "project db",
    leaves {
        List(OptionalTargetArgs) => "list",
        Show(TargetArgs) => "show",
        Create(ProjectDbCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
        Connection(TargetArgs) => "connection",
        Logs(LogsArgs) => "logs",
        Stats(PidArgs) => "stats",
    }
    groups {}
);

stub_group!(
    /// `project service custom-domain` — per-service domains.
    ProjectServiceCustomDomainCommand, "project service custom-domain",
    leaves {
        Add(DomainRefArgs) => "add",
        Remove(DomainRefArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// `project service` — one-click services.
    ProjectServiceCommand, "project service",
    leaves {
        List(OptionalTargetArgs) => "list",
        Show(TargetArgs) => "show",
        Create(ServiceCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
        Restart(TargetArgs) => "restart",
        Logs(LogsArgs) => "logs",
        Stats(PidArgs) => "stats",
    }
    groups {
        CustomDomain(ProjectServiceCustomDomainCommand) => "custom-domain",
    }
);

stub_group!(
    /// `project backup` — Coolify backups.
    ProjectBackupCommand, "project backup",
    leaves {
        List(OptionalTargetArgs) => "list",
        Enable(TargetArgs) => "enable",
        Disable(TargetArgs) => "disable",
        Restore(RestoreArgs) => "restore",
    }
    groups {}
);

stub_group!(
    /// `project snapshot` — Coolify snapshots.
    ProjectSnapshotCommand, "project snapshot",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `project ssh key` — project keys.
    ProjectSshKeyCommand, "project ssh key",
    leaves {
        List(OptionalTargetArgs) => "list",
        Add(SshKeyAddArgs) => "add",
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// `project ssh access` — access grants.
    ProjectSshAccessCommand, "project ssh access",
    leaves {
        Set(AccessSetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// `project ssh` — keys and access.
    ProjectSshCommand, "project ssh",
    leaves {
    }
    groups {
        Key(ProjectSshKeyCommand) => "key",
        Access(ProjectSshAccessCommand) => "access",
    }
);

stub_group!(
    /// Projects: in-project databases, services, backups, snapshots, SSH.
    ProjectCommand, "project",
    leaves {
        List(NoArgs) => "list",
        Show(TargetArgs) => "show",
        Create(NameArgs) => "create",
        Update(TargetArgs) => "update",
        Delete(TargetArgs) => "delete",
        Metrics(TargetArgs) => "metrics",
        Activities(ActivityListArgs) => "activities",
    }
    groups {
        Db(ProjectDbCommand) => "db",
        Service(ProjectServiceCommand) => "service",
        Backup(ProjectBackupCommand) => "backup",
        Snapshot(ProjectSnapshotCommand) => "snapshot",
        Ssh(ProjectSshCommand) => "ssh",
    }
);
