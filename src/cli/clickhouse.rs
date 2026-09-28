//! `clickhouse` — managed ClickHouse clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: clickhouse …`.

use super::*;

stub_group!(
    /// `clickhouse users` — database roles inside one instance.
    ClickhouseUsersCommand, "clickhouse users",
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
    /// `clickhouse config` — engine parameters.
    ClickhouseConfigCommand, "clickhouse config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `clickhouse snapshots` — provider snapshots.
    ClickhouseSnapshotsCommand, "clickhouse snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `clickhouse backups` — logical/provider backups.
    ClickhouseBackupsCommand, "clickhouse backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `clickhouse pitr` — point-in-time recovery.
    ClickhousePitrCommand, "clickhouse pitr",
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
    /// `clickhouse pool` — connection pooler.
    ClickhousePoolCommand, "clickhouse pool",
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
    /// Managed ClickHouse clusters.
    ClickhouseCommand, "clickhouse",
    leaves {
        List(EngineListArgs) => "list",
        Show(TargetArgs) => "show",
        Create(ClickHouseCreateArgs) => "create",
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
        Users(ClickhouseUsersCommand) => "users",
        Config(ClickhouseConfigCommand) => "config",
        Snapshots(ClickhouseSnapshotsCommand) => "snapshots",
        Backups(ClickhouseBackupsCommand) => "backups",
        Pitr(ClickhousePitrCommand) => "pitr",
        Pool(ClickhousePoolCommand) => "pool",
    }
);
