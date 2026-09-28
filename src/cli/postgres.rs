//! `postgres` — managed PostgreSQL databases (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs, `users`, `config`,
//! `snapshots`, `backups`, `pitr`, `pool` and the engine-specific groups. Every handler
//! answers `not implemented yet: postgres …`.

use super::*;

stub_group!(
    /// Database users and roles
    PostgresUsersCommand, "postgres users",
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
    PostgresConfigCommand, "postgres config",
    leaves {
        /// Show the current parameters
        Show(PidArgs) => "show",
        /// Preview what a parameter change would do
        Preview(PidArgs) => "preview",
        /// Change one parameter
        Set(EngineConfigSetArgs) => "set",
        /// List parameters that need a restart
        RestartRequired(PidArgs) => "restart-required",
    }
    groups {}
);

stub_group!(
    /// Storage snapshots
    PostgresSnapshotsCommand, "postgres snapshots",
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
    PostgresBackupsCommand, "postgres backups",
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
    PostgresPitrCommand, "postgres pitr",
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
    /// Connection pooler (PgBouncer)
    PostgresPoolCommand, "postgres pool",
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
    /// PostgreSQL extensions
    PostgresExtensionsCommand, "postgres extensions",
    leaves {
        /// List the extensions SelfHost can install
        Catalog(NoArgs) => "catalog",
        /// List the extensions installed on a database
        List(OptionalTargetArgs) => "list",
        /// Install an extension
        Enable(ExtensionEnableArgs) => "enable",
    }
    groups {}
);

stub_group!(
    /// TLS for client connections
    PostgresTlsCommand, "postgres tls",
    leaves {
        /// Turn TLS on
        Enable(PidArgs) => "enable",
        /// Check the TLS certificate
        Verify(PidArgs) => "verify",
        /// Turn TLS off
        Disable(PidArgs) => "disable",
    }
    groups {}
);

stub_group!(
    /// Write durability mode
    PostgresDurabilityCommand, "postgres durability",
    leaves {
        /// Choose the durability mode
        Set(DurabilitySetArgs) => "set",
    }
    groups {}
);

stub_group!(
    /// Read replicas
    PostgresReplicasCommand, "postgres replicas",
    leaves {
        /// List read replicas
        List(PidArgs) => "list",
        /// Add read replicas
        Create(ReplicaCreateArgs) => "create",
        /// Remove a read replica
        Delete(ReplicaDeleteArgs) => "delete",
    }
    groups {}
);

stub_group!(
    /// Managed PostgreSQL databases
    PostgresCommand, "postgres",
    leaves {
        /// List your PostgreSQL databases
        List(EngineListArgs) => "list",
        /// Show one PostgreSQL database
        Show(TargetArgs) => "show",
        /// Create a PostgreSQL database
        Create(PostgresCreateArgs) => "create",
        /// Delete a PostgreSQL database
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
