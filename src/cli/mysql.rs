//! `mysql` — managed MySQL clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: mysql …`.

use super::*;

stub_group!(
    /// `mysql users` — database roles inside one instance.
    MysqlUsersCommand, "mysql users",
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
    /// `mysql config` — engine parameters.
    MysqlConfigCommand, "mysql config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `mysql snapshots` — provider snapshots.
    MysqlSnapshotsCommand, "mysql snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `mysql backups` — logical/provider backups.
    MysqlBackupsCommand, "mysql backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `mysql pitr` — point-in-time recovery.
    MysqlPitrCommand, "mysql pitr",
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
    /// `mysql pool` — connection pooler.
    MysqlPoolCommand, "mysql pool",
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
    /// Managed MySQL clusters.
    MysqlCommand, "mysql",
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
        Users(MysqlUsersCommand) => "users",
        Config(MysqlConfigCommand) => "config",
        Snapshots(MysqlSnapshotsCommand) => "snapshots",
        Backups(MysqlBackupsCommand) => "backups",
        Pitr(MysqlPitrCommand) => "pitr",
        Pool(MysqlPoolCommand) => "pool",
    }
);
