//! `mongo` — managed MongoDB databases (design §4).
//!
//! Slice 0 registers the full surface: the shared instance verbs plus `snapshots` and
//! `backups`. Every handler answers `not implemented yet: mongo …`.
//!
//! MongoDB has no engine-side user-management, config-tuning, PITR or pooling scripts
//! (`app/services/database_adapters/mongo_adapter.rb#scripts`), so those groups are not
//! registered here — `TaskCreationService#create_database_user_task` raises outright for
//! an engine without a user-management script.

use super::*;

stub_group!(
    /// Storage snapshots
    MongoSnapshotsCommand, "mongo snapshots",
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
    MongoBackupsCommand, "mongo backups",
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
    /// Managed MongoDB databases
    MongoCommand, "mongo",
    leaves {
        /// List your MongoDB databases
        List(EngineListArgs) => "list",
        /// Show one MongoDB database
        Show(TargetArgs) => "show",
        /// Create a MongoDB database
        Create(EngineCreateArgs) => "create",
        /// Delete a MongoDB database
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
        Snapshots(MongoSnapshotsCommand) => "snapshots",
        Backups(MongoBackupsCommand) => "backups",
    }
);
