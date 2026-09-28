//! `project` — projects and their databases, services, backups, snapshots, SSH
//! (design §4).

use clap::Args;

use super::*;

// `project db create <engine>` (design §3 example).
#[derive(Debug, Clone, Args)]
pub struct ProjectDbCreateArgs {
    /// Database engine (postgresql, mysql, redis, mongodb)
    pub engine: String,

    /// Project the database belongs to
    #[arg(long)]
    pub project: Option<String>,

    /// Database name
    #[arg(long)]
    pub name: Option<String>,
}

// `project service create` — one-click templates.
#[derive(Debug, Clone, Args)]
pub struct ServiceCreateArgs {
    /// Project the service belongs to
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
    /// Databases running inside a project
    ProjectDbCommand, "project db",
    leaves {
        /// List databases
        List(OptionalTargetArgs) => "list",
        /// Show one database
        Show(TargetArgs) => "show",
        /// Create a database
        Create(ProjectDbCreateArgs) => "create",
        /// Delete a database
        Delete(TargetArgs) => "delete",
        /// Show connection details
        Connection(TargetArgs) => "connection",
        /// Show database logs
        Logs(LogsArgs) => "logs",
        /// Show database statistics
        Stats(PidArgs) => "stats",
    }
    groups {}
);

stub_group!(
    /// Domains pointing at one service
    ProjectServiceCustomDomainCommand, "project service custom-domain",
    leaves {
        /// Point a domain at the service
        Add(DomainRefArgs) => "add",
        /// Remove a domain
        Remove(DomainRefArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// One-click services running in a project
    ProjectServiceCommand, "project service",
    leaves {
        /// List services
        List(OptionalTargetArgs) => "list",
        /// Show one service
        Show(TargetArgs) => "show",
        /// Create a service from a template
        Create(ServiceCreateArgs) => "create",
        /// Delete a service
        Delete(TargetArgs) => "delete",
        /// Restart a service
        Restart(TargetArgs) => "restart",
        /// Show service logs
        Logs(LogsArgs) => "logs",
        /// Show service statistics
        Stats(PidArgs) => "stats",
    }
    groups {
        CustomDomain(ProjectServiceCustomDomainCommand) => "custom-domain",
    }
);

stub_group!(
    /// Automatic backups of a project server
    ProjectBackupCommand, "project backup",
    leaves {
        /// List backups
        List(OptionalTargetArgs) => "list",
        /// Turn automatic backups on
        Enable(TargetArgs) => "enable",
        /// Turn automatic backups off
        Disable(TargetArgs) => "disable",
        /// Restore a backup
        Restore(RestoreArgs) => "restore",
    }
    groups {}
);

stub_group!(
    /// Manual snapshots of a project server
    ProjectSnapshotCommand, "project snapshot",
    leaves {
        /// List snapshots
        List(OptionalTargetArgs) => "list",
        /// Take a snapshot now
        Create(TargetArgs) => "create",
        /// Restore a snapshot
        Restore(RestoreArgs) => "restore",
        /// Delete a snapshot
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// SSH keys that can reach the project
    ProjectSshKeyCommand, "project ssh key",
    leaves {
        /// List SSH keys
        List(OptionalTargetArgs) => "list",
        /// Add an SSH key
        Add(SshKeyAddArgs) => "add",
        /// Remove an SSH key
        Remove(TargetArgs) => "remove",
    }
    groups {}
);

stub_group!(
    /// Who may SSH into the project
    ProjectSshAccessCommand, "project ssh access",
    leaves {
        /// Choose the access mode
        Set(AccessSetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// SSH keys and access for the project
    ProjectSshCommand, "project ssh",
    leaves {
    }
    groups {
        Key(ProjectSshKeyCommand) => "key",
        Access(ProjectSshAccessCommand) => "access",
    }
);

stub_group!(
    /// Projects: their databases, services, backups, snapshots and SSH
    ProjectCommand, "project",
    leaves {
        /// List your projects
        List(NoArgs) => "list",
        /// Show one project
        Show(TargetArgs) => "show",
        /// Create a project
        Create(NameArgs) => "create",
        /// Change a project's details
        Update(TargetArgs) => "update",
        /// Delete a project
        Delete(TargetArgs) => "delete",
        /// Show CPU, memory and disk metrics
        Metrics(TargetArgs) => "metrics",
        /// Show what happened in the project
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
