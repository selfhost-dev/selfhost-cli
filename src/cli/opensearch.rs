//! `opensearch` — managed OpenSearch databases (design §4).
//!
//! Slice 0 registers the shared instance verbs, `users`, `snapshots`, `backups` and
//! `dashboards`. Every handler answers `not implemented yet: opensearch …`.
//!
//! OpenSearch reports `supports_config_tuning?` false, `supports_pitr?` false and no
//! pooler, so `config`, `pitr` and `pool` are not registered. Dashboards is
//! OpenSearch-only (`instances_controller.rb#dashboards`).

use super::*;

stub_group!(
    /// Security-plugin users
    OpensearchUsersCommand, "opensearch users",
    leaves {
        /// List users
        List(OptionalTargetArgs) => "list",
        /// Create a user
        Create(UserCreateArgs) => "create",
        /// Change a user's role
        Update(UserUpdateArgs) => "update",
        /// Delete a user
        Delete(UserRefArgs) => "delete",
        /// Replace a user's password
        RotatePassword(UserRefArgs) => "rotate-password",
    }
    groups {}
);

stub_group!(
    /// Storage snapshots
    OpensearchSnapshotsCommand, "opensearch snapshots",
    leaves {
        /// List snapshots
        List(OptionalTargetArgs) => "list",
        /// Take a snapshot now
        Create(TargetArgs) => "create",
        /// Restore a snapshot into a new database
        Restore(RestoreArgs) => "restore",
        /// Delete a snapshot
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Backups you can restore from
    OpensearchBackupsCommand, "opensearch backups",
    leaves {
        /// List backups
        List(OptionalTargetArgs) => "list",
        /// Create a backup now
        Create(TargetArgs) => "create",
        /// Restore a backup into a new database
        Restore(RestoreArgs) => "restore",
        /// Delete a backup
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// OpenSearch Dashboards
    OpensearchDashboardsCommand, "opensearch dashboards",
    leaves {
        /// Show the Dashboards address and status
        Show(PidArgs) => "show",
        /// Turn Dashboards on
        Enable(PidArgs) => "enable",
        /// Turn Dashboards off
        Disable(PidArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// Managed OpenSearch databases
    OpensearchCommand, "opensearch",
    leaves {
        /// List your OpenSearch databases
        List(EngineListArgs) => "list",
        /// Show one OpenSearch database
        Show(TargetArgs) => "show",
        /// Create an OpenSearch database
        Create(EngineCreateArgs) => "create",
        /// Delete an OpenSearch database
        Delete(TargetArgs) => "delete",
        /// Start a stopped database
        Start(TargetArgs) => "start",
        /// Stop a running database
        Stop(TargetArgs) => "stop",
        /// Reboot a database
        Reboot(TargetArgs) => "reboot",
        /// Clone a database into a new one
        Fork(ForkArgs) => "fork",
        /// Change the instance type or storage size
        Resize(ResizeArgs) => "resize",
        /// Change how many replicas the database runs
        Scale(ScaleArgs) => "scale",
        /// Promote another node of a high-availability database
        Failover(FailoverArgs) => "failover",
        /// Change tags, public access and delete protection
        Update(UpdateArgs) => "update",
        /// Wait until a database finishes provisioning
        Wait(WaitArgs) => "wait",
        /// Show database logs
        Logs(LogsArgs) => "logs",
        /// Show query and connection statistics
        Stats(PidArgs) => "stats",
        /// Show CPU, memory and disk metrics
        Metrics(PidArgs) => "metrics",
    }
    groups {
        Users(OpensearchUsersCommand) => "users",
        Snapshots(OpensearchSnapshotsCommand) => "snapshots",
        Backups(OpensearchBackupsCommand) => "backups",
        Dashboards(OpensearchDashboardsCommand) => "dashboards",
    }
);
