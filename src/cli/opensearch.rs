//! `opensearch` — managed OpenSearch clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: opensearch …`.

use super::*;

stub_group!(
    /// `opensearch users` — database roles inside one instance.
    OpensearchUsersCommand, "opensearch users",
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
    /// `opensearch config` — engine parameters.
    OpensearchConfigCommand, "opensearch config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `opensearch snapshots` — provider snapshots.
    OpensearchSnapshotsCommand, "opensearch snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `opensearch backups` — logical/provider backups.
    OpensearchBackupsCommand, "opensearch backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `opensearch pitr` — point-in-time recovery.
    OpensearchPitrCommand, "opensearch pitr",
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
    /// `opensearch pool` — connection pooler.
    OpensearchPoolCommand, "opensearch pool",
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
    /// `opensearch dashboards` — managed Dashboards.
    OpensearchDashboardsCommand, "opensearch dashboards",
    leaves {
        Show(PidArgs) => "show",
        Enable(PidArgs) => "enable",
        Disable(PidArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// Managed OpenSearch clusters.
    OpensearchCommand, "opensearch",
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
        Users(OpensearchUsersCommand) => "users",
        Config(OpensearchConfigCommand) => "config",
        Snapshots(OpensearchSnapshotsCommand) => "snapshots",
        Backups(OpensearchBackupsCommand) => "backups",
        Pitr(OpensearchPitrCommand) => "pitr",
        Pool(OpensearchPoolCommand) => "pool",
        Dashboards(OpensearchDashboardsCommand) => "dashboards",
    }
);
