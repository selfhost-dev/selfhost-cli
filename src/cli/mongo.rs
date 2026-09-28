//! `mongo` — managed MongoDB clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: mongo …`.

use super::*;

stub_group!(
    /// `mongo users` — database roles inside one instance.
    MongoUsersCommand, "mongo users",
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
    /// `mongo config` — engine parameters.
    MongoConfigCommand, "mongo config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `mongo snapshots` — provider snapshots.
    MongoSnapshotsCommand, "mongo snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `mongo backups` — logical/provider backups.
    MongoBackupsCommand, "mongo backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `mongo pitr` — point-in-time recovery.
    MongoPitrCommand, "mongo pitr",
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
    /// `mongo pool` — connection pooler.
    MongoPoolCommand, "mongo pool",
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
    /// Managed MongoDB clusters.
    MongoCommand, "mongo",
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
        Users(MongoUsersCommand) => "users",
        Config(MongoConfigCommand) => "config",
        Snapshots(MongoSnapshotsCommand) => "snapshots",
        Backups(MongoBackupsCommand) => "backups",
        Pitr(MongoPitrCommand) => "pitr",
        Pool(MongoPoolCommand) => "pool",
    }
);
