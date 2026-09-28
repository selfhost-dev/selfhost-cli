//! `redis` — managed Redis databases (design §4).
//!
//! Slice 0 registers the shared instance verbs, `users`, `snapshots` and `backups`.
//! Every handler answers `not implemented yet: redis …`.
//!
//! Redis has no config-tuning endpoint and no connection pooler, and the platform's
//! `supports_pitr?` is false for it, so `config`, `pool` and `pitr` are not registered.
//! `users` is registered because the API does support Redis ACL users
//! (`script/redis_user_management.sh`, wired by `redis_adapter.rb#scripts`), even though
//! the console hides them.

use super::*;

stub_group!(
    /// Redis ACL users
    RedisUsersCommand, "redis users",
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
    RedisSnapshotsCommand, "redis snapshots",
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
    RedisBackupsCommand, "redis backups",
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
    /// Managed Redis databases
    RedisCommand, "redis",
    leaves {
        /// List your Redis databases
        List(EngineListArgs) => "list",
        /// Show one Redis database
        Show(TargetArgs) => "show",
        /// Create a Redis database
        Create(EngineCreateArgs) => "create",
        /// Delete a Redis database
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
        Users(RedisUsersCommand) => "users",
        Snapshots(RedisSnapshotsCommand) => "snapshots",
        Backups(RedisBackupsCommand) => "backups",
    }
);
