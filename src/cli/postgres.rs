//! `postgres` — managed PostgreSQL clusters (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: postgres …`.

use super::*;

stub_group!(
    /// `postgres users` — database roles inside one instance.
    PostgresUsersCommand, "postgres users",
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
    /// `postgres config` — engine parameters.
    PostgresConfigCommand, "postgres config",
    leaves {
        Show(PidArgs) => "show",
        Preview(PidArgs) => "preview",
        Set(EngineConfigSetArgs) => "set",
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// `postgres snapshots` — provider snapshots.
    PostgresSnapshotsCommand, "postgres snapshots",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `postgres backups` — logical/provider backups.
    PostgresBackupsCommand, "postgres backups",
    leaves {
        List(OptionalTargetArgs) => "list",
        Create(TargetArgs) => "create",
        Restore(RestoreArgs) => "restore",
        Delete(TargetArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// `postgres pitr` — point-in-time recovery.
    PostgresPitrCommand, "postgres pitr",
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
    /// `postgres pool` — connection pooler.
    PostgresPoolCommand, "postgres pool",
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
    /// `postgres extensions` — extension catalog and install.
    PostgresExtensionsCommand, "postgres extensions",
    leaves {
        Catalog(NoArgs) => "catalog",
        List(OptionalTargetArgs) => "list",
        Enable(ExtensionEnableArgs) => "enable",
    }
    groups {}
);

stub_group!(
    /// `postgres tls` — TLS certificate handling.
    PostgresTlsCommand, "postgres tls",
    leaves {
        Enable(PidArgs) => "enable",
        Verify(PidArgs) => "verify",
        Disable(PidArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// `postgres durability` — durability mode.
    PostgresDurabilityCommand, "postgres durability",
    leaves {
        Set(DurabilitySetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// `postgres replicas` — read replicas.
    PostgresReplicasCommand, "postgres replicas",
    leaves {
        List(PidArgs) => "list",
        Create(ReplicaCreateArgs) => "create",
        Delete(ReplicaDeleteArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Managed PostgreSQL clusters.
    PostgresCommand, "postgres",
    leaves {
        List(EngineListArgs) => "list",
        Show(TargetArgs) => "show",
        Create(PostgresCreateArgs) => "create",
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
        Users(PostgresUsersCommand) => "users",
        Config(PostgresConfigCommand) => "config",
        Snapshots(PostgresSnapshotsCommand) => "snapshots",
        Backups(PostgresBackupsCommand) => "backups",
        Pitr(PostgresPitrCommand) => "pitr",
        Pool(PostgresPoolCommand) => "pool",
        Extensions(PostgresExtensionsCommand) => "extensions",
        Tls(PostgresTlsCommand) => "tls",
        Durability(PostgresDurabilityCommand) => "durability",
        Replicas(PostgresReplicasCommand) => "replicas",
    }
);
