//! `redis` — managed Redis clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: redis …`.

use super::*;

stub_group!(
    /// `redis users` — database roles inside one instance.
    RedisUsersCommand, "redis users",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(UserCreateArgs) => "create",
        Update(UserUpdateArgs) => "update",
        Delete(UserRefArgs) => "delete",
        RotatePassword(UserRefArgs) => "rotate-password",
    }
    groups {}
);

stub_group!(
    /// `redis config` — engine parameters.
    RedisConfigCommand, "redis config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `redis snapshots` — provider snapshots.
    RedisSnapshotsCommand, "redis snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `redis backups` — logical/provider backups.
    RedisBackupsCommand, "redis backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `redis pitr` — point-in-time recovery.
    RedisPitrCommand, "redis pitr",
    leaves {
        Status(PitrArgs) => "status",
        Enable(PitrArgs) => "enable",
        Configure(PitrArgs) => "configure",
        Pause(PitrArgs) => "pause",
        Resume(PitrArgs) => "resume",
        Retry(PitrArgs) => "retry",
        Restore(PitrArgs) => "restore",
    }
    groups {}
);

stub_group!(
    /// `redis pool` — connection pooler.
    RedisPoolCommand, "redis pool",
    leaves {
        Show(PidArgs) => "show",
        Enable(PidArgs) => "enable",
        Disable(PidArgs) => "disable",
        Update(PoolUpdateArgs) => "update",
        ReloadUsers(PidArgs) => "reload-users",
    }
    groups {}
);

stub_group!(
    /// Managed Redis clusters.
    RedisCommand, "redis",
    leaves {
        List(EngineListArgs) => "list",
        Show(TargetArgs) => "show",
        Create(EngineCreateArgs) => "create",
        Delete(TargetArgs) => "delete",
        Start(TargetArgs) => "start",
        Stop(TargetArgs) => "stop",
        Reboot(TargetArgs) => "reboot",
        Fork(ForkArgs) => "fork",
        Resize(ResizeArgs) => "resize",
        Scale(ScaleArgs) => "scale",
        Failover(FailoverArgs) => "failover",
        Update(UpdateArgs) => "update",
        Wait(WaitArgs) => "wait",
        Logs(LogsArgs) => "logs",
        Stats(PidArgs) => "stats",
        Metrics(PidArgs) => "metrics",
    }
    groups {
        Users(RedisUsersCommand) => "users",
        Config(RedisConfigCommand) => "config",
        Snapshots(RedisSnapshotsCommand) => "snapshots",
        Backups(RedisBackupsCommand) => "backups",
        Pitr(RedisPitrCommand) => "pitr",
        Pool(RedisPoolCommand) => "pool",
    }
);
