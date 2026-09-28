//! `mysql` — managed MySQL databases (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: mysql …`.

use super::*;

stub_group!(
    /// Database users and roles
    MysqlUsersCommand, "mysql users",
    leaves {
        /// List database users
        List(OptionalTargetArgs) => "list",
        /// Create a database user
        Create(UserCreateArgs) => "create",
        /// Change a user's role or connection limit
        Update(UserUpdateArgs) => "update",
        /// Delete a database user
        Delete(UserRefArgs) => "delete",
        /// Replace a user's password
        RotatePassword(UserRefArgs) => "rotate-password",
    }
    groups {}
);

stub_group!(
    /// Engine parameters
    MysqlConfigCommand, "mysql config",
    leaves {
        /// Show the current parameters
        Show(PidArgs) => "show",
        /// Change one parameter
        Set(EngineConfigSetArgs) => "set",
        /// List parameters that need a restart
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// Storage snapshots
    MysqlSnapshotsCommand, "mysql snapshots",
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
    MysqlBackupsCommand, "mysql backups",
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
    /// Point-in-time recovery
    MysqlPitrCommand, "mysql pitr",
    leaves {
        /// Show the recovery window and status
        Status(PitrArgs) => "status",
        /// Turn on point-in-time recovery
        Enable(PitrArgs) => "enable",
        /// Change the recovery window
        Configure(PitrArgs) => "configure",
        /// Pause recovery logging
        Pause(PitrArgs) => "pause",
        /// Resume recovery logging
        Resume(PitrArgs) => "resume",
        /// Retry a failed recovery job
        Retry(PitrArgs) => "retry",
        /// Restore the database to an earlier moment
        Restore(PitrArgs) => "restore",
    }
    groups {}
);

stub_group!(
    /// Connection pooler (ProxySQL)
    MysqlPoolCommand, "mysql pool",
    leaves {
        /// Show the connection pooler
        Show(PidArgs) => "show",
        /// Turn the connection pooler on
        Enable(PidArgs) => "enable",
        /// Turn the connection pooler off
        Disable(PidArgs) => "disable",
        /// Change connection pooler settings
        Update(PoolUpdateArgs) => "update",
    }
    groups {}
);

stub_group!(
    /// Managed MySQL databases
    MysqlCommand, "mysql",
    leaves {
        /// List your MySQL databases
        List(EngineListArgs) => "list",
        /// Show one MySQL database
        Show(TargetArgs) => "show",
        /// Create a MySQL database
        Create(EngineCreateArgs) => "create",
        /// Delete a MySQL database
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
        Users(MysqlUsersCommand) => "users",
        Config(MysqlConfigCommand) => "config",
        Snapshots(MysqlSnapshotsCommand) => "snapshots",
        Backups(MysqlBackupsCommand) => "backups",
        Pitr(MysqlPitrCommand) => "pitr",
        Pool(MysqlPoolCommand) => "pool",
    }
);
