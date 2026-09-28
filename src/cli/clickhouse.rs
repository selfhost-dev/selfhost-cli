//! `clickhouse` — managed ClickHouse databases (design §4).
//!
//! Slice 0 registers the shared instance verbs, `users`, `snapshots`, `backups` and
//! `pool`. Every handler answers `not implemented yet: clickhouse …`.
//!
//! ClickHouse has no config-tuning endpoint and `supports_pitr?` is false for it, so
//! `config` and `pitr` are not registered. Pooling is CHProxy
//! (`chproxy_controller.rb` guards on `type_of_dbms == "clickhouse"`).

use super::*;

stub_group!(
    /// Database users
    ClickhouseUsersCommand, "clickhouse users",
    leaves {
        /// List database users
        List(OptionalTargetArgs) => "list",
        /// Create a database user
        Create(UserCreateArgs) => "create",
        /// Change a user's role
        Update(UserUpdateArgs) => "update",
        /// Delete a database user
        Delete(UserRefArgs) => "delete",
        /// Replace a user's password
        RotatePassword(UserRefArgs) => "rotate-password",
    }
    groups {}
);

stub_group!(
    /// Storage snapshots
    ClickhouseSnapshotsCommand, "clickhouse snapshots",
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
    ClickhouseBackupsCommand, "clickhouse backups",
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
    /// Connection pooler (CHProxy)
    ClickhousePoolCommand, "clickhouse pool",
    leaves {
        /// Show the connection pooler
        Show(PidArgs) => "show",
        /// Turn the connection pooler on
        Enable(PidArgs) => "enable",
        /// Turn the connection pooler off
        Disable(PidArgs) => "disable",
        /// Change connection pooler settings
        Update(PoolUpdateArgs) => "update",
        /// Reload the pooler's user list
        ReloadUsers(PidArgs) => "reload-users",
    }
    groups {}
);

stub_group!(
    /// Managed ClickHouse databases
    ClickhouseCommand, "clickhouse",
    leaves {
        /// List your ClickHouse databases
        List(EngineListArgs) => "list",
        /// Show one ClickHouse database
        Show(TargetArgs) => "show",
        /// Create a ClickHouse database
        Create(ClickHouseCreateArgs) => "create",
        /// Delete a ClickHouse database
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
        Users(ClickhouseUsersCommand) => "users",
        Snapshots(ClickhouseSnapshotsCommand) => "snapshots",
        Backups(ClickhouseBackupsCommand) => "backups",
        Pool(ClickhousePoolCommand) => "pool",
    }
);
